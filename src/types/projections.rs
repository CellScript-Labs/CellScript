use super::{
    TypeChecker, CKB_BLOCK_NUMBER_TYPE, CKB_ENCODED_SINCE_TYPE, CKB_EPOCH_LENGTH_TYPE, CKB_EPOCH_NUMBER_TYPE,
    CKB_HEADER_DEP_VIEW_TYPE, CKB_LOCK_SCRIPT_REF_TYPE, CKB_OUT_POINT_TYPE, CKB_SCRIPT_ARGS_TYPE, CKB_SCRIPT_HASH_TYPE,
    CKB_SCRIPT_VALUE_TYPE, CKB_SCRIPT_VIEW_TYPE, CKB_TIMESTAMP_MILLIS_TYPE, CKB_TYPE_SCRIPT_REF_TYPE, CKB_WITNESS_ARGS_VIEW_TYPE,
    CKB_WITNESS_BYTES_VIEW_TYPE,
};
use crate::ast::Type;
use crate::error::{CompileError, Result, Span};
use std::collections::HashMap;

impl TypeChecker<'_> {
    pub(super) fn index_result_type(&self, ty: &Type, span: Span) -> Result<Type> {
        if let Some(element) = ty.vector_element() {
            return Ok(element.clone());
        }
        match self.nested_descriptions.source(ty)?.as_deref() {
            Some(crate::nested_values::NestedType::Values(values)) => return Ok(values.item_type()),
            Some(crate::nested_values::NestedType::BytesView { .. }) => return Ok(Type::U8),
            _ => {}
        }
        match ty {
            Type::Array(elem, _) => Ok((**elem).clone()),
            Type::Ref(inner) | Type::MutRef(inner) => self.index_result_type(inner, span),
            _ => Err(CompileError::new("indexing requires an array-like value", span)),
        }
    }

    pub(super) fn iter_item_type(&self, ty: &Type, span: Span) -> Result<Type> {
        if let Some(element) = ty.vector_element() {
            return Ok(element.clone());
        }
        if matches!(
            self.source_nested_type(ty).as_deref(),
            Some(crate::nested_values::NestedType::Values(_) | crate::nested_values::NestedType::BytesView { .. })
        ) {
            return self.index_result_type(ty, span);
        }
        match ty {
            Type::Array(elem, _) => Ok((**elem).clone()),
            Type::Ref(inner) => Ok(Type::Ref(Box::new(self.iter_item_type(inner, span)?))),
            Type::MutRef(inner) => Ok(Type::MutRef(Box::new(self.iter_item_type(inner, span)?))),
            Type::Named(name) if name == "Range" => Ok(Type::U64),
            _ => Err(CompileError::new("for-loop iterable must be a range or collection type", span)),
        }
    }

    pub(super) fn supports_collection_len(&self, ty: &Type) -> bool {
        if matches!(
            self.source_nested_type(ty).as_deref(),
            Some(crate::nested_values::NestedType::Values(_) | crate::nested_values::NestedType::BytesView { .. })
        ) {
            return true;
        }
        match ty {
            Type::Array(_, _) => true,
            Type::Applied(_) => ty.vector_element().is_some(),
            Type::Ref(inner) | Type::MutRef(inner) => self.supports_collection_len(inner),
            Type::Named(name) => name == "Vec",
            _ => false,
        }
    }

    pub(super) fn slice_item_type(ty: &Type) -> Option<Type> {
        match ty {
            Type::Array(inner, _) => Some((**inner).clone()),
            Type::Ref(inner) | Type::MutRef(inner) => Self::slice_item_type(inner),
            _ => None,
        }
    }

    pub(super) fn lookup_field_type(&self, ty: &Type, field: &str, span: Span) -> Result<Type> {
        if ty.as_witness_bytes_view().is_some() {
            return match field {
                "size" => Ok(Type::U64),
                _ => Err(CompileError::new(format!("unknown WitnessBytesView field '{}'; expected size", field), span)),
            };
        }
        if let Type::Selection(selection) = ty {
            return selection
                .project_role(field)
                .map(|projected| Type::Selection(Box::new(projected)))
                .ok_or_else(|| CompileError::new(format!("selection has no available named role '{field}'"), span));
        }
        if let Some(ty) = self.cell_data_view_field_type(ty, field, span)? {
            return Ok(ty);
        }
        if let Some(ty) = self.nested_field_type(ty, field)? {
            return Ok(ty);
        }
        if let Some(
            view @ (crate::ast::ContextView::Input(_) | crate::ast::ContextView::Output(_) | crate::ast::ContextView::CellDep),
        ) = ty.as_context_view()
        {
            let input = matches!(view, crate::ast::ContextView::Input(_));
            let output = matches!(view, crate::ast::ContextView::Output(_));
            return match field {
                "capacity" | "occupied_capacity" | "unoccupied_capacity" | "data_size" => Ok(Type::U64),
                "output_index" if output => Ok(Type::U64),
                "lock_hash" | "type_hash" => Ok(Type::Named(CKB_SCRIPT_HASH_TYPE.to_string())),
                "data_hash" => Ok(Type::Hash),
                "lock" | "type" | "type_script" => {
                    let result = Type::context_view(crate::ast::ContextView::Script);
                    self.validate_type(&result)?;
                    Ok(result)
                }
                "out_point" if input => Ok(Type::Named(CKB_OUT_POINT_TYPE.to_string())),
                "since" if input => Ok(Type::Named(CKB_ENCODED_SINCE_TYPE.to_string())),
                _ => Err(CompileError::new(format!(
                    "unknown transaction-view field '{}'; expected capacity, occupied_capacity, unoccupied_capacity, data_size, data_hash, lock_hash, type_hash, lock, type_script{}",
                    field, if input { ", out_point, or since" } else if output { ", or output_index" } else { "" }
                ), span)),
            };
        }
        match ty {
            Type::U64 if field == "lock" => Ok(Type::Named(CKB_LOCK_SCRIPT_REF_TYPE.to_string())),
            Type::U64 if field == "type" || field == "type_script" => Ok(Type::Named(CKB_TYPE_SCRIPT_REF_TYPE.to_string())),
            Type::Address | Type::Hash => {
                if field == "0" {
                    return Ok(Type::Array(Box::new(Type::U8), 32));
                }
                Err(CompileError::new(format!("builtin value '{:?}' only exposes tuple field '0'", ty), span))
            }
            Type::Tuple(items) => {
                if let Ok(index) = field.parse::<usize>() {
                    if let Some(item_ty) = items.get(index) {
                        return Ok(item_ty.clone());
                    }
                    return Err(CompileError::new(format!("tuple field '{}' is out of bounds", field), span));
                }
                Err(CompileError::new(format!("tuple field '{}' must be a numeric index", field), span))
            }
            Type::Ref(inner) | Type::MutRef(inner) => self.lookup_field_type(inner, field, span),
            Type::Named(name) => {
                let base_name = name.as_str();
                if base_name == CKB_HEADER_DEP_VIEW_TYPE {
                    return match field {
                        "epoch_number" => Ok(Type::Named(CKB_EPOCH_NUMBER_TYPE.to_string())),
                        "epoch_start_block_number" => Ok(Type::Named(CKB_BLOCK_NUMBER_TYPE.to_string())),
                        "epoch_length" => Ok(Type::Named(CKB_EPOCH_LENGTH_TYPE.to_string())),
                        "block_number" => Ok(Type::Named(CKB_BLOCK_NUMBER_TYPE.to_string())),
                        "timestamp" => Ok(Type::Named(CKB_TIMESTAMP_MILLIS_TYPE.to_string())),
                        _ => Err(CompileError::new(
                            format!(
                                "unknown HeaderDepView field '{}'; expected epoch_number, epoch_start_block_number, epoch_length, block_number, or timestamp",
                                field
                            ),
                            span,
                        )),
                    };
                }
                if base_name == CKB_WITNESS_ARGS_VIEW_TYPE {
                    return match field {
                        "size" => Ok(Type::U64),
                        "lock" | "input_type" | "output_type" => Err(CompileError::new(
                            format!("removed legacy byte-buffer field '{field}'; use a bounded witness field view or witness::{field}_exact32(view)"),
                            span,
                        )),
                        _ => Err(CompileError::new(
                            format!("unknown WitnessArgsView field '{}'; expected size; use bounded field views or explicit exact32 reads", field),
                            span,
                        )),
                    };
                }
                if base_name == CKB_WITNESS_BYTES_VIEW_TYPE {
                    return match field {
                        "size" => Ok(Type::U64),
                        _ => Err(CompileError::new(format!("unknown WitnessBytesView field '{}'; expected size", field), span)),
                    };
                }
                if base_name == CKB_OUT_POINT_TYPE {
                    return match field {
                        "index" => Ok(Type::U32),
                        "tx_hash" => Ok(Type::Hash),
                        _ => Err(CompileError::new(format!("unknown OutPoint field '{}'; expected tx_hash or index", field), span)),
                    };
                }
                if base_name == CKB_SCRIPT_VIEW_TYPE {
                    return match field {
                        "hash" => Ok(Type::Named(CKB_SCRIPT_HASH_TYPE.to_string())),
                        "code_hash" => Ok(Type::Hash),
                        "args32" => Ok(Type::Array(Box::new(Type::U8), 32)),
                        "args_hash" => Err(CompileError::new(
                            "removed legacy byte-buffer field 'args_hash'; use args32 for exactly 32 raw argument bytes",
                            span,
                        )),
                        "hash_type" => Ok(Type::U64),
                        "args_empty" => Ok(Type::Bool),
                        _ => Err(CompileError::new(
                            format!(
                                "unknown ScriptView field '{}'; expected hash, code_hash, hash_type, args_empty, or args32",
                                field
                            ),
                            span,
                        )),
                    };
                }
                if base_name == CKB_SCRIPT_HASH_TYPE && field == "0" {
                    return Ok(Type::Array(Box::new(Type::U8), 32));
                }
                if base_name == CKB_LOCK_SCRIPT_REF_TYPE || base_name == CKB_TYPE_SCRIPT_REF_TYPE {
                    return match field {
                        "code_hash" => Ok(Type::Hash),
                        "args32" => Ok(Type::Array(Box::new(Type::U8), 32)),
                        "args_hash" => Err(CompileError::new(
                            "removed legacy byte-buffer field 'args_hash'; use args32 for exactly 32 raw argument bytes",
                            span,
                        )),
                        "hash_type" => Ok(Type::U64),
                        "args_empty" => Ok(Type::Bool),
                        _ => Err(CompileError::new(
                            format!("unknown ScriptRef field '{}'; expected code_hash, hash_type, args_empty, or args32", field),
                            span,
                        )),
                    };
                }
                if base_name == CKB_SCRIPT_VALUE_TYPE {
                    return match field {
                        "code_hash" => Ok(Type::Hash),
                        "hash_type" => Ok(Type::U64),
                        "args" => Ok(Type::Named(CKB_SCRIPT_ARGS_TYPE.to_string())),
                        _ => Err(CompileError::new(
                            format!("unknown Script field '{}'; expected code_hash, hash_type, or args", field),
                            span,
                        )),
                    };
                }
                if base_name == CKB_SCRIPT_ARGS_TYPE {
                    return match field {
                        "len" => Ok(Type::U64),
                        "is_empty" => Ok(Type::Bool),
                        _ => Err(CompileError::new(format!("unknown ScriptArgs field '{}'; expected len or is_empty", field), span)),
                    };
                }
                if let Some(fields) = self.resolve_named_type_fields(base_name)
                    && let Some(field_ty) = fields.get(field)
                {
                    return Ok(field_ty.clone());
                }
                Err(CompileError::new(format!("unknown field '{}' on type '{}'", field, base_name), span))
            }
            _ => Err(CompileError::new(format!("type '{:?}' does not support field access", ty), span)),
        }
    }

    pub(super) fn resolve_named_type_fields(&self, type_name: &str) -> Option<HashMap<String, Type>> {
        let base_name = type_name;
        if let Some(fields) = self.type_fields.get(base_name) {
            return Some(fields.clone());
        }
        self.resolver
            .zip(self.current_module.as_deref())
            .and_then(|(resolver, module)| resolver.type_fields(module, base_name))
            .map(|fields| fields.into_iter().collect())
    }

    pub(super) fn resolve_flow_states(&self, type_name: &str) -> Option<Vec<String>> {
        let base_name = type_name;
        if let Some(states) = self.flow_states.get(base_name) {
            return Some(states.clone());
        }
        None
    }
}
