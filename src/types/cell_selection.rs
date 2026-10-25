use super::*;

impl TypeChecker<'_> {
    pub(super) fn cell_data_view_field_type(&self, ty: &Type, field: &str, span: Span) -> Result<Option<Type>> {
        let Type::Named(name) = ty else { return Ok(None) };
        let projection = field == "data" && crate::cell_data_view::transaction_schema(name).is_some();
        let schema = if projection { crate::cell_data_view::transaction_schema(name) } else { crate::cell_data_view::schema(name) };
        let Some(schema) = schema else { return Ok(None) };
        let fields = self
            .type_fields
            .get(schema)
            .cloned()
            .or_else(|| {
                let (resolver, module) = self.resolver.zip(self.current_module.as_deref())?;
                let fields = match resolver.resolve_type(module, schema)? {
                    TypeDef::Resource(value) => value.fields,
                    TypeDef::Shared(value) => value.fields,
                    TypeDef::Receipt(value) => value.fields,
                    _ => return None,
                };
                Some(fields.into_iter().map(|field| (field.name, field.ty)).collect())
            })
            .ok_or_else(|| CompileError::new("Cell data view requires a resolved Cell schema", span))?;
        let bytes = fields.values().try_fold(0usize, |total, ty| {
            crate::nested_values::fixed_field_width(ty)
                .filter(|width| *width > 0)
                .and_then(|width| total.checked_add(usize::from(width)))
        });
        let mut table_bytes = Some(4 * (fields.len() + 1));
        let mut nested = false;
        for ty in fields.values() {
            if let Type::Named(name) = ty
                && let Some(kind @ crate::nested_values::NestedType::Values(_)) = crate::nested_values::parsed(name)
            {
                self.validate_nested_type(&kind)?;
                let crate::nested_values::NestedType::Values(values) = kind else { unreachable!() };
                table_bytes = table_bytes.and_then(|bytes| bytes.checked_add(values.max_total_bytes as usize));
                nested = true;
            } else if let Some(width) = crate::nested_values::fixed_field_width(ty).filter(|width| *width > 0) {
                table_bytes = table_bytes.and_then(|bytes| bytes.checked_add(usize::from(width)));
            } else {
                table_bytes = None;
            }
        }
        if !(bytes.is_some_and(|bytes| (1..=512).contains(&bytes)) || nested && table_bytes.is_some_and(|bytes| bytes <= 32768)) {
            return Err(CompileError::new(
                "Cell data views require a fixed 1–512 byte schema or a canonical bounded nested table of at most 32768 bytes",
                span,
            ));
        }
        if projection {
            Ok(Some(Type::Named(format!("CellDataView<{schema}>"))))
        } else {
            fields
                .get(field)
                .cloned()
                .map(Some)
                .ok_or_else(|| CompileError::new(format!("Cell data schema '{schema}' has no field '{field}'"), span))
        }
    }

    pub(super) fn infer_named_role_method(&self, receiver: &Type, call: &CallExpr, arg_types: &[Type]) -> Result<Option<Type>> {
        if let Expr::FieldAccess(method) = call.func.as_ref()
            && let Type::Named(owner) = receiver
            && let Some(selection) = crate::cell_selection::parse_selection_type(owner)
        {
            let role =
                selection.selected.ok_or_else(|| CompileError::new("select a named role before reading its Cells", call.span))?;
            let read = crate::cell_selection::method(&method.field).ok_or_else(|| {
                CompileError::new("selected role supports input_count, output_count, input, output and successor", call.span)
            })?;
            if !call.type_args.is_empty() || call.args.len() != usize::from(read.indexed()) {
                return Err(CompileError::new(
                    "named role reads derive their schema from the catalog and take only a u64 ordinal for indexed reads",
                    call.span,
                ));
            }
            if read.indexed() && arg_types[0] != Type::U64 {
                return Err(CompileError::new("selected Cell ordinal must be u64", call.span));
            }
            return Ok(Some(if read.indexed() {
                Type::Named(read.result_type(&selection.schemas[role as usize]))
            } else {
                Type::U64
            }));
        }
        Ok(None)
    }

    pub(super) fn infer_exact_role_verification(&mut self, env: &mut TypeEnv, call: &CallExpr) -> Result<Option<Type>> {
        let Expr::Identifier(name) = call.func.as_ref() else { return Ok(None) };
        if let Some(read) = crate::cell_selection::read(name) {
            let arity = if read.indexed() { 3 } else { 2 };
            if call.args.len() != arity || call.type_args.len() != 1 {
                return Err(CompileError::new("selected-role reads require one Cell type, a selection, a literal role ordinal and (for indexed reads) a u64 ordinal", call.span));
            }
            let Type::Named(selection) = self.infer_expr(env, &call.args[0])? else {
                return Err(CompileError::new("selected-role read requires an ExactCellSelection", call.span));
            };
            let Some(selection) = crate::cell_selection::parse_selection_type(&selection) else {
                return Err(CompileError::new("selected-role read requires an ExactCellSelection", call.span));
            };
            if selection.catalog.is_some() {
                return Err(CompileError::new(
                    "named catalogs use selection.role.input/output/successor instead of numeric role arguments",
                    call.span,
                ));
            }
            let roles = &selection.schemas;
            let Expr::Integer(role) = call.args[1] else {
                return Err(CompileError::new("selected role ordinal must be a source integer literal", call.span));
            };
            let element = usize::try_from(role)
                .ok()
                .and_then(|role| roles.get(role))
                .ok_or_else(|| CompileError::new("selected role ordinal is outside its catalog", call.span))?;
            if call.type_args[0] != Type::Named(element.clone()) {
                return Err(CompileError::new("selected role Cell type differs from its nominal catalog schema", call.span));
            }
            if read.indexed() && self.infer_expr(env, &call.args[2])? != Type::U64 {
                return Err(CompileError::new("selected Cell ordinal must be u64", call.span));
            }
            return Ok(Some(if read.indexed() { Type::Named(read.result_type(element)) } else { Type::U64 }));
        }
        if name == crate::cell_selection::ROLE {
            return Err(CompileError::new(
                "ckb::exact_cell_role is a compile-time declaration usable only inline in ckb::verify_cell_roles or ckb::select_cell_roles",
                call.span,
            ));
        }
        if name != crate::cell_selection::VERIFY && name != crate::cell_selection::SELECT {
            return Ok(None);
        }
        if !matches!(self.current_callable, Some(CallableKind::Action | CallableKind::Lock)) {
            return Err(CompileError::new("exact-role verification requires an action or lock entry", call.span));
        }
        let contract = crate::cell_selection::resolve_call(call, &self.cell_role_catalogs)?;
        self.validate_cell_role_contract(&contract, call.span)?;
        Ok(Some(if name == crate::cell_selection::SELECT {
            Type::Named(crate::cell_selection::selection_type(&contract))
        } else {
            Type::Unit
        }))
    }

    pub(super) fn validate_cell_role_contract(&self, contract: &crate::cell_selection::Contract, span: Span) -> Result<()> {
        contract.validate().map_err(|message| CompileError::new(message, span))?;
        for role in &contract.roles {
            if self.resolve_cell_type_kind(&role.type_name).is_none() {
                return Err(CompileError::new(
                    format!("exact role '{}' must name a Cell-backed resource, shared type or receipt", role.type_name),
                    span,
                ));
            }
            let fields = self
                .type_fields
                .get(&role.type_name)
                .cloned()
                .or_else(|| {
                    let (resolver, module) = self.resolver.zip(self.current_module.as_deref())?;
                    let fields = match resolver.resolve_type(module, &role.type_name)? {
                        TypeDef::Resource(value) => value.fields,
                        TypeDef::Shared(value) => value.fields,
                        TypeDef::Receipt(value) => value.fields,
                        _ => return None,
                    };
                    Some(fields.into_iter().map(|field| (field.name, field.ty)).collect())
                })
                .ok_or_else(|| CompileError::new("exact role has no resolved Cell schema", span))?;
            let mut total = 0usize;
            let mut nested = false;
            for ty in fields.values() {
                let width = if let Type::Named(name) = ty
                    && let Some(kind @ crate::nested_values::NestedType::Values(_)) = crate::nested_values::parsed(name)
                {
                    self.validate_nested_type(&kind)?;
                    let crate::nested_values::NestedType::Values(values) = kind else { unreachable!() };
                    nested = true;
                    values.max_total_bytes as usize
                } else {
                    usize::from(crate::nested_values::fixed_field_width(ty).filter(|width| *width > 0)
                        .ok_or_else(|| CompileError::new("fixed exact roles require unsigned scalars, Hash, Address or fixed byte-array fields; bounded tables additionally admit direct NestedValues fields", span))?)
                };
                total = total.checked_add(width).ok_or_else(|| CompileError::new("exact-role layout size overflow", span))?;
            }
            let key = fields
                .get(&role.identity_field)
                .and_then(crate::nested_values::fixed_field_width)
                .filter(|width| (1..=32).contains(width))
                .ok_or_else(|| CompileError::new("exact-role identity field must exist and have a fixed width of 1–32 bytes", span))?;
            if nested {
                total = total
                    .checked_add(4 * (fields.len() + 1))
                    .ok_or_else(|| CompileError::new("exact-role table bound overflows", span))?;
                if total > 32768 {
                    return Err(CompileError::new("bounded exact-role table capacity must not exceed 32768 bytes", span));
                }
            } else if !(1..=512).contains(&total) {
                return Err(CompileError::new("fixed exact-role Cell size must be in 1–512 bytes", span));
            }
            if matches!(&role.correspondence, crate::cell_selection::Correspondence::Merge(target) if target.len() != usize::from(key))
            {
                return Err(CompileError::new("merge target byte length must equal the declared identity field width", span));
            }
        }
        Ok(())
    }
}
