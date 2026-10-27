//! Named codec declarations retain their element type and distinct numeric
//! limits in the AST. No witness budget or wire profile is inferred here.
use super::*;
use std::collections::BTreeSet;

impl Parser<'_> {
    pub(super) fn parse_data_shape(&mut self) -> Result<DataShapeDef> {
        let span = self.advance().span;
        let name = self.parse_name()?;
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();
        let (mut element, mut limits) = (None, None);
        let mut seen = BTreeSet::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field_span = self.current().span;
            let field = self.parse_name()?;
            if !seen.insert(field.clone()) {
                return Err(CompileError::new(format!("duplicate data_shape member '{field}'"), field_span));
            }
            match field.as_str() {
                "element" => {
                    self.expect(TokenKind::Colon)?;
                    element = Some(self.parse_type()?);
                }
                "limits" => limits = Some(self.parse_data_shape_limits()?),
                _ => return Err(CompileError::new(format!("unknown data_shape member '{field}'"), field_span)),
            }
            self.data_shape_separator()?;
        }
        self.expect(TokenKind::RBrace)?;
        let def = DataShapeDef {
            name,
            element: element.ok_or_else(|| CompileError::new("data_shape requires an element type", span))?,
            limits: limits.ok_or_else(|| CompileError::new("data_shape requires a named limits block", span))?,
            span: Span::new(span.start, self.previous_non_newline().span.end, span.line, span.column),
        };
        crate::nested_values::declared_values(&def)?;
        Ok(def)
    }

    fn data_shape_separator(&mut self) -> Result<()> {
        if self.check(&TokenKind::Comma) {
            self.advance();
        } else if !self.check(&TokenKind::Newline) && !self.check(&TokenKind::RBrace) {
            return Err(CompileError::new("expected a comma or newline after data_shape member", self.current().span));
        }
        self.skip_newlines();
        Ok(())
    }

    fn parse_data_shape_limits(&mut self) -> Result<DataShapeLimits> {
        self.skip_newlines();
        let span = self.expect(TokenKind::LBrace)?.span;
        self.skip_newlines();
        let mut values = std::collections::BTreeMap::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field_span = self.current().span;
            let field = self.parse_name()?;
            if !matches!(field.as_str(), "items" | "payload_bytes" | "encoded_bytes") {
                return Err(CompileError::new(format!("unknown data_shape limit '{field}'"), field_span));
            }
            if values.contains_key(&field) {
                return Err(CompileError::new(format!("duplicate data_shape limit '{field}'"), field_span));
            }
            self.expect(TokenKind::Colon)?;
            let token = self.advance();
            let TokenKind::Integer(value) = token.kind else {
                return Err(CompileError::new("data_shape limits require unsigned integer literals", token.span));
            };
            let value = u32::try_from(value).map_err(|_| CompileError::new("data_shape limit must fit in u32", token.span))?;
            values.insert(field, value);
            self.data_shape_separator()?;
        }
        self.expect(TokenKind::RBrace)?;
        let get =
            |field| values.get(field).copied().ok_or_else(|| CompileError::new(format!("missing data_shape limit '{field}'"), span));
        Ok(DataShapeLimits { items: get("items")?, payload_bytes: get("payload_bytes")?, encoded_bytes: get("encoded_bytes")? })
    }
}

#[cfg(test)]
mod tests {
    use crate::{ast::Item, frontend, CellScriptEdition, CompileOptions};

    const SOURCE: &str = include_str!("../../tests/syntax_combo/seeds/named-data-shapes.cell");

    #[test]
    fn named_data_shapes_elaborate_and_format_without_changing_bounds() {
        let ast = frontend::parse(SOURCE, CellScriptEdition::Edition2027).unwrap();
        let formatted = crate::fmt::format_default(&ast).unwrap();
        assert_eq!(
            formatted,
            crate::fmt::format_default(&frontend::parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap()
        );
        let concrete = crate::generics::monomorphize_for_edition(&ast, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&concrete).unwrap();
        let Item::Action(action) = concrete.items.last().unwrap() else { panic!("missing action") };
        assert_eq!(crate::generics::render_type(&action.params[0].ty), "NestedValues<Vec<u8>, 3, 8, 128>");
        assert_eq!(crate::generics::render_type(&action.params[1].ty), "NestedValues<Entry, 3, 8, 128>");
        for opt_level in [0, 3] {
            let compiled = crate::compile(
                SOURCE,
                CompileOptions {
                    edition: CellScriptEdition::Edition2027,
                    target: Some("riscv64-elf".into()),
                    opt_level,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(!compiled.metadata.runtime.fail_closed_runtime_features.contains(&"nested-values-artifact-proof".into()));
        }
    }

    #[test]
    fn named_data_shapes_resolve_forward_declarations_and_declaration_scopes() {
        let mut reordered = frontend::parse(SOURCE, CellScriptEdition::Edition2027).unwrap();
        reordered.items.reverse();
        let source = crate::fmt::format_default(&reordered).unwrap();
        let result = crate::compile(
            &source,
            CompileOptions { edition: CellScriptEdition::Edition2027, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let original = crate::compile(
            SOURCE,
            CompileOptions { edition: CellScriptEdition::Edition2027, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        // Declaration order can change binary layout, but not the public codec
        // identity or the entry's resolved argument bounds.
        assert_eq!(result.metadata.interface_hash, original.metadata.interface_hash);
        assert_eq!(
            serde_json::to_value(&result.metadata.actions[0].params).unwrap(),
            serde_json::to_value(&original.metadata.actions[0].params).unwrap()
        );

        // A helper can borrow the containing Cell. Its local shape annotation
        // must resolve in the declaration scope, not capture the helper's
        // same-named generic parameter. A generic parameter may itself shadow
        // a shape name without changing that module-local declaration.
        let source = format!(
            "{SOURCE}
#[effect(ReadOnly)]
fn scoped<Entry: fixed_value>(tag: Entry, batch: &Batch) -> u64 {{ let values: Entries = batch.entries\nreturn values.len() }}
fn shadow<Entries: fixed_value>(value: Entries) -> Entries {{ return value }}
#[effect(ReadOnly)]
fn instantiate(batch: &Batch) -> u64 {{ let a = scoped<u64>(1, batch)\nlet b = shadow<u64>(2)\nreturn a + b }}"
        );
        let ast = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap();
        let concrete = crate::generics::monomorphize_for_edition(&ast, CellScriptEdition::Edition2027).unwrap();
        for (name, expected) in [("scoped", "NestedValues<Entry, 3, 8, 128>"), ("shadow", "u64")] {
            let function = concrete
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Function(function)
                        if crate::generics::decode_monomorph_name(&function.name).is_some_and(|(base, _)| base == name) =>
                    {
                        Some(function)
                    }
                    _ => None,
                })
                .unwrap();
            let ty = if name == "scoped" {
                let crate::ast::Stmt::Let(binding) = &function.body[0] else { panic!("missing shape binding") };
                binding.ty.as_ref().unwrap()
            } else {
                function.return_type.as_ref().unwrap()
            };
            assert_eq!(crate::generics::render_type(ty), expected);
        }
        crate::compile(
            &source,
            CompileOptions { edition: CellScriptEdition::Edition2027, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        for import in ["use external::Other as Messages", "use external::{Messages}"] {
            let source = format!("{SOURCE}\n{import}\n");
            let ast = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap();
            assert!(crate::generics::monomorphize_for_edition(&ast, CellScriptEdition::Edition2027)
                .unwrap_err()
                .message
                .contains("conflicts with a data_shape"));
        }
    }

    #[test]
    fn named_data_shapes_reject_conflicting_limits_recursion_and_unsupported_elements() {
        assert!(frontend::parse(SOURCE, CellScriptEdition::Edition2026).unwrap_err().message.contains("Edition 2027"));
        for (source, expected) in [
            (SOURCE.replace("items: 3", "items: 33"), "items <= 32"),
            (SOURCE.replace("payload_bytes: 8", "payload_bytes: 257"), "payload_bytes <= 256"),
            (SOURCE.replace("encoded_bytes: 128", "encoded_bytes: 3"), "encoded_bytes in 4..=4096"),
            (SOURCE.replace("items: 3", "items: limit"), "unsigned integer literals"),
            (SOURCE.replace("items: 3", "items: 3, items: 2"), "duplicate data_shape limit"),
            (SOURCE.replace("encoded_bytes: 128", "witness_bytes: 128"), "unknown data_shape limit"),
            (SOURCE.replace("items: 3, ", ""), "missing data_shape limit"),
            (SOURCE.replace("element: Vec<u8>", "element: Vec<u8>, element: Entry"), "duplicate data_shape member"),
            (SOURCE.replace("element: Vec<u8>,", ""), "requires an element type"),
            (SOURCE.replace("element: Vec<u8>", "element: u64"), "element must be Vec<u8> or a record"),
            (SOURCE.replace("element: Vec<u8>", "element: Messages"), "cannot recursively contain"),
            (SOURCE.replace("element: Vec<u8>", "element: Entries"), "cannot recursively contain"),
            (SOURCE.replace("element: Vec<u8>", "element: Messages::Item"), "cannot recursively contain"),
            (SOURCE.replace("element: Vec<u8>", "element: Entries::Payload"), "cannot recursively contain"),
            (SOURCE.replace("element: Vec<u8>", "element: Messages::Missing"), "cannot recursively contain"),
            (SOURCE.replace("element: Vec<u8>", "element: Vec<Messages::Item>"), "cannot recursively contain"),
            (SOURCE.replace("element: Vec<u8>", "element: Absent"), "ordinary record"),
            (SOURCE.replace("element: Entry", "element: Batch"), "ordinary record"),
            (SOURCE.replace("payload: Vec<u8>", "payload: Vec<u64>"), "one Vec<u8> payload"),
            (SOURCE.replace("data_shape Messages", "public data_shape Messages"), "module-local"),
            (SOURCE.replace("data_shape Messages", "data_shape Entry"), "duplicate"),
            (SOURCE.replace("data_shape Messages", "data_shape Vec"), "reserved"),
        ] {
            let error =
                crate::compile(&source, CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() }).unwrap_err();
            assert!(error.message.contains(expected), "expected {expected}: {error}");
            assert_ne!(error.span, crate::error::Span::default());
        }
        // Unused declarations are still checked at their declaration span.
        let unused = "module invalid_shape\ndata_shape Wrong { element: Missing, limits { items: 1, payload_bytes: 8, encoded_bytes: 32 } }\naction inspect() { require true }";
        assert!(crate::compile(unused, CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() })
            .unwrap_err()
            .message
            .contains("ordinary record"));
    }

    #[test]
    fn named_nested_helpers_reject_effect_bound_and_escape_changes() {
        let source = include_str!("../../tests/syntax_combo/seeds/named-nested-helpers.cell");
        for (changed, expected) in [
            (source.replace("fn total(", "#[effect(Mutating)]\nfn total("), "Pure/ReadOnly"),
            (source.replace("fn message_total(values:", "fn message_total(mut values:"), "immutable"),
            (source.replace("fn message_total(values: Messages)", "fn message_total(values: &Messages)"), "immutable"),
            (source.replace("fn message_total(values: Messages) -> u64", "fn message_total(values: Messages) -> Messages"), "escape"),
            (
                source.replace("fn message_total(values: Messages)", "fn message_total(values: NestedValues<Vec<u8>, 2, 8, 128>)"),
                "argument",
            ),
            (source.replace("let alias = messages", "let mut alias = messages"), "read-only"),
            (source.replace("witness entries: Entries", "witness entries: NestedRecordView<Entry, 8>"), "only complete NestedValues"),
            (format!("{source}\nstruct Stored {{ values: Messages }}"), "Cell"),
        ] {
            let error = crate::compile(&changed, CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() })
                .unwrap_err();
            assert!(error.message.contains(expected), "expected {expected}: {error}");
        }
    }

    #[test]
    fn named_derived_helpers_resolve_types_and_reject_escape() {
        let source = include_str!("../../tests/syntax_combo/seeds/named-derived-helpers.cell");
        let ast = frontend::parse(source, CellScriptEdition::Edition2027).unwrap();
        let formatted = crate::fmt::format_default(&ast).unwrap();
        assert_eq!(
            formatted,
            crate::fmt::format_default(&frontend::parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap()
        );
        let concrete = crate::generics::monomorphize_for_edition(&ast, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&concrete).unwrap();
        for (name, expected) in [("payload_total", "NestedBytesView<8>"), ("record_total", "NestedRecordView<Entry, 8>")] {
            let function = concrete
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Function(function) if function.name == name => Some(function),
                    _ => None,
                })
                .unwrap();
            assert_eq!(crate::generics::render_type(&function.params[0].ty), expected);
        }
        for (changed, expected) in [
            (source.replace("Entries::Payload", "Entries::Missing"), "Shape::Item"),
            (source.replace("payload: Entries::Payload", "mut payload: Entries::Payload"), "immutable"),
            (source.replace("payload: Entries::Payload", "payload: &Entries::Payload"), "immutable"),
            (source.replace("fn payload_total(", "#[effect(Mutating)]\nfn payload_total("), "Pure/ReadOnly"),
            (source.replace("witness messages: Messages", "witness messages: Messages::Item"), "only complete NestedValues"),
            (
                source.replace(
                    "fn message_total(message: Messages::Item) -> u64",
                    "fn message_total(message: Messages::Item) -> Messages::Item",
                ),
                "escape",
            ),
            (
                source.replace("fn payload_total(payload: Entries::Payload)", "fn payload_total(payload: NestedBytesView<7>)"),
                "argument",
            ),
            (format!("{source}\nstruct Stored {{ payload: Entries::Payload }}"), "cannot be stored"),
        ] {
            let error = crate::compile(&changed, CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() })
                .unwrap_err();
            assert!(error.message.contains(expected), "expected {expected}: {error}");
        }
    }
}
