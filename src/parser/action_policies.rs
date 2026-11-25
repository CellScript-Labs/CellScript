//! Structured lifecycle and audit clauses shared by action declarations.

use super::Parser;
use crate::ast::{
    ActionDef, ActionOutput, BinaryExpr, BinaryOp, CastExpr, CreateExpr, CreateUniqueExpr, DestroyExpr, DestructionPolicy, Expr,
    FieldAccessExpr, IdentityPolicy, Item, Module, NextAudit, NextAuditEvidence, NextBodyItem, NextDisposition, NextEntrySurface,
    NextFreshOutput, NextPool, NextPoolField, NextPoolFieldTreatment, NextReplacement, NextRetirement, ParamSource,
    ReplaceDataTreatment, ReplaceFieldTreatment, ReplaceLockTreatment, RequireExpr, Stmt, Type,
};
use crate::error::{CompileError, Result, SourceDiagnosticCode, Span};
use crate::lexer::token::{Token, TokenKind};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct ParsedActionBody {
    pub(super) body: Vec<Stmt>,
    audits: Vec<NextAudit>,
    dispositions: Vec<NextDisposition>,
    body_items: Vec<NextBodyItem>,
}

impl ParsedActionBody {
    pub(super) fn push_statement(&mut self, statement: Stmt) {
        let index = self.body.len();
        self.add_replacements(index, &statement);
        self.body.push(statement);
        self.body_items.push(NextBodyItem::Statement(index));
    }

    fn add_replacements(&mut self, index: usize, statement: &Stmt) {
        self.dispositions.extend(
            (0..crate::ast::statement_replace_relations(statement).len())
                .map(|relation_index| NextDisposition::Replace(NextReplacement { statement_index: index, relation_index })),
        );
    }

    fn push_disposition(&mut self, disposition: NextDisposition, statements: Vec<Stmt>) {
        let start_statement = self.body.len();
        let statement_count = statements.len();
        for (index, statement) in statements.into_iter().enumerate() {
            self.add_replacements(start_statement + index, &statement);
            self.body.push(statement);
        }
        self.body_items.push(NextBodyItem::Disposition { index: self.dispositions.len(), start_statement, statement_count });
        self.dispositions.push(disposition);
    }

    pub(super) fn finish(self, inline_outputs: bool) -> (Vec<Stmt>, Option<NextEntrySurface>) {
        let structured = inline_outputs
            || !self.audits.is_empty()
            || self.dispositions.iter().any(|item| !matches!(item, NextDisposition::Replace(_)));
        let surface =
            structured.then(|| NextEntrySurface { audits: self.audits, dispositions: self.dispositions, body_items: self.body_items });
        (self.body, surface)
    }
}

impl Parser<'_> {
    pub(super) fn action_policy_name_ahead(&self) -> Option<&str> {
        let TokenKind::Identifier(word) = &self.current().kind else { return None };
        if !matches!(word.as_str(), "pool" | "retire" | "fresh" | "audit") {
            return None;
        }
        let mut next = self.tokens[self.position + 1..].iter().filter(|token| !matches!(token.kind, TokenKind::Newline));
        matches!(
            (next.next().map(|token| &token.kind), next.next().map(|token| &token.kind)),
            (Some(TokenKind::Identifier(_)), Some(TokenKind::LBrace))
        )
        .then_some(word.as_str())
    }

    pub(super) fn parse_action_policy(&mut self, outputs: &[ActionOutput], body: &mut ParsedActionBody) -> Result<bool> {
        let mut next = self.tokens[self.position + 1..].iter().filter(|token| !matches!(token.kind, TokenKind::Newline));
        let Some(name) = next.next() else { return Ok(false) };
        if self.current().text == "effects" && matches!(name.kind, TokenKind::LBrace) {
            return Err(SourceDiagnosticCode::RemovedNativeEffectsBlock
                .error("write replace, pool, retire, or fresh directly in the action body", self.current().span));
        }
        let Some(word) = self.action_policy_name_ahead() else { return Ok(false) };
        match word {
            "audit" => {
                let audit = self.parse_action_audit()?;
                if body.audits.iter().any(|previous| previous.name == audit.name) {
                    return Err(CompileError::new(format!("duplicate audit declaration '{}'", audit.name), audit.span));
                }
                body.body_items.push(NextBodyItem::Audit(body.audits.len()));
                body.audits.push(audit);
            }
            "pool" => {
                let pool = self.parse_pool(outputs)?;
                let statements = lower_pool_body(&pool)?;
                body.push_disposition(NextDisposition::Pool(pool), statements);
            }
            "retire" => {
                let retirement = self.parse_retirement()?;
                let statement = Stmt::Discard(Expr::Destroy(DestroyExpr {
                    expr: Box::new(Expr::Identifier(retirement.input.clone())),
                    policy: retirement.absence_policy.clone(),
                    span: retirement.span,
                }));
                body.push_disposition(NextDisposition::Retire(retirement), vec![statement]);
            }
            "fresh" => {
                let fresh = self.parse_fresh_output(outputs)?;
                let lock = exact_lock_expression(&fresh.lock).cloned().map(Box::new);
                let expression = if fresh.identity == IdentityPolicy::None {
                    Expr::Create(CreateExpr {
                        target: Some(fresh.output.clone()),
                        ty: fresh.ty.clone(),
                        fields: fresh.data_fields.clone(),
                        lock,
                        span: fresh.span,
                    })
                } else {
                    Expr::CreateUnique(CreateUniqueExpr {
                        target: Some(fresh.output.clone()),
                        ty: fresh.ty.clone(),
                        type_args: Vec::new(),
                        fields: fresh.data_fields.clone(),
                        lock,
                        identity: fresh.identity.clone(),
                        span: fresh.span,
                    })
                };
                body.push_disposition(NextDisposition::Fresh(fresh), vec![Stmt::Discard(expression)]);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_action_audit(&mut self) -> Result<NextAudit> {
        let start = self.expect_policy_word("audit")?.span;
        let name = self.parse_policy_name("audit declaration name")?;
        self.expect(TokenKind::LBrace)?;
        self.expect_policy_word("expected_evidence")?;
        self.expect(TokenKind::Eq)?;
        self.expect_policy_word("external_policy")?;
        let subject = self.parse_parenthesized_expression()?;
        self.expect_statement_end()?;
        let end = self.expect(TokenKind::RBrace)?.span;
        Ok(NextAudit { name, evidence: NextAuditEvidence::ExternalPolicy, subject, span: start.combine(&end) })
    }

    fn parse_policy_name(&mut self, context: &str) -> Result<String> {
        self.skip_newlines();
        match &self.current().kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            kind => Err(CompileError::new(format!("expected {context}, found {kind}"), self.current().span)),
        }
    }

    fn expect_policy_word(&mut self, word: &str) -> Result<&Token> {
        self.skip_newlines();
        if self.current().text == word {
            Ok(self.advance())
        } else {
            Err(CompileError::new(format!("expected '{word}', found {}", self.current().kind), self.current().span))
        }
    }

    fn parse_parenthesized_expression(&mut self) -> Result<Expr> {
        self.expect(TokenKind::LParen)?;
        let expression = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        Ok(expression)
    }

    fn parse_lock_target(&mut self) -> Result<ReplaceLockTreatment> {
        let spelling = self.parse_policy_name("lock target: exact(address) or exact_hash(script_hash)")?;
        let value = match spelling.as_str() {
            "exact" | "exact_hash" => self.parse_parenthesized_expression()?,
            _ => return Err(CompileError::new("lock target must be exact(address) or exact_hash(script_hash)", self.current().span)),
        };
        Ok(if spelling == "exact" {
            ReplaceLockTreatment::Exact(Box::new(value))
        } else {
            ReplaceLockTreatment::ExactHash(Box::new(value))
        })
    }

    fn parse_pool(&mut self, outputs: &[ActionOutput]) -> Result<NextPool> {
        let start = self.expect_policy_word("pool")?.span;
        let name = self.parse_policy_name("pool name")?;
        self.expect(TokenKind::LBrace)?;
        let mut inputs = None;
        let mut output_names = None;
        let mut data_fields = None;
        let mut output_locks = None;
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) {
            let clause = self.take_policy_name("pool")?;
            match clause.text.as_str() {
                "inputs" if inputs.is_none() => inputs = Some(self.parse_name_block("pool input role")?),
                "outputs" if output_names.is_none() => output_names = Some(self.parse_name_block("pool output role")?),
                "data" if data_fields.is_none() => data_fields = Some(self.parse_braced_list(Self::parse_pool_field)?),
                "lock" if output_locks.is_none() => {
                    output_locks = Some(self.parse_braced_list(|cursor| {
                        let output = cursor.parse_policy_name("pool output lock assignment")?;
                        cursor.expect(TokenKind::Eq)?;
                        Ok((output, cursor.parse_lock_target()?))
                    })?);
                }
                "inputs" | "outputs" | "data" | "lock" => {
                    return Err(CompileError::new(format!("duplicate pool clause '{}'", clause.text), clause.span));
                }
                _ => {
                    return Err(CompileError::new(
                        format!("unknown pool clause '{}'; expected inputs, outputs, data, or lock", clause.text),
                        clause.span,
                    ));
                }
            }
            self.expect_statement_end()?;
            self.skip_newlines();
        }
        let end = self.expect(TokenKind::RBrace)?.span;
        let (Some(inputs), Some(output_names), Some(data_fields), Some(output_locks)) =
            (inputs, output_names, data_fields, output_locks)
        else {
            return Err(CompileError::new("pool requires inputs, outputs, data, and lock clauses", start));
        };
        let ty = output_names
            .first()
            .and_then(|name| outputs.iter().find(|candidate| candidate.name == *name))
            .and_then(|candidate| match &candidate.ty {
                Type::Named(name) => Some(name.clone()),
                _ => None,
            })
            .ok_or_else(|| CompileError::new("pool outputs must name at least one declared Cell output role", start))?;
        Ok(NextPool { name, ty, inputs, outputs: output_names, data_fields, output_locks, span: start.combine(&end) })
    }

    fn parse_pool_field(&mut self) -> Result<NextPoolField> {
        let span = self.current().span;
        let field = self.parse_policy_name("pool data field")?;
        self.skip_newlines();
        let treatment = if self.check(&TokenKind::Eq) {
            self.advance();
            self.expect_policy_word("conserve")?;
            NextPoolFieldTreatment::Conserve
        } else {
            NextPoolFieldTreatment::Set(
                self.parse_braced_list(|cursor| cursor.parse_policy_assignment("pool output field assignment"))?,
            )
        };
        Ok(NextPoolField { span, field, treatment })
    }

    fn parse_name_block(&mut self, context: &str) -> Result<Vec<String>> {
        let names = self.parse_braced_list(|cursor| cursor.parse_policy_name(context))?;
        if names.is_empty() {
            return Err(SourceDiagnosticCode::PoolRoles.error(format!("{context} block must not be empty"), self.current().span));
        }
        Ok(names)
    }

    fn parse_braced_list<T>(&mut self, mut parse_item: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        self.expect(TokenKind::LBrace)?;
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) {
            items.push(parse_item(self)?);
            self.expect_list_separator(TokenKind::RBrace)?;
        }
        self.expect(TokenKind::RBrace)?;
        Ok(items)
    }

    fn parse_policy_assignment(&mut self, context: &str) -> Result<(String, Expr)> {
        let field = self.parse_policy_name(context)?;
        self.expect(TokenKind::Eq)?;
        Ok((field, self.parse_expr()?))
    }

    fn take_policy_name(&mut self, construct: &str) -> Result<Token> {
        let token = self.current();
        if !matches!(token.kind, TokenKind::Identifier(_) | TokenKind::Lock) {
            return Err(CompileError::new(format!("expected a {construct} clause, found {}", token.kind), token.span));
        }
        let token = token.clone();
        self.advance();
        Ok(token)
    }

    fn parse_retirement(&mut self) -> Result<NextRetirement> {
        let start = self.expect_policy_word("retire")?.span;
        let input = self.parse_policy_name("retirement input role")?;
        self.expect(TokenKind::LBrace)?;
        self.expect_policy_word("absence")?;
        self.expect(TokenKind::Eq)?;
        let absence_policy = self.parse_absence_policy()?;
        self.expect_statement_end()?;
        let end = self.expect(TokenKind::RBrace)?.span;
        Ok(NextRetirement { input, absence_policy, span: start.combine(&end) })
    }

    fn parse_fresh_output(&mut self, outputs: &[ActionOutput]) -> Result<NextFreshOutput> {
        let start = self.expect_policy_word("fresh")?.span;
        let output = self.parse_policy_name("fresh output role")?;
        let ty = outputs
            .iter()
            .find(|candidate| candidate.name == output)
            .and_then(|candidate| match &candidate.ty {
                Type::Named(name) => Some(name.clone()),
                _ => None,
            })
            .ok_or_else(|| CompileError::new(format!("fresh output '{output}' must name a declared Cell output role"), start))?;
        self.expect(TokenKind::LBrace)?;
        let mut data = None;
        let mut identity = None;
        let mut lock = None;
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) {
            let clause = self.take_policy_name("fresh")?;
            match clause.text.as_str() {
                "data" if data.is_none() => {
                    data = Some(self.parse_braced_list(|cursor| cursor.parse_policy_assignment("fresh output data field"))?);
                }
                "identity" if identity.is_none() => {
                    self.expect(TokenKind::Eq)?;
                    identity = Some(self.parse_identity_policy()?);
                }
                "lock" if lock.is_none() => {
                    self.expect(TokenKind::Eq)?;
                    lock = Some(self.parse_lock_target()?);
                }
                "data" | "identity" | "lock" => {
                    return Err(CompileError::new(format!("duplicate fresh clause '{}'", clause.text), clause.span));
                }
                _ => {
                    return Err(CompileError::new(
                        format!("unknown fresh clause '{}'; expected data, identity, or lock", clause.text),
                        clause.span,
                    ));
                }
            }
            self.expect_statement_end()?;
            self.skip_newlines();
        }
        let end = self.expect(TokenKind::RBrace)?.span;
        let (Some(data_fields), Some(identity), Some(lock)) = (data, identity, lock) else {
            return Err(CompileError::new("fresh requires data, identity, and lock clauses", start));
        };
        Ok(NextFreshOutput { output, ty, data_fields, identity, lock, span: start.combine(&end) })
    }

    fn parse_absence_policy(&mut self) -> Result<DestructionPolicy> {
        let policy = self.parse_policy_name("retirement absence policy")?;
        match policy.as_str() {
            "singleton_type" => Ok(DestructionPolicy::SingletonType),
            "ckb_type_id" => Ok(DestructionPolicy::Unique { identity: "ckb_type_id".to_string() }),
            "field" => {
                self.expect(TokenKind::LParen)?;
                let field = self.parse_policy_name("retirement identity field")?;
                self.expect(TokenKind::RParen)?;
                Ok(DestructionPolicy::Instance { identity_field: field })
            }
            _ => Err(CompileError::new(
                "retire absence policy must be singleton_type, ckb_type_id, or field(name)",
                self.current().span,
            )),
        }
    }

    fn parse_identity_policy(&mut self) -> Result<IdentityPolicy> {
        let policy = self.parse_policy_name("fresh output identity policy")?;
        match policy.as_str() {
            "none" => Ok(IdentityPolicy::None),
            "ckb_type_id" => Ok(IdentityPolicy::CkbTypeId),
            "script_args" => Ok(IdentityPolicy::ScriptArgs),
            "singleton_type" => Ok(IdentityPolicy::SingletonType),
            "field" => {
                self.expect(TokenKind::LParen)?;
                let field = self.parse_policy_name("fresh output identity field")?;
                self.expect(TokenKind::RParen)?;
                Ok(IdentityPolicy::Field(field))
            }
            _ => Err(CompileError::new(
                "fresh identity policy must be none, ckb_type_id, field(name), script_args, or singleton_type",
                self.current().span,
            )),
        }
    }
}

pub(crate) fn validate_action_surface(module: &Module, action: &ActionDef) -> Result<()> {
    let Some(surface) = &action.next_surface else { return Ok(()) };
    validate_action_dispositions(module, action, &surface.dispositions)
}

fn exact_lock_expression(treatment: &ReplaceLockTreatment) -> Option<&Expr> {
    match treatment {
        ReplaceLockTreatment::Exact(value) | ReplaceLockTreatment::ExactHash(value) => Some(value),
        ReplaceLockTreatment::Same => None,
    }
}

fn validate_pool_assignments(pool: &NextPool) -> Result<()> {
    if pool.inputs.is_empty() || pool.outputs.is_empty() {
        return Err(SourceDiagnosticCode::PoolRoles.error("pool requires nonempty input and output roles", pool.span));
    }
    if pool.inputs.iter().collect::<BTreeSet<_>>().len() != pool.inputs.len()
        || pool.outputs.iter().collect::<BTreeSet<_>>().len() != pool.outputs.len()
    {
        return Err(SourceDiagnosticCode::PoolRoles.error("pool input and output roles must each be distinct", pool.span));
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
            body.push(Stmt::Discard(Expr::Require(RequireExpr {
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
        let lock = pool
            .output_locks
            .iter()
            .find(|(candidate, _)| candidate == output)
            .and_then(|(_, treatment)| exact_lock_expression(treatment))
            .cloned()
            .map(Box::new);
        body.push(Stmt::Discard(Expr::Create(CreateExpr {
            target: Some(output.clone()),
            ty: pool.ty.clone(),
            fields,
            lock,
            span: pool.span,
        })));
    }
    body.extend(pool.inputs.iter().map(|input| {
        Stmt::Discard(Expr::Consume(crate::ast::ConsumeExpr { expr: Box::new(Expr::Identifier(input.clone())), span: pool.span }))
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

fn validate_action_dispositions(module: &Module, action: &ActionDef, dispositions: &[NextDisposition]) -> Result<()> {
    let inputs = action
        .params
        .iter()
        .filter(|param| param.source == ParamSource::Input)
        .map(|param| (param.name.as_str(), &param.ty))
        .collect::<BTreeMap<_, _>>();
    let outputs = action.outputs.iter().map(|output| (output.name.as_str(), &output.ty)).collect::<BTreeMap<_, _>>();
    let declared_fields = |schema: &str| {
        module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Resource(definition) if definition.name == schema => Some(&definition.fields),
                Item::Shared(definition) if definition.name == schema => Some(&definition.fields),
                Item::Receipt(definition) if definition.name == schema => Some(&definition.fields),
                _ => None,
            })
            .map(|fields| fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>())
    };
    let mut pool_names = BTreeSet::new();
    for disposition in dispositions {
        match disposition {
            NextDisposition::Replace(replacement) => {
                let relation = replacement
                    .relation(&action.body)
                    .ok_or_else(|| CompileError::new("replacement source must reference its shared body relation", action.span))?;
                let before = inputs.get(relation.before.as_str());
                if !matches!(before, Some(Type::Named(_))) || before != outputs.get(relation.after.as_str()) {
                    return Err(CompileError::new(
                        "replace must bind declared input and output roles of one Cell schema",
                        relation.span,
                    ));
                }
                if let Some(Type::Named(schema)) = before
                    && let Some(fields) = declared_fields(schema)
                {
                    validate_replacement_fields(relation, &fields)?;
                }
            }
            NextDisposition::Pool(pool) => {
                if !pool_names.insert(pool.name.as_str()) {
                    return Err(CompileError::new(format!("duplicate pool disposition '{}'", pool.name), pool.span));
                }
                let schema = Type::Named(pool.ty.clone());
                if pool.inputs.iter().any(|input| inputs.get(input.as_str()).copied() != Some(&schema))
                    || pool.outputs.iter().any(|output| outputs.get(output.as_str()).copied() != Some(&schema))
                {
                    return Err(CompileError::new(
                        format!("pool '{}' must bind declared input and output roles of schema '{}'", pool.name, pool.ty),
                        pool.span,
                    ));
                }
                if let Some(fields) = declared_fields(&pool.ty)
                    && pool.data_fields.iter().map(|field| &field.field).ne(&fields)
                {
                    return Err(CompileError::new(
                        format!("pool data must exhaustively list fields in schema order: {}", fields.join(", ")),
                        pool.span,
                    ));
                }
                if !pool.data_fields.iter().any(|field| matches!(field.treatment, NextPoolFieldTreatment::Conserve)) {
                    return Err(CompileError::new(
                        "pool must declare at least one numeric field as '= conserve' so pooled accounting is checked on chain",
                        pool.span,
                    ));
                }
                validate_pool_assignments(pool)?;
                if pool.output_locks.iter().map(|(output, _)| output).ne(&pool.outputs) {
                    return Err(CompileError::new(
                        format!("pool lock must assign every pool output exactly once in declared order: {}", pool.outputs.join(", ")),
                        pool.span,
                    ));
                }
            }
            NextDisposition::Retire(retirement) => {
                if !matches!(inputs.get(retirement.input.as_str()), Some(Type::Named(_))) {
                    return Err(CompileError::new(
                        format!("retire '{}' must bind a declared Cell input role", retirement.input),
                        retirement.span,
                    ));
                }
            }
            NextDisposition::Fresh(fresh) => {
                if !matches!(outputs.get(fresh.output.as_str()), Some(Type::Named(schema)) if *schema == fresh.ty) {
                    return Err(CompileError::new(
                        format!("fresh '{}' must bind a declared output role of schema '{}'", fresh.output, fresh.ty),
                        fresh.span,
                    ));
                }
                if let Some(fields) = declared_fields(&fresh.ty)
                    && fresh.data_fields.iter().map(|(field, _)| field).ne(&fields)
                {
                    return Err(CompileError::new(
                        format!("fresh data must exhaustively list fields in schema order: {}", fields.join(", ")),
                        fresh.span,
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_replacement_fields(relation: &crate::ast::ReplaceRelation, declared: &[String]) -> Result<()> {
    let fields = match &relation.data {
        ReplaceDataTreatment::Fields(fields) => fields
            .iter()
            .map(|field| match field {
                ReplaceFieldTreatment::Same(name) | ReplaceFieldTreatment::Assign(name, _) => name,
            })
            .collect::<Vec<_>>(),
        ReplaceDataTreatment::SameExcept(fields) => fields.iter().map(|(name, _)| name).collect(),
    };
    let mut seen = BTreeSet::new();
    for field in &fields {
        if !declared.contains(field) || !seen.insert(*field) {
            return Err(CompileError::new(format!("replace data contains an unknown or duplicate field '{field}'"), relation.span));
        }
    }
    if matches!(relation.data, ReplaceDataTreatment::Fields(_)) && seen.len() != declared.len() {
        return Err(CompileError::new(format!("replace data must exhaustively list fields: {}", declared.join(", ")), relation.span));
    }
    Ok(())
}
