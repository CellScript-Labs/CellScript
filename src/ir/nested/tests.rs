use super::*;

fn module(source: &str) -> Module {
    crate::parser::parse(&crate::lexer::lex(source).unwrap()).unwrap()
}

#[test]
fn nested_index_and_field_results_retain_resolved_descriptions() {
    let ast = module("module nested_ir\nstruct Entry { key: u64, payload: Vec<u8> }\nfn inspect(values: NestedValues<Entry, 2, 8, 128>) -> u64 { let entry = values[0] let bytes = entry.payload return entry.key + bytes.len() }\naction main(witness values: NestedValues<Entry, 2, 8, 128>) -> u64 { verification return inspect(values) }");
    crate::types::check(&ast).unwrap();
    let ir = super::super::generate(&ast).unwrap();
    let helper = ir.items.iter().find_map(|item| if let IrItem::Function(function) = item { Some(function) } else { None }).unwrap();
    let IrType::Nested(values) = &helper.params[0].ty else { panic!("nested parameter must be resolved") };
    assert_eq!(values.canonical_text(), "NestedValues<Entry, 2, 8, 128>");
    let mut indexed = false;
    let mut payload = false;
    let mut fixed = false;
    for instruction in helper.body.blocks.iter().flat_map(|block| &block.instructions) {
        match instruction {
            IrInstruction::Index { dest, .. } => {
                let IrType::Nested(record) = &dest.ty else { panic!("index must derive a structured record view") };
                assert_eq!(record.canonical_text(), "NestedRecordView<Entry, 8>");
                assert_eq!(
                    record.field_type("payload"),
                    Some(IrType::Nested(NestedViewType::new(Arc::new(NestedType::BytesView { max_payload_bytes: 8 }))))
                );
                indexed = true;
            }
            IrInstruction::FieldAccess { dest, field, .. } if field == "payload" => {
                let IrType::Nested(bytes) = &dest.ty else { panic!("payload must retain its structured bound") };
                assert_eq!(bytes.canonical_text(), "NestedBytesView<8>");
                assert_eq!(bytes.index_type(), Some(IrType::U8));
                payload = true;
            }
            IrInstruction::FieldAccess { dest, field, .. } if field == "key" => {
                assert_eq!(dest.ty, IrType::U64);
                fixed = true;
            }
            _ => {}
        }
    }
    assert!(indexed && payload && fixed);
}

#[test]
fn nested_ir_equality_uses_the_declaration_while_rendering_keeps_aliases() {
    let mut resolver = ModuleResolver::new();
    for owner in ["left", "right"] {
        resolver.register_module(module(&format!("module {owner}\npublic struct Entry {{ key: u64, payload: Vec<u8> }}"))).unwrap();
    }
    let ast = module("module consumer\nuse left::Entry as First\nuse left::Entry as Alias\nuse right::Entry as Other\naction inspect(witness first: NestedValues<First, 2, 8, 128>, witness alias: NestedValues<Alias, 2, 8, 128>, witness other: NestedValues<Other, 2, 8, 128>) { verification require first.len() == 0 require alias.len() == 0 require other.len() == 0 }");
    resolver.register_module(ast.clone()).unwrap();
    crate::types::check_with_resolver(&ast, &resolver, "consumer").unwrap();
    let ir = super::super::generate_with_resolver(&ast, &resolver, "consumer").unwrap();
    let action = ir.items.iter().find_map(|item| if let IrItem::Action(action) = item { Some(action) } else { None }).unwrap();
    assert_eq!(action.params[0].ty, action.params[1].ty);
    assert_ne!(action.params[0].ty, action.params[2].ty);
    for (param, spelling) in action.params.iter().zip(["First", "Alias", "Other"]) {
        let IrType::Nested(view) = &param.ty else { panic!("nested parameter must be resolved") };
        assert_eq!(view.canonical_text(), format!("NestedValues<{spelling}, 2, 8, 128>"));
    }
}

#[test]
fn malformed_source_descriptions_and_unresolved_named_ir_reject_without_artifacts() {
    let ast = module("module invalid_nested\naction main(witness values: NestedValues<Missing, 2, 8, 128>) { verification }");
    assert!(super::super::generate(&ast).is_err());
    let mut ir = super::super::generate(&module("module invalid_named\naction main(witness value: u64) { verification }")).unwrap();
    let IrItem::Action(action) = &mut ir.items[0] else { panic!() };
    action.params[0].ty = IrType::Named("NestedValues<Vec<u8>, 2, 8, 128>".into());
    assert_eq!(crate::executable_surface::validate_ir_module(&ir).unwrap_err().code.as_deref(), Some("E2105"));
}
