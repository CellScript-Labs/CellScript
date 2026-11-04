use super::*;
use crate::nested_values::{self, NestedType};

impl TypeChecker<'_> {
    pub(super) fn source_nested_type(&self, ty: &Type) -> Option<std::sync::Arc<NestedType>> {
        self.nested_descriptions.source(ty).ok().flatten()
    }

    pub(super) fn nested_readonly_helper(&self, name: &str) -> bool {
        let local_name =
            self.current_module.as_deref().and_then(|module| name.strip_prefix(module)?.strip_prefix("::")).unwrap_or(name);
        let helper = self.functions.get(local_name).map(|signature| signature.kind == CallableKind::Function).unwrap_or_else(|| {
            self.resolve_function(name).is_some_and(|function| function_def_kind(&function) == CallableKind::Function)
        });
        if helper {
            self.require_effect(name, false, Span::default(), "nested-view helper must be observation-only");
        }
        helper
    }

    pub(super) fn nested_record(&self, element: &str) -> Result<nested_values::RecordRef> {
        self.nested_descriptions.record(element, || {
            if let Some(fields) = self.nested_record_fields.get(element) {
                return Ok((self.current_module.clone().unwrap_or_default(), element.to_string(), fields.clone()));
            }
            self.resolver.zip(self.current_module.as_deref()).and_then(|(resolver, module)| {
                match resolver.resolve_type_with_module(module, element)? {
                    (owner, TypeDef::Struct(record)) => Some((owner, record.name, record.fields.into_iter().map(|field| (field.name, field.ty)).collect())),
                    _ => None,
                }
            }).ok_or_else(|| CompileError::new(
                format!("nested element '{element}' must be an ordinary record with fixed identity fields and one trailing Vec<u8> payload"), Span::default()))
        })
    }

    pub(super) fn validate_nested_type(&self, nested: &NestedType) -> Result<()> {
        let resolved = self.nested_descriptions.resolve(nested, |element| self.nested_record(element))?;
        if let NestedType::Values(values) = resolved.as_ref() {
            values.contract(values.element.shape()).validate().map_err(|error| CompileError::without_span(error.to_string()))?;
        }
        Ok(())
    }

    pub(super) fn nested_field_type(&self, ty: &Type, field: &str) -> Result<Option<Type>> {
        let Some(parsed) = self.nested_descriptions.source(ty)? else {
            return Ok(None);
        };
        if !matches!(parsed.as_ref(), NestedType::RecordView { .. }) {
            return Ok(None);
        }
        let resolved = self.nested_descriptions.resolve(&parsed, |element| self.nested_record(element))?;
        let NestedType::RecordView { element: nested_values::Element::Record(record), max_payload_bytes } = resolved.as_ref() else {
            return Ok(None);
        };
        record
            .layout()
            .field_type(field, *max_payload_bytes)
            .map(Some)
            .ok_or_else(|| CompileError::new(format!("unknown nested record field '{}.{field}'", record.spelling()), Span::default()))
    }
}

pub(super) fn whole_values(ty: &Type) -> bool {
    matches!(nested_values::from_source(ty), Ok(Some(NestedType::Values(_))))
}

pub(super) fn direct_view(ty: &Type) -> bool {
    nested_values::from_source(ty).is_ok_and(|view| view.is_some())
}

pub(super) fn contains_nested_type(ty: &Type) -> bool {
    match ty {
        Type::Applied(application) => {
            nested_values::is_constructor(&application.constructor)
                || application.arguments.iter().filter_map(TypeArgument::as_type).any(contains_nested_type)
        }
        Type::Named(name) => nested_values::is_nested_name(name),
        Type::Ref(inner) | Type::MutRef(inner) | Type::Array(inner, _) => contains_nested_type(inner),
        Type::Tuple(items) => items.iter().any(contains_nested_type),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(source: &str) -> Module {
        crate::parser::parse(&crate::lexer::lex(source).unwrap()).unwrap()
    }

    #[test]
    fn checker_reuses_nested_descriptions_within_one_resolver_revision() {
        let source = "module consumer\nuse library::Entry as Alias\n";
        for (field, expected) in [("u64", Type::U64), ("u8", Type::U8)] {
            let mut resolver = ModuleResolver::new();
            resolver
                .register_module(module(&format!("module library\npublic struct Entry {{ key: {field}, payload: Vec<u8> }}")))
                .unwrap();
            let consumer = module(source);
            resolver.register_module(consumer.clone()).unwrap();
            let mut checker = TypeChecker::with_resolver(&resolver, "consumer");
            checker.check_module(&consumer).unwrap();
            let ty = Type::applied(
                "NestedRecordView",
                vec![TypeArgument::Type(Type::Named("Alias".into())), TypeArgument::UnsignedConstant(8)],
            );
            for _ in 0..128 {
                assert_eq!(checker.nested_field_type(&ty, "key").unwrap(), Some(expected.clone()));
                assert_eq!(checker.nested_field_type(&ty, "payload").unwrap(), Some(nested_values::bytes_view(8)));
            }
            assert_eq!(checker.nested_descriptions.retained_counts(), (1, 1, 1));
        }
    }
}
