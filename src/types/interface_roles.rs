//! Interface designations have a source lexical role before specialization.
//! Materialized argument identities are checked separately against the closure.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenHandleRoleContext<'scope> {
    Source(&'scope HashSet<String>),
    Retained,
}

pub(crate) fn validate_source_open_handle_roles(module: &Module, resolver: &ModuleResolver) -> Result<()> {
    TypeChecker::with_resolver(resolver, &module.name).validate_source_interface_roles(module)
}

impl TypeChecker<'_> {
    pub(super) fn validate_source_interface_roles(&self, module: &Module) -> Result<()> {
        let visitor = SourceRoles { checker: self, type_parameters: HashSet::new() };
        for item in module.items.iter().chain(&module.interface_templates) {
            if !item.is_specialization() {
                visitor.item(item)?;
            }
        }
        Ok(())
    }
}

struct SourceRoles<'a, 'resolver> {
    checker: &'a TypeChecker<'resolver>,
    type_parameters: HashSet<String>,
}

impl SourceRoles<'_, '_> {
    fn ty(&self, ty: &Type, span: Span) -> Result<()> {
        self.checker.validate_interface_argument_roles(ty, 0, OpenHandleRoleContext::Source(&self.type_parameters)).map_err(
            |mut error| {
                if error.span == Span::default() {
                    error.span = span;
                }
                error
            },
        )
    }

    fn types(&self, types: &[Type], span: Span) -> Result<()> {
        for ty in types {
            self.ty(ty, span)?;
        }
        Ok(())
    }

    fn params(&self, params: &[Param]) -> Result<()> {
        for param in params {
            self.ty(&param.ty, param.span)?;
        }
        Ok(())
    }

    fn fields(&self, fields: &[Field], validity: Option<&ValidityBlock>) -> Result<()> {
        for field in fields {
            self.ty(&field.ty, field.span)?;
        }
        if let Some(validity) = validity {
            self.expressions(&validity.predicates)?;
        }
        Ok(())
    }

    fn target(&self, target: &AggregateTarget, span: Span) -> Result<()> {
        self.types(&target.type_args, span)
    }

    fn item(&self, item: &Item) -> Result<()> {
        let parameters: &[TypeParam] = match item {
            Item::Struct(def) => &def.type_params,
            Item::Enum(def) => &def.type_params,
            Item::Function(def) => &def.type_params,
            _ => &[],
        };
        let scoped =
            Self { checker: self.checker, type_parameters: parameters.iter().map(|parameter| parameter.name.clone()).collect() };
        scoped.item_types(item)
    }

    fn item_types(&self, item: &Item) -> Result<()> {
        match item {
            Item::Collections(def) => {
                for member in &def.members {
                    match &member.kind {
                        ValueCollectionKind::Vec(ty) | ValueCollectionKind::OrderedSet(ty) => self.ty(ty, member.span)?,
                        ValueCollectionKind::OrderedMap { key, value } => {
                            self.ty(key, member.span)?;
                            self.ty(value, member.span)?;
                        }
                    }
                }
            }
            Item::DataShape(def) => self.ty(&def.element, def.span)?,
            Item::CellRoles(def) => {
                for role in &def.roles {
                    self.ty(&role.schema, role.span)?;
                }
            }
            Item::Resource(def) => self.fields(&def.fields, def.validity.as_ref())?,
            Item::Shared(def) => self.fields(&def.fields, def.validity.as_ref())?,
            Item::Receipt(def) => {
                self.fields(&def.fields, def.validity.as_ref())?;
                if let Some(ty) = &def.claim_output {
                    self.ty(ty, def.span)?;
                }
            }
            Item::Struct(def) => self.fields(&def.fields, def.validity.as_ref())?,
            Item::Enum(def) => {
                for variant in &def.variants {
                    self.types(&variant.fields, variant.span)?;
                }
            }
            Item::Const(def) => {
                self.ty(&def.ty, def.span)?;
                self.expression(&def.value)?;
            }
            Item::Action(def) => {
                self.params(&def.params)?;
                if let Some(ty) = &def.return_type {
                    self.ty(ty, def.span)?;
                }
                for output in &def.outputs {
                    self.ty(&output.ty, output.span)?;
                }
                self.statements(&def.body)?;
                if let Some(surface) = &def.next_surface {
                    for audit in &surface.audits {
                        self.expression(&audit.subject)?;
                    }
                    for disposition in &surface.dispositions {
                        self.disposition(disposition)?;
                    }
                }
            }
            Item::Function(def) => {
                self.params(&def.params)?;
                if let Some(ty) = &def.return_type {
                    self.ty(ty, def.span)?;
                }
                self.statements(&def.body)?;
            }
            Item::Lock(def) => {
                self.params(&def.params)?;
                self.ty(&def.return_type, def.span)?;
                self.statements(&def.body)?;
                if let Some(surface) = &def.next_surface {
                    for audit in &surface.audits {
                        self.expression(&audit.subject)?;
                    }
                }
            }
            Item::Invariant(def) => {
                for target in &def.reads {
                    self.target(target, def.span)?;
                }
                for aggregate in &def.aggregates {
                    self.target(&aggregate.target, aggregate.span)?;
                    if let Some(rhs) = &aggregate.rhs {
                        self.target(rhs, aggregate.span)?;
                    }
                }
                for quantifier in &def.quantifiers {
                    self.target(&quantifier.range, quantifier.span)?;
                    self.expressions(&quantifier.predicates)?;
                    if let Some(expected) = &quantifier.expected {
                        self.expression(expected)?;
                    }
                }
                self.expressions(&def.asserts)?;
            }
            Item::Flow(_) | Item::Use(_) => {}
        }
        Ok(())
    }

    fn disposition(&self, disposition: &NextDisposition) -> Result<()> {
        match disposition {
            NextDisposition::Replace(_) => {}
            NextDisposition::Pool(value) => {
                for field in &value.data_fields {
                    match &field.treatment {
                        NextPoolFieldTreatment::Conserve => {}
                        NextPoolFieldTreatment::Set(values) => self.values(values)?,
                    }
                }
                for (_, treatment) in &value.output_locks {
                    if let ReplaceLockTreatment::Exact(value) | ReplaceLockTreatment::ExactHash(value) = treatment {
                        self.expression(value)?;
                    }
                }
            }
            NextDisposition::Fresh(value) => {
                self.values(&value.data_fields)?;
                if let ReplaceLockTreatment::Exact(lock) | ReplaceLockTreatment::ExactHash(lock) = &value.lock {
                    self.expression(lock)?;
                }
            }
            NextDisposition::Retire(_) => {}
        }
        Ok(())
    }

    fn statements(&self, statements: &[Stmt]) -> Result<()> {
        for statement in statements {
            match statement {
                Stmt::Let(value) => {
                    if let Some(ty) = &value.ty {
                        self.ty(ty, value.span)?;
                    }
                    self.expression(&value.value)?;
                }
                Stmt::Expr(value) | Stmt::Discard(value) => self.expression(value)?,
                Stmt::Return(value) => {
                    if let Some(value) = &value.value {
                        self.expression(value)?;
                    }
                }
                Stmt::If(value) => {
                    self.expression(&value.condition)?;
                    self.statements(&value.then_branch)?;
                    if let Some(branch) = &value.else_branch {
                        self.statements(branch)?;
                    }
                }
                Stmt::For(value) => {
                    self.expression(&value.iterable)?;
                    self.statements(&value.body)?;
                }
                Stmt::While(value) => {
                    self.expression(&value.condition)?;
                    self.statements(&value.body)?;
                }
                Stmt::Borrow(value) => self.statements(&value.body)?,
                Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
        Ok(())
    }

    fn values(&self, values: &[(String, Expr)]) -> Result<()> {
        for (_, value) in values {
            self.expression(value)?;
        }
        Ok(())
    }

    fn expressions(&self, values: &[Expr]) -> Result<()> {
        for value in values {
            self.expression(value)?;
        }
        Ok(())
    }

    fn expression(&self, expression: &Expr) -> Result<()> {
        match expression {
            Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) | Expr::Preserve(_) => {}
            Expr::Assign(value) => {
                self.expression(&value.target)?;
                self.expression(&value.value)?;
            }
            Expr::Binary(value) => {
                self.expression(&value.left)?;
                self.expression(&value.right)?;
            }
            Expr::Unary(value) => self.expression(&value.expr)?,
            Expr::Call(value) => {
                self.types(&value.type_args, value.span)?;
                self.expression(&value.func)?;
                self.expressions(&value.args)?;
            }
            Expr::FieldAccess(value) => self.expression(&value.expr)?,
            Expr::Index(value) => {
                self.expression(&value.expr)?;
                self.expression(&value.index)?;
            }
            Expr::Create(value) => {
                self.values(&value.fields)?;
                if let Some(lock) = &value.lock {
                    self.expression(lock)?;
                }
            }
            Expr::Consume(value) => self.expression(&value.expr)?,
            Expr::Destroy(value) => self.expression(&value.expr)?,
            Expr::ReadRef(value) => self.types(&value.type_args, value.span)?,
            Expr::Claim(value) => self.expression(&value.receipt)?,
            Expr::Settle(value) => self.expression(&value.expr)?,
            Expr::CreateUnique(value) => {
                self.types(&value.type_args, value.span)?;
                self.values(&value.fields)?;
                if let Some(lock) = &value.lock {
                    self.expression(lock)?;
                }
            }
            Expr::ReplaceUnique(value) => {
                self.types(&value.type_args, value.span)?;
                self.expression(&value.expr)?;
                self.values(&value.fields)?;
            }
            Expr::Assert(value) => {
                self.expression(&value.condition)?;
                self.expression(&value.message)?;
            }
            Expr::Require(value) => {
                self.expression(&value.condition)?;
                if let Some(message) = &value.message {
                    self.expression(message)?;
                }
            }
            Expr::RequireBlock(value) => self.expressions(&value.expressions)?,
            Expr::ReplaceRelation(value) => {
                for expression in replace_relation_inner_exprs(value) {
                    self.expression(expression)?;
                }
            }
            Expr::Block(value) => self.statements(value)?,
            Expr::Tuple(value) | Expr::Array(value) => self.expressions(value)?,
            Expr::If(value) => {
                self.expression(&value.condition)?;
                self.expression(&value.then_branch)?;
                self.expression(&value.else_branch)?;
            }
            Expr::Cast(value) => {
                self.ty(&value.ty, value.span)?;
                self.expression(&value.expr)?;
            }
            Expr::Range(value) => {
                self.expression(&value.start)?;
                self.expression(&value.end)?;
            }
            Expr::StructInit(value) => {
                self.types(&value.type_args, value.span)?;
                self.values(&value.fields)?;
            }
            Expr::Match(value) => {
                self.expression(&value.expr)?;
                for arm in &value.arms {
                    self.expression(&arm.value)?;
                }
            }
            Expr::StdlibCall(value) => self.expressions(&value.args)?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_role_scan_leaves_unrelated_type_limits_to_their_own_phase() {
        let mut module = crate::parser::parse(&crate::lexer::lex("module roles; struct Data { value: u8, }").unwrap()).unwrap();
        let Item::Struct(definition) = &mut module.items[0] else { panic!("expected source record") };
        for _ in 0..40 {
            definition.fields[0].ty = Type::Array(Box::new(definition.fields[0].ty.clone()), 1);
        }
        let mut resolver = ModuleResolver::new();
        resolver.register_module(module.clone()).unwrap();
        // This asserts the scope of the role pass, not executable admission.
        validate_source_open_handle_roles(&module, &resolver).unwrap();
    }
}
