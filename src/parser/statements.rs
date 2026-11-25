use super::{Parser, MAX_PARSE_RECURSION_DEPTH};
use crate::ast::{Expr, Stmt};
use crate::error::{CompileError, Result};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_block(&mut self) -> Result<Vec<Stmt>> {
        let start = self.current().span;
        let body = self.parse_block_inner()?;
        self.record_syntax_node(SyntaxKind::Block, start);
        Ok(body)
    }

    fn parse_block_inner(&mut self) -> Result<Vec<Stmt>> {
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if let Some(stmt) = self.parse_stmt_recovering()? {
                stmts.push(stmt);
            }
            self.skip_newlines();
        }

        self.expect(TokenKind::RBrace)?;
        Ok(stmts)
    }

    pub(super) fn parse_stmt(&mut self) -> Result<Stmt> {
        let syntax_start = self.current().span;
        if self.statement_depth >= MAX_PARSE_RECURSION_DEPTH {
            return Err(CompileError::new(
                format!("statement nesting exceeds the parser limit of {}", MAX_PARSE_RECURSION_DEPTH),
                self.current().span,
            ));
        }
        self.statement_depth += 1;
        let result = self.parse_stmt_inner();
        self.statement_depth -= 1;
        if let Ok(statement) = &result {
            self.record_syntax_node(
                SyntaxKind::Statement {
                    expression: matches!(statement, Stmt::Expr(_) | Stmt::Discard(_)),
                    terminated: matches!(self.previous_non_newline().kind, TokenKind::Semi),
                },
                syntax_start,
            );
        }
        result
    }

    fn parse_stmt_inner(&mut self) -> Result<Stmt> {
        // Retired condition sections are recognized only at statement
        // boundaries. These contextual names remain legal helpers/values.
        let next = self.tokens[self.position + 1..].iter().find(|token| !matches!(token.kind, TokenKind::Newline));
        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "verify")
            && next.is_some_and(|token| matches!(token.kind, TokenKind::LBrace))
        {
            return Err(CompileError::new("`verify` sections were removed; write `require` statements directly", self.current().span));
        }
        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "enforce")
            && matches!(
                self.peek(1).kind,
                TokenKind::Identifier(_) | TokenKind::Integer(_) | TokenKind::True | TokenKind::False | TokenKind::Not
            )
        {
            return Err(CompileError::new("`enforce` predicates were removed; use `require`", self.current().span));
        }

        let stmt = match &self.current().kind {
            TokenKind::Let => Stmt::Let(self.parse_let()?),
            TokenKind::Consume => return self.parse_consume_statement(),
            TokenKind::Return => Stmt::Return(self.parse_return()?),
            TokenKind::If => Stmt::If(self.parse_if()?),
            TokenKind::For => Stmt::For(self.parse_for()?),
            TokenKind::While => Stmt::While(self.parse_while()?),
            TokenKind::Break => Stmt::Break(self.parse_loop_control(TokenKind::Break)?),
            TokenKind::Continue => Stmt::Continue(self.parse_loop_control(TokenKind::Continue)?),
            TokenKind::Label => self.parse_labeled_loop()?,
            TokenKind::Borrow => Stmt::Borrow(self.parse_borrow()?),
            _ if self.is_replace_relation_ahead() => Stmt::Expr(self.parse_replace_relation()?),
            _ => {
                let expr = self.parse_expr()?;
                Stmt::Expr(expr)
            }
        };
        match stmt {
            Stmt::Let(_) | Stmt::Return(_) | Stmt::Break(_) | Stmt::Continue(_) => {
                self.expect_statement_end()?;
                Ok(stmt)
            }
            Stmt::Expr(expr) => {
                self.skip_newlines();
                let structured = matches!(
                    expr,
                    Expr::Block(_)
                        | Expr::Match(_)
                        | Expr::Create(_)
                        | Expr::CreateUnique(_)
                        | Expr::ReplaceUnique(_)
                        | Expr::ReplaceRelation(_)
                        | Expr::Preserve(_)
                ) || matches!(&expr, Expr::Call(call) if matches!(call.func.as_ref(), Expr::Identifier(name) if matches!(name.as_str(), "__cellscript_consume_each" | "__cellscript_create_each")));
                let must_terminate = matches!(expr, Expr::Assign(_) | Expr::Require(_) | Expr::RequireBlock(_) | Expr::Destroy(_));
                if self.check(&TokenKind::Semi) {
                    self.advance();
                    Ok(Stmt::Discard(expr))
                } else if must_terminate {
                    self.expect_statement_end()?;
                    Ok(Stmt::Discard(expr))
                } else if (structured && !self.check(&TokenKind::RBrace))
                    || matches!(expr, Expr::ReplaceRelation(_) | Expr::Preserve(_))
                {
                    Ok(Stmt::Discard(expr))
                } else if self.check(&TokenKind::RBrace) {
                    Ok(Stmt::Expr(expr))
                } else {
                    self.expect_statement_end()?;
                    Ok(Stmt::Discard(expr))
                }
            }
            _ => Ok(stmt),
        }
    }
}
