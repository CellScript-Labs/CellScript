//! Experimental named policy declarations. Parsing retains names and policy
//! variants; it never manufactures positional runtime calls or source text.
use super::*;
use std::collections::BTreeSet;

impl Parser<'_> {
    pub(super) fn parse_cell_roles(&mut self) -> Result<CellRolesDef> {
        let span = self.advance().span;
        let name = self.parse_name()?;
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut limits = None;
        let mut roles = Vec::new();
        let mut names = BTreeSet::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let role_span = self.current().span;
            let field = self.parse_name()?;
            if !names.insert(field.clone()) {
                return Err(CompileError::new(format!("duplicate cell_roles member '{field}'"), role_span));
            }
            if field == "limits" {
                limits = Some(self.parse_cell_role_limits()?);
            } else {
                if roles.len() == 8 {
                    return Err(CompileError::new("cell_roles admits at most eight named roles", role_span));
                }
                self.expect(TokenKind::Colon)?;
                let schema = self.parse_type()?;
                if !matches!(&schema, Type::Named(_)) {
                    return Err(CompileError::new("a named role requires a Cell schema", role_span));
                }
                roles.push(self.parse_named_cell_role(field, schema, role_span)?);
            }
            self.cell_role_separator()?;
        }
        self.expect(TokenKind::RBrace)?;
        if roles.is_empty() {
            return Err(CompileError::new("cell_roles requires at least one named role", span));
        }
        Ok(CellRolesDef {
            name,
            limits: limits.ok_or_else(|| CompileError::new("cell_roles requires a named limits block", span))?,
            roles,
            span: Span::new(span.start, self.previous_non_newline().span.end, span.line, span.column),
        })
    }

    fn cell_role_separator(&mut self) -> Result<()> {
        if self.check(&TokenKind::Comma) {
            self.advance();
        } else if !self.check(&TokenKind::Newline) && !self.check(&TokenKind::RBrace) {
            return Err(CompileError::new("expected a comma or newline after policy member", self.current().span));
        }
        self.skip_newlines();
        Ok(())
    }

    fn cell_role_number(&mut self) -> Result<u32> {
        let token = self.advance();
        match token.kind {
            TokenKind::Integer(value) => u32::try_from(value).map_err(|_| CompileError::new("role limit must fit in u32", token.span)),
            _ => Err(CompileError::new("role limits require explicit unsigned integer literals", token.span)),
        }
    }

    fn cell_role_interval(&mut self) -> Result<(u32, u32)> {
        let min = self.cell_role_number()?;
        self.expect(TokenKind::Dot)?;
        self.expect(TokenKind::Dot)?;
        let interval_span = self.current().span;
        self.expect(TokenKind::Eq).map_err(|_| CompileError::new("role counts use an inclusive min..=max interval", interval_span))?;
        let max = self.cell_role_number()?;
        if min > max || max > 32 {
            return Err(CompileError::new("role count interval must be ordered within 0..=32", self.current().span));
        }
        Ok((min, max))
    }

    fn cell_role_hash(&mut self) -> Result<[u8; 32]> {
        let token = self.advance();
        let bytes = match &token.kind {
            TokenKind::String(value) => value
                .strip_prefix("0x")
                .filter(|text| text.len() == 64)
                .and_then(|text| hex::decode(text).ok())
                .and_then(|bytes| bytes.try_into().ok()),
            _ => None,
        };
        bytes.ok_or_else(|| CompileError::new("complete Script hash must be a 0x-prefixed 32-byte hex literal", token.span))
    }

    fn parse_cell_role_limits(&mut self) -> Result<CellRoleLimits> {
        self.skip_newlines();
        let span = self.expect(TokenKind::LBrace)?.span;
        self.skip_newlines();
        let mut fields = std::collections::BTreeMap::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field_span = self.current().span;
            let name = self.parse_name()?;
            let maximum = match name.as_str() {
                "input_scan" | "output_scan" => 64,
                "selected_per_side" => 32,
                "aggregate_bytes" => 32768,
                _ => return Err(CompileError::new(format!("unknown cell_roles limit '{name}'"), field_span)),
            };
            self.expect(TokenKind::Colon)?;
            let value = self.cell_role_number()?;
            if !(1..=maximum).contains(&value) {
                return Err(CompileError::new(format!("{name} must be within 1..={maximum}"), field_span));
            }
            if fields.insert(name.clone(), value).is_some() {
                return Err(CompileError::new(format!("duplicate cell_roles limit '{name}'"), field_span));
            }
            self.cell_role_separator()?;
        }
        self.expect(TokenKind::RBrace)?;
        let required = |name: &str| {
            fields.get(name).copied().ok_or_else(|| CompileError::new(format!("missing cell_roles limit '{name}'"), span))
        };
        Ok(CellRoleLimits {
            input_scan: required("input_scan")?,
            output_scan: required("output_scan")?,
            selected_per_side: required("selected_per_side")?,
            aggregate_bytes: required("aggregate_bytes")?,
        })
    }

    fn parse_named_cell_role(&mut self, name: String, schema: Type, span: Span) -> Result<NamedCellRole> {
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();
        let (mut type_hash, mut lock_hash, mut identity_field) = (None, None, None);
        let (mut authority, mut correspondence, mut inputs, mut outputs) = (None, None, None, None);
        let mut seen = BTreeSet::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field_span = self.current().span;
            let field = self.parse_name()?;
            if !seen.insert(field.clone()) {
                return Err(CompileError::new(format!("duplicate policy member '{field}' in role '{name}'"), field_span));
            }
            self.expect(TokenKind::Colon)?;
            match field.as_str() {
                "type_hash" => type_hash = Some(self.cell_role_hash()?),
                "lock_hash" => lock_hash = Some(self.cell_role_hash()?),
                "identity" => identity_field = Some(self.parse_name()?),
                "inputs" => inputs = Some(self.cell_role_interval()?),
                "outputs" => outputs = Some(self.cell_role_interval()?),
                "authority" => {
                    authority = Some(match self.parse_name()?.as_str() {
                        "observe" => CellRoleAuthority::Observe,
                        "local_type" => CellRoleAuthority::LocalType,
                        _ => return Err(CompileError::new("role authority must be observe or local_type", field_span)),
                    });
                }
                "correspondence" => {
                    // preserve/create are keyword tokens in other contexts.
                    let mode = self.advance().clone();
                    correspondence = Some(match mode.text.as_str() {
                        "preserve" => CellRoleCorrespondence::Preserve,
                        "create" => CellRoleCorrespondence::Create,
                        "burn" => CellRoleCorrespondence::Burn,
                        "merge" => {
                            self.expect(TokenKind::LParen)?;
                            let key = self.advance();
                            let TokenKind::ByteString(bytes) = &key.kind else {
                                return Err(CompileError::new("merge requires a literal byte-string identity", key.span));
                            };
                            if !(1..=32).contains(&bytes.len()) {
                                return Err(CompileError::new("merge identity requires 1..=32 bytes", key.span));
                            }
                            let bytes = bytes.clone();
                            self.expect(TokenKind::RParen)?;
                            CellRoleCorrespondence::Merge(bytes)
                        }
                        _ => {
                            return Err(CompileError::new("correspondence must be preserve, create, burn or merge(bytes)", mode.span))
                        }
                    });
                }
                _ => return Err(CompileError::new(format!("unknown policy member '{field}' in role '{name}'"), field_span)),
            }
            self.cell_role_separator()?;
        }
        self.expect(TokenKind::RBrace)?;
        let missing = |field| CompileError::new(format!("role '{name}' requires '{field}'"), span);
        Ok(NamedCellRole {
            type_hash: type_hash.ok_or_else(|| missing("type_hash"))?,
            lock_hash: lock_hash.ok_or_else(|| missing("lock_hash"))?,
            identity_field: identity_field.ok_or_else(|| missing("identity"))?,
            authority: authority.ok_or_else(|| missing("authority"))?,
            correspondence: correspondence.ok_or_else(|| missing("correspondence"))?,
            inputs: inputs.ok_or_else(|| missing("inputs"))?,
            outputs: outputs.ok_or_else(|| missing("outputs"))?,
            name,
            schema,
            span,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{frontend, CellScriptEdition, CompileOptions};

    const SOURCE: &str = include_str!("../../tests/syntax_combo/seeds/named-role-verification.cell");

    fn parse(source: &str) -> Result<Module> {
        frontend::parse(source, CellScriptEdition::Edition2027)
    }

    #[test]
    fn named_cell_roles_retain_structured_policies_and_format_idempotently() {
        let module = parse(SOURCE).unwrap();
        let Item::CellRoles(def) = &module.items[1] else { panic!("catalog missing") };
        assert_eq!(def.name, "Settlement");
        assert_eq!(def.roles[0].name, "orders");
        assert_eq!(def.roles[0].schema, Type::Named("Order".into()));
        assert_eq!(def.roles[0].authority, CellRoleAuthority::Observe);
        assert_eq!(def.roles[0].inputs, (1, 16));
        assert_eq!(
            &SOURCE[def.span.start..def.span.end],
            SOURCE.split("cell_roles").nth(1).unwrap().split("\naction").next().map(|s| format!("cell_roles{s}")).unwrap()
        );
        crate::types::check(&module).unwrap();
        let formatted = crate::fmt::format_default(&module).unwrap();
        let reparsed = parse(&formatted).unwrap();
        assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);
        let Item::CellRoles(roundtrip) = &reparsed.items[1] else { panic!("catalog lost") };
        assert_eq!(crate::cell_selection::named_contract(def).unwrap(), crate::cell_selection::named_contract(roundtrip).unwrap());
    }

    #[test]
    fn named_cell_roles_reject_ambiguous_missing_and_unbounded_declarations() {
        for (source, expected) in [
            (SOURCE.replace("input_scan: 64,", "input_scan: 64, input_scan: 32,"), "duplicate"),
            (SOURCE.replace("input_scan: 64,", ""), "missing cell_roles limit"),
            (SOURCE.replace("input_scan: 64", "input_scan: 65"), "1..=64"),
            (SOURCE.replace("selected_per_side: 32", "selected_per_side: 33"), "1..=32"),
            (SOURCE.replace("aggregate_bytes: 32768", "aggregate_bytes: 32769"), "1..=32768"),
            (SOURCE.replace("input_scan: 64", "input_scan: 4294967296"), "fit in u32"),
            (SOURCE.replace("input_scan: 64", "input_scan: requested"), "integer literals"),
            (SOURCE.replace("input_scan: 64", "input_scan: 32 + 32"), "comma or newline"),
            (SOURCE.replace("input_scan:", "inpt_scan:"), "unknown cell_roles limit"),
            (SOURCE.replace("identity: id", "identity: id, identity: amount"), "duplicate policy"),
            (SOURCE.replace("identity: id,", ""), "requires 'identity'"),
            (SOURCE.replace("identity:", "identty:"), "unknown policy"),
            (SOURCE.replace("authority: observe", "authority: spend"), "observe or local_type"),
            (SOURCE.replace("correspondence: preserve", "correspondence: infer"), "correspondence must be"),
            (SOURCE.replace("inputs: 1..=16", "inputs: 16..=1"), "ordered"),
            (SOURCE.replace("inputs: 1..=16", "inputs: 0..=33"), "0..=32"),
            (SOURCE.replace("inputs: 1..=16", "inputs: 1..16"), "inclusive"),
            (SOURCE.replace("orders: Order", "orders: u64"), "Cell schema"),
            (SOURCE.replace("cell_roles Settlement", "public cell_roles Settlement"), "module-local"),
            (SOURCE.replace("identity: id", "identity: \"id\""), "expected identifier"),
        ] {
            let error = parse(&source).unwrap_err();
            assert!(error.message.contains(expected), "expected {expected}: {error}");
            assert_ne!(error.span, Span::default());
        }
    }

    #[test]
    fn named_cell_roles_resolve_schema_fields_and_do_not_infer_authority() {
        for (source, expected) in [
            (SOURCE.replace("identity: id", "identity: absent"), "identity field"),
            (SOURCE.replace("orders: Order", "orders: Absent"), "Cell-backed"),
            (SOURCE.replace("resource Order has store", "struct Order"), "Cell-backed"),
            (SOURCE.replace("amount: u64", "amount: bool"), "fixed exact roles"),
            (SOURCE.replace("cell_roles Settlement", "cell_roles Order"), "duplicate symbol"),
        ] {
            let error = crate::types::check(&parse(&source).unwrap()).unwrap_err();
            assert!(error.message.contains(expected), "expected {expected}: {error}");
        }
        for authority in ["observe", "local_type"] {
            let module = parse(&SOURCE.replace("authority: observe", &format!("authority: {authority}"))).unwrap();
            crate::types::check(&module).unwrap();
            let Item::CellRoles(def) = &module.items[1] else { panic!("catalog missing") };
            assert_eq!(crate::cell_selection::named_contract(def).unwrap().roles[0].local_type, authority == "local_type");
        }
    }

    #[test]
    fn named_cell_roles_format_policy_variants_and_reject_duplicate_roles() {
        for mode in ["preserve", "create", "burn", r#"merge(b"\x00\x01\x02\x03\x04\x05\x06\x07")"#] {
            let source = SOURCE.replace("correspondence: preserve", &format!("correspondence: {mode}"));
            let module = parse(&source).unwrap();
            crate::types::check(&module).unwrap();
            let formatted = crate::fmt::format_default(&module).unwrap();
            let reparsed = parse(&formatted).unwrap();
            assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);
            let Item::CellRoles(original) = &module.items[1] else { panic!("catalog missing") };
            let Item::CellRoles(roundtrip) = &reparsed.items[1] else { panic!("catalog missing") };
            assert_eq!(original.roles[0].correspondence, roundtrip.roles[0].correspondence);
        }
        let declaration = SOURCE.split_once("    orders:").unwrap().1.split_once("\n}\naction").unwrap().0;
        let repeated = SOURCE.replace("\n}\naction", &format!("\n    orders:{declaration}\n}}\naction"));
        assert!(parse(&repeated).unwrap_err().message.contains("duplicate cell_roles member"));
        let source = SOURCE.replace("correspondence: preserve", r#"correspondence: merge(b"\x00")"#);
        assert!(crate::types::check(&parse(&source).unwrap()).unwrap_err().message.contains("merge target byte length"));
    }

    #[test]
    fn named_cell_roles_reserve_names_against_imports_and_value_types() {
        for source in [
            SOURCE.replace("cell_roles Settlement", "cell_roles Order"),
            SOURCE.replace("resource Order", "use foreign::Schema as Settlement\nresource Order"),
            format!("{SOURCE}\nuse foreign::Schema as Settlement\n"),
        ] {
            let mut resolver = crate::resolve::ModuleResolver::new();
            let error = resolver.register_module(parse(&source).unwrap()).unwrap_err();
            assert!(error.message.contains("duplicate symbol"), "{error}");
        }
    }

    #[test]
    fn named_cell_roles_verify_through_the_existing_checked_contract() {
        let error = frontend::parse(SOURCE, CellScriptEdition::Edition2026).unwrap_err();
        assert!(error.message.contains("Edition 2027"), "{error}");
        for target in [None, Some("riscv64-elf".into())] {
            let result =
                crate::compile(SOURCE, CompileOptions { edition: CellScriptEdition::Edition2027, target, ..Default::default() })
                    .unwrap();
            assert!(!result.metadata.runtime.fail_closed_runtime_features.contains(&"exact-role-selection-artifact-proof".into()));
        }
        for (source, expected) in [
            (SOURCE.replace("verify_cell_roles<Settlement>()", "verify_cell_roles<Unknown>()"), "unknown local"),
            (SOURCE.replace("verify_cell_roles<Settlement>()", "verify_cell_roles<Settlement>(1)"), "without runtime arguments"),
        ] {
            let error =
                crate::compile(&source, CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() }).unwrap_err();
            assert!(error.message.contains(expected), "expected {expected}: {error}");
        }
    }

    #[test]
    fn named_cell_roles_select_alias_and_read_without_repeating_schema_or_role() {
        let source = include_str!("../../tests/syntax_combo/seeds/named-role-selection.cell");
        for opt_level in [0, 3] {
            crate::compile(
                source,
                CompileOptions {
                    edition: CellScriptEdition::Edition2027,
                    opt_level,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("O{opt_level}: {error}"));
        }
        let formatted = crate::fmt::format_default(&parse(source).unwrap()).unwrap();
        assert_eq!(crate::fmt::format_default(&parse(&formatted).unwrap()).unwrap(), formatted);
        for (changed, expected) in [
            (source.replace("let orders = selected.orders", "let orders = selected.absent"), "no available named role"),
            (source.replace("alias.input(count - 1)", "alias.input<Order>(count - 1)"), "no available named role"),
            (source.replace("alias.input(count - 1)", "alias.input(0, count - 1)"), "derive their schema"),
            (source.replace("alias.input(count - 1)", "alias.input(true)"), "must be u64"),
            (source.replace("alias.input(count - 1)", "selected.input(count - 1)"), "select a named role"),
            (source.replace("let orders = selected.orders", "let mut orders = selected.orders"), "immutable"),
            (source.replace("let alias = orders", "let alias = orders.refunds"), "no available named role"),
            (source.replace("alias.input(count - 1)", "ckb::selected_input<Order>(selected, 0, count - 1)"), "instead of numeric"),
            (source.replace("let alias = orders", "let alias = [orders]"), "cannot be wrapped"),
        ] {
            let error = crate::types::check(&parse(&changed).unwrap()).unwrap_err();
            assert!(error.message.contains(expected), "expected {expected}: {error}");
        }
    }
}
