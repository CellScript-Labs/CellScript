//! Structured lifecycle and audit clauses shared by action declarations.

use super::Parser;
use crate::ast::{
    ActionDef, ActionOutput, DestructionPolicy, Expr, IdentityPolicy, NextAudit, NextAuditEvidence, NextBodyItem, NextDisposition,
    NextEntrySurface, NextFreshOutput, NextPool, NextPoolField, NextPoolFieldTreatment, NextRetirement, ParamSource,
    ReplaceLockTreatment, Stmt, Type,
};
use crate::error::{CompileError, Result, SourceDiagnosticCode};
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
    pub(super) fn finish_lock(self) -> (Vec<Stmt>, crate::ast::NextLockSurface) {
        (self.body, crate::ast::NextLockSurface { audits: self.audits, body_items: self.body_items })
    }
    pub(super) fn push_statement(&mut self, statement: Stmt) {
        let index = self.body.len();
        self.add_replacements(index, &statement);
        self.body.push(statement);
        self.body_items.push(NextBodyItem::Statement(index));
    }

    fn add_replacements(&mut self, index: usize, statement: &Stmt) {
        self.dispositions.extend(NextDisposition::for_statement(index, statement));
    }

    pub(super) fn finish(self) -> (Vec<Stmt>, NextEntrySurface) {
        let surface = NextEntrySurface { audits: self.audits, dispositions: self.dispositions, body_items: self.body_items };
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

    pub(super) fn parse_action_policy(&mut self, body: &mut ParsedActionBody) -> Result<bool> {
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
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn parse_lifecycle_statement(&mut self) -> Result<Stmt> {
        let outputs = self
            .entry_outputs
            .clone()
            .ok_or_else(|| CompileError::new("lifecycle policies require an action entry with declared roles", self.current().span))?;
        let policy = match self.action_policy_name_ahead() {
            Some("pool") => crate::ast::LifecycleStmt::Pool(self.parse_pool(&outputs)?),
            Some("retire") => crate::ast::LifecycleStmt::Retire(self.parse_retirement()?),
            Some("fresh") => crate::ast::LifecycleStmt::Fresh(self.parse_fresh_output(&outputs)?),
            _ => return Err(CompileError::new("expected a lifecycle statement", self.current().span)),
        };
        Ok(Stmt::Lifecycle(Box::new(policy)))
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
        let (Some(data_fields), Some(lock)) = (data, lock) else {
            return Err(CompileError::new(
                "fresh requires data and lock clauses; identity may derive from its Cell declaration",
                start,
            ));
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

pub(crate) fn validate_action_surface(action: &ActionDef) -> Result<()> {
    let Some(surface) = &action.next_surface else { return Ok(()) };
    validate_action_dispositions(action, &surface.dispositions)
}

fn validate_action_dispositions(action: &ActionDef, dispositions: &[NextDisposition]) -> Result<()> {
    let inputs = action
        .params
        .iter()
        .filter(|param| matches!(param.source, ParamSource::Input | ParamSource::Default) && !param.is_read_ref)
        .map(|param| (param.name.as_str(), &param.ty))
        .collect::<BTreeMap<_, _>>();
    let outputs = action.outputs.iter().map(|output| (output.name.as_str(), &output.ty)).collect::<BTreeMap<_, _>>();
    let mut pool_names = BTreeSet::new();
    for disposition in dispositions {
        match disposition
            .resolve(&action.body)
            .ok_or_else(|| CompileError::new("lifecycle source must reference its shared body occurrence", action.span))?
        {
            crate::ast::ResolvedDisposition::Replace(relation) => {
                if !inputs.contains_key(relation.before.as_str()) || !outputs.contains_key(relation.after.as_str()) {
                    return Err(CompileError::new("replace must bind declared input and output roles", relation.span));
                }
                // Schema compatibility, named fields and completeness belong
                // to resolved typing, including imported and generic schemas.
            }
            crate::ast::ResolvedDisposition::Pool(pool) => {
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
                if !pool.data_fields.iter().any(|field| matches!(field.treatment, NextPoolFieldTreatment::Conserve)) {
                    return Err(CompileError::new(
                        "pool must declare at least one numeric field as '= conserve' so pooled accounting is checked on chain",
                        pool.span,
                    ));
                }
                crate::ast::validate_pool_assignments(pool)?;
                if pool.output_locks.iter().map(|(output, _)| output).ne(&pool.outputs) {
                    return Err(CompileError::new(
                        format!("pool lock must assign every pool output exactly once in declared order: {}", pool.outputs.join(", ")),
                        pool.span,
                    ));
                }
            }
            crate::ast::ResolvedDisposition::Retire(retirement) => {
                if !matches!(inputs.get(retirement.input.as_str()), Some(Type::Named(_))) {
                    return Err(CompileError::new(
                        format!("retire '{}' must bind a declared Cell input role", retirement.input),
                        retirement.span,
                    ));
                }
            }
            crate::ast::ResolvedDisposition::Fresh(fresh) => {
                if !matches!(outputs.get(fresh.output.as_str()), Some(Type::Named(schema)) if *schema == fresh.ty) {
                    return Err(CompileError::new(
                        format!("fresh '{}' must bind a declared output role of schema '{}'", fresh.output, fresh.ty),
                        fresh.span,
                    ));
                }
            }
        }
    }
    Ok(())
}
