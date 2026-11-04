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

    pub(super) fn with_record_spelling(&self, map: impl FnOnce(&str) -> String) -> Self {
        let mut description = self.description().clone();
        let element = match &mut description {
            NestedType::Values(values) => Some(&mut values.element),
            NestedType::RecordView { element, .. } => Some(element),
            NestedType::BytesView { .. } => None,
        };
        if let Some(Element::Record(record)) = element {
            *record = record.with_spelling(map(record.spelling()));
        }
        Self::new(Arc::new(description))
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

    /// Resolve typed source applications before they enter executable IR.
    /// Missing declarations and malformed descriptions record a diagnostic;
    /// recovery Unit cannot escape the checked completion path.
    pub(super) fn resolve_projection_type(&self, ty: &IrType) -> IrType {
        let result = crate::nested_values::from_ir_application(ty).and_then(|parsed| {
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
                                    CompileError::without_span(format!("nested record '{element}' has no defining declaration"))
                                })
                        })
                    })
                })
                .transpose()
        });
        match result {
            Ok(Some(description)) => return IrType::Nested(NestedViewType::new(description)),
            Ok(None) => {}
            Err(error) => {
                self.errors.borrow_mut().push(error);
                return IrType::Unit;
            }
        }
        match ty {
            IrType::Commitment(value) => IrType::Commitment(Box::new(self.resolve_projection_type(value))),
            IrType::Opening(value) => IrType::Opening(Box::new(self.resolve_projection_type(value))),
            IrType::Applied(application) => IrType::Applied(Box::new(application.map_types(|ty| self.resolve_projection_type(ty)))),
            IrType::Bounded(collection) => IrType::Bounded(Box::new(collection.map_element(|ty| self.resolve_projection_type(ty)))),
            IrType::Context(view) => IrType::Context(view.map(|schema| Box::new(self.resolve_projection_type(schema)))),
            IrType::Array(element, length) => IrType::Array(Box::new(self.resolve_projection_type(element)), *length),
            IrType::Vector(element) => IrType::Vector(element.as_ref().map(|element| Box::new(self.resolve_projection_type(element)))),
            IrType::Tuple(items) => IrType::Tuple(items.iter().map(|ty| self.resolve_projection_type(ty)).collect()),
            IrType::Ref(inner) => IrType::Ref(Box::new(self.resolve_projection_type(inner))),
            IrType::MutRef(inner) => IrType::MutRef(Box::new(self.resolve_projection_type(inner))),
            _ => ty.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
