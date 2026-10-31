//! Resolved nested-view types. No internal reference is serialized: all wire
//! boundaries render the established canonical source text explicitly.
use super::*;
use crate::nested_values::{Element, NestedType};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NestedViewType(Arc<NestedType<Element>>);

impl NestedViewType {
    pub(crate) fn new(description: Arc<NestedType<Element>>) -> Self {
        Self(description)
    }

    pub(crate) fn description(&self) -> &NestedType<Element> {
        &self.0
    }

    pub(crate) fn canonical_text(&self) -> String {
        self.0.canonical_text()
    }

    pub(crate) fn index_type(&self) -> Option<IrType> {
        match self.description() {
            NestedType::Values(values) => Some(IrType::Nested(Self::new(Arc::new(match &values.element {
                Element::Bytes => NestedType::BytesView { max_payload_bytes: values.max_payload_bytes },
                Element::Record(record) => {
                    NestedType::RecordView { element: Element::Record(record.clone()), max_payload_bytes: values.max_payload_bytes }
                }
            })))),
            NestedType::BytesView { .. } => Some(IrType::U8),
            NestedType::RecordView { .. } => None,
        }
    }

    pub(crate) fn field_type(&self, field: &str) -> Option<IrType> {
        let NestedType::RecordView { element: Element::Record(record), max_payload_bytes } = self.description() else { return None };
        let layout = record.layout();
        if field == layout.payload_field {
            Some(IrType::Nested(Self::new(Arc::new(NestedType::BytesView { max_payload_bytes: *max_payload_bytes }))))
        } else {
            layout.fixed_fields.iter().find(|(name, _)| name == field).map(|(_, ty)| ast_type_to_ir(&ty.source_type()))
        }
    }
}

impl IrGenerator<'_> {
    pub(super) fn prepare_nested_types(&mut self, ast: &Module) {
        self.nested_source_records = ast
            .items
            .iter()
            .filter_map(|item| if let Item::Struct(record) = item { Some((record.name.clone(), record.clone())) } else { None })
            .collect();
    }

    /// This is the source-text-to-IR boundary, including imported source ABI
    /// projections. Recognized malformed names record a diagnostic; a recovery
    /// Unit cannot escape the generator's checked completion path.
    pub(super) fn resolve_projection_type(&self, ty: &IrType) -> IrType {
        match ty {
            IrType::Named(name) => {
                let result = self.nested_descriptions.parse(name).and_then(|parsed| {
                    parsed
                        .map(|nested| {
                            self.nested_descriptions.resolve(&nested, |element| {
                                self.nested_descriptions.record(element, || {
                                    if let Some(record) = self.nested_source_records.get(element) {
                                        return Ok((
                                            self.module.name.clone(),
                                            record.name.clone(),
                                            record.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                                        ));
                                    }
                                    self.nested_resolver
                                        .and_then(|resolver| resolver.resolve_type_with_module(&self.module.name, element))
                                        .and_then(|(owner, definition)| match definition {
                                            TypeDef::Struct(record) => Some((
                                                owner,
                                                record.name,
                                                record.fields.into_iter().map(|field| (field.name, field.ty)).collect(),
                                            )),
                                            _ => None,
                                        })
                                        .ok_or_else(|| {
                                            CompileError::without_span(format!(
                                                "nested record '{element}' has no defining declaration"
                                            ))
                                        })
                                })
                            })
                        })
                        .transpose()
                });
                match result {
                    Ok(Some(description)) => IrType::Nested(NestedViewType::new(description)),
                    Ok(None) => ty.clone(),
                    Err(error) => {
                        self.errors.borrow_mut().push(error);
                        IrType::Unit
                    }
                }
            }
            IrType::Array(element, length) => IrType::Array(Box::new(self.resolve_projection_type(element)), *length),
            IrType::Tuple(items) => IrType::Tuple(items.iter().map(|ty| self.resolve_projection_type(ty)).collect()),
            IrType::Ref(inner) => IrType::Ref(Box::new(self.resolve_projection_type(inner))),
            IrType::MutRef(inner) => IrType::MutRef(Box::new(self.resolve_projection_type(inner))),
            _ => ty.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(source: &str) -> Module {
        crate::parser::parse(&crate::lexer::lex(source).unwrap()).unwrap()
    }

    #[test]
    fn nested_index_and_field_results_retain_resolved_descriptions() {
        let ast = module("module nested_ir\nstruct Entry { key: u64, payload: Vec<u8> }\nfn inspect(values: NestedValues<Entry, 2, 8, 128>) -> u64 { let entry = values[0] let bytes = entry.payload return entry.key + bytes.len() }\naction main(witness values: NestedValues<Entry, 2, 8, 128>) -> u64 { verification return inspect(values) }");
        crate::types::check(&ast).unwrap();
        let ir = super::super::generate(&ast).unwrap();
        let helper =
            ir.items.iter().find_map(|item| if let IrItem::Function(function) = item { Some(function) } else { None }).unwrap();
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
            resolver
                .register_module(module(&format!("module {owner}\npublic struct Entry {{ key: u64, payload: Vec<u8> }}")))
                .unwrap();
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
        let mut ir =
            super::super::generate(&module("module invalid_named\naction main(witness value: u64) { verification }")).unwrap();
        let IrItem::Action(action) = &mut ir.items[0] else { panic!() };
        action.params[0].ty = IrType::Named("NestedValues<Vec<u8>, 2, 8, 128>".into());
        assert_eq!(crate::executable_surface::validate_ir_module(&ir).unwrap_err().code.as_deref(), Some("E2105"));
    }
}
