use super::{aggregate_scope_from_targets, Parser};
use crate::ast::{
    AggregateInvariant, AggregateInvariantKind, AggregateRelation, AggregateTarget, AssertExpr, BoundedQuantifier,
    BoundedQuantifierKind, Expr, InvariantDef, SourceView,
};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_invariant(&mut self) -> Result<InvariantDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Invariant)?;
        let name = self.parse_name()?;
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut trigger = None;
        let mut scope = None;
        let mut reads = Vec::new();
        let mut aggregates = Vec::new();
        let mut quantifiers = Vec::new();
        let mut asserts = Vec::new();

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            self.skip_newlines();
            if self.check(&TokenKind::RBrace) {
                break;
            }

            if self.check(&TokenKind::Assert) {
                if self.current().text == "assert_invariant" {
                    asserts.push(self.parse_invariant_call_assert()?);
                } else {
                    asserts.push(self.parse_invariant_assert()?);
                }
                self.record_end(SyntaxKind::ClauseEnd);
                self.expect_statement_end()?;
                self.skip_newlines();
                continue;
            }

            if let Some(name) = self.ident_like_name() {
                if name == "forall" {
                    quantifiers.push(self.parse_bounded_forall()?);
                    self.record_end(SyntaxKind::ClauseEnd);
                    self.skip_newlines();
                    continue;
                }
                if name == "count" {
                    quantifiers.push(self.parse_bounded_count()?);
                    self.record_end(SyntaxKind::ClauseEnd);
                    self.expect_statement_end()?;
                    self.skip_newlines();
                    continue;
                }
                if Self::is_invariant_aggregate_start(&name) {
                    aggregates.push(self.parse_aggregate_invariant()?);
                    self.record_end(SyntaxKind::ClauseEnd);
                    self.expect_statement_end()?;
                    self.skip_newlines();
                    continue;
                }
            }

            let key_span = self.current().span;
            let key = self.ident_like_name().ok_or_else(|| {
                CompileError::new("expected invariant field, aggregate assertion, or assert_invariant", self.current().span)
            })?;
            self.advance();
            self.expect(TokenKind::Colon)?;

            match key.as_str() {
                "trigger" => {
                    if trigger.is_some() {
                        return Err(CompileError::new("duplicate invariant trigger declaration", key_span));
                    }
                    trigger = Some(self.parse_name_path()?);
                }
                "scope" => {
                    if scope.is_some() {
                        return Err(CompileError::new("duplicate invariant scope declaration", key_span));
                    }
                    scope = Some(self.parse_name_path()?);
                }
                "reads" => {
                    reads.extend(self.parse_invariant_reads()?);
                }
                _ => {
                    return Err(CompileError::new(
                        format!("unknown invariant field '{}'; expected trigger, scope, reads, or assert_invariant", key),
                        key_span,
                    ));
                }
            }

            self.record_end(SyntaxKind::ClauseEnd);
            self.expect_statement_end()?;
            self.skip_newlines();
        }

        let end_span = self.current().span;
        self.expect(TokenKind::RBrace)?;
        // Explicit views retain their own scope for type checking. Bare
        // targets inherit the enclosing declaration, even if it comes later.
        for aggregate in &mut aggregates {
            let inherited = scope.as_ref().ok_or_else(|| {
                CompileError::new("aggregate assertion requires a `scope` declaration in its enclosing invariant", aggregate.span)
            })?;
            if aggregate.scope.is_empty() {
                aggregate.scope.clone_from(inherited);
            }
        }
        Ok(InvariantDef {
            name,
            trigger,
            scope,
            reads,
            aggregates,
            quantifiers,
            asserts,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_bounded_forall(&mut self) -> Result<BoundedQuantifier> {
        let start_span = self.current().span;
        let keyword = self.parse_name()?;
        debug_assert_eq!(keyword, "forall");
        let binding_span = self.current().span;
        let binding = self.parse_name().map_err(|_| {
            CompileError::new(
                "forall requires a binding and finite source view: `forall token in outputs<Token> { ... }`",
                binding_span,
            )
        })?;
        if !self.check(&TokenKind::In) {
            return Err(CompileError::new(
                "unbounded forall is not supported; expected `in` followed by a finite source view",
                self.current().span,
            ));
        }
        self.advance();
        let range = self.parse_invariant_target()?;
        let role = range.source.quantifier_role();
        if range.field.is_some() {
            return Err(CompileError::new("forall range must name a source view and cell type, not a projected field", start_span));
        }
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut predicates = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if !self.check(&TokenKind::Require) {
                return Err(CompileError::new(
                    "forall body only accepts pure `require` predicates; lifecycle and control-flow statements are not allowed",
                    self.current().span,
                ));
            }
            predicates.push(self.parse_require()?);
            self.record_end(SyntaxKind::ClauseEnd);
            self.expect_statement_end()?;
            self.skip_newlines();
        }
        if predicates.is_empty() {
            return Err(CompileError::new("forall body must contain at least one require predicate", start_span));
        }
        let end_span = self.current().span;
        self.expect(TokenKind::RBrace)?;
        Ok(BoundedQuantifier {
            kind: BoundedQuantifierKind::ForAll,
            role: role.map(str::to_string),
            binding: Some(binding),
            range,
            predicates,
            relation: None,
            expected: None,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_bounded_count(&mut self) -> Result<BoundedQuantifier> {
        let start_span = self.current().span;
        let keyword = self.parse_name()?;
        debug_assert_eq!(keyword, "count");
        self.expect(TokenKind::LParen)?;
        let range = self.parse_invariant_target()?;
        if range.field.is_some() {
            return Err(CompileError::new("count range must name a source view and cell type, not a projected field", start_span));
        }
        let where_span = self.current().span;
        let where_name = self
            .ident_like_name()
            .ok_or_else(|| CompileError::new("count requires `where <predicate>` after its finite source view", where_span))?;
        if where_name != "where" {
            return Err(CompileError::new("count requires `where <predicate>` after its finite source view", where_span));
        }
        self.advance();
        let predicate = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let relation = match self.current().kind {
            TokenKind::Lt => AggregateRelation::Lt,
            TokenKind::Le => AggregateRelation::Le,
            TokenKind::EqEq => AggregateRelation::Eq,
            TokenKind::Ge => AggregateRelation::Ge,
            TokenKind::Gt => AggregateRelation::Gt,
            _ => return Err(CompileError::new("count requires a comparison against a u64 expression", self.current().span)),
        };
        self.advance();
        let expected = self.parse_expr()?;
        let end_span = self.current().span;
        Ok(BoundedQuantifier {
            kind: BoundedQuantifierKind::Count,
            role: None,
            binding: None,
            range,
            predicates: vec![predicate],
            relation: Some(relation),
            expected: Some(expected),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn is_invariant_aggregate_start(name: &str) -> bool {
        matches!(name, "assert_sum" | "assert_conserved" | "assert_delta" | "assert_distinct" | "assert_singleton")
    }

    fn parse_aggregate_invariant(&mut self) -> Result<AggregateInvariant> {
        let start_span = self.current().span;
        let name = self.parse_name()?;
        if name == "assert_sum" {
            return self.parse_aggregate_sum_comparison(start_span);
        }

        self.expect(TokenKind::LParen)?;
        self.skip_newlines();
        let (target, view_scope) = self.parse_invariant_target_with_scope()?;

        let (kind, argument) = match name.as_str() {
            "assert_conserved" => (AggregateInvariantKind::Conserved, None),
            "assert_distinct" => (AggregateInvariantKind::Distinct, None),
            "assert_singleton" => (AggregateInvariantKind::Singleton, None),
            "assert_delta" => {
                self.skip_newlines();
                self.expect(TokenKind::Comma)?;
                self.skip_newlines();
                (AggregateInvariantKind::Delta, Some(self.parse_aggregate_argument()?))
            }
            _ => {
                return Err(CompileError::new(format!("unknown aggregate invariant primitive '{}'", name), start_span));
            }
        };

        self.skip_newlines();
        let end_span = self.current().span;
        self.expect(TokenKind::RParen)?;
        Ok(AggregateInvariant {
            kind,
            target,
            scope: view_scope.unwrap_or_default().to_string(),
            argument,
            relation: None,
            rhs: None,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_aggregate_sum_comparison(&mut self, start_span: Span) -> Result<AggregateInvariant> {
        let left = self.parse_aggregate_sum_operand()?;
        self.skip_newlines();
        let relation = match self.current().kind {
            TokenKind::Lt => AggregateRelation::Lt,
            TokenKind::Le => AggregateRelation::Le,
            TokenKind::EqEq => AggregateRelation::Eq,
            TokenKind::Ge => AggregateRelation::Ge,
            TokenKind::Gt => AggregateRelation::Gt,
            _ => {
                return Err(CompileError::new("assert_sum aggregate assertion requires a comparison operator", self.current().span));
            }
        };
        self.advance();
        self.skip_newlines();

        let rhs_start = self.current().span;
        let rhs_name = self.parse_name()?;
        if rhs_name != "assert_sum" {
            return Err(CompileError::new("assert_sum comparison right-hand side must be assert_sum(...)", rhs_start));
        }
        let right = self.parse_aggregate_sum_operand()?;
        let scope = aggregate_scope_from_targets(&left, Some(&right)).ok_or_else(|| {
            CompileError::new("assert_sum aggregate assertion requires explicit input/output source views", start_span)
        })?;
        let end_span = self.current().span;

        Ok(AggregateInvariant {
            kind: AggregateInvariantKind::Sum,
            target: left,
            scope,
            argument: None,
            relation: Some(relation),
            rhs: Some(right),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_aggregate_sum_operand(&mut self) -> Result<AggregateTarget> {
        self.expect(TokenKind::LParen)?;
        self.skip_newlines();
        let target = self.parse_invariant_target()?;
        self.skip_newlines();
        self.expect(TokenKind::RParen)?;
        Ok(target)
    }

    fn parse_aggregate_argument(&mut self) -> Result<String> {
        match &self.current().kind {
            TokenKind::Integer(value) => {
                let rendered = value.to_string();
                self.advance();
                Ok(rendered)
            }
            TokenKind::String(value) => {
                let rendered = format!("{:?}", value);
                self.advance();
                Ok(rendered)
            }
            _ => self.parse_invariant_target().map(|target| target.to_string()),
        }
    }

    fn parse_invariant_assert(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Assert)?;
        let condition = self.parse_expr()?;
        let end_span = self.current().span;
        Ok(Expr::Assert(AssertExpr {
            condition: Box::new(condition),
            message: Box::new(Expr::String("invariant assertion failed".to_string())),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_invariant_call_assert(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Assert)?;
        self.expect(TokenKind::LParen)?;
        self.skip_newlines();
        let condition = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        self.skip_newlines();
        let message = self.parse_expr()?;
        self.skip_newlines();
        let end_span = self.current().span;
        self.expect(TokenKind::RParen)?;
        Ok(Expr::Assert(AssertExpr {
            condition: Box::new(condition),
            message: Box::new(message),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_invariant_reads(&mut self) -> Result<Vec<AggregateTarget>> {
        let mut reads = Vec::new();
        self.skip_newlines();
        if self.check(&TokenKind::RBrace) || self.check(&TokenKind::Semi) {
            return Ok(reads);
        }

        loop {
            reads.push(self.parse_invariant_target()?);
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
                if self.check(&TokenKind::RBrace) || self.check(&TokenKind::Semi) {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(reads)
    }

    fn parse_invariant_target(&mut self) -> Result<AggregateTarget> {
        self.parse_invariant_target_with_scope().map(|(target, _)| target)
    }

    fn parse_invariant_target_with_scope(&mut self) -> Result<(AggregateTarget, Option<&'static str>)> {
        let start_span = self.current().span;
        let root = self.parse_name_path()?;
        let mut type_name = None;
        let mut type_args = Vec::new();
        if self.check(&TokenKind::Lt) {
            let arguments_start = self.current().span;
            self.advance();
            let ty = self.parse_type()?;
            self.expect(TokenKind::Gt)?;
            self.record_syntax_node(SyntaxKind::GenericArguments, arguments_start);
            let (name, arguments) = Self::nominal_designation(ty, start_span)?;
            type_name = Some(name);
            type_args = arguments;
        }

        let mut fields = Vec::new();
        while self.check(&TokenKind::Dot) {
            self.advance();
            fields.push(self.parse_name()?);
        }

        let field = (!fields.is_empty()).then(|| fields.join("."));
        let explicit_source = SourceView::from_source_name(&root);
        let source = if let Some(source) = explicit_source {
            source
        } else {
            if type_name.is_some() {
                return Err(CompileError::new(
                    format!("unknown invariant source view '{}'; generic source views use the closed CKB view registry", root),
                    start_span,
                ));
            }
            type_name = Some(root);
            SourceView::SelectedCells
        };

        if source == SourceView::TypeIdentity && (type_name.is_some() || field.is_some()) {
            return Err(CompileError::new("type_id aggregate target cannot have a type argument or field projection", start_span));
        }

        let view_scope = explicit_source.and_then(|source| match source {
            SourceView::SelectedCells => Some("selected_cells"),
            SourceView::CellDep | SourceView::HeaderDep => Some("transaction"),
            _ => source.aggregate_scope(),
        });
        Ok((AggregateTarget { source, type_name, type_args, field }, view_scope))
    }
}
