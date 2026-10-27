//! Declaration admission for invocation-local owned value collections.
//! Local owner/helper typing lives in types::allocated. Local runtime operations
//! and helper transport compose independent allocation and lifetime checks.
use crate::ast::{CollectionsDef, Type, ValueCollectionKind};
use crate::error::{CompileError, Result};
use std::collections::BTreeSet;

pub(crate) mod budget;

pub(crate) fn validate_declaration(def: &CollectionsDef) -> Result<()> {
    validate_widths(def, fixed_width, true)
}

/// Layout checks run after name resolution and generic specialization. Parsing
/// may retain a record name, but that never supplies a fabricated byte width.
pub(crate) fn validate_resolved_declaration(def: &CollectionsDef, width: impl Fn(&Type) -> Option<u32>) -> Result<()> {
    validate_widths(def, width, false)
}

fn validate_widths(def: &CollectionsDef, width: impl Fn(&Type) -> Option<u32>, defer_records: bool) -> Result<()> {
    if !(64..=65536).contains(&def.bytes) || !def.bytes.is_multiple_of(16) || !(1..=256).contains(&def.allocations) {
        return Err(CompileError::new("collections requires 64–65536 aligned bytes and 1–256 allocation events", def.span));
    }
    if def.members.is_empty() || def.members.len() > 32 {
        return Err(CompileError::new("collections requires 1–32 named members", def.span));
    }
    if matches!(
        def.name.as_str(),
        "Vec"
            | "OrderedMap"
            | "OrderedSet"
            | "HashMap"
            | "HashSet"
            | "Map"
            | "Set"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "i32"
            | "bool"
            | "Hash"
            | "Address"
            | "String"
            | "Option"
            | "Result"
            | "BoundedCellSet"
            | "BoundedList"
            | "NestedValues"
            | "NestedBytesView"
            | "NestedRecordView"
    ) {
        return Err(CompileError::new("collections name is reserved by the prelude", def.span));
    }
    let mut names = BTreeSet::new();
    for member in &def.members {
        if !names.insert(&member.name) || matches!(member.name.as_str(), "limits" | "new" | "drop") {
            return Err(CompileError::new(format!("duplicate or reserved collections member '{}'", member.name), member.span));
        }
        if !(1..=1024).contains(&member.items) {
            return Err(CompileError::new("collections member items must be between 1 and 1024", member.span));
        }
        let invalid = || {
            CompileError::new(
            "collections requires fixed non-Cell values (1–64 bytes for Vec, 1–32 for Map); ordered keys must be unsigned u8/u16/u32/u64, Hash, Address or 1–32 bytes; references, hidden Cells and dynamic owned values are not admitted",
            member.span,
        )
        };
        let value_width = |ty: &Type, maximum: u32| -> Result<Option<u32>> {
            match width(ty) {
                Some(width) if (1..=maximum).contains(&width) => Ok(Some(width)),
                None if defer_records && matches!(ty, Type::Named(name) if !name.starts_with("Vec<") && !name.starts_with("&")) => {
                    Ok(None)
                }
                _ => Err(invalid()),
            }
        };
        let member_width = match &member.kind {
            ValueCollectionKind::Vec(element) => value_width(element, 64)?,
            ValueCollectionKind::OrderedMap { key, value } => {
                let key = key_width(key).ok_or_else(invalid)?;
                value_width(value, 32)?.map(|value| key + value)
            }
            ValueCollectionKind::OrderedSet(key) => Some(key_width(key).ok_or_else(invalid)?),
        };
        let Some(member_width) = member_width else { continue };
        let payload = member_width * member.items;
        if payload.div_ceil(16) * 16 + 32 > def.bytes {
            return Err(CompileError::new(
                format!("collections member '{}' maximum payload and allocation record exceed its arena byte budget", member.name),
                member.span,
            ));
        }
    }
    Ok(())
}

pub(crate) fn fixed_width(ty: &Type) -> Option<u32> {
    match ty {
        Type::U8 | Type::Bool => Some(1),
        Type::U16 => Some(2),
        Type::U32 | Type::I32 => Some(4),
        Type::U64 => Some(8),
        Type::U128 => Some(16),
        Type::Hash | Type::Address => Some(32),
        Type::Array(inner, count) if **inner == Type::U8 && (1..=64).contains(count) => Some(*count as u32),
        _ => None,
    }
}

fn key_width(ty: &Type) -> Option<u32> {
    match ty {
        Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Hash | Type::Address => fixed_width(ty),
        Type::Array(inner, count) if **inner == Type::U8 && (1..=32).contains(count) => Some(*count as u32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ast::Item, frontend, CellScriptEdition, CompileOptions};
    const SOURCE: &str = include_str!("../tests/syntax_combo/seeds/named-collections.cell");

    #[test]
    fn named_collections_roundtrip_structured_limits_under_strict_admission() {
        let ast = frontend::parse(SOURCE, CellScriptEdition::Edition2027).unwrap();
        let Item::Collections(def) = &ast.items[0] else { panic!("missing collections declaration") };
        assert_eq!((def.bytes, def.allocations, def.members.len()), (8192, 32, 3));
        assert!(matches!(&def.members[0].kind, ValueCollectionKind::OrderedMap { key: Type::Hash, value: Type::U64 }));
        let formatted = crate::fmt::format_default(&ast).unwrap();
        let reparsed = frontend::parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        assert_eq!(formatted, crate::fmt::format_default(&reparsed).unwrap());
        let options =
            CompileOptions { edition: CellScriptEdition::Edition2027, target: Some("riscv64-elf".into()), ..Default::default() };
        let compiled = crate::compile(SOURCE, options.clone()).unwrap();
        let mut no_decl = ast;
        no_decl.items.remove(0);
        let plain = crate::compile(&crate::fmt::format_default(&no_decl).unwrap(), options.clone()).unwrap();
        assert_eq!(compiled.metadata.interface_hash, plain.metadata.interface_hash);
        assert_eq!(compiled.metadata.runtime.fail_closed_runtime_features, plain.metadata.runtime.fail_closed_runtime_features);
        let constructed = SOURCE.replace("return 0", "let values = Scratch::new()\nreturn 0");
        let allocated = crate::compile(&constructed, options.clone()).unwrap();
        assert!(!allocated
            .metadata
            .runtime
            .fail_closed_runtime_features
            .iter()
            .any(|feature| feature == "allocated-values-artifact-proof"));
        crate::compile_with_executable_surface_policy(&constructed, options, crate::ExecutableSurfacePolicy::DenyFailClosed).unwrap();
        assert!(frontend::parse(SOURCE, CellScriptEdition::Edition2026).is_err());
    }

    #[test]
    fn named_collections_reject_bad_budgets_types_names_and_ownership() {
        for (from, to) in [
            ("bytes: 8192", "bytes: 63"),
            ("bytes: 8192", "bytes: 8193"),
            ("bytes: 8192", "bytes: 65552"),
            ("allocations: 32", "allocations: 0"),
            ("allocations: 32", "allocations: 257"),
            ("items: 64", "items: 0"),
            ("items: 64", "items: 1025"),
            ("bytes: 8192", "bytes: 64"),
            ("items: 64", "items: bound"),
            ("items: 64", "items: -1"),
            ("items: 64", "items: 4294967296"),
            ("items: 64", "items: 64, items: 32"),
            ("bytes: 8192", "memory: 8192"),
            ("limits { bytes: 8192, allocations: 32 }", "limits { bytes: 8192 }"),
            ("totals:", "seen:"),
            ("totals:", "new:"),
            ("Scratch", "Vec"),
            ("OrderedMap<Hash, u64>", "HashMap<Hash, u64>"),
            ("OrderedMap<Hash, u64>", "OrderedMap<i32, u64>"),
            ("OrderedMap<Hash, u64>", "OrderedMap<u128, u64>"),
            ("OrderedSet<Hash>", "OrderedSet<bool>"),
            ("Vec<u64>", "Vec<&u64>"),
            ("Vec<u64>", "Vec<Vec<u8>>"),
            ("Vec<u64>", "Vec<[u8; 65]>"),
            ("OrderedMap<Hash, u64>", "OrderedMap<Hash, [u8; 33]>"),
            ("collections Scratch", "pub collections Scratch"),
        ] {
            let source = SOURCE.replace(from, to);
            assert_ne!(source, SOURCE);
            assert!(frontend::parse(&source, CellScriptEdition::Edition2027).is_err(), "accepted: {from} -> {to}");
        }
        let options = CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() };
        // Named records are resolved after parsing; an unknown name is still
        // rejected even when no action constructs the collection owner.
        assert!(crate::compile(&SOURCE.replace("Vec<u64>", "Vec<Token>"), options.clone()).is_err());
        assert!(crate::compile(&format!("{SOURCE}\nstruct Scratch {{ id: u64 }}"), options).is_err());
        assert!(frontend::parse("module m\ncollections C { limits { bytes: 64, allocations: 1 } }", CellScriptEdition::Edition2027)
            .is_err());
    }

    #[test]
    fn named_collections_validate_shared_budget_without_pretending_all_members_fit_at_once() {
        // Each maximum fits individually. Simultaneous use and growth history
        // still consume the shared runtime budget; this is not a reservation.
        let source = "module m\ncollections C { limits { bytes: 64, allocations: 1 }\na: Vec<u64> { items: 4 }\nb: OrderedSet<[u8; 32]> { items: 1 } }\naction run() -> u64 { return 0 }";
        let ast = frontend::parse(source, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&ast).unwrap();
        let mut mutated = ast;
        let Item::Collections(def) = &mut mutated.items[0] else { unreachable!() };
        def.members[0].items = 0;
        assert!(crate::types::check(&mutated).is_err());
    }

    #[test]
    fn named_collections_member_count_and_contextual_keyword_boundaries() {
        for count in [32, 33] {
            let members = (0..count).map(|index| format!("v{index}: Vec<u8> {{ items: 1 }}\n")).collect::<String>();
            let source = format!("module m\ncollections C {{ limits {{ bytes: 64, allocations: 1 }}\n{members}}}");
            assert_eq!(frontend::parse(&source, CellScriptEdition::Edition2027).is_ok(), count == 32);
        }
        let source = "module m\nfn collections() -> u64 { return 7 }\naction inspect() -> u64 { verification\nreturn collections() }";
        crate::compile(source, CompileOptions { edition: CellScriptEdition::Edition2026, ..Default::default() }).unwrap();
    }
}
