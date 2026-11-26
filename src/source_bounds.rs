//! Closed unsigned scalar resolution for source bounds. This admits literal
//! constants and references to such constants, not general constant evaluation.
use crate::ast::{BoundReference, Expr, Item, Module, Type, TypeArgument, UnsignedBound, ValueCollectionKind};
use crate::error::{CompileError, DiagnosticCode, Result, Span};
use crate::resolve::ModuleResolver;

const MAX_BOUND_REFERENCE_DEPTH: usize = 32;

pub(crate) fn resolve(bound: &UnsignedBound, module: &str, resolver: &ModuleResolver) -> Result<u128> {
    match bound {
        UnsignedBound::Literal(value) => Ok(*value),
        UnsignedBound::Reference(reference) => resolve_reference(reference, module, resolver, &mut Vec::new()),
    }
}

fn error(message: impl Into<String>, span: Span) -> CompileError {
    CompileError::new(message, span).with_code(DiagnosticCode::E2111)
}

fn resolve_reference(
    reference: &BoundReference,
    module: &str,
    resolver: &ModuleResolver,
    active: &mut Vec<(String, String)>,
) -> Result<u128> {
    if active.len() >= MAX_BOUND_REFERENCE_DEPTH {
        return Err(error(format!("unsigned bound references exceed the depth limit of {MAX_BOUND_REFERENCE_DEPTH}"), reference.span));
    }
    let (owner, constant) = resolver
        .resolve_constant_with_module(module, &reference.path)
        .ok_or_else(|| error(format!("unsigned bound '{}' must name an accessible constant", reference.path), reference.span))?;
    let identity = (owner.clone(), constant.name.clone());
    if active.contains(&identity) {
        return Err(error(format!("cyclic unsigned bound reference to '{}::{}'", owner, constant.name), reference.span));
    }
    let maximum = match constant.ty {
        Type::U8 => u8::MAX as u128,
        Type::U16 => u16::MAX as u128,
        Type::U32 => u32::MAX as u128,
        Type::U64 => u64::MAX as u128,
        Type::U128 => u128::MAX,
        _ => {
            return Err(error(
                format!("unsigned bound '{}::{}' requires an unsigned scalar constant", owner, constant.name),
                reference.span,
            ))
        }
    };
    active.push(identity);
    let value = match constant.value {
        Expr::Integer(value) => Ok(value),
        Expr::Identifier(path) => resolve_reference(&BoundReference { path, span: reference.span }, &owner, resolver, active),
        _ => Err(error(
            format!(
                "unsigned bound '{}::{}' requires an integer literal or constant reference; expressions are not evaluated",
                owner, constant.name
            ),
            reference.span,
        )),
    };
    active.pop();
    let value = value?;
    if value > maximum {
        return Err(error(format!("unsigned bound '{}::{}' exceeds its declared scalar type", owner, constant.name), reference.span));
    }
    Ok(value)
}

/// The constructor owns its argument kinds. Ordinary nominal applications
/// retain type arguments, even when a type happens to share a constant's name.
pub(crate) fn is_unsigned_argument(constructor: &str, index: usize) -> bool {
    matches!(
        (constructor, index),
        ("NestedValues", 1..=3)
            | ("NestedBytesView", 0)
            | ("NestedRecordView", 1)
            | ("BoundedCellSet" | "BoundedList" | "WitnessBytesView", 1)
    )
}

pub(crate) fn resolve_policies(module: &Module, resolver: &ModuleResolver) -> Result<Module> {
    let mut normalized = module.clone();
    for item in &mut normalized.items {
        if let Item::BoundPolicy(policy) = item {
            let span = policy.span();
            *item = policy.clone().resolve(|bound| {
                let value = resolve(bound, &module.name, resolver)?;
                u32::try_from(value).map_err(|_| {
                    error(
                        "named policy bound must fit in u32",
                        match bound {
                            UnsignedBound::Reference(reference) => reference.span,
                            UnsignedBound::Literal(_) => span,
                        },
                    )
                })
            })?;
            validate_policy_bounds(item)?;
        }
        normalize_declaration_types(item, &module.name, resolver)?;
    }
    Ok(normalized)
}

pub(crate) fn validate_policy_bounds(item: &Item) -> Result<()> {
    match item {
        Item::DataShape(def) => crate::nested_values::validate_limits(def),
        Item::Collections(def) => {
            if !(64..=65536).contains(&def.bytes) || !def.bytes.is_multiple_of(16) || !(1..=256).contains(&def.allocations) {
                return Err(CompileError::new("collections requires 64–65536 aligned bytes and 1–256 allocation events", def.span));
            }
            if def.members.iter().any(|member| !(1..=1024).contains(&member.items)) {
                return Err(CompileError::new("collections member items must be between 1 and 1024", def.span));
            }
            Ok(())
        }
        Item::CellRoles(def) => {
            for (name, value, maximum) in [
                ("input_scan", def.limits.input_scan, 64),
                ("output_scan", def.limits.output_scan, 64),
                ("selected_per_side", def.limits.selected_per_side, 32),
                ("aggregate_bytes", def.limits.aggregate_bytes, 32768),
            ] {
                if !(1..=maximum).contains(&value) {
                    return Err(CompileError::new(format!("{name} must be within 1..={maximum}"), def.span));
                }
            }
            for role in &def.roles {
                if [role.inputs, role.outputs].into_iter().any(|(min, max)| min > max || max > 32) {
                    return Err(CompileError::new("role count interval must be ordered within 0..=32", role.span));
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(crate) fn unresolved_type(ty: &Type) -> bool {
    match ty {
        Type::ArrayBound(_, _) => true,
        Type::Array(inner, _) | Type::Ref(inner) | Type::MutRef(inner) => unresolved_type(inner),
        Type::Tuple(items) => items.iter().any(unresolved_type),
        Type::Applied(application) => application.arguments.iter().any(|argument| match argument {
            TypeArgument::UnsignedReference(_) => true,
            TypeArgument::Type(ty) => unresolved_type(ty),
            TypeArgument::UnsignedConstant(_) => false,
        }),
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
        | Type::Named(_)
        | Type::Selection(_) => false,
    }
}

pub(crate) fn collection_has_unresolved_types(def: &crate::ast::CollectionsDef) -> bool {
    def.members.iter().any(|member| match &member.kind {
        ValueCollectionKind::Vec(ty) | ValueCollectionKind::OrderedSet(ty) => unresolved_type(ty),
        ValueCollectionKind::OrderedMap { key, value } => unresolved_type(key) || unresolved_type(value),
    })
}

/// Normalize type nodes before template catalogs retain public signatures or
/// move a declaration into a requester's scope. This changes no wire identity:
/// the resulting nodes are the same literal types used by existing consumers.
pub(crate) fn normalize_type(ty: &Type, scope: &str, resolver: &ModuleResolver) -> Result<Type> {
    Ok(match ty {
        Type::ArrayBound(inner, bound) => {
            let value = resolve(bound, scope, resolver)?;
            let span = match bound {
                UnsignedBound::Reference(reference) => reference.span,
                UnsignedBound::Literal(_) => Span::default(),
            };
            let length = usize::try_from(value).map_err(|_| error("array size exceeds the supported platform range", span))?;
            Type::Array(Box::new(normalize_type(inner, scope, resolver)?), length)
        }
        Type::Array(inner, length) => Type::Array(Box::new(normalize_type(inner, scope, resolver)?), *length),
        Type::Tuple(items) => Type::Tuple(items.iter().map(|ty| normalize_type(ty, scope, resolver)).collect::<Result<Vec<_>>>()?),
        Type::Ref(inner) => Type::Ref(Box::new(normalize_type(inner, scope, resolver)?)),
        Type::MutRef(inner) => Type::MutRef(Box::new(normalize_type(inner, scope, resolver)?)),
        Type::Applied(application) => {
            let mut application = application.try_map_types(|ty| normalize_type(ty, scope, resolver))?;
            for argument in &mut application.arguments {
                if let TypeArgument::UnsignedReference(reference) = argument {
                    *argument =
                        TypeArgument::UnsignedConstant(resolve(&UnsignedBound::Reference(reference.clone()), scope, resolver)?);
                }
            }
            Type::Applied(Box::new(application))
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
        | Type::Named(_)
        | Type::Selection(_) => ty.clone(),
    })
}

pub(crate) fn normalize_declaration_types(item: &mut Item, scope: &str, resolver: &ModuleResolver) -> Result<()> {
    use crate::ast::{Field, Param};
    let normalize = |ty: &mut Type| -> Result<()> {
        *ty = normalize_type(ty, scope, resolver)?;
        Ok(())
    };
    let fields = |fields: &mut [Field]| -> Result<()> { fields.iter_mut().try_for_each(|field| normalize(&mut field.ty)) };
    let params = |params: &mut [Param]| -> Result<()> { params.iter_mut().try_for_each(|param| normalize(&mut param.ty)) };
    match item {
        Item::Resource(def) => fields(&mut def.fields)?,
        Item::Shared(def) => fields(&mut def.fields)?,
        Item::Receipt(def) => {
            fields(&mut def.fields)?;
            if let Some(ty) = &mut def.claim_output {
                normalize(ty)?;
            }
        }
        Item::Struct(def) => fields(&mut def.fields)?,
        Item::Enum(def) => {
            for variant in &mut def.variants {
                for ty in &mut variant.fields {
                    normalize(ty)?;
                }
            }
        }
        Item::Const(def) => normalize(&mut def.ty)?,
        Item::Action(def) => {
            params(&mut def.params)?;
            if let Some(ty) = &mut def.return_type {
                normalize(ty)?;
            }
            for output in &mut def.outputs {
                normalize(&mut output.ty)?;
            }
        }
        Item::Function(def) => {
            params(&mut def.params)?;
            if let Some(ty) = &mut def.return_type {
                normalize(ty)?;
            }
        }
        Item::Lock(def) => {
            params(&mut def.params)?;
            normalize(&mut def.return_type)?;
        }
        Item::DataShape(def) => normalize(&mut def.element)?,
        Item::Collections(def) => {
            for member in &mut def.members {
                match &mut member.kind {
                    ValueCollectionKind::Vec(ty) | ValueCollectionKind::OrderedSet(ty) => normalize(ty)?,
                    ValueCollectionKind::OrderedMap { key, value } => {
                        normalize(key)?;
                        normalize(value)?;
                    }
                }
            }
        }
        Item::CellRoles(def) => {
            for role in &mut def.roles {
                normalize(&mut role.schema)?;
            }
        }
        Item::BoundPolicy(_) | Item::Flow(_) | Item::Invariant(_) | Item::Use(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{frontend, generics, CellScriptEdition, CompileOptions};
    const EDITION: CellScriptEdition = CellScriptEdition::Edition2027;

    fn options() -> CompileOptions {
        CompileOptions { edition: EDITION, target: Some("riscv64-elf".into()), ..Default::default() }
    }

    fn inject(source: &str, declarations: &str) -> String {
        let end = source.find("module ").unwrap();
        let end = end + source[end..].find(';').unwrap() + 1;
        format!("{}\n{declarations}\n{}", &source[..end], &source[end..])
    }

    #[test]
    fn named_unsigned_bounds_arrays_preserve_normalized_types_and_executable_bytes() {
        for scalar in ["u8", "u16", "u32", "u64", "u128"] {
            let source = format!(
                "module bounds; const WIDTH: {scalar} = 4; action inspect(witness value: [u8; WIDTH]) {{ require value[0] == 7; }}"
            );
            let ast = frontend::parse(&source, EDITION).unwrap();
            let formatted = crate::fmt::format_default(&ast).unwrap();
            let reparsed = frontend::parse(&formatted, EDITION).unwrap();
            assert!(crate::ast::semantic_shape::SemanticEq::same_semantics(&ast, &reparsed));
            assert_eq!(formatted, crate::fmt::format_default(&reparsed).unwrap());
            let normalized = generics::monomorphize_for_edition(&ast, EDITION).unwrap();
            let Item::Action(action) = normalized.items.last().unwrap() else { panic!("missing action") };
            assert_eq!(action.params[0].ty, Type::Array(Box::new(Type::U8), 4));
            let named = crate::compile(&source, options()).unwrap();
            let literal = crate::compile(&source.replace("[u8; WIDTH]", "[u8; 4]"), options()).unwrap();
            named.validate().unwrap();
            literal.validate().unwrap();
            assert_eq!(named.artifact_bytes, literal.artifact_bytes, "{scalar}");
            assert_eq!(named.metadata.interface_hash, literal.metadata.interface_hash);
        }
    }

    #[test]
    fn named_unsigned_bounds_all_named_policy_families_keep_literal_execution() {
        let fixtures = [
            (
                include_str!("../tests/syntax_combo/seeds/named-data-shapes.cell"),
                "const ITEMS: u32 = 3; const PAYLOAD: u32 = 8; const ENCODED: u32 = 128;",
                vec![
                    ("items: 3", "items: ITEMS"),
                    ("payload_bytes: 8", "payload_bytes: PAYLOAD"),
                    ("encoded_bytes: 128", "encoded_bytes: ENCODED"),
                ],
            ),
            (
                include_str!("../tests/syntax_combo/seeds/named-collections.cell"),
                "const BYTES: u32 = 8192; const EVENTS: u32 = 32; const ITEMS: u32 = 64;",
                vec![("bytes: 8192", "bytes: BYTES"), ("allocations: 32", "allocations: EVENTS"), ("items: 64", "items: ITEMS")],
            ),
            (
                include_str!("../tests/syntax_combo/seeds/named-role-selection.cell"),
                "const SCAN: u32 = 64; const SELECTED: u32 = 32; const BYTES: u32 = 32768; const MIN: u32 = 1; const COUNT: u32 = 16;",
                vec![
                    ("input_scan: 64", "input_scan: SCAN"),
                    ("output_scan: 64", "output_scan: SCAN"),
                    ("selected_per_side: 32", "selected_per_side: SELECTED"),
                    ("aggregate_bytes: 32768", "aggregate_bytes: BYTES"),
                    ("1..=16", "MIN..=COUNT"),
                    ("0..=16", "0..=COUNT"),
                ],
            ),
        ];
        for (fixture, constants, replacements) in fixtures {
            let literal_source = inject(fixture, constants);
            let mut source = literal_source.clone();
            for (before, after) in replacements {
                assert!(source.contains(before));
                source = source.replace(before, after);
            }
            let ast = frontend::parse(&source, EDITION).unwrap();
            assert!(ast.items.iter().any(|item| matches!(item, Item::BoundPolicy(_))));
            let formatted = crate::fmt::format_default(&ast).unwrap();
            let reparsed = frontend::parse(&formatted, EDITION).unwrap();
            assert!(crate::ast::semantic_shape::SemanticEq::same_semantics(&ast, &reparsed));
            let named = crate::compile(&source, options()).unwrap();
            let literal = crate::compile(&literal_source, options()).unwrap();
            named.validate().unwrap();
            literal.validate().unwrap();
            assert_eq!(named.artifact_bytes, literal.artifact_bytes);
            assert_eq!(named.metadata.interface_hash, literal.metadata.interface_hash);
        }
    }

    #[test]
    fn named_unsigned_bounds_builtin_argument_kinds_normalize_without_nominal_guessing() {
        let source = "module bounds; const N: u32 = 3; const P: u32 = 8; const B: u32 = 128;
            struct Entry { id: u64, payload: Vec<u8>, }
            resource Note has store { id: u64, }
            struct Shapes { values: NestedValues<Vec<u8>, N, P, B>, bytes: NestedBytesView<P>,
                record: NestedRecordView<Entry, P>, inputs: BoundedCellSet<Note, N>,
                outputs: BoundedList<Note, N>, witness: WitnessBytesView<lock, B>, }
            action inspect() {}";
        let parsed = frontend::parse(source, EDITION).unwrap();
        let ast = generics::monomorphize_for_edition(&parsed, EDITION).unwrap();
        let Item::Struct(shapes) = &ast.items[5] else { panic!("missing shapes") };
        for field in &shapes.fields {
            assert!(!unresolved_type(&field.ty));
        }
        assert_eq!(shapes.fields[0].ty.source_text(), "NestedValues<Vec<u8>, 3, 8, 128>");
        // An ordinary generic declaration's parameter still denotes a type.
        let nominal = "module nominal; const N: u32 = 4; struct N { value: u64, } action inspect() {}";
        assert!(crate::compile(nominal, options()).is_err(), "same-name declarations must retain symbol checks");
    }

    #[test]
    fn named_unsigned_bounds_reject_non_scalar_expressions_cycles_overflow_and_unknown_names() {
        for (declarations, name, expected) in [
            ("const N: bool = true;", "N", "unsigned scalar"),
            ("const N: i32 = 4;", "N", "unsigned scalar"),
            ("const N: u8 = 256;", "N", "declared scalar type"),
            ("const N: u32 = 2 + 2;", "N", "expressions are not evaluated"),
            ("const N: u32 = f(); fn f() -> u32 { return 4; }", "N", "expressions are not evaluated"),
            ("const N: u32 = M; const M: u32 = N;", "N", "cyclic"),
            ("", "MISSING", "accessible constant"),
            ("const N: u128 = 18446744073709551616;", "N", "platform range"),
        ] {
            let source = format!("module invalid; {declarations} action inspect(witness bytes: [u8; {name}]) {{}} ");
            let error = crate::compile(&source, options()).unwrap_err();
            assert!(error.message.contains(expected), "{source}: {error:?}");
        }
        for expression in ["N + 1", "-1", "f()"] {
            let source = format!("module invalid; const N: u32 = 4; action inspect(witness bytes: [u8; {expression}]) {{}} ");
            assert!(frontend::parse(&source, EDITION).is_err(), "{source}");
        }
    }

    #[test]
    fn named_unsigned_bounds_resolve_accessible_aliases_in_the_defining_constant_scope() {
        let mut resolver = ModuleResolver::new();
        for source in [
            "module library; private const PRIVATE: u32 = 4; public const WIDTH: u32 = PRIVATE;",
            "module consumer; use library::WIDTH as IMPORTED; const PRIVATE: u32 = 9; action inspect() {}",
        ] {
            resolver.register_module(frontend::parse(source, EDITION).unwrap()).unwrap();
        }
        let reference = |path: &str| UnsignedBound::Reference(BoundReference { path: path.into(), span: Span::new(20, 25, 2, 1) });
        assert_eq!(resolve(&reference("IMPORTED"), "consumer", &resolver).unwrap(), 4);
        assert_eq!(resolve(&reference("library::WIDTH"), "consumer", &resolver).unwrap(), 4);
        assert!(resolve(&reference("library::PRIVATE"), "consumer", &resolver).is_err());
    }

    #[test]
    fn named_unsigned_bounds_imported_templates_keep_the_defining_private_constant() {
        let sources = [
            (
                "src/library.cell",
                "module library; private const WIDTH: u32 = 4;
                public struct Packet<T: fixed_value> { value: T, bytes: [u8; WIDTH], }
                public fn size<T: fixed_value>(value: T) -> u64 {
                    let bytes: [u8; WIDTH] = b\"1234\";
                    return bytes.len();
                }",
            ),
            (
                "src/main.cell",
                "module consumer; use library::Packet as Parcel; use library::size;
                const WIDTH: u32 = 9;
                action inspect(witness packet: Parcel<u64>) -> u64 {
                    require packet.bytes.len() == 4;
                    return size::<u64>(7);
                }",
            ),
        ]
        .into_iter()
        .map(|(path, source)| crate::InMemorySource { path: path.into(), source: source.into(), role: None })
        .collect::<Vec<_>>();
        let project = crate::source_loading::load_virtual_project_for_entry_diagnostics(&sources, "src/main.cell", EDITION).unwrap();
        let mut widths = 0;
        for module in &project.modules {
            for item in module.ast.items.iter().chain(&module.ast.interface_templates) {
                if let Item::Struct(def) = item {
                    for field in &def.fields {
                        if field.name == "bytes" {
                            assert_eq!(field.ty, Type::Array(Box::new(Type::U8), 4));
                            widths += 1;
                        }
                    }
                }
            }
        }
        assert!(widths >= 2, "both template and concrete signatures must normalize");
        let report = crate::compile_sources_metadata_with_diagnostics(&sources, "src/main.cell", EDITION, None);
        assert!(report.metadata.is_some(), "{:?}", report.diagnostics);
    }

    #[test]
    fn named_unsigned_bounds_contextual_names_and_reference_depth_share_the_same_rule() {
        for name in ["size", "store", "lock"] {
            let source = format!(
                "module contextual; const {name}: u32 = 4; action inspect(witness bytes: [u8; {name}]) {{ require bytes[0] == 0; }}"
            );
            let result = crate::compile(&source, options()).unwrap();
            result.validate().unwrap();
        }
        for (levels, accepted) in [(32, true), (33, false)] {
            let mut source = "module depth;".to_string();
            for index in 0..levels {
                let value = if index + 1 == levels { "4".to_string() } else { format!("N{}", index + 1) };
                source.push_str(&format!("const N{index}: u32 = {value};"));
            }
            let mut resolver = ModuleResolver::new();
            resolver.register_module(frontend::parse(&source, EDITION).unwrap()).unwrap();
            let span = Span::new(10, 12, 1, 11);
            let result = resolve(&UnsignedBound::Reference(BoundReference { path: "N0".into(), span }), "depth", &resolver);
            if accepted {
                assert_eq!(result.unwrap(), 4);
            } else {
                let error = result.unwrap_err();
                assert!(error.message.contains("depth limit of 32"));
                assert_eq!(error.span, span);
            }
        }
    }
}
