use super::{identity_condition, CellTypeKind, TypeChecker, TypeEnv};
use crate::ast::{parse_bounded_collection_type, BoundedCollectionKind, Capability, CapabilityOperation, Expr, IdentityPolicy, Type};
use crate::error::{CompileError, Result, Span};
use crate::resolve::TypeDef;
use std::collections::HashSet;

impl TypeChecker<'_> {
    pub(super) fn lifecycle_kernel(&self, value: &crate::ast::LifecycleStmt) -> Result<Vec<crate::ast::Stmt>> {
        let identity = match value {
            crate::ast::LifecycleStmt::Fresh(fresh) => {
                Some(self.resolve_operation_identity(&fresh.ty, fresh.identity.as_ref(), false, fresh.span, "fresh")?)
            }
            _ => None,
        };
        value.kernel(identity.as_ref())
    }

    pub(super) fn base_type_name(ty: &Type) -> Option<&str> {
        match ty {
            Type::Applied(application) => Some(&application.constructor),
            Type::Named(name) => Some(name.as_str()),
            Type::Ref(inner) | Type::MutRef(inner) => Self::base_type_name(inner),
            _ => None,
        }
    }

    pub(super) fn is_receipt_type(&self, ty: &Type) -> bool {
        Self::base_type_name(ty).and_then(|name| self.resolve_cell_type_kind(name)).is_some_and(|kind| kind == CellTypeKind::Receipt)
    }

    pub(super) fn resolve_cell_type_kind(&self, name: &str) -> Option<CellTypeKind> {
        if let Some(kind) = self.cell_type_kinds.get(name).copied() {
            return Some(kind);
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, name)? {
            TypeDef::Resource(_) => Some(CellTypeKind::Resource),
            TypeDef::Shared(_) => Some(CellTypeKind::Shared),
            TypeDef::Receipt(_) => Some(CellTypeKind::Receipt),
            TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    pub(super) fn validate_receipt_claim_output(&self, output: &Type, span: Span) -> Result<()> {
        let Some(type_name) = Self::base_type_name(output) else {
            return Err(CompileError::new("receipt claim output must be a cell-backed resource or shared type", span));
        };
        match self.resolve_cell_type_kind(type_name) {
            Some(CellTypeKind::Resource | CellTypeKind::Shared) => Ok(()),
            Some(CellTypeKind::Receipt) => Err(CompileError::new("receipt claim output must not be another receipt", span)),
            None => Err(CompileError::new("receipt claim output must be a cell-backed resource or shared type", span)),
        }
    }

    pub(super) fn resolve_receipt_claim_output(&self, name: &str) -> Option<Option<Type>> {
        if let Some(output) = self.receipt_claim_outputs.get(name) {
            return Some(output.clone());
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, name)? {
            TypeDef::Receipt(receipt) => Some(receipt.claim_output),
            TypeDef::Resource(_) | TypeDef::Shared(_) | TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    pub(super) fn require_named_linear_cell_operand(
        &mut self,
        env: &mut TypeEnv,
        expr: &Expr,
        operation: &str,
        span: Span,
    ) -> Result<(Type, String)> {
        let ty = self.infer_expr(env, expr)?;
        if !self.is_linear_type(&ty) || self.contains_collection_owner(&ty) {
            return Err(CompileError::new(format!("{} requires a cell-backed linear value", operation), span));
        }
        match expr {
            Expr::Identifier(name) => {
                self.reject_active_borrow_root_lifecycle(name, operation, span)?;
                Ok((ty, name.clone()))
            }
            _ => Err(CompileError::new(
                format!("{} requires a named cell-backed value so the compiler can track linear ownership", operation),
                span,
            )),
        }
    }

    fn resolve_capabilities(&self, name: &str) -> Option<HashSet<Capability>> {
        if let Some(capabilities) = self.type_capabilities.get(name) {
            return Some(capabilities.clone());
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, name)? {
            TypeDef::Resource(resource) => Some(resource.capabilities.into_iter().collect()),
            TypeDef::Shared(shared) => Some(shared.capabilities.into_iter().collect()),
            TypeDef::Receipt(receipt) => Some(receipt.capabilities.into_iter().collect()),
            TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    fn resolve_identity_policy(&self, name: &str) -> Option<IdentityPolicy> {
        if let Some(identity) = self.type_identities.get(name) {
            return Some(identity.clone());
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, name)? {
            TypeDef::Resource(resource) => Some(resource.identity),
            TypeDef::Shared(shared) => Some(shared.identity),
            TypeDef::Receipt(receipt) => Some(receipt.identity),
            TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    pub(super) fn resolve_operation_identity(
        &self,
        type_name: &str,
        requested: Option<&IdentityPolicy>,
        require_unique: bool,
        span: Span,
        operation: &str,
    ) -> Result<IdentityPolicy> {
        let declared = self.resolve_identity_policy(type_name).ok_or_else(|| {
            CompileError::new(format!("{operation} identity requires a resolved Cell declaration for '{type_name}'"), span)
        })?;
        let requested = requested.unwrap_or(&declared);
        if declared == *requested && !(require_unique && declared == IdentityPolicy::None) {
            return Ok(declared);
        }
        let required = identity_condition(requested);
        let provided = identity_condition(&declared);
        Err(CompileError::new(
            format!(
                "{} rejected for type '{}': required identity condition '{}'; provided '{}'; missing '{}'; capability-set version {}; entailment version {}",
                operation,
                type_name,
                required,
                provided,
                required,
                Capability::REGISTRY_VERSION,
                CapabilityOperation::ENTAILMENT_VERSION
            ),
            span,
        ))
    }

    pub(super) fn require_capability_operation(
        &self,
        ty: &Type,
        operation: CapabilityOperation,
        requested_identity: Option<&IdentityPolicy>,
        span: Span,
    ) -> Result<()> {
        let Some(type_name) = Self::base_type_name(ty) else {
            return Err(CompileError::new(format!("{} requires a cell-backed value", operation.as_str()), span));
        };
        let Some(capabilities) = self.resolve_capabilities(type_name) else {
            return Err(CompileError::new(format!("{} requires a cell-backed value", operation.as_str()), span));
        };
        let entailment = operation.evaluate(&capabilities);
        let declared_identity = self.resolve_identity_policy(type_name).unwrap_or_default();
        let identity_matches = !operation.requires_identity_preservation()
            || requested_identity
                .is_some_and(|requested| *requested == declared_identity && !matches!(&declared_identity, IdentityPolicy::None));
        if entailment.missing.is_empty() && identity_matches {
            return Ok(());
        }

        let mut required = entailment.required.iter().map(|capability| capability.as_str().to_string()).collect::<Vec<_>>();
        if operation.requires_identity_preservation() {
            required.push("identity-preservation".to_string());
        }
        let mut provided = capabilities.iter().copied().collect::<Vec<_>>();
        provided.sort_by_key(|capability| capability.registry_index());
        let mut provided = provided.iter().map(|capability| capability.as_str().to_string()).collect::<Vec<_>>();
        if !matches!(&declared_identity, IdentityPolicy::None) {
            provided.push(identity_condition(&declared_identity));
        }
        let mut entailed = entailment.entailed.iter().map(|capability| capability.as_str().to_string()).collect::<Vec<_>>();
        let mut missing = entailment.missing.iter().map(|capability| capability.as_str().to_string()).collect::<Vec<_>>();
        if operation.requires_identity_preservation() {
            if identity_matches {
                entailed.push("identity-preservation".to_string());
            } else {
                missing.push(requested_identity.map_or_else(|| "identity(...)".to_string(), identity_condition));
            }
        }
        Err(CompileError::new(
            format!(
                "capability check rejected for operation '{}' on type '{}': required [{}]; provided [{}]; entailed [{}]; missing [{}]; capability-set version {}; entailment version {}",
                operation.as_str(),
                type_name,
                required.join(", "),
                provided.join(", "),
                entailed.join(", "),
                missing.join(", "),
                Capability::REGISTRY_VERSION,
                CapabilityOperation::ENTAILMENT_VERSION
            ),
            span,
        ))
    }

    pub(super) fn is_linear_type(&self, ty: &Type) -> bool {
        self.collection_owner(ty).is_some() || self.is_linear_type_with_seen(ty, &mut HashSet::new())
    }

    pub(super) fn is_linear_type_with_seen(&self, ty: &Type, visiting: &mut HashSet<String>) -> bool {
        if ty.opening_value_type().is_some() {
            return true;
        }
        if parse_bounded_collection_type(ty).is_some_and(|collection| collection.kind == BoundedCollectionKind::CellSet) {
            return true;
        }
        match ty {
            Type::Array(inner, _) => self.is_linear_type_with_seen(inner, visiting),
            Type::Tuple(items) => items.iter().any(|item| self.is_linear_type_with_seen(item, visiting)),
            Type::Named(name) => {
                let base_name = name.as_str();
                if self.linear_types.contains(base_name)
                    || self
                        .resolver
                        .zip(self.current_module.as_ref())
                        .is_some_and(|(resolver, module)| resolver.type_is_linear(module, base_name))
                {
                    return true;
                }
                if !visiting.insert(name.clone()) {
                    return false;
                }
                let linear_struct = self
                    .resolve_named_type_fields(name)
                    .is_some_and(|fields| fields.values().any(|field| self.is_linear_type_with_seen(field, visiting)));
                let linear_enum = self
                    .resolve_enum_variant_fields(name)
                    .is_some_and(|variants| variants.values().flatten().any(|field| self.is_linear_type_with_seen(field, visiting)));
                visiting.remove(name);
                linear_struct || linear_enum
            }
            _ => false,
        }
    }
}
