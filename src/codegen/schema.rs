//! Schema field access and type-layout helpers for CellScript codegen.
//!
//! Contains the schema data model (`SchemaFieldLayout`, `SchemaFieldValueSource`,
//! `ExpectedFixedByteSource`, `SourcePointer`), layout width computation,
//! Molecule table helpers, fixed-byte comparison and loading, prelude u64
//! value resolution, and field access dispatch.

use std::collections::HashMap;

use crate::ast::BinaryOp;
use crate::error::Result;
use crate::ir::*;

use super::{
    ckb_source_name, CellScriptRuntimeError, CodeGenerator, PreludeU64OperandSource, PreludeU64ValueSource, CKB_CELL_FIELD_CAPACITY,
    CKB_CELL_FIELD_LOCK_HASH, RUNTIME_EXPR_TEMP_SLOTS, RUNTIME_SCRATCH_BUFFER_SIZE,
};

#[derive(Debug, Clone)]
pub(crate) struct SchemaFieldLayout {
    pub(crate) index: usize,
    pub(crate) offset: usize,
    pub(crate) ty: IrType,
    pub(crate) fixed_size: Option<usize>,
    pub(crate) fixed_enum_size: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct SchemaFieldValueSource {
    pub(crate) obj_var_id: usize,
    pub(crate) type_name: String,
    pub(crate) field: String,
    pub(crate) layout: SchemaFieldLayout,
}

#[derive(Debug, Clone)]
pub(crate) struct AggregatePointerSource {
    pub(crate) ty: IrType,
}

#[derive(Debug, Clone)]
pub(crate) enum ExpectedFixedByteSource {
    SchemaField(SchemaFieldValueSource),
    Const(Vec<u8>),
    StackSlot { var_id: usize, width: usize },
    PointerBytes { var_id: usize, width: usize },
    ParamBytes { var_id: usize, size_offset: usize, width: usize },
    LoadedBytes { var_id: usize, size_offset: usize, width: usize },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SourcePointer {
    LoadedStackPointer { var_id: usize, offset: usize },
    StackAddress { offset: usize },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CanonicalMoleculeTable<'a> {
    pub(crate) base_reg: &'a str,
    pub(crate) size_offset: usize,
    pub(crate) field_count: usize,
    pub(crate) header_size: usize,
    pub(crate) context: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ValidatedFieldSpan<'a> {
    pub(crate) table: CanonicalMoleculeTable<'a>,
    pub(crate) field_index: usize,
    pub(crate) start_reg: &'static str,
    pub(crate) end_reg: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TypedField<'a> {
    pub(crate) span: ValidatedFieldSpan<'a>,
    pub(crate) exact_width: Option<usize>,
}

pub(crate) fn fixed_scalar_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    match (ty, fixed_size) {
        (IrType::Bool | IrType::U8, Some(1)) => Some(1),
        (IrType::U16, Some(2)) => Some(2),
        (IrType::U32, Some(4)) => Some(4),
        (IrType::U64, Some(8)) => Some(8),
        _ => None,
    }
}

pub(crate) fn fixed_register_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    let w = fixed_scalar_width(ty, fixed_size)?;
    (w <= 8).then_some(w)
}

pub(crate) fn fixed_byte_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    if let Some(width) = fixed_scalar_width(ty, fixed_size) {
        return Some(width);
    }
    match (ty, fixed_size) {
        (IrType::Address | IrType::Hash, Some(32)) => Some(32),
        (IrType::U128, Some(16)) => Some(16),
        (IrType::Array(inner, len), Some(size)) if matches!(inner.as_ref(), IrType::U8) && *len == size => Some(size),
        (IrType::Ref(inner) | IrType::MutRef(inner), _) => fixed_byte_width(inner, type_static_length(inner)),
        _ => None,
    }
}

pub(crate) fn molecule_vector_element_fixed_width(
    ty: &IrType,
    type_fixed_sizes: &HashMap<String, usize>,
    enum_fixed_sizes: &HashMap<String, usize>,
) -> Option<usize> {
    let IrType::Named(name) = ty else {
        return None;
    };
    if name == "String" {
        return Some(1);
    }
    let inner = name.strip_prefix("Vec<")?.strip_suffix('>')?;
    molecule_inline_type_fixed_width(inner, type_fixed_sizes, enum_fixed_sizes)
}

pub(crate) fn molecule_inline_type_fixed_width(
    ty: &str,
    type_fixed_sizes: &HashMap<String, usize>,
    enum_fixed_sizes: &HashMap<String, usize>,
) -> Option<usize> {
    match ty.trim() {
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" => Some(4),
        "u64" => Some(8),
        "u128" => Some(16),
        "Address" | "Hash" => Some(32),
        other => type_fixed_sizes.get(other).copied().or_else(|| enum_fixed_sizes.get(other).copied()),
    }
}

pub(crate) fn layout_fixed_scalar_width(layout: &SchemaFieldLayout) -> Option<usize> {
    fixed_scalar_width(&layout.ty, layout.fixed_size).or(layout.fixed_enum_size)
}

pub(crate) fn layout_fixed_byte_width(layout: &SchemaFieldLayout) -> Option<usize> {
    fixed_byte_width(&layout.ty, layout.fixed_size).or(layout.fixed_enum_size)
}

pub(crate) fn type_static_length(ty: &IrType) -> Option<usize> {
    match ty {
        IrType::Bool | IrType::U8 => Some(1),
        IrType::U16 => Some(2),
        IrType::U32 => Some(4),
        IrType::U64 => Some(8),
        IrType::U128 => Some(16),
        IrType::Address | IrType::Hash => Some(32),
        IrType::Array(inner, len) => type_static_length(inner).map(|inner_len| inner_len * len),
        IrType::Tuple(items) => items.iter().try_fold(0usize, |acc, item| type_static_length(item).map(|len| acc + len)),
        IrType::Unit => Some(0),
        IrType::Ref(inner) | IrType::MutRef(inner) => type_static_length(inner),
        IrType::Named(_) => None,
    }
}

pub(crate) fn operand_fixed_byte_width(operand: &IrOperand) -> Option<usize> {
    let ty = match operand {
        IrOperand::Const(IrConst::Address(_)) | IrOperand::Const(IrConst::Hash(_)) => return Some(32),
        IrOperand::Const(IrConst::Array(values)) => return Some(values.len()),
        IrOperand::Const(IrConst::U128(_)) => return Some(16),
        IrOperand::Var(var) => &var.ty,
        _ => return None,
    };
    match ty {
        IrType::Address | IrType::Hash => Some(32),
        IrType::U128 => Some(16),
        IrType::Array(inner, len) if matches!(inner.as_ref(), IrType::U8) => Some(*len),
        _ => None,
    }
}

pub(crate) fn constructed_byte_vector_part_width(operand: &IrOperand) -> Option<usize> {
    operand_fixed_byte_width(operand).or_else(|| match operand {
        IrOperand::Var(var) => fixed_scalar_width(&var.ty, type_static_length(&var.ty)),
        IrOperand::Const(IrConst::Bool(_)) | IrOperand::Const(IrConst::U8(_)) => Some(1),
        IrOperand::Const(IrConst::U16(_)) => Some(2),
        IrOperand::Const(IrConst::U32(_)) => Some(4),
        IrOperand::Const(IrConst::U64(_)) => Some(8),
        _ => None,
    })
}

pub(crate) fn fixed_scalar_operand_width(operand: &IrOperand) -> Option<usize> {
    match operand {
        IrOperand::Var(var) => fixed_scalar_width(&var.ty, type_static_length(&var.ty)),
        IrOperand::Const(IrConst::Bool(_)) | IrOperand::Const(IrConst::U8(_)) => Some(1),
        IrOperand::Const(IrConst::U16(_)) => Some(2),
        IrOperand::Const(IrConst::U32(_)) => Some(4),
        IrOperand::Const(IrConst::U64(_)) => Some(8),
        _ => None,
    }
}

pub(crate) fn fixed_byte_pointer_param_width(ty: &IrType) -> Option<usize> {
    fixed_byte_width(ty, type_static_length(ty)).filter(|width| *width > 8)
}

pub(crate) fn fixed_aggregate_pointer_param_width(ty: &IrType) -> Option<usize> {
    match ty {
        IrType::Array(_, _) | IrType::Tuple(_) => type_static_length(ty).filter(|width| *width > 8),
        _ => None,
    }
}

pub(crate) fn fixed_byte_const_bytes(value: &IrConst) -> Option<Vec<u8>> {
    match value {
        IrConst::Address(bytes) | IrConst::Hash(bytes) => Some(bytes.to_vec()),
        IrConst::U128(value) => Some(value.to_le_bytes().to_vec()),
        IrConst::Array(values) => values
            .iter()
            .map(|value| match value {
                IrConst::U8(byte) => Some(*byte),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

pub(crate) fn fixed_scalar_const_value(value: &IrConst) -> Option<u64> {
    match value {
        IrConst::Bool(value) => Some(u64::from(*value)),
        IrConst::U8(value) => Some((*value).into()),
        IrConst::U16(value) => Some((*value).into()),
        IrConst::U32(value) => Some((*value).into()),
        IrConst::U64(value) => Some(*value),
        _ => None,
    }
}

pub(crate) fn const_ir_type(value: &IrConst) -> IrType {
    match value {
        IrConst::Unit => IrType::Unit,
        IrConst::U8(_) => IrType::U8,
        IrConst::U16(_) => IrType::U16,
        IrConst::U32(_) => IrType::U32,
        IrConst::U64(_) => IrType::U64,
        IrConst::U128(_) => IrType::U128,
        IrConst::Bool(_) => IrType::Bool,
        IrConst::Address(_) => IrType::Address,
        IrConst::Hash(_) => IrType::Hash,
        IrConst::Array(values) => IrType::Array(Box::new(values.first().map(const_ir_type).unwrap_or(IrType::U8)), values.len()),
    }
}

pub(crate) fn const_usize_operand(operand: &IrOperand) -> Option<usize> {
    match operand {
        IrOperand::Const(IrConst::U8(value)) => Some((*value).into()),
        IrOperand::Const(IrConst::U16(value)) => Some((*value).into()),
        IrOperand::Const(IrConst::U32(value)) => Some(*value as usize),
        IrOperand::Const(IrConst::U64(value)) => usize::try_from(*value).ok(),
        _ => None,
    }
}

pub(crate) fn aggregate_type_label(ty: &IrType) -> String {
    match ty {
        IrType::Tuple(_) => "tuple".to_string(),
        IrType::Array(_, len) => format!("array{}", len),
        IrType::Address => "Address".to_string(),
        IrType::Hash => "Hash".to_string(),
        other => format!("{:?}", other),
    }
}

pub(crate) fn aggregate_field_layout(ty: &IrType, field: &str) -> Option<SchemaFieldLayout> {
    match ty {
        IrType::Tuple(items) => {
            let index = field.parse::<usize>().ok()?;
            let field_ty = items.get(index)?.clone();
            let offset = items.iter().take(index).try_fold(0usize, |acc, item| type_static_length(item).map(|size| acc + size))?;
            let fixed_size = type_static_length(&field_ty);
            Some(SchemaFieldLayout { index, offset, ty: field_ty, fixed_size, fixed_enum_size: None })
        }
        IrType::Address | IrType::Hash if field == "0" => Some(SchemaFieldLayout {
            index: 0,
            offset: 0,
            ty: IrType::Array(Box::new(IrType::U8), 32),
            fixed_size: Some(32),
            fixed_enum_size: None,
        }),
        _ => None,
    }
}

pub(crate) fn tuple_return_field_type(ty: &IrType, field: &str) -> Option<IrType> {
    let IrType::Tuple(items) = ty else {
        return None;
    };
    let index = field.parse::<usize>().ok()?;
    items.get(index).cloned()
}

pub(crate) fn named_type_name(ty: &IrType) -> Option<&str> {
    match ty {
        IrType::Named(name) => Some(name.as_str()),
        IrType::Ref(inner) | IrType::MutRef(inner) => named_type_name(inner),
        _ => None,
    }
}

impl CodeGenerator {
    pub(crate) fn fixed_named_type_width(&self, ty: &IrType) -> Option<usize> {
        match ty {
            IrType::Named(name) => self.type_fixed_sizes.get(name).copied().or_else(|| self.enum_fixed_sizes.get(name).copied()),
            IrType::Ref(inner) | IrType::MutRef(inner) => self.fixed_named_type_width(inner),
            _ => None,
        }
    }

    pub(crate) fn fixed_byte_like_width(&self, ty: &IrType) -> Option<usize> {
        fixed_byte_width(ty, type_static_length(ty)).or_else(|| self.fixed_named_type_width(ty))
    }

    pub(crate) fn constructed_byte_vector_part_width(&self, operand: &IrOperand) -> Option<usize> {
        constructed_byte_vector_part_width(operand).or_else(|| match operand {
            IrOperand::Var(var) => self.fixed_named_type_width(&var.ty),
            _ => None,
        })
    }

    pub(crate) fn static_length(&self, operand: &IrOperand) -> Option<usize> {
        match operand {
            IrOperand::Var(var) => Self::static_length_from_type(&var.ty),
            IrOperand::Const(IrConst::Array(items)) => Some(items.len()),
            _ => None,
        }
    }

    pub(crate) fn static_length_from_type(ty: &IrType) -> Option<usize> {
        match ty {
            IrType::Array(_, size) => Some(*size),
            IrType::Ref(inner) | IrType::MutRef(inner) => Self::static_length_from_type(inner),
            _ => None,
        }
    }

    pub(crate) fn emit_loaded_schema_bounds_check(&mut self, size_offset: usize, required_size: usize, context: &str) {
        self.emit(format!("# cellscript abi: bounds check {} required={}", context, required_size));
        let ok_label = self.fresh_label("schema_bounds_ok");
        self.emit_stack_load("a0", size_offset);
        self.emit(format!("li a1, {}", required_size));
        self.emit("call __cellscript_require_min_size");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&ok_label);
    }

    pub(crate) fn emit_loaded_schema_exact_size_check(&mut self, size_offset: usize, expected_size: usize, context: &str) {
        self.emit(format!("# cellscript abi: exact size check {} expected={}", context, expected_size));
        let ok_label = self.fresh_label("schema_size_ok");
        self.emit_stack_load("a0", size_offset);
        self.emit(format!("li a1, {}", expected_size));
        self.emit("call __cellscript_require_exact_size");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::ExactSizeMismatch);
        self.emit_label(&ok_label);
    }

    pub(crate) fn emit_validated_molecule_table_field_bounds_to_t5(
        &mut self,
        base_reg: &str,
        size_offset: usize,
        field_index: usize,
        field_count: usize,
        field_width: usize,
        context: &str,
    ) {
        let table = self.canonical_molecule_table_plan(base_reg, size_offset, field_count, context);
        self.emit(format!(
            "# cellscript abi: CanonicalMoleculeTable field {} index={} field_count={} width={}",
            context, field_index, field_count, field_width
        ));
        let span = self.emit_validated_field_span_from_canonical_table_to_t5_t6(&table, field_index);
        let typed = TypedField { span, exact_width: (field_width > 0).then_some(field_width) };
        self.emit_typed_field_exact_width_check(&typed);
    }

    fn canonical_molecule_table_plan<'a>(
        &self,
        base_reg: &'a str,
        size_offset: usize,
        field_count: usize,
        context: &'a str,
    ) -> CanonicalMoleculeTable<'a> {
        CanonicalMoleculeTable { base_reg, size_offset, field_count, header_size: 4 + 4 * field_count, context }
    }

    fn emit_validate_canonical_molecule_table(&mut self, table: &CanonicalMoleculeTable<'_>) {
        self.emit(format!(
            "# cellscript abi: validate CanonicalMoleculeTable {} field_count={} header_size={}",
            table.context, table.field_count, table.header_size
        ));
        self.emit_loaded_schema_bounds_check(table.size_offset, table.header_size, table.context);

        self.emit_stack_load("a0", table.size_offset);
        let total_ok = self.fresh_label("molecule_table_total_ok");
        self.emit_unaligned_scalar_load(table.base_reg, "t0", "t2", 0, 4);
        self.emit("sub t2, t0, a0");
        self.emit(format!("beqz t2, {}", total_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&total_ok);

        self.emit("mv a1, a0");
        self.emit(format!("mv a0, {}", table.base_reg));
        self.emit(format!("li a2, {}", table.field_count));
        self.emit(format!("li a3, {}", table.header_size));
        self.emit("call __cellscript_validate_molecule_table_offsets");
        let offsets_ok = self.fresh_label("molecule_table_offsets_ok");
        self.emit(format!("beqz a0, {}", offsets_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&offsets_ok);
        self.emit_stack_load("a0", table.size_offset);
    }

    fn emit_validated_field_span_from_canonical_table_to_t5_t6<'a>(
        &mut self,
        table: &CanonicalMoleculeTable<'a>,
        field_index: usize,
    ) -> ValidatedFieldSpan<'a> {
        if field_index >= table.field_count {
            self.emit(format!(
                "# cellscript abi: fail closed because Molecule table field {} index {} is outside field_count {}",
                table.context, field_index, table.field_count
            ));
            self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
            return ValidatedFieldSpan { table: *table, field_index, start_reg: "t5", end_reg: "t6" };
        }
        self.emit_validate_canonical_molecule_table(table);
        self.emit(format!("# cellscript abi: ValidatedFieldSpan {} field_index={} start=t5 end=t6", table.context, field_index));
        self.emit_unaligned_scalar_load(table.base_reg, "t5", "t2", 4 + 4 * field_index, 4);
        if field_index + 1 < table.field_count {
            self.emit_unaligned_scalar_load(table.base_reg, "t6", "t2", 4 + 4 * (field_index + 1), 4);
        } else {
            self.emit("add t6, a0, zero");
        }
        self.emit(format!("li t1, {}", table.header_size));
        self.emit("sltu t2, t5, t1");
        let start_ok = self.fresh_label("molecule_table_start_ok");
        self.emit(format!("beqz t2, {}", start_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&start_ok);

        self.emit("sltu t2, t6, t5");
        let order_ok = self.fresh_label("molecule_table_order_ok");
        self.emit(format!("beqz t2, {}", order_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&order_ok);

        self.emit("sltu t2, a0, t6");
        let end_ok = self.fresh_label("molecule_table_end_ok");
        self.emit(format!("beqz t2, {}", end_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&end_ok);

        ValidatedFieldSpan { table: *table, field_index, start_reg: "t5", end_reg: "t6" }
    }

    fn emit_typed_field_exact_width_check(&mut self, field: &TypedField<'_>) {
        let Some(width) = field.exact_width else {
            return;
        };
        let _boundary_context = (field.span.table.context, field.span.field_index);
        self.emit(format!("sub t3, {}, {}", field.span.end_reg, field.span.start_reg));
        self.emit(format!("li t1, {}", width));
        self.emit("sub t2, t3, t1");
        let width_ok = self.fresh_label("molecule_table_width_ok");
        self.emit(format!("beqz t2, {}", width_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&width_ok);
    }

    pub(crate) fn emit_validated_molecule_table_field_span_to_t5_t6(
        &mut self,
        base_reg: &str,
        size_offset: usize,
        field_index: usize,
        field_count: usize,
        context: &str,
    ) {
        let table = self.canonical_molecule_table_plan(base_reg, size_offset, field_count, context);
        self.emit(format!(
            "# cellscript abi: CanonicalMoleculeTable dynamic field {} index={} field_count={}",
            context, field_index, field_count
        ));
        self.emit_validated_field_span_from_canonical_table_to_t5_t6(&table, field_index);
    }

    pub(crate) fn emit_cell_metadata_equality(&mut self, left: &IrOperand, right: &IrOperand, field: CellMetadataField) -> Result<()> {
        let Some((left_source, left_index)) = self.operand_cell_location(left) else {
            self.emit("# cellscript abi: fail closed because left cell metadata source cannot be determined");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(());
        };
        let Some((right_source, right_index)) = self.operand_cell_location(right) else {
            self.emit("# cellscript abi: fail closed because right cell metadata source cannot be determined");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(());
        };
        let (cell_field, field_name, width, mismatch_error) = match field {
            CellMetadataField::LockHash => {
                (CKB_CELL_FIELD_LOCK_HASH, "lock_hash", 32usize, CellScriptRuntimeError::LockHashPreservationMismatch)
            }
            CellMetadataField::Capacity => {
                (CKB_CELL_FIELD_CAPACITY, "capacity", 8usize, CellScriptRuntimeError::CapacityPreservationMismatch)
            }
        };

        let left_size_offset = self.runtime_scratch_size_offset();
        let left_buffer_offset = self.runtime_scratch_buffer_offset();
        let right_size_offset = self.runtime_scratch2_size_offset();
        let right_buffer_offset = self.runtime_scratch2_buffer_offset();

        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("cell_metadata_left_{}", field_name),
            left_source,
            left_index,
            cell_field,
            left_size_offset,
            left_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("cell_metadata_right_{}", field_name),
            right_source,
            right_index,
            cell_field,
            right_size_offset,
            right_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(left_size_offset, width, &format!("cell metadata left {}", field_name));
        self.emit_loaded_schema_exact_size_check(right_size_offset, width, &format!("cell metadata right {}", field_name));
        self.emit(format!(
            "# cellscript abi: verify cell metadata {} equality {}#{} == {}#{} size={}",
            field_name,
            ckb_source_name(left_source),
            left_index,
            ckb_source_name(right_source),
            right_index,
            width
        ));
        self.emit_sp_addi("t4", left_buffer_offset);
        self.emit_sp_addi("t5", right_buffer_offset);
        for byte_index in 0..width {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("cell_metadata_byte_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_runtime_error_comment(mismatch_error);
            self.emit(format!("li a0, {}", mismatch_error.code()));
            self.emit_epilogue();
            self.emit_label(&ok_label);
        }
        Ok(())
    }

    pub(crate) fn emit_loaded_fixed_field_pointer_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        width: usize,
        context: &str,
        pointer_stack_offset: usize,
    ) {
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        self.emit_sp_addi("t5", buffer_offset + layout.offset);
        self.emit_stack_store("t5", pointer_stack_offset);
    }

    pub(crate) fn emit_dynamic_fixed_field_pointer_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        field_count: usize,
        width: usize,
        context: &str,
        pointer_stack_offset: usize,
        len_stack_offset: usize,
    ) {
        self.emit_dynamic_table_field_span_to_stack(
            size_offset,
            buffer_offset,
            layout.index,
            field_count,
            context,
            pointer_stack_offset,
            len_stack_offset,
        );
        self.emit_stack_load("t0", len_stack_offset);
        self.emit(format!("li t1, {}", width));
        self.emit("sub t2, t0, t1");
        let ok_label = self.fresh_label("identity_field_len_ok");
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit_runtime_error_comment(CellScriptRuntimeError::DynamicFieldValueMismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DynamicFieldValueMismatch.code()));
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    pub(crate) fn emit_fixed_pointer_equality(
        &mut self,
        left_pointer_stack_offset: usize,
        right_pointer_stack_offset: usize,
        width: usize,
        context: &str,
        error: CellScriptRuntimeError,
    ) {
        self.emit(format!("# cellscript abi: verify {} size={}", context, width));
        self.emit_stack_load("a0", left_pointer_stack_offset);
        self.emit_stack_load("a1", right_pointer_stack_offset);
        self.emit(format!("li a2, {}", width));
        self.emit("call __cellscript_memcmp_fixed");
        let ok_label = self.fresh_label("identity_field_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_runtime_error_comment(error);
        self.emit(format!("li a0, {}", error.code()));
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    pub(crate) fn emit_dynamic_table_field_equality_check(
        &mut self,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
        field_count: usize,
        input_size_offset: usize,
        input_buffer_offset: usize,
        output_size_offset: usize,
        output_buffer_offset: usize,
        fail_code: CellScriptRuntimeError,
    ) {
        let Some(start_offset) = self.runtime_expr_temp_offset_or_record(0) else {
            return;
        };
        let Some(len_offset) = self.runtime_expr_temp_offset_or_record(1) else {
            return;
        };
        let Some(output_start_offset) = self.runtime_expr_temp_offset_or_record(2) else {
            return;
        };
        if let Some(width) = layout_fixed_byte_width(layout) {
            self.emit_dynamic_table_fixed_field_pointer_to_stack(
                input_size_offset,
                input_buffer_offset,
                layout,
                field_count,
                width,
                &format!("{} input.{}", type_name, field),
                start_offset,
            );
            self.emit_dynamic_table_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                layout,
                field_count,
                width,
                &format!("{} output.{}", type_name, field),
                output_start_offset,
            );
            self.emit(format!("li t0, {}", width));
            self.emit_stack_store("t0", len_offset);
        } else {
            self.emit_dynamic_table_field_span_to_stack(
                input_size_offset,
                input_buffer_offset,
                layout.index,
                field_count,
                &format!("{} input.{}", type_name, field),
                start_offset,
                len_offset,
            );
            let Some(output_len_offset) = self.runtime_expr_temp_offset_or_record(3) else {
                return;
            };
            self.emit_dynamic_table_field_span_to_stack(
                output_size_offset,
                output_buffer_offset,
                layout.index,
                field_count,
                &format!("{} output.{}", type_name, field),
                output_start_offset,
                output_len_offset,
            );
            self.emit_stack_load("t0", len_offset);
            self.emit_stack_load("t1", output_len_offset);
            self.emit("sub t2, t0, t1");
            let len_ok = self.fresh_label("mutate_table_field_len_ok");
            self.emit(format!("beqz t2, {}", len_ok));
            self.emit_fail(fail_code);
            self.emit_label(&len_ok);
        }

        self.emit(format!(
            "# cellscript abi: verify mutate preserved Molecule table field {}.{} Input#{} == Output#{}",
            type_name, field, 0, 1
        ));
        let mismatch_label = self.fresh_label("mutate_table_field_mismatch");
        self.emit_stack_load("a0", start_offset);
        self.emit_stack_load("a1", output_start_offset);
        self.emit_stack_load("a2", len_offset);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch_label));
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, fail_code);
    }

    pub(crate) fn emit_dynamic_table_field_span_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        field_index: usize,
        field_count: usize,
        context: &str,
        start_stack_offset: usize,
        len_stack_offset: usize,
    ) {
        self.emit_sp_addi("t4", buffer_offset);
        self.emit_validated_molecule_table_field_span_to_t5_t6("t4", size_offset, field_index, field_count, context);
        self.emit_sp_addi("t4", buffer_offset);
        self.emit("add t5, t4, t5");
        self.emit("add t6, t4, t6");
        self.emit("sub t0, t6, t5");
        self.emit_stack_store("t5", start_stack_offset);
        self.emit_stack_store("t0", len_stack_offset);
    }

    pub(crate) fn emit_dynamic_table_fixed_field_pointer_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        field_count: usize,
        width: usize,
        context: &str,
        start_stack_offset: usize,
    ) {
        self.emit_sp_addi("t4", buffer_offset);
        self.emit_validated_molecule_table_field_bounds_to_t5("t4", size_offset, layout.index, field_count, width, context);
        self.emit_sp_addi("t4", buffer_offset);
        self.emit("add t5, t4, t5");
        self.emit_stack_store("t5", start_stack_offset);
    }

    pub(crate) fn emit_loaded_field_equals_expected(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        expected: &IrOperand,
        context: &str,
    ) {
        let Some(width) = layout_fixed_scalar_width(layout) else {
            return;
        };
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        self.emit(format!("# cellscript abi: verify output field {} offset={} size={}", context, layout.offset, width));
        self.emit_sp_addi("t4", buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
        let Some(actual_value_offset) = self.runtime_expr_temp_offset_or_record(RUNTIME_EXPR_TEMP_SLOTS - 1) else {
            return;
        };
        self.emit("# cellscript abi: preserve output scalar before expected expression");
        self.emit_stack_store("t0", actual_value_offset);
        self.emit_expected_operand_to_t1(expected);
        self.emit_stack_load("t0", actual_value_offset);
        self.emit("sub t2, t0, t1");
        let ok_label = self.fresh_label("output_field_ok");
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&ok_label);
    }

    pub(crate) fn emit_loaded_fixed_bytes_against_source(
        &mut self,
        output_buffer_offset: usize,
        output_field_offset: usize,
        source: &ExpectedFixedByteSource,
        width: usize,
        fail_code: CellScriptRuntimeError,
    ) {
        let mismatch_label = self.fresh_label("fixed_byte_mismatch");
        self.emit_sp_addi("t4", output_buffer_offset);
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                if self.emit_schema_field_source_pointer_to("a1", source, width) {
                    self.emit_sp_addi("a0", output_buffer_offset + output_field_offset);
                    self.emit(format!("li a2, {}", width));
                    self.emit("call __cellscript_memcmp_fixed");
                    self.emit(format!("bnez a0, {}", mismatch_label));
                } else {
                    self.emit("# cellscript abi: fail closed because schema field byte source is not addressable");
                    self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
                }
            }
            ExpectedFixedByteSource::Const(bytes) => {
                if width >= 8 && bytes.iter().take(width).all(|byte| *byte == 0) {
                    self.emit_sp_addi("a0", output_buffer_offset + output_field_offset);
                    self.emit(format!("li a1, {}", width));
                    self.emit("call __cellscript_memzero_fixed");
                    self.emit(format!("bnez a0, {}", mismatch_label));
                } else {
                    for (byte_index, byte) in bytes.iter().take(width).enumerate() {
                        self.emit(format!("lbu t0, {}(t4)", output_field_offset + byte_index));
                        self.emit(format!("li t1, {}", byte));
                        self.emit("sub t2, t0, t1");
                        self.emit(format!("bnez t2, {}", mismatch_label));
                    }
                }
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_loaded_fixed_bytes_helper_call(
                    output_buffer_offset,
                    output_field_offset,
                    SourcePointer::StackAddress { offset: var_id * 8 },
                    width,
                    &mismatch_label,
                );
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_loaded_fixed_bytes_helper_call(
                    output_buffer_offset,
                    output_field_offset,
                    SourcePointer::LoadedStackPointer { var_id: *var_id, offset: 0 },
                    width,
                    &mismatch_label,
                );
            }
        }
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, fail_code);
    }

    pub(crate) fn emit_loaded_fixed_bytes_helper_call(
        &mut self,
        output_buffer_offset: usize,
        output_field_offset: usize,
        source: SourcePointer,
        width: usize,
        mismatch_label: &str,
    ) {
        self.emit_sp_addi("a0", output_buffer_offset + output_field_offset);
        match source {
            SourcePointer::LoadedStackPointer { var_id, offset } => {
                self.emit_stack_load("a1", var_id * 8);
                if offset != 0 {
                    self.emit_large_addi("a1", "a1", offset as i64);
                }
            }
            SourcePointer::StackAddress { offset } => {
                self.emit_sp_addi("a1", offset);
            }
        }
        self.emit(format!("li a2, {}", width));
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch_label));
    }

    pub(crate) fn emit_loaded_field_bytes_equals_expected(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        expected: &IrOperand,
        context: &str,
    ) -> bool {
        if layout_fixed_scalar_width(layout).is_some() {
            self.emit_loaded_field_equals_expected(size_offset, buffer_offset, layout, expected, context);
            return true;
        }
        let Some(width) = layout_fixed_byte_width(layout) else {
            return false;
        };
        let Some(source) = self.expected_fixed_byte_source(expected, width) else {
            return false;
        };
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                if let Some(source_size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
                    if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                        self.emit_loaded_schema_exact_size_check(source_size_offset, expected_size, &source.type_name);
                    }
                    self.emit_loaded_schema_bounds_check(
                        source_size_offset,
                        source.layout.offset + width,
                        &format!("{}.{}", source.type_name, source.field),
                    );
                }
                self.emit(format!("# cellscript abi: verify output bytes field {} offset={} size={}", context, layout.offset, width));
                self.emit(format!(
                    "# cellscript abi: expected bytes field {}.{} offset={} size={}",
                    source.type_name, source.field, source.layout.offset, width
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::SchemaField(source),
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::Const(bytes) => {
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against const",
                    context, layout.offset, width
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::Const(bytes),
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::StackSlot { var_id, width } => {
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against stack slot var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::StackSlot { var_id, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::PointerBytes { var_id, width } => {
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against pointer var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::PointerBytes { var_id, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::ParamBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(size_offset, width, &format!("param var{}", var_id));
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against fixed-byte param var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::ParamBytes { var_id, size_offset, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::LoadedBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(size_offset, width, &format!("loaded bytes var{}", var_id));
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against loaded bytes var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::LoadedBytes { var_id, size_offset, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
        }
        true
    }

    pub(crate) fn emit_prepare_fixed_byte_source(&mut self, source: &ExpectedFixedByteSource, width: usize, context: &str) {
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                self.emit_prepare_schema_field_source(source, width);
            }
            ExpectedFixedByteSource::ParamBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(*size_offset, *width, &format!("{} param var{}", context, var_id));
            }
            ExpectedFixedByteSource::LoadedBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(*size_offset, *width, &format!("{} loaded bytes var{}", context, var_id));
            }
            ExpectedFixedByteSource::Const(_)
            | ExpectedFixedByteSource::StackSlot { .. }
            | ExpectedFixedByteSource::PointerBytes { .. } => {}
        }
    }

    pub(crate) fn emit_fixed_byte_source_byte_to(
        &mut self,
        dest_reg: &str,
        base_reg: &str,
        source: &ExpectedFixedByteSource,
        byte_index: usize,
    ) {
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                if self.emit_schema_field_source_pointer_to(base_reg, source, byte_index + 1) {
                    self.emit(format!("lbu {}, {}({})", dest_reg, byte_index, base_reg));
                } else {
                    self.emit("# cellscript abi: fail closed because schema field byte source is not addressable");
                    self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
                }
            }
            ExpectedFixedByteSource::Const(bytes) => {
                self.emit(format!("li {}, {}", dest_reg, bytes[byte_index]));
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_sp_addi(base_reg, var_id * 8);
                self.emit(format!("lbu {}, {}({})", dest_reg, byte_index, base_reg));
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_stack_load(base_reg, var_id * 8);
                self.emit(format!("lbu {}, {}({})", dest_reg, byte_index, base_reg));
            }
        }
    }

    pub(crate) fn emit_fixed_byte_source_pointer_to(&mut self, dest_reg: &str, source: &ExpectedFixedByteSource) -> bool {
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                let Some(width) = layout_fixed_byte_width(&source.layout) else {
                    return false;
                };
                self.emit_schema_field_source_pointer_to(dest_reg, source, width)
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_sp_addi(dest_reg, var_id * 8);
                true
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_stack_load(dest_reg, var_id * 8);
                true
            }
            ExpectedFixedByteSource::Const(_) => false,
        }
    }

    pub(crate) fn emit_fixed_byte_source_pointer_or_const_to(&mut self, dest_reg: &str, source: &ExpectedFixedByteSource) -> bool {
        if let ExpectedFixedByteSource::Const(bytes) = source {
            let label = self.const_data_label_for_bytes(bytes.clone());
            self.emit(format!("la {}, {}", dest_reg, label));
            true
        } else {
            self.emit_fixed_byte_source_pointer_to(dest_reg, source)
        }
    }

    pub(crate) fn emit_fixed_byte_mismatch_fail(&mut self, mismatch_label: &str, fail_code: CellScriptRuntimeError) {
        let done_label = self.fresh_label("fixed_byte_verify_done");
        self.emit(format!("j {}", done_label));
        self.emit_label(mismatch_label);
        self.emit_fail(fail_code);
        self.emit_label(&done_label);
    }

    pub(crate) fn emit_fixed_byte_comparison(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        let Some(width) = operand_fixed_byte_width(left) else {
            return false;
        };
        if operand_fixed_byte_width(right) != Some(width) {
            return false;
        }
        let Some(left_source) = self.expected_fixed_byte_source(left, width) else {
            return false;
        };
        let Some(right_source) = self.expected_fixed_byte_source(right, width) else {
            return false;
        };
        self.emit(format!("# cellscript abi: fixed-byte {:?} comparison size={}", op, width));
        self.emit_prepare_fixed_byte_source(&left_source, width, "left fixed-byte comparison");
        self.emit_prepare_fixed_byte_source(&right_source, width, "right fixed-byte comparison");
        if width >= 8 && self.emit_fixed_byte_comparison_helper(dest, op, &left_source, &right_source, width) {
            return true;
        }
        let mismatch_label = self.fresh_label("fixed_byte_mismatch");
        let done_label = self.fresh_label("fixed_byte_done");
        for byte_index in 0..width {
            self.emit_fixed_byte_source_byte_to("t0", "t4", &left_source, byte_index);
            self.emit_fixed_byte_source_byte_to("t1", "t5", &right_source, byte_index);
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", mismatch_label));
        }
        let equal_value = if matches!(op, BinaryOp::Eq) { 1 } else { 0 };
        let mismatch_value = if matches!(op, BinaryOp::Eq) { 0 } else { 1 };
        self.emit(format!("li t3, {}", equal_value));
        self.emit(format!("j {}", done_label));
        self.emit_label(&mismatch_label);
        self.emit(format!("li t3, {}", mismatch_value));
        self.emit_label(&done_label);
        self.emit_stack_store("t3", dest.id * 8);
        true
    }

    pub(crate) fn emit_fixed_byte_comparison_helper(
        &mut self,
        dest: &IrVar,
        op: BinaryOp,
        left_source: &ExpectedFixedByteSource,
        right_source: &ExpectedFixedByteSource,
        width: usize,
    ) -> bool {
        match (left_source, right_source) {
            (ExpectedFixedByteSource::Const(bytes), source) if bytes.iter().take(width).all(|byte| *byte == 0) => {
                if !self.emit_fixed_byte_source_pointer_to("a0", source) {
                    return false;
                }
                self.emit(format!("li a1, {}", width));
                self.emit("call __cellscript_memzero_fixed");
            }
            (source, ExpectedFixedByteSource::Const(bytes)) if bytes.iter().take(width).all(|byte| *byte == 0) => {
                if !self.emit_fixed_byte_source_pointer_to("a0", source) {
                    return false;
                }
                self.emit(format!("li a1, {}", width));
                self.emit("call __cellscript_memzero_fixed");
            }
            (ExpectedFixedByteSource::Const(_), _) | (_, ExpectedFixedByteSource::Const(_)) => return false,
            _ => {
                if !self.emit_fixed_byte_source_pointer_to("a0", left_source) {
                    return false;
                }
                let Some(left_pointer_offset) = self.runtime_expr_temp_offset(0) else {
                    return false;
                };
                self.emit_stack_store("a0", left_pointer_offset);
                if !self.emit_fixed_byte_source_pointer_to("a1", right_source) {
                    return false;
                }
                self.emit_stack_load("a0", left_pointer_offset);
                self.emit(format!("li a2, {}", width));
                self.emit("call __cellscript_memcmp_fixed");
            }
        }
        if matches!(op, BinaryOp::Eq) {
            self.emit("seqz t3, a0");
        } else {
            self.emit("snez t3, a0");
        }
        self.emit_stack_store("t3", dest.id * 8);
        true
    }

    pub(crate) fn expected_fixed_byte_source(&self, operand: &IrOperand, expected_width: usize) -> Option<ExpectedFixedByteSource> {
        match operand {
            IrOperand::Const(value) => {
                let bytes = fixed_byte_const_bytes(value)?;
                (bytes.len() == expected_width).then_some(ExpectedFixedByteSource::Const(bytes))
            }
            IrOperand::Var(var) if self.fixed_byte_like_width(&var.ty).is_some() => {
                let var_width = self.fixed_byte_like_width(&var.ty)?;
                if let Some(source) = self.schema_field_value_sources.get(&var.id).cloned() {
                    let source_width = layout_fixed_byte_width(&source.layout)?;
                    if source_width == expected_width {
                        return Some(ExpectedFixedByteSource::SchemaField(source));
                    }
                }
                if let Some(bytes) = self.prelude_fixed_byte_constants.get(&var.id).cloned() {
                    if bytes.len() == expected_width {
                        return Some(ExpectedFixedByteSource::Const(bytes));
                    }
                }
                if self.fixed_byte_local_offsets.contains_key(&var.id) && var_width == expected_width {
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if expected_width <= 8
                    && (fixed_scalar_width(&var.ty, type_static_length(&var.ty)).is_some()
                        || (var_width == expected_width && fixed_byte_width(&var.ty, type_static_length(&var.ty)).is_some()))
                    && expected_width <= var_width
                {
                    return Some(ExpectedFixedByteSource::StackSlot { var_id: var.id, width: expected_width });
                }
                if self.aggregate_pointer_sources.contains_key(&var.id) && var_width == expected_width {
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if self.schema_pointer_vars.contains(&var.id) && var_width == expected_width {
                    if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
                        return Some(ExpectedFixedByteSource::LoadedBytes { var_id: var.id, size_offset, width: expected_width });
                    }
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if self.param_vars.contains(&var.id) && var_width == expected_width {
                    if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&var.id).copied() {
                        return Some(ExpectedFixedByteSource::ParamBytes { var_id: var.id, size_offset, width: expected_width });
                    }
                }
                if let Some(size_offset) = self.cell_buffer_size_offsets.get(&var.id).copied() {
                    if var_width == expected_width {
                        return Some(ExpectedFixedByteSource::LoadedBytes { var_id: var.id, size_offset, width: expected_width });
                    }
                }
                if let Some(param_id) = self.param_type_hash_sources.get(&var.id).copied() {
                    if var_width == expected_width {
                        if let Some(size_offset) = self.param_type_hash_size_offsets.get(&param_id).copied() {
                            return Some(ExpectedFixedByteSource::LoadedBytes { var_id: var.id, size_offset, width: expected_width });
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub(crate) fn emit_generic_fixed_byte_comparison(
        &mut self,
        dest: &IrVar,
        op: BinaryOp,
        left: &IrOperand,
        right: &IrOperand,
    ) -> bool {
        let left_width = operand_fixed_byte_width(left);
        let right_width = operand_fixed_byte_width(right);

        // Need at least one Var operand with known width for this to work
        let width = match (left_width, right_width) {
            (Some(w), Some(r)) if w == r => w,
            (Some(w), None) | (None, Some(w)) => w,
            _ => return false,
        };

        if width == 0 {
            return false;
        }

        // We need at least one Var operand
        let left_var = match left {
            IrOperand::Var(v) => Some(v),
            _ => None,
        };
        let right_var = match right {
            IrOperand::Var(v) => Some(v),
            _ => None,
        };
        if left_var.is_none() && right_var.is_none() {
            return false;
        }

        self.emit(format!("# cellscript abi: generic fixed-byte {:?} comparison size={}", op, width));

        // Load left pointer to t4
        if let Some(v) = left_var {
            self.emit_stack_load("t4", v.id * 8);
        } else {
            // Left is a constant – store it to scratch buffer and point t4 there
            let size_offset = self.runtime_scratch_size_offset();
            let buffer_offset = self.runtime_scratch_buffer_offset();
            self.emit_store_fixed_byte_const_to_scratch(left, size_offset, buffer_offset, width);
            self.emit_sp_addi("t4", buffer_offset);
        }

        // Load right pointer to t5
        if let Some(v) = right_var {
            self.emit_stack_load("t5", v.id * 8);
        } else {
            let size_offset = self.runtime_scratch2_size_offset();
            let buffer_offset = self.runtime_scratch2_buffer_offset();
            self.emit_store_fixed_byte_const_to_scratch(right, size_offset, buffer_offset, width);
            self.emit_sp_addi("t5", buffer_offset);
        }

        let mismatch_label = self.fresh_label("gen_fb_mismatch");
        let done_label = self.fresh_label("gen_fb_done");
        for byte_index in 0..width {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", mismatch_label));
        }
        let equal_value = if matches!(op, BinaryOp::Eq) { 1 } else { 0 };
        let mismatch_value = if matches!(op, BinaryOp::Eq) { 0 } else { 1 };
        self.emit(format!("li t3, {}", equal_value));
        self.emit(format!("j {}", done_label));
        self.emit_label(&mismatch_label);
        self.emit(format!("li t3, {}", mismatch_value));
        self.emit_label(&done_label);
        self.emit_stack_store("t3", dest.id * 8);
        true
    }

    pub(crate) fn emit_store_fixed_byte_const_to_scratch(
        &mut self,
        operand: &IrOperand,
        size_offset: usize,
        buffer_offset: usize,
        width: usize,
    ) {
        match operand {
            IrOperand::Const(IrConst::Address(bytes)) | IrOperand::Const(IrConst::Hash(bytes)) => {
                self.emit(format!("# cellscript abi: store fixed-byte const size={}", width));
                self.emit(format!("li t0, {}", width));
                self.emit_stack_store("t0", size_offset);
                for (i, byte) in bytes.iter().enumerate() {
                    self.emit(format!("li t0, {}", byte));
                    if buffer_offset + i <= 2047 {
                        self.emit_stack_store_byte("t0", buffer_offset + i);
                    } else {
                        self.emit(format!("li t6, {}", buffer_offset + i));
                        self.emit("add t6, sp, t6");
                        self.emit("sb t0, 0(t6)");
                    }
                }
            }
            IrOperand::Const(IrConst::U128(value)) => {
                self.emit(format!("# cellscript abi: store u128 const size={}", width));
                self.emit(format!("li t0, {}", width));
                self.emit_stack_store("t0", size_offset);
                for (i, byte) in value.to_le_bytes().iter().enumerate() {
                    self.emit(format!("li t0, {}", byte));
                    self.emit_stack_store_byte("t0", buffer_offset + i);
                }
            }
            IrOperand::Const(IrConst::Array(values)) => {
                self.emit(format!("# cellscript abi: store fixed-byte array const size={}", width));
                self.emit(format!("li t0, {}", width));
                self.emit_stack_store("t0", size_offset);
                for (i, value) in values.iter().enumerate() {
                    if let IrConst::U8(byte) = value {
                        self.emit(format!("li t0, {}", byte));
                        if buffer_offset + i <= 2047 {
                            self.emit_stack_store_byte("t0", buffer_offset + i);
                        } else {
                            self.emit(format!("li t6, {}", buffer_offset + i));
                            self.emit("add t6, sp, t6");
                            self.emit("sb t0, 0(t6)");
                        }
                    }
                }
            }
            _ => {
                self.emit("# cellscript abi: fail closed because unknown const type cannot be stored to scratch");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            }
        }
    }

    pub(crate) fn emit_fixed_byte_source_scalar_to(
        &mut self,
        dest_reg: &str,
        scratch_reg: &str,
        base_reg: &str,
        source: &ExpectedFixedByteSource,
        start: usize,
        width: usize,
    ) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit_fixed_byte_source_byte_to(scratch_reg, base_reg, source, start + byte_index);
            if byte_index != 0 {
                self.emit(format!("slli {}, {}, {}", scratch_reg, scratch_reg, byte_index * 8));
            }
            self.emit(format!("or {}, {}, {}", dest_reg, dest_reg, scratch_reg));
        }
    }

    pub(crate) fn operand_is_u128(&self, operand: &IrOperand) -> bool {
        match operand {
            IrOperand::Const(IrConst::U128(_)) => true,
            IrOperand::Var(var) => var.ty == IrType::U128,
            _ => false,
        }
    }

    pub(crate) fn emit_u128_add_sub_with_u64(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        if dest.ty != IrType::U128 || !matches!(op, BinaryOp::Add | BinaryOp::Sub) {
            return false;
        }
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            return false;
        };

        let (wide_operand, delta_operand) = match op {
            BinaryOp::Add if self.operand_is_u128(left) => (left, right),
            BinaryOp::Add if self.operand_is_u128(right) => (right, left),
            BinaryOp::Sub if self.operand_is_u128(left) => (left, right),
            _ => return false,
        };
        let Some(source) = self.expected_fixed_byte_source(wide_operand, 16) else {
            return false;
        };
        let Some(delta) = self.prelude_u64_operand_source(delta_operand) else {
            return false;
        };

        self.emit(format!("# cellscript abi: u128 {:?} with u64 delta", op));
        self.emit_fixed_byte_source_scalar_to("t0", "t2", "t4", &source, 0, 8);
        self.emit_fixed_byte_source_scalar_to("t3", "t2", "t4", &source, 8, 8);
        self.emit_prelude_u64_operand_source_to_t1(&delta);
        match op {
            BinaryOp::Add => {
                self.emit("add t5, t0, t1");
                self.emit("sltu t2, t5, t0");
                self.emit("add t6, t3, t2");
            }
            BinaryOp::Sub => {
                self.emit("sub t5, t0, t1");
                self.emit("sltu t2, t0, t1");
                self.emit("sub t6, t3, t2");
            }
            _ => {
                self.record_fatal_error("non add/sub operation reached u128 add/sub emitter");
                return false;
            }
        }
        self.emit_stack_store("t5", dest_offset);
        self.emit_stack_store("t6", dest_offset + 8);
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    pub(crate) fn emit_expected_operand_to_t1(&mut self, operand: &IrOperand) {
        match operand {
            IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("li t1, {}", if *b { 1 } else { 0 })),
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Var(var) => {
                if let Some(source) = self.schema_field_value_sources.get(&var.id).cloned() {
                    self.emit_schema_field_source_to_t1(&source);
                } else if let Some(source) = self.prelude_u64_value_sources.get(&var.id).cloned() {
                    self.emit_prelude_u64_value_source_to_t1(&source);
                } else if matches!(var.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64) {
                    self.emit_stack_load("t1", var.id * 8);
                } else if let Some(value) = self.prelude_scalar_immediates.get(&var.id).copied() {
                    self.emit(format!("li t1, {}", value));
                } else {
                    self.emit_stack_load("t1", var.id * 8);
                }
            }
            operand => self.emit_operand_to_register("t1", operand),
        }
    }

    pub(crate) fn emit_prelude_u64_value_source_to_t1(&mut self, source: &PreludeU64ValueSource) {
        self.emit_prelude_u64_value_source_to_t1_at_depth(source, 0);
    }

    pub(crate) fn emit_prelude_u64_value_source_to_t1_at_depth(&mut self, source: &PreludeU64ValueSource, _depth: usize) {
        match source {
            PreludeU64ValueSource::Const(n) => self.emit(format!("li t1, {}", n)),
            PreludeU64ValueSource::ParamVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64ValueSource::StackVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64ValueSource::Field(source) => self.emit_schema_field_source_to_t1(source),
            PreludeU64ValueSource::Binary { op, left, right } => {
                self.emit(format!("# cellscript abi: expected expression u64 {:?}", op));
                let Some(temp_offset) = self.runtime_expr_temp_offset(_depth) else {
                    self.emit("# cellscript abi: fail closed because expression verifier temp stack is exhausted");
                    self.emit_fail(CellScriptRuntimeError::DataPreservationMismatch);
                    return;
                };
                self.emit_prelude_u64_value_source_to_t1_at_depth(left, _depth + 1);
                self.emit_stack_store("t1", temp_offset);
                self.emit_prelude_u64_operand_source_to_t1_at_depth(right, _depth + 1);
                self.emit_stack_load("t3", temp_offset);
                match op {
                    BinaryOp::Add => self.emit("add t1, t3, t1"),
                    BinaryOp::Sub => self.emit("sub t1, t3, t1"),
                    BinaryOp::Mul => self.emit("mul t1, t3, t1"),
                    BinaryOp::Div => {
                        self.emit_divisor_nonzero_guard("t1");
                        self.emit("divu t1, t3, t1");
                    }
                    _ => {
                        self.record_fatal_error("unsupported prelude u64 binary operation reached emitter");
                    }
                }
            }
            PreludeU64ValueSource::Min { left, right } => {
                self.emit("# cellscript abi: expected expression u64 min");
                let Some(temp_offset) = self.runtime_expr_temp_offset(_depth) else {
                    self.emit("# cellscript abi: fail closed because expression verifier temp stack is exhausted");
                    self.emit_fail(CellScriptRuntimeError::DataPreservationMismatch);
                    return;
                };
                self.emit_prelude_u64_value_source_to_t1_at_depth(left, _depth + 1);
                self.emit_stack_store("t1", temp_offset);
                self.emit_prelude_u64_operand_source_to_t1_at_depth(right, _depth + 1);
                self.emit_stack_load("t3", temp_offset);
                self.emit("sltu t2, t3, t1");
                let right_ok_label = self.fresh_label("prelude_min_right_ok");
                self.emit(format!("beqz t2, {}", right_ok_label));
                self.emit("add t1, t3, zero");
                self.emit_label(&right_ok_label);
            }
        }
    }

    pub(crate) fn emit_prelude_u64_operand_source_to_t1(&mut self, source: &PreludeU64OperandSource) {
        self.emit_prelude_u64_operand_source_to_t1_at_depth(source, 0);
    }

    pub(crate) fn emit_prelude_u64_operand_source_to_t1_at_depth(&mut self, source: &PreludeU64OperandSource, _depth: usize) {
        match source {
            PreludeU64OperandSource::Const(n) => self.emit(format!("li t1, {}", n)),
            PreludeU64OperandSource::ParamVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64OperandSource::StackVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64OperandSource::Field(source) => self.emit_schema_field_source_to_t1(source),
            PreludeU64OperandSource::Expr(source) => self.emit_prelude_u64_value_source_to_t1_at_depth(source, _depth),
        }
    }

    pub(crate) fn emit_schema_field_source_to_t1(&mut self, source: &SchemaFieldValueSource) {
        let context = format!("{}.{}", source.type_name, source.field);
        let Some(width) = layout_fixed_scalar_width(&source.layout) else {
            self.emit("li t1, 0");
            return;
        };
        if !self.type_fixed_sizes.contains_key(&source.type_name) {
            if self.emit_schema_field_source_pointer_to("t4", source, width) {
                self.emit(format!("# cellscript abi: expected table field {} index={} size={}", context, source.layout.index, width));
                self.emit_unaligned_scalar_load("t4", "t1", "t2", 0, width);
            } else {
                self.emit("li t1, 0");
            }
            return;
        }
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
            }
            self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
        }
        self.emit(format!("# cellscript abi: expected field {} offset={} size={}", context, source.layout.offset, width));
        self.emit_stack_load("t4", source.obj_var_id * 8);
        self.emit_unaligned_scalar_load("t4", "t1", "t2", source.layout.offset, width);
    }

    pub(crate) fn emit_prepare_schema_field_source(&mut self, source: &SchemaFieldValueSource, width: usize) {
        let context = format!("{}.{}", source.type_name, source.field);
        let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() else {
            return;
        };
        if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
            self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
        } else {
            let Some(field_count) = self.type_layouts.get(&source.type_name).map(|fields| fields.len()) else {
                self.emit(format!(
                    "# cellscript abi: fail closed because schema field source {}.{} has no type layout",
                    source.type_name, source.field
                ));
                self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
                return;
            };
            self.emit_stack_load("t4", source.obj_var_id * 8);
            self.emit_validated_molecule_table_field_bounds_to_t5(
                "t4",
                size_offset,
                source.layout.index,
                field_count,
                width,
                &context,
            );
        }
    }

    pub(crate) fn emit_schema_field_source_pointer_to(
        &mut self,
        dest_reg: &str,
        source: &SchemaFieldValueSource,
        width: usize,
    ) -> bool {
        let context = format!("{}.{}", source.type_name, source.field);
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
                self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
                self.emit_stack_load(dest_reg, source.obj_var_id * 8);
                if source.layout.offset != 0 {
                    self.emit_large_addi(dest_reg, dest_reg, source.layout.offset as i64);
                }
            } else {
                let Some(field_count) = self.type_layouts.get(&source.type_name).map(|fields| fields.len()) else {
                    self.emit(format!(
                        "# cellscript abi: fail closed because schema field source {}.{} has no type layout",
                        source.type_name, source.field
                    ));
                    self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
                    return false;
                };
                self.emit_stack_load("t4", source.obj_var_id * 8);
                self.emit_validated_molecule_table_field_bounds_to_t5(
                    "t4",
                    size_offset,
                    source.layout.index,
                    field_count,
                    width,
                    &context,
                );
                self.emit(format!("add {}, t4, t5", dest_reg));
            }
            true
        } else if self.aggregate_pointer_sources.contains_key(&source.obj_var_id)
            || self.type_fixed_sizes.contains_key(&source.type_name)
        {
            self.emit_stack_load(dest_reg, source.obj_var_id * 8);
            if source.layout.offset != 0 {
                self.emit_large_addi(dest_reg, dest_reg, source.layout.offset as i64);
            }
            true
        } else {
            false
        }
    }

    pub(crate) fn emit_schema_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        if !self.schema_pointer_vars.contains(&var.id) {
            return false;
        }
        let Some(type_name) = named_type_name(&var.ty) else {
            return false;
        };
        let Some(layouts) = self.type_layouts.get(type_name) else {
            return false;
        };
        let field_count = layouts.len();
        let Some(layout) = layouts.get(field).cloned() else {
            return false;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            return self.emit_dynamic_schema_field_access(dest, var, type_name, field, &layout);
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: schema field {}.{} offset={} size={}", type_name, field, layout.offset, width));
        self.emit_stack_load("t4", var.id * 8);
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, type_name);
                self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
                if layout_fixed_scalar_width(&layout).is_some() {
                    self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
                    if dest.ty == IrType::Bool {
                        self.emit_bool_canonical_check("t0");
                    }
                } else {
                    self.emit(format!("addi t0, t4, {}", layout.offset));
                }
            } else {
                self.emit_validated_molecule_table_field_bounds_to_t5(
                    "t4",
                    size_offset,
                    layout.index,
                    field_count,
                    width,
                    &format!("{}.{}", type_name, field),
                );
                self.emit("add t4, t4, t5");
                if layout_fixed_scalar_width(&layout).is_some() {
                    self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
                    if dest.ty == IrType::Bool {
                        self.emit_bool_canonical_check("t0");
                    }
                } else {
                    self.emit("addi t0, t4, 0");
                }
            }
        } else {
            if !self.type_fixed_sizes.contains_key(type_name) {
                return false;
            }
            if layout_fixed_scalar_width(&layout).is_some() {
                self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
                if dest.ty == IrType::Bool {
                    self.emit_bool_canonical_check("t0");
                }
            } else {
                self.emit(format!("addi t0, t4, {}", layout.offset));
            }
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    pub(crate) fn emit_dynamic_schema_field_access(
        &mut self,
        dest: &IrVar,
        obj: &IrVar,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
    ) -> bool {
        if molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).is_none() {
            return false;
        }
        let Some(size_offset) = self.schema_pointer_size_offsets.get(&obj.id).copied() else {
            return false;
        };
        let Some(dest_size_offset) = self.dynamic_value_size_offsets.get(&dest.id).copied() else {
            return false;
        };
        let Some(field_count) = self.type_layouts.get(type_name).map(|fields| fields.len()) else {
            return false;
        };

        let context = format!("{}.{}", type_name, field);
        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: dynamic schema field {} index={} as Molecule vector bytes", context, layout.index));
        self.emit_stack_load("t4", obj.id * 8);
        self.emit_validated_molecule_table_field_span_to_t5_t6("t4", size_offset, layout.index, field_count, &context);
        self.emit("add t0, t4, t5");
        self.emit("sub t1, t6, t5");
        self.emit_stack_store("t0", dest.id * 8);
        self.emit_stack_store("t1", dest_size_offset);
        true
    }

    pub(crate) fn emit_aggregate_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let Some(source) = self.aggregate_pointer_sources.get(&var.id) else {
            return false;
        };
        let source_ty = source.ty.clone();
        let Some(layout) = aggregate_field_layout(&source_ty, field) else {
            return false;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            return false;
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!(
            "# cellscript abi: fixed aggregate field {}.{} offset={} size={}",
            aggregate_type_label(&source_ty),
            field,
            layout.offset,
            width
        ));
        self.emit_stack_load("t4", var.id * 8);
        if layout_fixed_scalar_width(&layout).is_some() {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            if dest.ty == IrType::Bool {
                self.emit_bool_canonical_check("t0");
            }
        } else {
            self.emit(format!("addi t0, t4, {}", layout.offset));
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    pub(crate) fn emit_tuple_call_return_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let Some(slot_var_id) = self.tuple_call_return_field_slots.get(&(var.id, field.to_string())).copied() else {
            return false;
        };
        if slot_var_id != dest.id {
            return false;
        }
        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: tuple call return field .{} projected from return register", field));
        true
    }

    pub(crate) fn emit_generic_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let Some(type_name) = named_type_name(&var.ty) else {
            return false;
        };
        if !self.type_fixed_sizes.contains_key(type_name) {
            return false;
        }
        let Some(layout) = self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
            return false;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            return false;
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: generic field {}.{} offset={} size={}", type_name, field, layout.offset, width));

        // Bounds check: if the object has a known size offset, verify the data
        // is large enough to contain this field.
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
            self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
        } else if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&var.id).copied() {
            self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
        }

        // Load the object pointer from the stack slot
        self.emit_stack_load("t4", var.id * 8);
        if layout_fixed_scalar_width(&layout).is_some() {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            if dest.ty == IrType::Bool {
                self.emit_bool_canonical_check("t0");
            }
        } else {
            self.emit(format!("addi t0, t4, {}", layout.offset));
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }
}
