//! Parse compile-time exact-role policy arguments. Byte offsets and schema
//! widths are never supplied by source; later phases derive them from types.
use crate::{
    ast::{CallExpr, Expr, Type},
    error::{CompileError, Result, Span},
};
pub use cellscript_artifact_checker::cell_selection::{
    parse_selection_type, project_role_type, selection_type, CatalogNames, Contract, Correspondence, Read, Role, BOUNDED_PROFILE,
    PROFILE,
};

pub const VERIFY: &str = "ckb::verify_cell_roles";
pub const SELECT: &str = "ckb::select_cell_roles";
pub const ROLE: &str = "ckb::exact_cell_role";

pub(crate) fn resolve_call(
    call: &CallExpr,
    catalogs: &std::collections::HashMap<String, crate::ast::CellRolesDef>,
) -> Result<Contract> {
    if call.type_args.is_empty() {
        return parse(call);
    }
    let [Type::Named(name)] = call.type_args.as_slice() else {
        return Err(CompileError::new("named role verification requires one catalog name", call.span));
    };
    if !call.args.is_empty() {
        return Err(CompileError::new(
            "named role verification takes its limits and policies from the catalog, without runtime arguments",
            call.span,
        ));
    }
    let catalog =
        catalogs.get(name).ok_or_else(|| CompileError::new(format!("unknown local cell_roles catalog '{name}'"), call.span))?;
    named_contract(catalog)
}

/// Bind source role names into the exact typed runtime contract.
/// Declaration resolution uses the same schema/policy checks as the primitive.
pub(crate) fn named_contract(declaration: &crate::ast::CellRolesDef) -> Result<Contract> {
    use crate::ast::{CellRoleAuthority, CellRoleCorrespondence};
    let roles = declaration
        .roles
        .iter()
        .map(|role| {
            let Type::Named(schema) = &role.schema else {
                return Err(CompileError::new("a named role requires a Cell schema", role.span));
            };
            Ok(Role {
                type_name: schema.clone(),
                type_hash: role.type_hash,
                lock_hash: role.lock_hash,
                identity_field: role.identity_field.clone(),
                inputs: role.inputs,
                outputs: role.outputs,
                local_type: role.authority == CellRoleAuthority::LocalType,
                correspondence: match &role.correspondence {
                    CellRoleCorrespondence::Preserve => Correspondence::Preserve,
                    CellRoleCorrespondence::Create => Correspondence::Create,
                    CellRoleCorrespondence::Burn => Correspondence::Burn,
                    CellRoleCorrespondence::Merge(key) => Correspondence::Merge(key.clone()),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let limits = &declaration.limits;
    let contract = Contract {
        profile: PROFILE.into(),
        names: Some(CatalogNames {
            catalog: declaration.name.clone(),
            roles: declaration.roles.iter().map(|role| role.name.clone()).collect(),
        }),
        roles,
        input_scan: limits.input_scan,
        output_scan: limits.output_scan,
        selected_per_side: limits.selected_per_side,
        aggregate_bytes: limits.aggregate_bytes,
    };
    contract.validate().map_err(|message| CompileError::new(message, declaration.span))?;
    Ok(contract)
}

pub(crate) fn read(name: &str) -> Option<Read> {
    Some(match name {
        "ckb::selected_input_count" => Read::InputCount,
        "ckb::selected_output_count" => Read::OutputCount,
        "ckb::selected_input" => Read::Input,
        "ckb::selected_output" => Read::Output,
        "ckb::selected_successor" => Read::Successor,
        _ => return None,
    })
}

pub(crate) fn method(name: &str) -> Option<Read> {
    Some(match name {
        "input_count" => Read::InputCount,
        "output_count" => Read::OutputCount,
        "input" => Read::Input,
        "output" => Read::Output,
        "successor" => Read::Successor,
        _ => return None,
    })
}

pub(crate) fn contains_selection(ty: &Type) -> bool {
    match ty {
        Type::Named(name) => {
            name.split('<').next().is_some_and(|base| base.trim() == "ExactCellSelection")
                || crate::generics::applied_type(name)
                    .is_some_and(|(_, args)| args.iter().any(|arg| contains_selection(&Type::Named(arg.clone()))))
        }
        Type::Ref(inner) | Type::MutRef(inner) | Type::Array(inner, _) => contains_selection(inner),
        Type::Tuple(items) => items.iter().any(contains_selection),
        _ => false,
    }
}

/// Declare the actual complete scans and anchor reads, including the end probe.
/// These accesses describe execution; they do not discharge the machine proof.
pub(crate) fn runtime_accesses(contract: &Contract, layouts: &crate::MetadataTypeLayouts) -> Vec<crate::CkbRuntimeAccessMetadata> {
    use crate::{
        runtime_access_metadata, runtime_range_fixed, runtime_range_whole_value, runtime_scalar_bounded_scan,
        runtime_scalar_not_applicable, runtime_scalar_static,
    };
    let mut accesses = Vec::new();
    for (source, limit) in [("Input", contract.input_scan), ("Output", contract.output_scan)] {
        for (operation, width, maximum) in
            [("exact-role-count-end", 8, limit), ("exact-role-type-hash", 32, limit - 1), ("exact-role-lock-hash", 32, limit - 1)]
        {
            accesses.push(runtime_access_metadata(
                operation,
                "LOAD_CELL_BY_FIELD",
                source,
                VERIFY,
                source,
                "bounded-scan",
                None,
                runtime_scalar_bounded_scan(u64::from(maximum)),
                runtime_range_fixed(width),
            ));
        }
        for role in &contract.roles {
            let width = layouts
                .get(&role.type_name)
                .and_then(|fields| fields.values().try_fold(0usize, |total, field| total.checked_add(field.fixed_size?)));
            accesses.push(runtime_access_metadata(
                "exact-role-selected-data",
                "LOAD_CELL_DATA",
                source,
                &role.type_name,
                source,
                "bounded-scan",
                None,
                runtime_scalar_bounded_scan(u64::from(limit - 1)),
                width.map_or_else(runtime_range_whole_value, |width| runtime_range_fixed(width as u64)),
            ));
        }
    }
    accesses.push(runtime_access_metadata(
        "exact-role-current-script-hash",
        "LOAD_SCRIPT_HASH",
        "CurrentScript",
        VERIFY,
        "CurrentScript",
        "bounded-scan",
        None,
        runtime_scalar_not_applicable(),
        runtime_range_fixed(32),
    ));
    for (source, operation, width) in [
        ("GroupInput", "exact-role-anchor-capacity", 8),
        ("GroupInput", "exact-role-anchor-type-hash", 32),
        ("GroupInput", "exact-role-anchor-lock-hash", 32),
        ("GroupOutput", "exact-role-anchor-type-hash", 32),
        ("GroupOutput", "exact-role-anchor-lock-hash", 32),
    ] {
        accesses.push(runtime_access_metadata(
            operation,
            "LOAD_CELL_BY_FIELD",
            source,
            VERIFY,
            source,
            "bounded-scan",
            None,
            runtime_scalar_static(0, Some(0)),
            runtime_range_fixed(width),
        ));
    }
    accesses
}

fn text(expr: &Expr, what: &str, span: Span) -> Result<String> {
    match expr {
        Expr::String(value) => Ok(value.clone()),
        _ => Err(CompileError::new(format!("{what} must be a source string literal"), span)),
    }
}

fn number(expr: &Expr, what: &str, span: Span) -> Result<u32> {
    match expr {
        Expr::Integer(value) => u32::try_from(*value).map_err(|_| CompileError::new(format!("{what} is out of range"), span)),
        _ => Err(CompileError::new(format!("{what} must be a source integer literal"), span)),
    }
}

fn hash(expr: &Expr, span: Span) -> Result<[u8; 32]> {
    let value = text(expr, "complete Script hash", span)?;
    let bytes = value
        .strip_prefix("0x")
        .filter(|value| value.len() == 64)
        .and_then(|value| hex::decode(value).ok())
        .and_then(|value| value.try_into().ok());
    bytes.ok_or_else(|| CompileError::new("complete Script hash must be a 0x-prefixed 32-byte hex literal", span))
}

pub fn parse(call: &CallExpr) -> Result<Contract> {
    if !call.type_args.is_empty() || !(5..=12).contains(&call.args.len()) {
        return Err(CompileError::new(
            "ckb::verify_cell_roles and ckb::select_cell_roles expect four literal budgets followed by 1–8 inline ckb::exact_cell_role<T> declarations",
            call.span,
        ));
    }
    let mut roles = Vec::new();
    for declaration in &call.args[4..] {
        let Expr::Call(role) = declaration else {
            return Err(CompileError::new("exact roles must be inline declarations, not runtime-selected values", call.span));
        };
        if !matches!(role.func.as_ref(), Expr::Identifier(name) if name == ROLE)
            || role.type_args.len() != 1
            || !(9..=10).contains(&role.args.len())
        {
            return Err(CompileError::new("ckb::exact_cell_role<T> expects (type_hash, lock_hash, identity_field, access, correspondence, input_min, input_max, output_min, output_max[, merge_key])", role.span));
        }
        let Type::Named(type_name) = &role.type_args[0] else {
            return Err(CompileError::new("an exact role requires a named Cell type", role.span));
        };
        let local_type = match text(&role.args[3], "role access", role.span)?.as_str() {
            "observe" => false,
            "local-type" => true,
            _ => return Err(CompileError::new("exact-role access must be observe or local-type", role.span)),
        };
        let mode = text(&role.args[4], "correspondence", role.span)?;
        let correspondence = match (mode.as_str(), role.args.get(9)) {
            ("preserve", None) => Correspondence::Preserve,
            ("create", None) => Correspondence::Create,
            ("burn", None) => Correspondence::Burn,
            ("merge", Some(Expr::ByteString(key))) => Correspondence::Merge(key.clone()),
            _ => {
                return Err(CompileError::new(
                    "exact-role correspondence must be preserve/create/burn, or merge with a literal byte-string target key",
                    role.span,
                ))
            }
        };
        roles.push(Role {
            type_name: type_name.clone(),
            type_hash: hash(&role.args[0], role.span)?,
            lock_hash: hash(&role.args[1], role.span)?,
            identity_field: text(&role.args[2], "identity field", role.span)?,
            local_type,
            correspondence,
            inputs: (number(&role.args[5], "input minimum", role.span)?, number(&role.args[6], "input maximum", role.span)?),
            outputs: (number(&role.args[7], "output minimum", role.span)?, number(&role.args[8], "output maximum", role.span)?),
        });
    }
    let contract = Contract {
        profile: PROFILE.into(),
        names: None,
        roles,
        input_scan: number(&call.args[0], "input scan bound", call.span)?,
        output_scan: number(&call.args[1], "output scan bound", call.span)?,
        selected_per_side: number(&call.args[2], "selected Cell bound", call.span)?,
        aggregate_bytes: number(&call.args[3], "aggregate byte bound", call.span)?,
    };
    contract.validate().map_err(|message| CompileError::new(message, call.span))?;
    Ok(contract)
}

#[cfg(test)]
mod tests {
    fn source(role: &str, budgets: &str, fields: &str) -> String {
        format!("module exact_roles::typing\nresource Token has store {{ {fields} }}\naction verify() -> u64 {{\nverification\nckb::verify_cell_roles({budgets}, {role})\nreturn 0\n}}")
    }

    fn role() -> String {
        format!(
            "ckb::exact_cell_role<Token>(\"0x{}\", \"0x{}\", \"id\", \"observe\", \"preserve\", 0, 32, 0, 32)",
            "01".repeat(32),
            "02".repeat(32)
        )
    }

    #[test]
    fn exact_role_source_rejects_runtime_policy_and_unsupported_shapes() {
        let role = role();
        let budgets = "64, 64, 32, 32768";
        for (source, expected) in [
            (source(&role, "0, 64, 32, 32768", "id: u64"), "scan limits"),
            (source(&role, "64, 65, 32, 32768", "id: u64"), "scan limits"),
            (source(&role, "64, 64, 33, 32768", "id: u64"), "selected limit"),
            (source(&role, "64, 64, 32, 32769", "id: u64"), "aggregate bytes"),
            (source(&format!("{role}, {role}"), budgets, "id: u64"), "unique"),
            (source(&role.replace("\"id\"", "\"missing\""), budgets, "id: u64"), "identity field"),
            (source(&role, budgets, "id: u64, flag: bool"), "fixed exact roles"),
            (source(&role, budgets, "id: [u8; 33]"), "identity field"),
            (source(&role.replace("\"observe\"", "\"consume\""), budgets, "id: u64"), "observe or local-type"),
            (source(&role.replace("\"preserve\"", "\"merge\""), budgets, "id: u64"), "literal byte-string"),
        ] {
            let error = crate::compile(&source, crate::CompileOptions::default()).unwrap_err();
            assert!(error.message.contains(expected), "expected {expected}: {error}\n{source}");
        }
        let dynamic = source(&role.replace(&format!("\"0x{}\"", "01".repeat(32)), "chosen"), budgets, "id: u64")
            .replace("verify()", "verify(witness chosen: Hash)");
        let error = crate::compile(&dynamic, crate::CompileOptions::default()).unwrap_err();
        assert!(error.message.contains("source string literal"), "{error}");
    }

    #[test]
    fn exact_role_source_declarations_cannot_escape_the_verification_call() {
        let source = format!("module exact_roles::escape\nresource Token has store {{ id: u64 }}\naction verify() -> u64 {{\nverification\n{}\nreturn 0\n}}", role());
        let error = crate::compile(&source, crate::CompileOptions::default()).unwrap_err();
        assert!(error.message.contains("usable only inline"), "{error}");
    }

    #[test]
    fn exact_role_selection_reads_are_nominal_immutable_and_invocation_local() {
        let source = source(&role(), "64, 64, 32, 32768", "id: u64")
            .replace("ckb::verify_cell_roles(", "let selected = ckb::select_cell_roles(")
            .replace("return 0", "let alias = selected\nlet count = ckb::selected_input_count<Token>(alias, 0)\nlet input = ckb::selected_input<Token>(selected, 0, count - 1)\nlet output = ckb::selected_successor<Token>(alias, 0, 0)\nrequire input.capacity == output.capacity\nreturn ckb::selected_output_count<Token>(selected, 0)");
        for opt_level in 0..=3 {
            let compiled =
                crate::compile(&source, crate::CompileOptions { opt_level, target: Some("riscv64-elf".into()), ..Default::default() })
                    .unwrap_or_else(|error| panic!("O{opt_level}: {error}"));
            assert!(!compiled
                .metadata
                .runtime
                .fail_closed_runtime_features
                .iter()
                .any(|feature| feature == "exact-role-selection-artifact-proof"));
            let views = &compiled.metadata.runtime.transaction_view_handles;
            assert!(views.iter().any(|view| view.handle_type == "InputView<Token>"
                && view.source == "Input"
                && view.provenance.index.kind == "dynamic"
                && !view.lifecycle_authority));
            assert!(views.iter().any(|view| view.handle_type == "OutputView<Token>"
                && view.source == "Output"
                && view.provenance.index.kind == "dynamic"
                && !view.lifecycle_authority));
        }
        for (changed, expected) in [
            (source.replace("let alias = selected", "let mut alias = selected"), "immutable"),
            (source.replace("let alias = selected", "require selected == selected\nlet alias = selected"), "pointer-identity"),
            (source.replace("let alias = selected", "let saved = [selected]\nlet alias = selected"), "container"),
            (
                source
                    .replace("selected_input<Token>", "selected_input<Other>")
                    .replace("action verify", "resource Other has store { id: u64 }\naction verify"),
                "nominal catalog",
            ),
            (source.replace("selected_input<Token>(selected, 0,", "selected_input<Token>(selected, 1,"), "outside its catalog"),
            (source.replace("selected_input<Token>(selected, 0,", "selected_input<Token>(selected, count,"), "source integer literal"),
            (source.replace("verify()", "verify(witness forged: ExactCellSelection<Token>)"), "constructed locally"),
            (
                source
                    .replace("let alias = selected", "let alias = copy(selected)")
                    .replace("action verify", "fn copy(value: ExactCellSelection<Token>) -> u64 { return 0 }\naction verify"),
                "constructed locally",
            ),
        ] {
            let error = crate::compile(&changed, crate::CompileOptions::default()).unwrap_err();
            assert!(error.message.contains(expected), "{expected}: {error}");
        }
    }
}
