use super::{Parser, MAX_PARSE_RECURSION_DEPTH};
use crate::ast::{BorrowStmt, ForStmt, IfStmt, LetStmt, LoopControlStmt, ReturnStmt, Stmt, WhileStmt};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_let(&mut self) -> Result<LetStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::Let)?;

        let is_mut = if self.check(&TokenKind::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let pattern = self.parse_binding_pattern()?;

        let ty = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;

        let end_span = self.current().span;
        Ok(LetStmt { pattern, ty, value, is_mut, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    pub(super) fn parse_return(&mut self) -> Result<ReturnStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::Return)?;

        self.skip_newlines();
        let value = if self.check(&TokenKind::Semi) { None } else { Some(self.parse_expr()?) };
        let end_span = self.previous_non_newline().span;
        Ok(ReturnStmt { value, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    pub(super) fn parse_if(&mut self) -> Result<IfStmt> {
        if self.if_depth >= MAX_PARSE_RECURSION_DEPTH {
            return Err(CompileError::new(
                format!("if nesting exceeds the parser limit of {}", MAX_PARSE_RECURSION_DEPTH),
                self.current().span,
            ));
        }
        self.if_depth += 1;
        let result = self.parse_if_inner();
        self.if_depth -= 1;
        result
    }

    fn parse_if_inner(&mut self) -> Result<IfStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::If)?;

        let condition = self.parse_expr()?;
        let then_branch = self.parse_block()?;
        self.skip_newlines();

        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                // else if
                let inner_if = self.parse_if()?;
                Some(vec![Stmt::If(inner_if)])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        let end_span = self.current().span;
        Ok(IfStmt {
            condition,
            then_branch,
            else_branch,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    pub(super) fn parse_for(&mut self) -> Result<ForStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::For)?;

        let pattern = self.parse_binding_pattern()?;

        self.expect(TokenKind::In)?;
        let iterable = self.parse_expr()?;
        let body_start = self.current().span;
        let body = self.parse_block()?;
        self.record_syntax_node(SyntaxKind::LoopBody, body_start);

        let end_span = self.current().span;
        Ok(ForStmt {
            label: None,
            pattern,
            iterable,
            body,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    pub(super) fn parse_while(&mut self) -> Result<WhileStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::While)?;

        let condition = self.parse_expr()?;
        let body_start = self.current().span;
        let body = self.parse_block()?;
        self.record_syntax_node(SyntaxKind::LoopBody, body_start);

        let end_span = self.current().span;
        Ok(WhileStmt {
            label: None,
            condition,
            body,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    pub(super) fn parse_loop_control(&mut self, keyword: TokenKind) -> Result<LoopControlStmt> {
        let start = self.current().span;
        self.expect(keyword)?;
        let label = self.ident_like_name().map(|_| self.parse_name()).transpose()?;
        let end = self.current().span;
        Ok(LoopControlStmt { label, span: Span::new(start.start, end.start, start.line, start.column) })
    }

    pub(super) fn parse_labeled_loop(&mut self) -> Result<Stmt> {
        self.expect(TokenKind::Label)?;
        let label = self.parse_name()?;
        self.expect(TokenKind::Colon)?;
        match self.current().kind {
            TokenKind::For => {
                let mut statement = self.parse_for()?;
                statement.label = Some(label);
                Ok(Stmt::For(statement))
            }
            TokenKind::While => {
                let mut statement = self.parse_while()?;
                statement.label = Some(label);
                Ok(Stmt::While(statement))
            }
            _ => Err(CompileError::new("label must precede a for or while loop", self.current().span)),
        }
    }

    pub(super) fn parse_borrow(&mut self) -> Result<BorrowStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::Borrow)?;
        let root = self.parse_name()?;
        let mut path = Vec::new();
        while self.check(&TokenKind::Dot) {
            self.advance();
            path.push(self.parse_name()?);
        }
        let as_span = self.current().span;
        let keyword = self.parse_name()?;
        if keyword != "as" {
            return Err(CompileError::new("expected 'as' in borrow block", as_span));
        }
        let binding = self.parse_name()?;
        let body = self.parse_block()?;
        let end_span = self.previous_non_newline().span;
        Ok(BorrowStmt {
            root,
            path,
            binding,
            body,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }
}
