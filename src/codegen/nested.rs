//! Canonical bounded nested-value validation. Source admission is deliberately
//! separate: this emitter only proves byte structure within a caller-owned
//! immutable, readable slice. It grants no Cell or lifecycle authority.
use super::*;
use cellscript_artifact_checker::bounded_nested::{ElementShape, NestedValueContract};

#[derive(Clone, Debug)]
pub(super) struct NestedCellLayout {
    pub(super) capacity: usize,
    pub(super) fields: Vec<NestedCellField>,
}

#[derive(Clone, Debug)]
pub(super) enum NestedCellField {
    Fixed(usize),
    Values(NestedValueContract),
}

impl CodeGenerator {
    /// Fixed siblings use the same complete output span contract. Scalars keep
    /// full register equality; byte values retain their declared storage ABI.
    pub(super) fn emit_nested_fixed_output_field(
        &mut self,
        output_size: usize,
        output_buffer: usize,
        layout: &SchemaFieldLayout,
        field_count: usize,
        expected: &IrOperand,
    ) -> bool {
        let Some(width) = nested_fixed_width(&layout.ty).map(|width| width as usize) else { return false };
        let constant = match expected {
            IrOperand::Const(value) => {
                let Some(bytes) = fixed_byte_const_bytes(value)
                    .or_else(|| {
                        fixed_scalar_const_value(value).and_then(|value| (width <= 8).then(|| value.to_le_bytes()[..width].to_vec()))
                    })
                    .filter(|bytes| bytes.len() == width)
                else {
                    return false;
                };
                Some(bytes)
            }
            IrOperand::Var(_) => None,
        };
        let source = self
            .nested_cell_buffers
            .keys()
            .find(|id| self.cell_buffer_offsets.get(id) == Some(&output_buffer))
            .copied()
            .expect("nested output belongs to a typed Cell buffer");
        let expected_id = match expected {
            IrOperand::Var(var) => var.id as u64,
            _ => u64::MAX,
        };
        let start = self.fresh_label(&format!("nested_output_check_{source}_{}_{expected_id}", layout.index));
        let done = format!("{start}_done");
        let invalid = self.fresh_label("nested_output_invalid");
        self.emit_label(&start);
        self.emit_nested_output_span(output_size, output_buffer, layout.index, field_count, &invalid);
        self.emit(format!("li a3, {width}"));
        self.emit(format!("bne a2, a3, {invalid}"));
        self.emit("add a0, t4, t5");
        let scalar = matches!(layout.ty, IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64);
        if scalar {
            match expected {
                IrOperand::Var(var) => self.emit_stack_load("a3", self.scalar_slot_offset(var.id)),
                IrOperand::Const(value) => {
                    self.emit(format!("li a3, {}", fixed_scalar_const_value(value).expect("checked scalar constant")))
                }
            }
            self.emit_unaligned_scalar_load("a0", "t0", "t2", 0, width);
            self.emit(format!("bne t0, a3, {invalid}"));
            self.emit(format!("j {done}"));
        } else if let Some(bytes) = constant {
            for (index, byte) in bytes.into_iter().enumerate() {
                self.emit(format!("lbu t0, {index}(a0)"));
                self.emit(format!("li t1, {byte}"));
                self.emit(format!("bne t0, t1, {invalid}"));
            }
            self.emit(format!("j {done}"));
        } else if let IrOperand::Var(var) = expected {
            if matches!(self.expected_fixed_byte_source(expected, width), Some(ExpectedFixedByteSource::StackSlot { .. })) {
                self.emit_sp_addi("a1", self.scalar_slot_offset(var.id));
            } else {
                self.emit_stack_load("a1", self.scalar_slot_offset(var.id));
            }
            let bytes = self.fresh_label("nested_output_bytes");
            self.emit_label(&bytes);
            self.emit(format!("beqz a2, {done}"));
            self.emit("lbu t0, 0(a0)");
            self.emit("lbu t1, 0(a1)");
            self.emit(format!("bne t0, t1, {invalid}"));
            self.emit("addi a0, a0, 1");
            self.emit("addi a1, a1, 1");
            self.emit("addi a2, a2, -1");
            self.emit(format!("j {bytes}"));
        }
        self.emit_label(&invalid);
        self.emit_fail(if scalar {
            CellScriptRuntimeError::CellLoadFailed
        } else {
            CellScriptRuntimeError::DynamicFieldValueMismatch
        });
        self.emit_label(&done);
        true
    }

    /// Compare a canonical output field to the immutable typed value. Keep the
    /// span and byte loop explicit for independent machine verification.
    pub(super) fn emit_nested_output_field(
        &mut self,
        output_size: usize,
        output_buffer: usize,
        field_index: usize,
        field_count: usize,
        expected: &IrVar,
        expected_size: usize,
    ) {
        let source = self
            .nested_cell_buffers
            .keys()
            .find(|id| self.cell_buffer_offsets.get(id) == Some(&output_buffer))
            .copied()
            .expect("nested output belongs to a typed Cell buffer");
        let start = self.fresh_label(&format!("nested_output_check_{source}_{field_index}_{}", expected.id));
        let done = format!("{start}_done");
        let invalid = self.fresh_label("nested_output_invalid");
        let bytes = self.fresh_label("nested_output_bytes");
        self.emit_label(&start);
        self.emit_nested_output_span(output_size, output_buffer, field_index, field_count, &invalid);
        // t6 is dead before either stack helper can use it as scratch.
        self.emit_stack_load("a3", expected_size);
        self.emit(format!("bne a2, a3, {invalid}"));
        self.emit("add a0, t4, t5");
        self.emit_stack_load("a1", self.scalar_slot_offset(expected.id));
        self.emit_label(&bytes);
        self.emit(format!("beqz a2, {done}"));
        self.emit("lbu t0, 0(a0)");
        self.emit("lbu t1, 0(a1)");
        self.emit(format!("bne t0, t1, {invalid}"));
        self.emit("addi a0, a0, 1");
        self.emit("addi a1, a1, 1");
        self.emit("addi a2, a2, -1");
        self.emit(format!("j {bytes}"));
        self.emit_label(&invalid);
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&done);
    }

    /// Leaves the field length in a2 and its buffer-relative start in t5.
    /// t6 is dead on return; stack helpers may reuse it afterwards.
    fn emit_nested_output_span(
        &mut self,
        output_size: usize,
        output_buffer: usize,
        field_index: usize,
        field_count: usize,
        invalid: &str,
    ) {
        self.emit_sp_addi("t4", output_buffer);
        self.emit_stack_load("a0", output_size);
        self.emit(format!("li t1, {}", 4 * (field_count + 1)));
        self.emit(format!("bltu a0, t1, {invalid}"));
        self.emit_u32_le_from_base_to("t0", "t4", 0, "t2");
        self.emit(format!("bne t0, a0, {invalid}"));
        self.emit_u32_le_from_base_to("t5", "t4", 4 * (field_index + 1), "t2");
        if field_index + 1 < field_count {
            self.emit_u32_le_from_base_to("t6", "t4", 4 * (field_index + 2), "t2");
        } else {
            self.emit("addi t6, a0, 0");
        }
        self.emit(format!("bltu t5, t1, {invalid}"));
        self.emit(format!("bltu t6, t5, {invalid}"));
        self.emit(format!("bltu a0, t6, {invalid}"));
        self.emit("sub a2, t6, t5");
    }

    /// A nested Cell field is an immutable bounded span. Keep its complete
    /// projection explicit even when a surrounding schema check was cached.
    pub(super) fn emit_nested_cell_field(
        &mut self,
        dest: &IrVar,
        obj: &IrVar,
        source_size: usize,
        destination_size: usize,
        field_index: usize,
        field_count: usize,
    ) {
        let done = self.emit_nested_use_marker("cellfield", dest);
        let invalid = self.fresh_label("nested_cell_field_invalid");
        self.emit_stack_load("t4", self.scalar_slot_offset(obj.id));
        self.emit_stack_load("a0", source_size);
        let header = 4 * (field_count + 1);
        self.emit(format!("li t1, {header}"));
        self.emit(format!("bltu a0, t1, {invalid}"));
        self.emit_u32_le_from_base_to("t0", "t4", 0, "t2");
        self.emit(format!("bne t0, a0, {invalid}"));
        self.emit_u32_le_from_base_to("t5", "t4", 4 * (field_index + 1), "t2");
        if field_index + 1 < field_count {
            self.emit_u32_le_from_base_to("t6", "t4", 4 * (field_index + 2), "t2");
        } else {
            self.emit("addi t6, a0, 0");
        }
        self.emit(format!("bltu t5, t1, {invalid}"));
        self.emit(format!("bltu t6, t5, {invalid}"));
        self.emit(format!("bltu a0, t6, {invalid}"));
        self.emit("add t0, t4, t5");
        self.emit("sub t1, t6, t5");
        self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
        self.emit_schema_size_store("t1", destination_size);
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&done);
    }

    /// Derive the complete Cell table bound, including its header. Existing
    /// fixed/dynamic Cell profiles retain their legacy buffer allocation.
    pub(super) fn nested_cell_layout(&self, name: &str) -> Result<Option<NestedCellLayout>> {
        let Some(fields) = self.type_layouts.get(name) else { return Ok(None) };
        if !fields.values().any(|field| matches!(Self::nested_type(&field.ty), Some(crate::nested_values::NestedType::Values(_)))) {
            return Ok(None);
        }
        let mut ordered = fields.values().collect::<Vec<_>>();
        ordered.sort_by_key(|field| field.index);
        let mut capacity =
            4usize.checked_mul(ordered.len() + 1).ok_or_else(|| CompileError::without_span("nested Cell table header overflows"))?;
        let mut layout = Vec::new();
        for field in ordered {
            let (item, width) = if let Some(crate::nested_values::NestedType::Values(values)) = Self::nested_type(&field.ty) {
                (NestedCellField::Values(self.nested_contract(&values)?), values.max_total_bytes as usize)
            } else {
                let width = nested_fixed_width(&field.ty).ok_or_else(|| CompileError::without_span("a Cell containing NestedValues requires bounded nested fields or fixed unsigned/Hash/Address/byte-array fields"))? as usize;
                (NestedCellField::Fixed(width), width)
            };
            capacity = capacity.checked_add(width).ok_or_else(|| CompileError::without_span("nested Cell byte bound overflows"))?;
            layout.push(item);
        }
        Ok(Some(NestedCellLayout { capacity: capacity.max(RUNTIME_CELL_BUFFER_SIZE), fields: layout }))
    }

    pub(super) fn cell_buffer_capacity(&self, id: usize) -> usize {
        self.nested_cell_buffers.get(&id).map_or(RUNTIME_CELL_BUFFER_SIZE, |layout| layout.capacity)
    }

    pub(super) fn nested_cell_buffer_layout(&self, offset: usize) -> Option<NestedCellLayout> {
        self.nested_cell_buffers
            .iter()
            .find_map(|(id, layout)| (self.cell_buffer_offsets.get(id) == Some(&offset)).then(|| layout.clone()))
    }

    pub(super) fn nested_cell_load_marker(&mut self, size: usize, buffer: usize) -> String {
        let source = self
            .nested_cell_buffers
            .keys()
            .find(|id| self.cell_buffer_offsets.get(id) == Some(&buffer))
            .copied()
            .expect("nested Cell layout belongs to a typed buffer");
        let label = self.fresh_label(&format!("nested_cell_codec_{source}_{size}_{buffer}"));
        self.emit_label(&label);
        label
    }

    /// Runs only on successfully loaded typed Cell buffers. Full-table
    /// canonical validation dominates application reads, even unused fields.
    /// Restore the syscall success result expected by the existing caller.
    pub(super) fn emit_nested_loaded_cell_validation(&mut self, size: usize, buffer: usize, layout: &NestedCellLayout, start: &str) {
        self.emit_nested_table_validation(size, buffer, layout, start, None);
    }

    /// A private selection frame supplies its own fatal exits; it cannot join
    /// an entry's shared failure handler while SP addresses its subregion.
    pub(super) fn emit_nested_table_validation(
        &mut self,
        size: usize,
        buffer: usize,
        layout: &NestedCellLayout,
        start: &str,
        failures: Option<(&str, &str)>,
    ) {
        let done = format!("{start}_done");
        let invalid = self.fresh_label("nested_cell_invalid");
        let bounds = self.fresh_label("nested_cell_bounds");
        self.emit_label(&format!("{start}_validate"));
        self.emit(format!("bnez a0, {done}"));
        self.emit_stack_load("t0", size);
        self.emit(format!("li t1, {}", layout.capacity));
        self.emit(format!("bltu t1, t0, {invalid}"));
        let header = 4 * (layout.fields.len() + 1);
        self.emit(format!("li t1, {header}"));
        self.emit(format!("bltu t0, t1, {invalid}"));
        self.emit_sp_addi("a0", buffer);
        self.emit_u32_le_from_base_to("t0", "a0", 4, "t2");
        self.emit(format!("bne t0, t1, {invalid}"));
        // The complete table codec has its own straight-line contract. Do not
        // depend on cached per-field schema checks: all fields, including
        // unused ones, must be checked on each successful load.
        self.emit_u32_le_from_base_to("t0", "a0", 0, "t2");
        self.emit_stack_load("t1", size);
        self.emit(format!("bne t0, t1, {bounds}"));
        for (index, field) in layout.fields.iter().enumerate() {
            self.emit_sp_addi("t4", buffer);
            self.emit_stack_load("a1", size);
            self.emit_u32_le_from_base_to("t5", "t4", 4 * (index + 1), "t2");
            if index + 1 < layout.fields.len() {
                self.emit_u32_le_from_base_to("t6", "t4", 4 * (index + 2), "t2");
            } else {
                self.emit("addi t6, a1, 0");
            }
            self.emit(format!("bltu t6, t5, {bounds}"));
            self.emit(format!("bltu a1, t6, {bounds}"));
            self.emit("sub a1, t6, t5");
            match field {
                NestedCellField::Fixed(width) => {
                    self.emit(format!("li t0, {width}"));
                    self.emit(format!("bne a1, t0, {invalid}"));
                }
                NestedCellField::Values(contract) => {
                    self.emit("add a0, t4, t5");
                    self.emit_bounded_nested_validation(contract, &invalid)
                        .expect("layout contract validated before frame allocation");
                }
            }
        }
        self.emit("li a0, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        if let Some((invalid, _)) = failures {
            self.emit(format!("j {invalid}"));
        } else {
            self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
        }
        self.emit_label(&bounds);
        if let Some((_, bounds)) = failures {
            self.emit(format!("j {bounds}"));
        } else {
            self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        }
        self.emit_label(&done);
    }
}

impl CodeGenerator {
    /// Validate all elements before exposing any view.
    ///
    /// ABI: a0 = readable immutable bytes, a1 = encoded byte length. On success
    /// these registers are preserved and a2 is the element count. Clobbers
    /// a3–a5 and t0–t3 only; no calls, stack use, allocation or data stores.
    /// The caller owns the slice lifetime and branches at `invalid` to a
    /// terminal error, never to a value-returning success continuation.
    ///
    /// Stable artifact admission separately binds source bounds and machine
    /// obligations; byte validity alone does not establish that evidence.
    pub(super) fn emit_bounded_nested_validation(&mut self, contract: &NestedValueContract, invalid: &str) -> Result<()> {
        contract.validate().map_err(|error| CompileError::new(error.to_string(), crate::error::Span::default()))?;
        let done = self.fresh_label("nested_value_valid");
        let item = self.fresh_label("nested_value_item");
        let end_ready = self.fresh_label("nested_value_end_ready");
        self.emit(format!(
            "# cellscript nested codec: {} items={} payload={} total={}",
            contract.profile, contract.max_items, contract.max_payload_bytes, contract.max_total_bytes
        ));
        self.emit("li a2, 0");
        self.emit("li t0, 4");
        self.emit(format!("bltu a1, t0, {invalid}"));
        self.emit(format!("li t0, {}", contract.max_total_bytes));
        self.emit(format!("bltu t0, a1, {invalid}"));
        // Size is bounded above before pointer arithmetic. Reject wrapping
        // slices before even the first byte load (caller proves readability).
        self.emit("add t0, a0, a1");
        self.emit(format!("bltu t0, a0, {invalid}"));
        self.emit_u32_le_from_base_to("t0", "a0", 0, "t1");
        self.emit(format!("bne t0, a1, {invalid}"));
        self.emit("li t0, 4");
        self.emit(format!("beq a1, t0, {done}"));
        self.emit("li t0, 8");
        self.emit(format!("bltu a1, t0, {invalid}"));
        self.emit_u32_le_from_base_to("a4", "a0", 4, "t1");
        self.emit(format!("bltu a4, t0, {invalid}"));
        self.emit(format!("bltu a1, a4, {invalid}"));
        self.emit("andi t0, a4, 3");
        self.emit(format!("bnez t0, {invalid}"));
        self.emit("srli a2, a4, 2");
        self.emit("addi a2, a2, -1");
        self.emit(format!("li t0, {}", contract.max_items));
        self.emit(format!("bltu t0, a2, {invalid}"));
        self.emit("li a3, 0");
        self.emit_label(&item);
        self.emit("addi t0, a3, 1");
        self.emit("addi a5, a1, 0");
        self.emit(format!("beq t0, a2, {end_ready}"));
        // a3 + 1 < count, so this offset word lies inside the already checked
        // header. Preserve a4 (the previous end) across each item.
        self.emit("slli t0, t0, 2");
        self.emit("addi t0, t0, 4");
        self.emit("add t0, a0, t0");
        self.emit_u32_le_from_base_to("a5", "t0", 0, "t1");
        self.emit_label(&end_ready);
        self.emit(format!("bltu a5, a4, {invalid}"));
        self.emit(format!("bltu a1, a5, {invalid}"));
        self.emit("sub t2, a5, a4");
        self.emit("add t0, a0, a4");
        self.emit_nested_element_validation(&contract.element, contract.max_payload_bytes, invalid);
        self.emit("addi a4, a5, 0");
        self.emit("addi a3, a3, 1");
        self.emit(format!("bltu a3, a2, {item}"));
        self.emit_label(&done);
        Ok(())
    }
}

#[cfg(all(test, feature = "vm-runner"))]
mod tests;

impl CodeGenerator {
    pub(super) fn nested_type(ty: &IrType) -> Option<crate::nested_values::NestedType<crate::nested_values::Element>> {
        crate::nested_values::ir_type(ty)
    }

    pub(super) fn nested_size_offset(&self, var: &IrVar) -> Option<usize> {
        self.dynamic_value_size_offsets.get(&var.id).or_else(|| self.schema_pointer_size_offsets.get(&var.id)).copied()
    }

    fn nested_contract(
        &self,
        values: &crate::nested_values::ValuesType<crate::nested_values::Element>,
    ) -> Result<NestedValueContract> {
        let contract = values.contract(values.element.shape());
        contract.validate().map_err(|error| CompileError::without_span(error.to_string()))?;
        Ok(contract)
    }

    /// Validate one encoded element in t0/t2, preserving a0/a1 and memory.
    /// t1/t3 are scratch; record offsets are bounded by the admitted layout.
    fn emit_nested_element_validation(&mut self, element: &ElementShape, payload: u32, invalid: &str) {
        if let ElementShape::Record { fixed_field_widths } = element {
            // All record fields before Bytes have exact widths. Their offsets
            // are therefore constants; verifying them all binds the table's
            // exact field count, non-aliasing and lack of gaps in one pass.
            let header = (fixed_field_widths.len() + 2) * 4;
            let payload_start = header + fixed_field_widths.iter().map(|width| usize::from(*width)).sum::<usize>();
            self.emit(format!("li t1, {}", payload_start + 4));
            self.emit(format!("bltu t2, t1, {invalid}"));
            self.emit_u32_le_from_base_to("t1", "t0", 0, "t3");
            self.emit(format!("bne t1, t2, {invalid}"));
            let mut offset = header;
            for field in 0..=fixed_field_widths.len() {
                self.emit_u32_le_from_base_to("t1", "t0", 4 * (field + 1), "t3");
                self.emit(format!("li t3, {offset}"));
                self.emit(format!("bne t1, t3, {invalid}"));
                if let Some(width) = fixed_field_widths.get(field) {
                    offset += usize::from(*width);
                }
            }
            // payload_start can exceed ADDI's positive immediate range; use
            // the already loaded, checked final offset without a scratch helper.
            self.emit("add t0, t0, t3");
            self.emit("sub t2, t2, t3");
        }
        self.emit("li t1, 4");
        self.emit(format!("bltu t2, t1, {invalid}"));
        self.emit_u32_le_from_base_to("t1", "t0", 0, "t3");
        self.emit(format!("li t3, {}", payload));
        self.emit(format!("bltu t3, t1, {invalid}"));
        self.emit("addi t1, t1, 4");
        self.emit(format!("bne t1, t2, {invalid}"));
    }

    fn emit_nested_view_validation(
        &mut self,
        kind: &crate::nested_values::NestedType<crate::nested_values::Element>,
        invalid: &str,
    ) -> Result<()> {
        use crate::nested_values::{Element, NestedType, ValuesType};
        let (element, payload) = match kind {
            NestedType::BytesView { max_payload_bytes } => (Element::Bytes, *max_payload_bytes),
            NestedType::RecordView { element, max_payload_bytes } => (element.clone(), *max_payload_bytes),
            NestedType::Values(_) => return Err(CompileError::without_span("nested element validation requires a derived view")),
        };
        let contract =
            self.nested_contract(&ValuesType { element, max_items: 1, max_payload_bytes: payload, max_total_bytes: 4096 })?;
        let header = match &contract.element {
            ElementShape::Bytes {} => 4,
            ElementShape::Record { fixed_field_widths } => {
                (fixed_field_widths.len() as u32 + 2) * 4 + fixed_field_widths.iter().map(|width| u32::from(*width)).sum::<u32>() + 4
            }
        };
        self.emit("li t0, 4");
        self.emit(format!("bltu a1, t0, {invalid}"));
        self.emit(format!("li t0, {}", header + payload));
        self.emit(format!("bltu t0, a1, {invalid}"));
        self.emit("add t0, a0, a1");
        self.emit(format!("bltu t0, a0, {invalid}"));
        self.emit("addi t0, a0, 0");
        self.emit("addi t2, a1, 0");
        self.emit_nested_element_validation(&contract.element, payload, invalid);
        Ok(())
    }

    /// Run after every ABI argument has been spilled. Even an unused nested
    /// witness must validate before application control flow can return success.
    pub(super) fn emit_nested_param_validation(&mut self, params: &[IrParam]) -> Result<()> {
        for param in params {
            let Some(kind) = Self::nested_type(&param.ty) else {
                continue;
            };
            let size = self
                .nested_size_offset(&param.binding)
                .ok_or_else(|| CompileError::without_span("nested witness has no ABI length slot"))?;
            let invalid = self.fresh_label("nested_param_invalid");
            let ready = self.fresh_label("nested_param_ready");
            self.emit_stack_load("a0", self.scalar_slot_offset(param.binding.id));
            self.emit_stack_load("a1", size);
            let codec = self.fresh_label(&format!("nested_param_codec_{}", param.binding.id));
            self.emit_label(&codec);
            match &kind {
                crate::nested_values::NestedType::Values(values) => {
                    let contract = self.nested_contract(values)?;
                    self.emit_bounded_nested_validation(&contract, &invalid)?;
                }
                _ => self.emit_nested_view_validation(&kind, &invalid)?,
            }
            self.emit(format!("j {ready}"));
            self.emit_label(&invalid);
            self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
            self.emit_label(&ready);
        }
        Ok(())
    }

    pub(super) fn emit_nested_use_marker(&mut self, kind: &str, dest: &IrVar) -> String {
        let label = self.fresh_label(&format!("nested_use_{kind}_{}", dest.id));
        self.emit_label(&label);
        format!("{label}_done")
    }

    pub(super) fn emit_nested_length_result(&mut self, dest: &IrVar, operand: &IrOperand) -> bool {
        if !matches!(operand, IrOperand::Var(var) if matches!(Self::nested_type(&var.ty),
            Some(crate::nested_values::NestedType::Values(_) | crate::nested_values::NestedType::BytesView { .. })))
        {
            return false;
        }
        let done = self.emit_nested_use_marker("length", dest);
        self.emit_nested_length(operand);
        self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
        self.emit_label(&done);
        true
    }

    /// Entry validation dominates every use and source views are immutable.
    /// Decode just the count here; rescanning every payload inside a user loop
    /// would turn bounded traversal into unnecessarily quadratic work.
    pub(super) fn emit_nested_length(&mut self, operand: &IrOperand) -> bool {
        let IrOperand::Var(var) = operand else {
            return false;
        };
        let Some(kind) = Self::nested_type(&var.ty) else {
            return false;
        };
        if matches!(kind, crate::nested_values::NestedType::RecordView { .. }) {
            return false;
        }
        let Some(size) = self.nested_size_offset(var) else {
            self.emit_fail(CellScriptRuntimeError::CollectionRuntimeUnsupported);
            return true;
        };
        self.emit_stack_load("a0", self.scalar_slot_offset(var.id));
        self.emit_stack_load("a1", size);
        match kind {
            crate::nested_values::NestedType::Values(_) => {
                let done = self.fresh_label("nested_count_ready");
                self.emit("li t0, 0");
                self.emit("li t1, 4");
                self.emit(format!("beq a1, t1, {done}"));
                self.emit_u32_le_from_base_to("t0", "a0", 4, "t1");
                self.emit("srli t0, t0, 2");
                self.emit("addi t0, t0, -1");
                self.emit_label(&done);
            }
            crate::nested_values::NestedType::BytesView { .. } => self.emit_u32_le_from_base_to("t0", "a0", 0, "t1"),
            _ => unreachable!(),
        }
        true
    }

    pub(super) fn emit_nested_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> bool {
        let IrOperand::Var(var) = arr else {
            return false;
        };
        let Some(kind) = Self::nested_type(&var.ty) else {
            return false;
        };
        if matches!(kind, crate::nested_values::NestedType::RecordView { .. }) {
            return false;
        }
        let done = self.emit_nested_use_marker("index", dest);
        // a3 survives the count read; no temporary register is live across a
        // stack helper's possible large-offset address materialization.
        self.emit_operand_to_register("a3", idx);
        self.emit_nested_length(arr);
        let in_range = self.fresh_label("nested_index_in_range");
        self.emit(format!("bltu a3, t0, {in_range}"));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&in_range);
        if matches!(kind, crate::nested_values::NestedType::BytesView { .. }) {
            self.emit("add t1, a0, a3");
            self.emit("lbu t0, 4(t1)");
        } else {
            let Some(dest_size) = self.nested_size_offset(dest) else {
                self.emit_fail(CellScriptRuntimeError::CollectionRuntimeUnsupported);
                return true;
            };
            self.emit("slli t1, a3, 2");
            self.emit("add t1, a0, t1");
            self.emit_u32_le_from_base_to("t2", "t1", 4, "t3");
            self.emit("addi a3, a3, 1");
            let end_ready = self.fresh_label("nested_index_end_ready");
            self.emit("addi t4, a1, 0");
            self.emit(format!("beq a3, t0, {end_ready}"));
            self.emit_u32_le_from_base_to("t4", "t1", 8, "t3");
            self.emit_label(&end_ready);
            self.emit("sub t4, t4, t2");
            self.emit("add t0, a0, t2");
            self.emit_schema_size_store("t4", dest_size);
        }
        self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
        self.emit_label(&done);
        true
    }

    pub(super) fn emit_nested_field(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> Result<bool> {
        let IrOperand::Var(var) = obj else {
            return Ok(false);
        };
        let Some(crate::nested_values::NestedType::RecordView { element: crate::nested_values::Element::Record(record), .. }) =
            Self::nested_type(&var.ty)
        else {
            return Ok(false);
        };
        let layout = record.layout();
        let index = if field == layout.payload_field {
            layout.fixed_fields.len()
        } else {
            layout
                .fixed_fields
                .iter()
                .position(|(name, _)| name == field)
                .ok_or_else(|| CompileError::without_span(format!("unknown nested record field {}.{field}", record.spelling())))?
        };
        let Some(size) = self.nested_size_offset(var) else {
            self.emit_fail(CellScriptRuntimeError::CollectionRuntimeUnsupported);
            return Ok(true);
        };
        let offset = (layout.fixed_fields.len() + 2) * 4
            + layout.fixed_fields[..index].iter().map(|(_, ty)| usize::from(ty.width())).sum::<usize>();
        let done = self.emit_nested_use_marker("field", dest);
        self.emit_stack_load("a0", self.scalar_slot_offset(var.id));
        self.emit(format!("li t1, {offset}"));
        self.emit("add t0, a0, t1");
        if index == layout.fixed_fields.len() {
            let dest_size =
                self.nested_size_offset(dest).ok_or_else(|| CompileError::without_span("nested payload view has no length slot"))?;
            self.emit_stack_load("a1", size);
            self.emit("sub a1, a1, t1");
            self.emit_schema_size_store("a1", dest_size);
        } else if let Some(width) = fixed_scalar_width(&dest.ty, type_static_length(&dest.ty)) {
            self.emit("addi a0, t0, 0");
            self.emit_unaligned_scalar_load("a0", "t0", "t2", 0, width);
        } else {
            self.aggregate_pointer_sources.insert(dest.id, AggregatePointerSource { ty: dest.ty.clone() });
        }
        self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
        self.emit_label(&done);
        Ok(true)
    }
}

fn nested_fixed_width(ty: &IrType) -> Option<u16> {
    match ty {
        IrType::U8 => Some(1),
        IrType::U16 => Some(2),
        IrType::U32 => Some(4),
        IrType::U64 => Some(8),
        IrType::U128 => Some(16),
        IrType::Hash | IrType::Address => Some(32),
        IrType::Array(inner, len) if **inner == IrType::U8 && (1..=256).contains(len) => Some(*len as u16),
        _ => None,
    }
}
