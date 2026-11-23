use super::{IrConst, IrGenerator, IrOperand, IrType, CKB_SCRIPT_ARGS_TYPE, CKB_SCRIPT_HASH_TYPE, CKB_SCRIPT_VALUE_TYPE};

impl IrGenerator<'_> {
    pub(super) fn lower_flow_state_name(&self, name: &str) -> Option<IrOperand> {
        let (type_name, _) = name.rsplit_once("::")?;
        self.lower_flow_state_operand(type_name, name)
    }

    pub(super) fn lower_flow_state_operand(&self, type_name: &str, name: &str) -> Option<IrOperand> {
        let index = self.flow_state_index(type_name, name)? as u64;
        let field_ty = self.flow_state_field_ir_type(type_name).unwrap_or(IrType::U64);
        let value = Self::integer_const_for_expected_type(index.into(), &field_ty).unwrap_or(IrConst::U64(index));
        Some(IrOperand::Const(value))
    }

    fn flow_state_field_ir_type(&self, type_name: &str) -> Option<IrType> {
        let field_name = self.flow_state_fields.get(type_name)?;
        self.type_fields.get(type_name)?.get(field_name).cloned()
    }

    fn flow_state_index(&self, type_name: &str, name: &str) -> Option<usize> {
        let states = self.flow_states.get(type_name)?;
        if let Some((qualified_type, state_name)) = name.rsplit_once("::") {
            if qualified_type != type_name {
                return None;
            }
            states.iter().position(|state| state == state_name)
        } else {
            states.iter().position(|state| state == name)
        }
    }

    pub(super) fn lookup_field_ir_type(&self, ty: &IrType, field: &str) -> Option<IrType> {
        if field == "data"
            && let IrType::Context(crate::ast::ContextView::Input(schema) | crate::ast::ContextView::Output(schema)) = ty
        {
            return Some(IrType::Context(crate::ast::ContextView::CellData(schema.clone())));
        }
        if let Some(schema) = crate::cell_data_view::ir_schema(ty) {
            return self.type_fields.get(schema)?.get(field).cloned();
        }
        if let IrType::Nested(view) = ty {
            return view.field_type(field);
        }
        match ty {
            IrType::Tuple(items) => field.parse::<usize>().ok().and_then(|index| items.get(index)).cloned(),
            IrType::Named(name) if name == CKB_SCRIPT_VALUE_TYPE => match field {
                "code_hash" => Some(IrType::Hash),
                "hash_type" => Some(IrType::U64),
                "args" => Some(IrType::Named(CKB_SCRIPT_ARGS_TYPE.to_string())),
                _ => None,
            },
            IrType::Named(name) if name == CKB_SCRIPT_ARGS_TYPE => match field {
                "len" => Some(IrType::U64),
                "is_empty" => Some(IrType::Bool),
                _ => None,
            },
            IrType::Named(name) if name == CKB_SCRIPT_HASH_TYPE && field == "0" => Some(IrType::Array(Box::new(IrType::U8), 32)),
            IrType::Named(name) => self.type_fields.get(name).and_then(|fields| fields.get(field)).cloned(),
            IrType::Ref(inner) | IrType::MutRef(inner) => self.lookup_field_ir_type(inner, field),
            IrType::Address | IrType::Hash if field == "0" => Some(IrType::Array(Box::new(IrType::U8), 32)),
            _ => None,
        }
    }

    pub(super) fn index_result_type(&self, operand: &IrOperand) -> Option<IrType> {
        match operand {
            IrOperand::Var(var) => self.index_result_type_from_ir_type(&var.ty),
            IrOperand::Const(IrConst::Array(items)) => items.first().map(|item| self.const_type(item)),
            _ => None,
        }
    }

    fn index_result_type_from_ir_type(&self, ty: &IrType) -> Option<IrType> {
        if let IrType::Nested(view) = ty {
            return view.index_type();
        }
        match ty {
            IrType::Array(inner, _) => Some((**inner).clone()),
            IrType::Ref(inner) | IrType::MutRef(inner) => self.index_result_type_from_ir_type(inner),
            IrType::Vector(element) => element.as_deref().cloned(),
            _ => None,
        }
    }

    pub(super) fn iter_item_type(&self, operand: &IrOperand) -> Option<IrType> {
        match operand {
            IrOperand::Var(var) => self.index_result_type_from_ir_type(&var.ty),
            IrOperand::Const(IrConst::Array(items)) => items.first().map(|item| self.const_type(item)),
            _ => None,
        }
    }
}
