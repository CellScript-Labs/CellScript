use super::interface_roles::OpenHandleRoleContext;
use super::{
    type_repr, CellTypeKind, TypeChecker, CKB_ABSOLUTE_BLOCK_SINCE_TYPE, CKB_ABSOLUTE_EPOCH_SINCE_TYPE,
    CKB_ABSOLUTE_TIMESTAMP_SINCE_TYPE, CKB_BLOCK_NUMBER_TYPE, CKB_CELL_DEP_VIEW_TYPE, CKB_DECODED_SINCE_TYPE,
    CKB_DEPLOYMENT_LINE_HANDLE_TYPE, CKB_ENCODED_SINCE_TYPE, CKB_EPOCH_DURATION_TYPE, CKB_EPOCH_LENGTH_TYPE, CKB_EPOCH_NUMBER_TYPE,
    CKB_EXACT_SCRIPT_HANDLE_TYPE, CKB_HEADER_DEP_VIEW_TYPE, CKB_INPUT_VIEW_TYPE, CKB_OUTPUT_VIEW_TYPE, CKB_OUT_POINT_TYPE,
    CKB_RELATIVE_BLOCK_SINCE_TYPE, CKB_RELATIVE_EPOCH_SINCE_TYPE, CKB_RELATIVE_TIMESTAMP_SINCE_TYPE, CKB_SCRIPT_ARGS_TYPE,
    CKB_SCRIPT_HASH_TYPE, CKB_SCRIPT_VALUE_TYPE, CKB_SCRIPT_VIEW_TYPE, CKB_SIGHASH_ALL_DIGEST_TYPE, CKB_TIMESTAMP_MILLIS_TYPE,
    CKB_WITNESS_ARGS_VIEW_TYPE, CKB_WITNESS_BYTES_MAX, CKB_WITNESS_BYTES_VIEW_TYPE,
};
use crate::ast::{parse_bounded_collection_type, BoundedCollectionKind, OpenHandleClass, OpenHandleType, Type, TypeArgument};
use crate::error::{CompileError, Result, Span};
use std::collections::HashSet;

impl TypeChecker<'_> {
    pub(super) fn validate_type(&self, ty: &Type) -> Result<()> {
        crate::ir::validate_fixed_layout(&crate::ir::ast_type_to_ir(ty))?;
        if let Type::Selection(selection) = ty {
            if !selection.is_well_formed() || selection.schemas.iter().any(|schema| self.resolve_cell_type_kind(schema).is_none()) {
                return Err(CompileError::without_span("ExactCellSelection requires 1–8 resolved nominal Cell schemas")
                    .with_code(crate::error::DiagnosticCode::E2111));
            }
            if let Some(catalog) = &selection.catalog {
                let declaration = self
                    .cell_role_catalogs
                    .get(catalog)
                    .ok_or_else(|| CompileError::without_span("selection refers to an unknown local catalog"))?;
                let mut root = (**selection).clone();
                root.selected = None;
                if root != crate::cell_selection::SelectionType::from_contract(&crate::cell_selection::named_contract(declaration)?) {
                    return Err(CompileError::without_span("selection role names or schemas differ from their catalog"));
                }
            }
            return Ok(());
        }
        if let Type::Applied(application) = ty
            && application.constructor == CKB_WITNESS_BYTES_VIEW_TYPE
        {
            if self.type_fields.contains_key(CKB_WITNESS_BYTES_VIEW_TYPE)
                || self.enum_variants.contains_key(CKB_WITNESS_BYTES_VIEW_TYPE)
                || self
                    .resolver
                    .zip(self.current_module.as_deref())
                    .is_some_and(|(resolver, module)| resolver.resolve_type_identity(module, CKB_WITNESS_BYTES_VIEW_TYPE).is_some())
            {
                return Err(CompileError::without_span("a locally bound declaration cannot masquerade as a bounded witness view")
                    .with_code(crate::error::DiagnosticCode::E2111));
            }
            return if Self::witness_bytes_view_parts(ty).is_some() {
                Ok(())
            } else {
                Err(CompileError::without_span(format!(
                    "WitnessBytesView requires an owner raw, lock, entry, or output_type and an unsigned maximum in 0..={CKB_WITNESS_BYTES_MAX}"
                ))
                .with_code(crate::error::DiagnosticCode::E2111))
            };
        }
        if let Type::Applied(application) = ty
            && let Some(class) = OpenHandleClass::from_constructor(&application.constructor)
        {
            let handle = ty.as_open_handle().ok_or_else(|| {
                CompileError::without_span(format!(
                    "type '{}' requires exactly one imported module interface designation",
                    class.constructor()
                ))
                .with_code(crate::error::DiagnosticCode::E2111)
            })?;
            return self.validate_open_handle_type(&handle);
        }
        if matches!(ty, Type::Applied(application) if application.constructor == "ExactCellSelection") {
            return Err(CompileError::without_span("ExactCellSelection must be constructed locally from a checked catalog, never supplied through a declared type application")
                .with_code(crate::error::DiagnosticCode::E2111));
        }
        if let Some(view) = ty.as_context_view() {
            if self.type_fields.contains_key(view.constructor())
                || self.enum_variants.contains_key(view.constructor())
                || self
                    .resolver
                    .zip(self.current_module.as_deref())
                    .and_then(|(resolver, module)| resolver.resolve_type(module, view.constructor()))
                    .is_some()
            {
                return Err(CompileError::without_span("a locally bound declaration cannot masquerade as a builtin transaction view")
                    .with_code(crate::error::DiagnosticCode::E2111));
            }
            if let Some(schema) = view.schema() {
                let Type::Named(name) = schema else {
                    return Err(CompileError::without_span(format!(
                        "type '{}' requires a named cell type argument",
                        view.constructor()
                    ))
                    .with_code(crate::error::DiagnosticCode::E2111));
                };
                self.validate_type(schema)?;
                if self.resolve_cell_type_kind(name).is_none() {
                    return Err(CompileError::without_span(format!(
                        "type '{}' requires a cell-backed resource, shared type, or receipt argument",
                        view.constructor()
                    ))
                    .with_code(crate::error::DiagnosticCode::E2111));
                }
            }
            return Ok(());
        }
        if let Some(collection) = parse_bounded_collection_type(ty) {
            let name = ty.source_text();
            let constructor = collection.kind.constructor();
            if self.type_fields.contains_key(constructor)
                || self.enum_variants.contains_key(constructor)
                || self
                    .resolver
                    .zip(self.current_module.as_deref())
                    .and_then(|(resolver, module)| resolver.resolve_type(module, constructor))
                    .is_some()
            {
                return Err(CompileError::without_span(
                    "a locally bound declaration cannot masquerade as a builtin bounded collection",
                )
                .with_code(crate::error::DiagnosticCode::E2111));
            }
            self.validate_type(&collection.element_type)?;
            if collection.max_elements == 0 || collection.max_elements > 1024 {
                return Err(CompileError::new(format!("type '{}' requires a maximum cardinality in 1..=1024", name), Span::default())
                    .with_code(crate::error::DiagnosticCode::E2111));
            }
            let element_ty = &collection.element_type;
            match collection.kind {
                BoundedCollectionKind::CellSet => {
                    let Type::Named(element_name) = element_ty else {
                        return Err(CompileError::new(
                            format!("type '{}' requires a named cell-backed element type", name),
                            Span::default(),
                        )
                        .with_code(crate::error::DiagnosticCode::E2111));
                    };
                    match self.resolve_cell_type_kind(element_name) {
                        Some(CellTypeKind::Resource) => {}
                        Some(CellTypeKind::Shared | CellTypeKind::Receipt) => {
                            return Err(CompileError::new(
                                format!(
                                    "type '{}' requires a linear resource element; shared and receipt Cell kinds are not accepted",
                                    name
                                ),
                                Span::default(),
                            )
                            .with_code(crate::error::DiagnosticCode::E2111));
                        }
                        None => {
                            return Err(CompileError::new(
                                format!("type '{}' requires a cell-backed linear resource element", name),
                                Span::default(),
                            )
                            .with_code(crate::error::DiagnosticCode::E2111));
                        }
                    }
                }
                BoundedCollectionKind::List => {
                    if Self::base_type_name(element_ty).and_then(|element| self.resolve_cell_type_kind(element)).is_some() {
                        return Err(CompileError::new(
                            format!("type '{}' cannot store a cell-backed linear value; use BoundedCellSet<T, N>", name),
                            Span::default(),
                        )
                        .with_code(crate::error::DiagnosticCode::E2111));
                    }
                    if !self.bounded_list_element_is_fixed_width(element_ty, &mut HashSet::new()) {
                        return Err(CompileError::new(
                            format!("type '{}' requires a fixed-width element type in the first 0.22 surface", name),
                            Span::default(),
                        )
                        .with_code(crate::error::DiagnosticCode::E2111));
                    }
                }
            }
            return Ok(());
        }

        if let Some(application) = ty.application()
            && matches!(application.constructor.as_str(), "Commitment" | "Opening")
        {
            let constructor = application.constructor.as_str();
            let Some(value) = application.unary_type(constructor) else {
                return Err(CompileError::without_span(format!(
                    "type '{constructor}' requires exactly one fixed-width value type argument"
                ))
                .with_code(crate::error::DiagnosticCode::E2111));
            };
            if self.type_fields.contains_key(constructor)
                || self.enum_variants.contains_key(constructor)
                || self
                    .resolver
                    .zip(self.current_module.as_deref())
                    .and_then(|(resolver, module)| resolver.resolve_type(module, constructor))
                    .is_some()
            {
                return Err(CompileError::without_span(
                    "a locally bound declaration cannot masquerade as a builtin committed-state wrapper",
                )
                .with_code(crate::error::DiagnosticCode::E2111));
            }
            return self.validate_committed_value_type(value, Span::default());
        }
        if let Some(nested) = self.nested_descriptions.source(ty)? {
            let constructor = &ty.application().expect("source nested application").constructor;
            if self.type_fields.contains_key(constructor)
                || self.enum_variants.contains_key(constructor)
                || self
                    .resolver
                    .zip(self.current_module.as_deref())
                    .and_then(|(resolver, module)| resolver.resolve_type(module, constructor))
                    .is_some()
            {
                return Err(CompileError::without_span("a locally bound declaration cannot masquerade as a builtin nested view")
                    .with_code(crate::error::DiagnosticCode::E2111));
            }
            return self.validate_nested_type(&nested);
        }
        match ty {
            Type::Applied(application) if matches!(application.constructor.as_str(), "BoundedCellSet" | "BoundedList") => {
                Err(CompileError::without_span(format!(
                    "type '{}' requires exactly one element type and an explicit maximum cardinality in 1..=1024",
                    ty.source_text()
                ))
                .with_code(crate::error::DiagnosticCode::E2111))
            }
            Type::Applied(application) if application.constructor == "Vec" => {
                let Some(element) = application.unary_type("Vec") else {
                    return Err(CompileError::without_span("Vec requires exactly one type argument")
                        .with_code(crate::error::DiagnosticCode::E2111));
                };
                if self.type_fields.contains_key("Vec")
                    || self.enum_variants.contains_key("Vec")
                    || self
                        .resolver
                        .zip(self.current_module.as_deref())
                        .and_then(|(resolver, module)| resolver.resolve_type(module, "Vec"))
                        .is_some()
                {
                    return Err(CompileError::without_span("a locally bound Vec declaration is not the builtin vector constructor")
                        .with_code(crate::error::DiagnosticCode::E2111));
                }
                self.validate_type(element)?;
                if self.type_contains_cell_backed_value(element) {
                    return Err(CompileError::without_span(format!(
                        "type '{}' cannot store a cell-backed resource; use a source-aware BoundedCellSet<T, N> with explicit ownership",
                        ty.source_text()
                    )));
                }
                if self.type_contains_reference(element) {
                    return Err(CompileError::without_span(format!(
                        "type '{}' cannot contain reference type; Vec<T> values must use owned non-reference items",
                        ty.source_text()
                    )));
                }
                Ok(())
            }
            Type::Applied(application) => {
                // Source checking must fail at the same unfinished family
                // boundary as executable admission, including metadata-only
                // compilation. Migrate each family rather than accepting a
                // constructor by spelling or silently ignoring its arguments.
                Err(CompileError::without_span(format!(
                    "type application '{}' has no local template available and no admitted builtin family description",
                    application.constructor
                ))
                .with_code(crate::error::DiagnosticCode::E2111))
            }
            Type::Unit => Ok(()),
            Type::Array(elem_ty, length) => {
                self.validate_type(elem_ty)?;
                if let Some(width) = self.payload_type_runtime_width(elem_ty, &mut HashSet::new()) {
                    crate::ir::checked_fixed_size(width.checked_mul(*length))?;
                }
                Ok(())
            }
            Type::Tuple(types) => {
                let mut minimum_fixed_bytes = 0usize;
                for t in types {
                    self.validate_type(t)?;
                    if let Some(width) = self.payload_type_runtime_width(t, &mut HashSet::new()) {
                        minimum_fixed_bytes = crate::ir::checked_fixed_size(minimum_fixed_bytes.checked_add(width))?;
                    }
                }
                Ok(())
            }
            Type::Ref(inner) | Type::MutRef(inner) => self.validate_type(inner),
            Type::Named(name) => self.validate_named_type(name),
            _ => Ok(()),
        }
    }

    pub(super) fn validate_interface_argument_roles(&self, ty: &Type, depth: usize, context: OpenHandleRoleContext<'_>) -> Result<()> {
        let mut pending = vec![(ty, depth)];
        while let Some((ty, depth)) = pending.pop() {
            // Retained specialization arguments keep their existing bound.
            // Source role scanning does not admit or restrict unrelated types.
            if depth > 32 && matches!(context, OpenHandleRoleContext::Retained) {
                return Err(CompileError::without_span("specialization argument nesting exceeds 32")
                    .with_code(crate::error::DiagnosticCode::E2111));
            }
            match ty {
                Type::Applied(application) if OpenHandleClass::from_constructor(&application.constructor).is_some() => {
                    match ty.as_open_handle() {
                        Some(handle) => self.validate_open_handle_role(&handle, context)?,
                        None => self.validate_type(ty)?,
                    }
                }
                Type::Applied(application) => {
                    pending.extend(application.arguments.iter().rev().filter_map(TypeArgument::as_type).map(|ty| (ty, depth + 1)));
                }
                Type::Array(inner, _) | Type::Ref(inner) | Type::MutRef(inner) => pending.push((inner.as_ref(), depth + 1)),
                Type::Tuple(items) => pending.extend(items.iter().rev().map(|ty| (ty, depth + 1))),
                _ => {}
            }
        }
        Ok(())
    }

    /// Materialized types retain a module designation. Source spelling roles
    /// are checked in the original module before generic substitution.
    fn validate_open_handle_type(&self, handle: &OpenHandleType) -> Result<()> {
        self.validate_open_handle_role(handle, OpenHandleRoleContext::Retained)
    }

    fn validate_open_handle_role(&self, handle: &OpenHandleType, context: OpenHandleRoleContext<'_>) -> Result<()> {
        let base_name = handle.class.constructor();
        let argument = handle.interface.as_str();
        if !handle.has_bounded_module_path() {
            return Err(CompileError::new(
                format!("interface parameter of '{base_name}' must be a plain module path"),
                Span::default(),
            ));
        }
        if matches!(
            argument,
            "u8" | "u16"
                | "u32"
                | "i32"
                | "u64"
                | "u128"
                | "bool"
                | "unit"
                | "address"
                | "hash"
                | "usize"
                | "isize"
                | "String"
                | "Vec"
                | "Script"
        ) {
            return Err(CompileError::new(
                format!("'{argument}' is a value type and cannot designate a module interface"),
                Span::default(),
            ));
        }
        if let OpenHandleRoleContext::Source(parameters) = context
            && parameters.contains(argument)
        {
            return Err(CompileError::without_span(format!(
                "type parameter '{argument}' cannot designate a module interface; use an imported module path"
            ))
            .with_code(crate::error::DiagnosticCode::E2111));
        }
        let local = self.current_module.as_deref().map(|module| {
            self.type_fields.contains_key(argument)
                || self.enum_variants.contains_key(argument)
                || self.resolver.is_some_and(|resolver| resolver.resolve_type_identity(module, argument).is_some())
        });
        if matches!(context, OpenHandleRoleContext::Source(_)) && local == Some(true) {
            return Err(CompileError::new(
                format!("'{argument}' is a locally visible type; an interface parameter must designate an imported module"),
                Span::default(),
            ));
        }
        let (Some(resolver), Some(current)) = (self.resolver, self.current_module.as_deref()) else {
            return Err(CompileError::new(
                "an interface designation requires the project resolver; single-file compilation cannot bind it",
                Span::default(),
            ));
        };
        if matches!(context, OpenHandleRoleContext::Source(_)) && argument == current {
            return Err(CompileError::new(
                "designate an imported module's interface; the current module's own interface is implicit",
                Span::default(),
            ));
        }
        if resolver.module(argument).is_none() {
            return Err(CompileError::new(
                format!("module '{argument}' is not loaded; an interface designation must name an imported module"),
                Span::default(),
            ));
        }
        Ok(())
    }

    pub(super) fn validate_named_type(&self, name: &str) -> Result<()> {
        if self.collection_pools.contains_key(name) {
            return Ok(());
        }
        if crate::cell_selection::contains_selection(&Type::Named(name.to_string())) {
            return Err(CompileError::without_span("ExactCellSelection requires a retained invocation-local catalog description")
                .with_code(crate::error::DiagnosticCode::E2111));
        }
        if name.contains(['<', '>']) {
            return Err(CompileError::without_span("type applications must retain their structured arguments")
                .with_code(crate::error::DiagnosticCode::E2111));
        }
        let base_name = name;
        match base_name {
            "Option" | "Result" => {
                return Err(CompileError::new(
                    format!("type '{}' is reserved for the explicit error model but is not implemented yet", base_name),
                    Span::default(),
                ));
            }
            _ => {}
        }

        if matches!(
            base_name,
            crate::script_handle_contract::OPEN_SCRIPT_HANDLE_TYPE
                | crate::script_handle_contract::OPEN_VERIFIER_HANDLE_TYPE
                | crate::script_handle_contract::INSTANCE_SCRIPT_HANDLE_TYPE
        ) {
            return Err(CompileError::without_span(format!(
                "type '{base_name}' requires a structured imported module interface designation"
            ))
            .with_code(crate::error::DiagnosticCode::E2111));
        }

        if matches!(base_name, crate::commitment_contract::COMMITMENT_TYPE | crate::commitment_contract::OPENING_TYPE) {
            return Err(CompileError::new(
                format!("type '{}' requires exactly one fixed-width value type argument", base_name),
                Span::default(),
            ));
        }

        if matches!(base_name, "BoundedCellSet" | "BoundedList") {
            return Err(CompileError::new(
                format!(
                    "type '{}' requires exactly one element type and an explicit maximum cardinality, for example {}<T, 16>",
                    name, base_name
                ),
                Span::default(),
            ));
        }

        if matches!(base_name, CKB_INPUT_VIEW_TYPE | CKB_OUTPUT_VIEW_TYPE | "CellDataView") {
            return Err(CompileError::new(format!("type '{}' requires one retained cell type argument", base_name), Span::default())
                .with_code(crate::error::DiagnosticCode::E2111));
        }

        if base_name == CKB_WITNESS_BYTES_VIEW_TYPE {
            return Err(CompileError::without_span("bounded witness views require retained owner and maximum arguments")
                .with_code(crate::error::DiagnosticCode::E2111));
        }

        match base_name {
            "String"
            | "Range"
            | "Vec"
            | "usize"
            | "isize"
            | CKB_SCRIPT_ARGS_TYPE
            | CKB_SCRIPT_VALUE_TYPE
            | CKB_CELL_DEP_VIEW_TYPE
            | CKB_HEADER_DEP_VIEW_TYPE
            | CKB_WITNESS_ARGS_VIEW_TYPE
            | CKB_WITNESS_BYTES_VIEW_TYPE
            | CKB_OUT_POINT_TYPE
            | CKB_SCRIPT_VIEW_TYPE
            | CKB_SCRIPT_HASH_TYPE
            | "ZkTransitionProof"
            | CKB_EXACT_SCRIPT_HANDLE_TYPE
            | CKB_DEPLOYMENT_LINE_HANDLE_TYPE
            | CKB_SIGHASH_ALL_DIGEST_TYPE
            | CKB_EPOCH_NUMBER_TYPE
            | CKB_EPOCH_DURATION_TYPE
            | CKB_BLOCK_NUMBER_TYPE
            | CKB_EPOCH_LENGTH_TYPE
            | CKB_TIMESTAMP_MILLIS_TYPE
            | CKB_ENCODED_SINCE_TYPE
            | CKB_DECODED_SINCE_TYPE
            | CKB_ABSOLUTE_BLOCK_SINCE_TYPE
            | CKB_ABSOLUTE_EPOCH_SINCE_TYPE
            | CKB_ABSOLUTE_TIMESTAMP_SINCE_TYPE
            | CKB_RELATIVE_BLOCK_SINCE_TYPE
            | CKB_RELATIVE_EPOCH_SINCE_TYPE
            | CKB_RELATIVE_TIMESTAMP_SINCE_TYPE
            | "Since"
            | "Absolute"
            | "Relative"
            | "EpochFraction" => return Ok(()),
            _ => {}
        }

        if self.type_fields.contains_key(base_name)
            || self.enum_variants.contains_key(base_name)
            || self.cell_type_kinds.contains_key(base_name)
            || self
                .resolver
                .zip(self.current_module.as_deref())
                .and_then(|(resolver, module)| resolver.resolve_type(module, base_name))
                .is_some()
        {
            Ok(())
        } else {
            Err(CompileError::new(format!("unknown type '{}'", name), Span::default()))
        }
    }

    pub(super) fn validate_committed_value_type(&self, ty: &Type, span: Span) -> Result<()> {
        if matches!(ty, Type::Unit | Type::Ref(_) | Type::MutRef(_))
            || self.type_contains_reference(ty)
            || self.type_contains_opening(ty)
            || self.type_contains_commitment(ty)
            || self.type_contains_cell_backed_value(ty)
        {
            return Err(CompileError::new(
                format!("committed-state profile requires an owned, non-Cell, non-wrapper fixed-width value; found {}", type_repr(ty)),
                span,
            ));
        }
        self.validate_type(ty)?;
        let width = self.payload_type_runtime_width(ty, &mut HashSet::new()).ok_or_else(|| {
            CompileError::new(
                format!("committed-state type {} does not have a concrete fixed-width packed layout", type_repr(ty)),
                span,
            )
        })?;
        let preimage_len = crate::commitment_contract::PACKED_HASH_DOMAIN_PREFIX
            .len()
            .checked_add(type_repr(ty).len())
            .and_then(|len| len.checked_add(1 + 4))
            .and_then(|len| len.checked_add(width))
            .ok_or_else(|| CompileError::new("committed-state packed width overflow", span))?;
        if width == 0 || preimage_len + 32 > crate::commitment_contract::PACKED_HASH_SCRATCH_BYTES {
            return Err(CompileError::new(
                format!(
                    "committed-state type {} requires {} packed bytes; the fixed-width profile permits preimage plus digest within {} bytes",
                    type_repr(ty),
                    width,
                    crate::commitment_contract::PACKED_HASH_SCRATCH_BYTES
                ),
                span,
            ));
        }
        Ok(())
    }

    pub(super) fn bounded_list_element_is_fixed_width(&self, ty: &Type, visiting: &mut HashSet<String>) -> bool {
        match ty {
            Type::Applied(_) => false,
            Type::U8 | Type::U16 | Type::U32 | Type::I32 | Type::U64 | Type::U128 | Type::Bool | Type::Address | Type::Hash => true,
            Type::Array(inner, _) => self.bounded_list_element_is_fixed_width(inner, visiting),
            Type::Tuple(items) => items.iter().all(|item| self.bounded_list_element_is_fixed_width(item, visiting)),
            Type::Named(name) => {
                let base = name.as_str();
                if matches!(base, "String" | "Vec" | "BoundedList" | "BoundedCellSet") || !visiting.insert(base.to_string()) {
                    return false;
                }
                let fixed = if self.resolve_enum_variant_fields(base).is_some() {
                    self.payload_type_runtime_width(ty, &mut HashSet::new()).is_some()
                        && !self.enum_type_contains_linear_payload(ty, &mut HashSet::new())
                } else {
                    self.resolve_named_type_fields(base)
                        .is_some_and(|fields| fields.values().all(|field| self.bounded_list_element_is_fixed_width(field, visiting)))
                };
                visiting.remove(base);
                fixed
            }
            Type::Selection(_) | Type::Unit | Type::Ref(_) | Type::MutRef(_) => false,
        }
    }

    pub(super) fn types_equal(&self, a: &Type, b: &Type) -> bool {
        if let (Some(a), Some(b)) = (self.source_nested_type(a), self.source_nested_type(b)) {
            return match (
                self.nested_descriptions.resolve(&a, |element| self.nested_record(element)),
                self.nested_descriptions.resolve(&b, |element| self.nested_record(element)),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            };
        }
        match (a, b) {
            (Type::U8, Type::U8) => true,
            (Type::U16, Type::U16) => true,
            (Type::U32, Type::U32) => true,
            (Type::I32, Type::I32) => true,
            (Type::U64, Type::U64) => true,
            (Type::U128, Type::U128) => true,
            (Type::Bool, Type::Bool) => true,
            (Type::Unit, Type::Unit) => true,
            (Type::Unit, Type::Tuple(items)) | (Type::Tuple(items), Type::Unit) => items.is_empty(),
            (Type::Address, Type::Address) => true,
            (Type::Hash, Type::Hash) => true,
            (Type::Array(a1, n1), Type::Array(b1, n2)) => n1 == n2 && self.types_equal(a1, b1),
            (Type::Tuple(a1), Type::Tuple(b1)) => {
                a1.len() == b1.len() && a1.iter().zip(b1.iter()).all(|(x, y)| self.types_equal(x, y))
            }
            (Type::Named(a1), Type::Named(b1)) => match (self.nominal_identities.get(a1), self.nominal_identities.get(b1)) {
                (Some(Some(a)), Some(Some(b))) => a == b,
                (None, None) => a1 == b1,
                _ => false,
            },
            (Type::Applied(a1), Type::Applied(b1)) => {
                a1.constructor == b1.constructor
                    && (!matches!(a1.constructor.as_str(), "Commitment" | "Opening") || a.source_text() == b.source_text())
                    && a1.arguments.len() == b1.arguments.len()
                    && a1.arguments.iter().zip(&b1.arguments).all(|(a, b)| match (a, b) {
                        (TypeArgument::Type(a), TypeArgument::Type(b)) => self.types_equal(a, b),
                        (TypeArgument::UnsignedConstant(a), TypeArgument::UnsignedConstant(b)) => a == b,
                        _ => false,
                    })
            }
            (Type::Ref(a1), Type::Ref(b1)) => self.types_equal(a1, b1),
            (Type::MutRef(a1), Type::MutRef(b1)) => self.types_equal(a1, b1),
            _ => false,
        }
    }
}
