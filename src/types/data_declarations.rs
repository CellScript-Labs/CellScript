use super::{aggregate_field_type_is_supported, expr_span, nested, type_repr, CallableKind, TypeChecker, TypeEnv};
use crate::ast::{
    AggregateInvariant, AggregateInvariantKind, AggregateTarget, BoundedQuantifier, BoundedQuantifierKind, Capability, ConstDef,
    EnumDef, Expr, Field, InvariantDef, OpenHandleClass, ReceiptDef, ResourceDef, SharedDef, SourceView, StructDef, Type,
    TypeArgument, ValidityBlock,
};
use crate::error::{CompileError, Result, Span};
use std::collections::HashSet;

impl TypeChecker<'_> {
    pub(super) fn check_resource(&mut self, resource: &ResourceDef) -> Result<()> {
        self.validate_capability_set(&resource.name, &resource.capabilities, resource.span)?;
        self.validate_schema_fields(&resource.fields, "resource", &resource.name)?;
        self.check_type_validity(&resource.name, &resource.fields, resource.validity.as_ref())
    }

    pub(super) fn check_shared(&mut self, shared: &SharedDef) -> Result<()> {
        self.validate_capability_set(&shared.name, &shared.capabilities, shared.span)?;
        self.validate_schema_fields(&shared.fields, "shared", &shared.name)?;
        self.check_type_validity(&shared.name, &shared.fields, shared.validity.as_ref())
    }

    pub(super) fn check_receipt(&mut self, receipt: &ReceiptDef) -> Result<()> {
        self.validate_capability_set(&receipt.name, &receipt.capabilities, receipt.span)?;
        self.validate_schema_fields(&receipt.fields, "receipt", &receipt.name)?;
        if let Some(output) = &receipt.claim_output {
            self.validate_type(output)?;
            self.validate_receipt_claim_output(output, receipt.span)?;
        }
        self.check_type_validity(&receipt.name, &receipt.fields, receipt.validity.as_ref())
    }

    fn validate_capability_set(&self, type_name: &str, capabilities: &[Capability], span: Span) -> Result<()> {
        let mut seen = HashSet::new();
        for capability in capabilities {
            if !seen.insert(*capability) {
                return Err(CompileError::new(
                    format!(
                        "type '{}' repeats canonical capability '{}' (capability-set version {})",
                        type_name,
                        capability.as_str(),
                        Capability::REGISTRY_VERSION
                    ),
                    span,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn check_struct(&mut self, struct_def: &StructDef) -> Result<()> {
        self.validate_schema_fields(&struct_def.fields, "struct", &struct_def.name)?;
        self.check_type_validity(&struct_def.name, &struct_def.fields, struct_def.validity.as_ref())
    }

    fn check_type_validity(&mut self, type_name: &str, fields: &[Field], validity: Option<&ValidityBlock>) -> Result<()> {
        let Some(validity) = validity else {
            return Ok(());
        };
        let mut env = self.env.child();
        for field in fields {
            env.insert(field.name.clone(), field.ty.clone(), false, false);
        }
        let previous_callable = self.current_callable.replace(CallableKind::Invariant);
        let previous_validity = std::mem::replace(&mut self.current_validity, true);
        let result = (|| {
            for predicate in &validity.predicates {
                let Expr::Require(require) = predicate else {
                    return Err(CompileError::new(
                        format!("validity block for '{}' contains non-require syntax", type_name),
                        predicate.span(),
                    ));
                };
                Self::validate_require_condition_is_pure(&require.condition, "validity predicate")?;
                Self::validate_validity_surface(&require.condition)?;
                let ty = self.infer_expr(&mut env, &require.condition)?;
                if !self.is_bool_type(&ty) {
                    return Err(CompileError::new(format!("validity predicate for '{}' must be boolean", type_name), require.span));
                }
                if let Some(message) = &require.message
                    && !matches!(message.as_ref(), Expr::String(_))
                {
                    return Err(CompileError::new("validity require message must be a string literal", expr_span(message)));
                }
            }
            Ok(())
        })();
        self.current_callable = previous_callable;
        self.current_validity = previous_validity;
        result
    }

    fn validate_validity_surface(expr: &Expr) -> Result<()> {
        match expr {
            Expr::Call(call) => {
                if let Expr::Identifier(name) = call.func.as_ref() {
                    if name == "env::block_number" {
                        if !call.args.is_empty() {
                            return Err(CompileError::new("env::block_number expects 0 arguments", call.span));
                        }
                    } else if name.starts_with("env::") {
                        return Err(CompileError::new(
                            format!("unknown validity environment read '{}'; only env::block_number() is approved", name),
                            call.span,
                        ));
                    } else if ["source::", "ckb::", "witness::", "dao::", "xudt::"].iter().any(|prefix| name.starts_with(prefix)) {
                        return Err(CompileError::new(
                            format!("validity predicate cannot read transaction view '{}'; use the approved env::block_number evidence contract or an action invariant", name),
                            call.span,
                        ));
                    }
                }
                Self::validate_validity_surface(&call.func)?;
                for arg in &call.args {
                    Self::validate_validity_surface(arg)?;
                }
            }
            Expr::Binary(binary) => {
                Self::validate_validity_surface(&binary.left)?;
                Self::validate_validity_surface(&binary.right)?;
            }
            Expr::Unary(unary) => Self::validate_validity_surface(&unary.expr)?,
            Expr::FieldAccess(field) => Self::validate_validity_surface(&field.expr)?,
            Expr::Index(index) => {
                Self::validate_validity_surface(&index.expr)?;
                Self::validate_validity_surface(&index.index)?;
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    Self::validate_validity_surface(item)?;
                }
            }
            Expr::Cast(cast) => Self::validate_validity_surface(&cast.expr)?,
            Expr::Range(range) => {
                Self::validate_validity_surface(&range.start)?;
                Self::validate_validity_surface(&range.end)?;
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    Self::validate_validity_surface(value)?;
                }
            }
            Expr::Create(_)
            | Expr::Consume(_)
            | Expr::Destroy(_)
            | Expr::ReadRef(_)
            | Expr::Claim(_)
            | Expr::Settle(_)
            | Expr::CreateUnique(_)
            | Expr::ReplaceUnique(_)
            | Expr::Assign(_)
            | Expr::If(_)
            | Expr::Match(_)
            | Expr::Block(_)
            | Expr::Assert(_)
            | Expr::Require(_)
            | Expr::RequireBlock(_)
            | Expr::Preserve(_)
            | Expr::ReplaceRelation(_)
            | Expr::StdlibCall(_) => {
                return Err(CompileError::new(
                    "validity predicates must be pure expressions without lifecycle, verifier-boundary, or control-flow syntax",
                    expr.span(),
                ));
            }
            Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) => {}
        }
        Ok(())
    }

    pub(super) fn check_invariant(&mut self, invariant: &InvariantDef) -> Result<()> {
        let missing_trigger = invariant.trigger.is_none();
        let missing_scope = invariant.scope.is_none();
        if missing_trigger || missing_scope {
            return Err(CompileError::new(
                format!("strict CKB invariant '{}' must declare explicit trigger and scope", invariant.name),
                invariant.span,
            ));
        }

        if let Some(trigger) = &invariant.trigger {
            match trigger.as_str() {
                "explicit_entry" | "lock_group" | "type_group" => {}
                _ => {
                    return Err(CompileError::new(
                        format!(
                            "invariant '{}' has unsupported trigger '{}'; expected explicit_entry, lock_group, or type_group",
                            invariant.name, trigger
                        ),
                        invariant.span,
                    ));
                }
            }
        }

        if let Some(scope) = &invariant.scope {
            match scope.as_str() {
                "selected_cells" | "group" | "transaction" => {}
                _ => {
                    return Err(CompileError::new(
                        format!(
                            "invariant '{}' has unsupported scope '{}'; expected selected_cells, group, or transaction",
                            invariant.name, scope
                        ),
                        invariant.span,
                    ));
                }
            }
        }

        if invariant.reads.is_empty() {
            return Err(CompileError::new(
                format!("invariant '{}' must declare at least one read source", invariant.name),
                invariant.span,
            ));
        }

        for read in &invariant.reads {
            if !SourceView::TRANSACTION_VIEWS.contains(&read.source) {
                return Err(CompileError::new(
                    format!(
                        "invariant '{}' has unsupported read source '{}'; expected input/output/group_input/group_output/cell_dep/header_dep/witness/lock_args variants",
                        invariant.name, read
                    ),
                    invariant.span,
                ));
            }
        }

        if invariant.asserts.is_empty() && invariant.aggregates.is_empty() && invariant.quantifiers.is_empty() {
            return Err(CompileError::new(
                format!(
                    "invariant '{}' must contain at least one assert_invariant expression, aggregate primitive, or bounded quantifier",
                    invariant.name
                ),
                invariant.span,
            ));
        }

        for aggregate in &invariant.aggregates {
            self.check_aggregate_invariant(invariant, aggregate)?;
        }

        let previous_callable = self.current_callable.replace(CallableKind::Invariant);
        let mut assert_env = TypeEnv::default();
        let assert_result = (|| -> Result<()> {
            for expr in &invariant.asserts {
                if !matches!(expr, Expr::Assert(_)) {
                    return Err(CompileError::new(
                        format!("invariant '{}' body only supports assert_invariant expressions", invariant.name),
                        invariant.span,
                    ));
                }
                self.infer_expr(&mut assert_env, expr)?;
            }
            for quantifier in &invariant.quantifiers {
                self.check_bounded_quantifier(invariant, quantifier)?;
            }
            Ok(())
        })();
        self.current_callable = previous_callable;
        assert_result?;

        Ok(())
    }

    fn check_bounded_quantifier(&mut self, invariant: &InvariantDef, quantifier: &BoundedQuantifier) -> Result<()> {
        let Some(expected_role) = quantifier.range.source.quantifier_role() else {
            return Err(CompileError::new(
                format!(
                    "bounded quantifier in '{}' uses unsupported source '{}'; expected inputs, outputs, group_inputs, group_outputs, selected_cells, or cell_deps",
                    invariant.name, quantifier.range
                ),
                quantifier.span,
            ));
        };
        let Some(type_name) = quantifier.range.type_name.as_deref() else {
            return Err(CompileError::new(
                format!("bounded quantifier in '{}' must name a cell-backed type argument", invariant.name),
                quantifier.span,
            ));
        };
        if self.resolve_cell_type_kind(type_name).is_none() {
            return Err(CompileError::new(
                format!("bounded quantifier in '{}' references non-cell-backed type '{}'", invariant.name, type_name),
                quantifier.span,
            ));
        }
        if !invariant.reads.iter().any(|read| read.source == quantifier.range.source && read.type_name.as_deref() == Some(type_name)) {
            return Err(CompileError::new(
                format!(
                    "bounded quantifier range '{}' must be declared by invariant '{}' reads metadata",
                    quantifier.range, invariant.name
                ),
                quantifier.span,
            ));
        }
        let range_scope = match quantifier.range.source {
            SourceView::GroupInput | SourceView::GroupOutput => "group",
            SourceView::Input | SourceView::Output | SourceView::CellDep => "transaction",
            SourceView::SelectedCells => "selected_cells",
            _ => unreachable!("unsupported quantifier source rejected above"),
        };
        if invariant.scope.as_deref().is_some_and(|scope| scope != range_scope) {
            return Err(CompileError::new(
                format!(
                    "bounded quantifier range '{}' has scope '{}', which does not match enclosing invariant scope '{}'",
                    quantifier.range,
                    range_scope,
                    invariant.scope.as_deref().unwrap_or("unspecified")
                ),
                quantifier.span,
            ));
        }

        let mut env = TypeEnv::default();
        match quantifier.kind {
            BoundedQuantifierKind::ForAll => {
                if quantifier.role.as_deref() != Some(expected_role) {
                    return Err(CompileError::new(
                        format!(
                            "forall role '{}' does not match source '{}'; expected role '{}'",
                            quantifier.role.as_deref().unwrap_or("missing"),
                            quantifier.range,
                            expected_role
                        ),
                        quantifier.span,
                    ));
                }
                let binding = quantifier
                    .binding
                    .as_ref()
                    .ok_or_else(|| CompileError::new("forall quantifier is missing its element binding", quantifier.span))?;
                env.insert(binding.clone(), Type::Named(type_name.to_string()), false, false);
                for predicate in &quantifier.predicates {
                    let predicate_ty = self.infer_expr(&mut env, predicate)?;
                    if predicate_ty != Type::Bool {
                        return Err(CompileError::new("forall require predicate must be boolean", predicate.span()));
                    }
                }
            }
            BoundedQuantifierKind::Count => {
                let fields = self.resolve_named_type_fields(type_name).ok_or_else(|| {
                    CompileError::new(format!("count quantifier cannot resolve fields for type '{}'", type_name), quantifier.span)
                })?;
                for (field, ty) in fields {
                    env.insert(field, ty, false, false);
                }
                let predicate = quantifier
                    .predicates
                    .first()
                    .ok_or_else(|| CompileError::new("count quantifier is missing its where predicate", quantifier.span))?;
                let predicate_ty = self.infer_expr(&mut env, predicate)?;
                if predicate_ty != Type::Bool {
                    return Err(CompileError::new("count where predicate must be boolean", predicate.span()));
                }
                let expected = quantifier
                    .expected
                    .as_ref()
                    .ok_or_else(|| CompileError::new("count quantifier is missing its comparison expression", quantifier.span))?;
                self.infer_expr_with_expected_type(&mut env, expected, &Type::U64, expected.span())?;
            }
        }
        Ok(())
    }

    fn check_aggregate_invariant(&self, invariant: &InvariantDef, aggregate: &AggregateInvariant) -> Result<()> {
        match aggregate.scope.as_str() {
            "selected_cells" | "group" | "transaction" => {}
            _ => {
                return Err(CompileError::new(
                    format!(
                        "aggregate invariant in '{}' has unsupported scope '{}'; expected selected_cells, group, or transaction",
                        invariant.name, aggregate.scope
                    ),
                    aggregate.span,
                ));
            }
        }

        if invariant.scope.as_deref().is_some_and(|scope| scope != aggregate.scope) {
            return Err(CompileError::new(
                format!(
                    "aggregate invariant scope '{}' must match enclosing invariant scope '{}'",
                    aggregate.scope,
                    invariant.scope.as_deref().unwrap_or("unspecified")
                ),
                aggregate.span,
            ));
        }

        match aggregate.kind {
            AggregateInvariantKind::Conserved | AggregateInvariantKind::Distinct => {
                self.validate_aggregate_field_target(invariant, aggregate, &aggregate.target)?;
            }
            AggregateInvariantKind::Delta => {
                if aggregate.argument.is_none() {
                    return Err(CompileError::new(
                        format!("assert_delta in invariant '{}' requires a delta argument", invariant.name),
                        aggregate.span,
                    ));
                }
                self.validate_aggregate_field_target(invariant, aggregate, &aggregate.target)?;
            }
            AggregateInvariantKind::Sum => {
                if aggregate.relation.is_none() || aggregate.rhs.is_none() {
                    return Err(CompileError::new(
                        format!("assert_sum in invariant '{}' requires a comparison", invariant.name),
                        aggregate.span,
                    ));
                }
                self.validate_aggregate_field_target(invariant, aggregate, &aggregate.target)?;
                if let Some(rhs) = &aggregate.rhs {
                    self.validate_aggregate_field_target(invariant, aggregate, rhs)?;
                }
            }
            AggregateInvariantKind::Singleton => {
                if aggregate.target.source != SourceView::TypeIdentity {
                    self.validate_aggregate_field_target(invariant, aggregate, &aggregate.target)?;
                }
            }
        }

        Ok(())
    }

    fn validate_aggregate_field_target(
        &self,
        invariant: &InvariantDef,
        aggregate: &AggregateInvariant,
        target: &AggregateTarget,
    ) -> Result<()> {
        let Some((type_name, field_name)) = target.type_and_field() else {
            return Err(CompileError::new(
                format!(
                    "aggregate invariant in '{}' must target a concrete field like Token.amount or group_inputs<Token>.amount",
                    invariant.name
                ),
                aggregate.span,
            ));
        };
        let Some(fields) = self.type_fields.get(type_name) else {
            return Err(CompileError::new(
                format!("aggregate invariant in '{}' references unknown type '{}'", invariant.name, type_name),
                aggregate.span,
            ));
        };
        let Some(field_ty) = fields.get(field_name) else {
            return Err(CompileError::new(
                format!("aggregate invariant in '{}' references unknown field '{}.{}'", invariant.name, type_name, field_name),
                aggregate.span,
            ));
        };
        if !aggregate_field_type_is_supported(field_ty) {
            return Err(CompileError::new(
                format!(
                    "aggregate invariant in '{}' field '{}.{}' must be a fixed-width integer or fixed bytes, found {}",
                    invariant.name,
                    type_name,
                    field_name,
                    type_repr(field_ty)
                ),
                aggregate.span,
            ));
        }
        Ok(())
    }

    /// Open handle types are identity-only in this release: they may appear
    /// only as generic application arguments (phantom identity use). A field,
    /// parameter or nested value position would need the runtime encoding
    /// slice, so it fails closed here.
    pub(super) fn is_open_handle_type(&self, ty: &Type) -> bool {
        ty.as_open_handle().is_some()
    }

    pub(super) fn reject_open_handle_value_type(&self, ty: &Type, position: &str, span: crate::error::Span) -> Result<()> {
        match ty {
            Type::Applied(application) => {
                if OpenHandleClass::from_constructor(&application.constructor).is_some() {
                    return Err(CompileError::new(
                        format!(
                            "open handle types are identity-only in this release; {position} would need the runtime encoding slice"
                        ),
                        span,
                    ));
                }
                for argument in application.arguments.iter().filter_map(TypeArgument::as_type) {
                    self.reject_open_handle_value_type(argument, position, span)?;
                }
                Ok(())
            }
            Type::Named(name) => {
                let base = name.as_str();
                if matches!(
                    base,
                    crate::script_handle_contract::OPEN_SCRIPT_HANDLE_TYPE
                        | crate::script_handle_contract::OPEN_VERIFIER_HANDLE_TYPE
                        | crate::script_handle_contract::INSTANCE_SCRIPT_HANDLE_TYPE
                ) {
                    return Err(CompileError::new(
                        format!(
                            "open handle types are identity-only in this release; {position} would need the runtime encoding slice"
                        ),
                        span,
                    ));
                }
                Ok(())
            }
            Type::Array(inner, _) | Type::Ref(inner) | Type::MutRef(inner) => {
                self.reject_open_handle_value_type(inner, position, span)
            }
            Type::Tuple(items) => {
                for item in items {
                    self.reject_open_handle_value_type(item, position, span)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn validate_schema_fields(&self, fields: &[Field], item_kind: &str, item_name: &str) -> Result<()> {
        let mut seen = HashSet::new();
        let has_nested_values = fields.iter().any(|field| nested::whole_values(&field.ty));
        let mut minimum_fixed_bytes = 0usize;
        for field in fields {
            if field.name == "_" {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' field must have a stable name; '_' is reserved for local wildcard bindings",
                        item_kind, item_name
                    ),
                    field.span,
                ));
            }
            if !seen.insert(field.name.clone()) {
                return Err(CompileError::new(
                    format!("duplicate field '{}' in {} '{}'", field.name, item_kind, item_name),
                    field.span,
                ));
            }
            self.reject_collection_container(&field.ty, field.span)?;
            self.validate_type(&field.ty).map_err(|mut error| {
                if error.span == Span::default() {
                    error.span = field.span;
                }
                error
            })?;
            if let Some(width) = self.payload_type_runtime_width(&field.ty, &mut HashSet::new()) {
                minimum_fixed_bytes = crate::ir::checked_fixed_size(minimum_fixed_bytes.checked_add(width)).map_err(|mut error| {
                    error.span = field.span;
                    error
                })?;
            }
            if crate::cell_data_view::invocation_local(&field.ty) {
                return Err(CompileError::new(
                    "ExactCellSelection and CellDataView cannot be stored in a field or serialized",
                    field.span,
                ));
            }
            let direct_nested = nested::whole_values(&field.ty);
            if nested::contains_nested_type(&field.ty) && !(matches!(item_kind, "resource" | "shared" | "receipt") && direct_nested) {
                return Err(CompileError::new("only a direct NestedValues field may be encoded in a Cell; derived views and nested view containers cannot be stored", field.span));
            }
            if has_nested_values && !direct_nested && crate::nested_values::fixed_field_width(&field.ty).is_none() {
                return Err(CompileError::new(
                    "a Cell containing NestedValues requires bounded nested fields or fixed unsigned/Hash/Address/byte-array fields",
                    field.span,
                ));
            }
            self.reject_open_handle_value_type(
                &field.ty,
                &format!("{} '{}' field '{}'", item_kind, item_name, field.name),
                field.span,
            )?;
            self.validate_stored_type_has_no_references(
                &field.ty,
                &format!("{} '{}' field '{}'", item_kind, item_name, field.name),
                field.span,
            )?;
            if self.enum_type_contains_linear_payload(&field.ty, &mut HashSet::new()) {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' field '{}' cannot store an enum with a linear Cell payload; linear payload enums are local-only handles",
                        item_kind, item_name, field.name
                    ),
                    field.span,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn check_enum(&mut self, enum_def: &EnumDef) -> Result<()> {
        if enum_def.variants.is_empty() {
            return Err(CompileError::new(format!("enum '{}' must declare at least one variant", enum_def.name), enum_def.span));
        }
        if enum_def.variants.len() > u8::MAX as usize + 1 {
            return Err(CompileError::new(
                format!("enum '{}' has too many variants for the 0.22 u8 tag ABI", enum_def.name),
                enum_def.span,
            ));
        }
        let mut seen = HashSet::new();
        for variant in &enum_def.variants {
            if !seen.insert(variant.name.clone()) {
                return Err(CompileError::new(format!("duplicate enum variant '{}::{}'", enum_def.name, variant.name), variant.span));
            }
            for field_ty in &variant.fields {
                self.reject_collection_container(field_ty, variant.span)?;
                self.validate_type(field_ty)?;
                self.validate_stored_type_has_no_references(
                    field_ty,
                    &format!("enum variant '{}::{}' payload", enum_def.name, variant.name),
                    variant.span,
                )?;
                if self.payload_type_runtime_width(field_ty, &mut HashSet::new()).is_none() {
                    return Err(CompileError::new(
                        format!(
                            "enum variant '{}::{}' payload type '{}' is variable-width or recursive; 0.22 payload enums require a concrete fixed-width layout",
                            enum_def.name,
                            variant.name,
                            type_repr(field_ty)
                        ),
                        variant.span,
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn payload_type_runtime_width(&self, ty: &Type, visiting: &mut HashSet<String>) -> Option<usize> {
        match ty {
            Type::Applied(application) => {
                if application.unary_type("Commitment").is_some() {
                    Some(32)
                } else if let Some(inner) = application.unary_type("Opening") {
                    self.payload_type_runtime_width(inner, visiting)
                } else {
                    None
                }
            }
            Type::U8 | Type::Bool => Some(1),
            Type::U16 => Some(2),
            Type::U32 | Type::I32 => Some(4),
            Type::U64 => Some(8),
            Type::U128 => Some(16),
            Type::Address | Type::Hash => Some(32),
            Type::Unit => Some(0),
            Type::Array(inner, len) => self.payload_type_runtime_width(inner, visiting)?.checked_mul(*len),
            Type::Tuple(items) => {
                items.iter().try_fold(0usize, |width, item| width.checked_add(self.payload_type_runtime_width(item, visiting)?))
            }
            Type::Selection(_) | Type::ArrayBound(_, _) | Type::Ref(_) | Type::MutRef(_) => None,
            Type::Named(name) => {
                if name.contains('<') {
                    return None;
                }
                if self.resolve_cell_type_kind(name).is_some() {
                    return Some(8);
                }
                if !visiting.insert(name.clone()) {
                    return None;
                }
                let width = if let Some(variants) = self.resolve_enum_variant_fields(name) {
                    variants
                        .values()
                        .map(|fields| {
                            fields
                                .iter()
                                .try_fold(0usize, |width, field| width.checked_add(self.payload_type_runtime_width(field, visiting)?))
                        })
                        .collect::<Option<Vec<_>>>()?
                        .into_iter()
                        .max()
                        .unwrap_or(0)
                        .checked_add(1)
                } else {
                    self.resolve_named_type_fields(name).and_then(|fields| {
                        fields
                            .values()
                            .try_fold(0usize, |width, field| width.checked_add(self.payload_type_runtime_width(field, visiting)?))
                    })
                };
                visiting.remove(name);
                width
            }
        }
    }

    pub(super) fn enum_type_contains_linear_payload(&self, ty: &Type, visiting: &mut HashSet<String>) -> bool {
        match ty {
            Type::Applied(application) => application
                .arguments
                .iter()
                .filter_map(TypeArgument::as_type)
                .any(|ty| self.enum_type_contains_linear_payload(ty, visiting)),
            Type::Array(inner, _) | Type::ArrayBound(inner, _) => self.enum_type_contains_linear_payload(inner, visiting),
            Type::Tuple(items) => items.iter().any(|item| self.enum_type_contains_linear_payload(item, visiting)),
            Type::Named(name) => {
                if !visiting.insert(name.clone()) {
                    return false;
                }
                let contains = self
                    .resolve_enum_variant_fields(name)
                    .is_some_and(|variants| variants.values().flatten().any(|field| self.is_linear_type_with_seen(field, visiting)));
                visiting.remove(name);
                contains
            }
            Type::U8
            | Type::U16
            | Type::U32
            | Type::I32
            | Type::U64
            | Type::U128
            | Type::Bool
            | Type::Unit
            | Type::Address
            | Type::Hash
            | Type::Selection(_)
            | Type::Ref(_)
            | Type::MutRef(_) => false,
        }
    }

    pub(super) fn check_const(&mut self, const_def: &ConstDef) -> Result<()> {
        self.reject_collection_container(&const_def.ty, const_def.span)?;
        let mut env = self.env.clone();
        let value_ty = self.infer_expr_with_expected_type(&mut env, &const_def.value, &const_def.ty, const_def.span)?;
        if !self.types_equal(&value_ty, &const_def.ty) {
            return Err(CompileError::new(
                format!("const '{}' has type mismatch: expected {:?}, found {:?}", const_def.name, const_def.ty, value_ty),
                const_def.span,
            ));
        }
        Ok(())
    }
}
