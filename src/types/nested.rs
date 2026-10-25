use super::*;
use crate::nested_values::{self, NestedType};

impl TypeChecker<'_> {
    pub(super) fn nested_readonly_helper(&self, name: &str) -> bool {
        let local_name =
            self.current_module.as_deref().and_then(|module| name.strip_prefix(module)?.strip_prefix("::")).unwrap_or(name);
        if let Some(signature) = self.functions.get(local_name) {
            return signature.kind == CallableKind::Function && matches!(signature.effect, EffectClass::Pure | EffectClass::ReadOnly);
        }
        self.resolve_function(name).is_some_and(|function| {
            function_def_kind(&function) == CallableKind::Function
                && matches!(function_def_effect(&function), EffectClass::Pure | EffectClass::ReadOnly)
        })
    }

    pub(super) fn checked_nested_record_fields(&self, element: &str) -> Result<Vec<(String, Type)>> {
        let fields = self.nested_record_fields.get(element).cloned().or_else(|| {
            let (resolver, module) = self.resolver.zip(self.current_module.as_deref())?;
            match resolver.resolve_type(module, element)? {
                TypeDef::Struct(record) => Some(record.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
                _ => None,
            }
        }).ok_or_else(|| CompileError::new(
            format!("nested element '{element}' must be an ordinary record with fixed identity fields and one trailing Vec<u8> payload"), Span::default()))?;
        nested_values::record_shape(&fields)?;
        Ok(fields)
    }

    pub(super) fn validate_nested_type(&self, nested: &NestedType) -> Result<()> {
        match nested {
            NestedType::Values(values) if !nested_values::is_bytes_element(&values.element) => {
                self.checked_nested_record_fields(&values.element)?;
            }
            NestedType::RecordView { element, .. } => {
                self.checked_nested_record_fields(element)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn nested_field_type(&self, ty: &Type, field: &str) -> Result<Option<Type>> {
        let Type::Named(name) = ty else {
            return Ok(None);
        };
        let Some(NestedType::RecordView { element, max_payload_bytes }) = nested_values::parsed(name) else {
            return Ok(None);
        };
        let fields = self.checked_nested_record_fields(&element)?;
        let (index, (_, ty)) = fields
            .iter()
            .enumerate()
            .find(|(_, (name, _))| name == field)
            .ok_or_else(|| CompileError::new(format!("unknown nested record field '{element}.{field}'"), Span::default()))?;
        Ok(Some(if index + 1 == fields.len() { Type::Named(nested_values::bytes_view_type(max_payload_bytes)) } else { ty.clone() }))
    }
}

pub(super) fn whole_values(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if matches!(nested_values::parsed(name), Some(NestedType::Values(_))))
}

pub(super) fn direct_view(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if nested_values::parsed(name).is_some())
}

pub(super) fn contains_nested_type(ty: &Type) -> bool {
    match ty {
        Type::Named(name) => {
            nested_values::is_nested_name(name)
                || crate::generics::applied_type(name)
                    .is_some_and(|(_, args)| args.iter().any(|arg| contains_nested_type(&Type::Named(arg.clone()))))
        }
        Type::Ref(inner) | Type::MutRef(inner) | Type::Array(inner, _) => contains_nested_type(inner),
        Type::Tuple(items) => items.iter().any(contains_nested_type),
        _ => false,
    }
}
