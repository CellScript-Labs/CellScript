//! Internal exact-role selection lowering, including a bounded nested-table
//! prerequisite. Source admission and independent machine proof are separate;
//! the public experimental source profile still permits only fixed layouts.

use super::nested::{NestedCellField, NestedCellLayout};
use super::*;

const SOURCE: usize = 0;
const INDEX: usize = 8;
const LENGTH: usize = 16;
const INPUTS: usize = 24;
const OUTPUTS: usize = 32;
const SELECTED_INPUTS: usize = 40;
const SELECTED_OUTPUTS: usize = 48;
const BYTES: usize = 56;
const GROUP_COUNT: usize = 64;
const GROUP_INDEX: usize = 72;
const CURRENT_ROLE: usize = 80;
const CURSOR: usize = 88;
const PARTNER: usize = 96;
const HAS_TYPE: usize = 104;
const CURRENT_HASH: usize = 128;
const TYPE_HASH: usize = 160;
const LOCK_HASH: usize = 192;
const SCRATCH: usize = 224;
const DATA: usize = 256;
const ROLE_COUNTS: usize = 768;
pub(super) const INPUT_TABLE: usize = 896;
pub(super) const OUTPUT_TABLE: usize = 2944;
pub(super) const SELECTION_FRAME_BYTES: usize = 4992;
pub(super) const RECORD_BYTES: usize = 64;

#[derive(Clone, Debug)]
pub(super) enum Correspondence {
    Preserve,
    Create,
    Burn,
    Merge(Vec<u8>),
}

/// Derived layout, not a public byte-offset source API. Nested tables normalize
/// their validated key into private scratch before the shared pairing logic.
/// Schema identity is nominal and must be authenticated by source admission.
#[derive(Clone, Debug)]
pub(super) struct ExactRole {
    pub type_hash: [u8; 32],
    pub lock_hash: [u8; 32],
    pub schema: [u8; 32],
    pub width: usize,
    pub key_offset: usize,
    pub key_width: usize,
    pub inputs: (usize, usize),
    pub outputs: (usize, usize),
    pub local_type: bool,
    pub correspondence: Correspondence,
    /// Internal bounded-table primitive. Source/checker admission is separate.
    pub nested: Option<NestedRole>,
}

#[derive(Clone, Debug)]
pub(super) struct NestedRole {
    pub layout: NestedCellLayout,
    pub key_field: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ExactSelection {
    pub roles: Vec<ExactRole>,
    pub input_scan: usize,
    pub output_scan: usize,
    pub selected_per_side: usize,
    pub aggregate_bytes: usize,
}

impl ExactSelection {
    pub(super) fn frame_bytes(&self) -> usize {
        SELECTION_FRAME_BYTES
            + self
                .roles
                .iter()
                .filter_map(|role| role.nested.as_ref())
                .map(|role| align_up(role.layout.capacity, 16))
                .max()
                .unwrap_or(0)
    }

    fn validate(&self) -> Result<()> {
        let invalid = || CompileError::without_span("invalid internal fixed-role selection contract");
        if !(1..=8).contains(&self.roles.len())
            || !(1..=64).contains(&self.input_scan)
            || !(1..=64).contains(&self.output_scan)
            || !(1..=32).contains(&self.selected_per_side)
            || !(1..=32768).contains(&self.aggregate_bytes)
        {
            return Err(invalid());
        }
        for (index, role) in self.roles.iter().enumerate() {
            if role.nested.as_ref().map_or(!(1..=512).contains(&role.width), |nested| {
                let mut capacity = (nested.layout.fields.len() + 1).checked_mul(4);
                let mut has_values = false;
                for field in &nested.layout.fields {
                    let width = match field {
                        NestedCellField::Fixed(width) if (1..=256).contains(width) => *width,
                        NestedCellField::Values(contract) if contract.validate().is_ok() => {
                            has_values = true;
                            contract.max_total_bytes as usize
                        }
                        _ => return true,
                    };
                    capacity = capacity.and_then(|total| total.checked_add(width));
                }
                !has_values || capacity.is_none_or(|size| size.max(512) != nested.layout.capacity)
                    || !(512..=32768).contains(&nested.layout.capacity)
                    || role.width != nested.layout.capacity || role.key_offset != 0
                    || !matches!(nested.layout.fields.get(nested.key_field), Some(NestedCellField::Fixed(width)) if *width == role.key_width)
            })
                || !(1..=32).contains(&role.key_width)
                || role.key_offset.checked_add(role.key_width).is_none_or(|end| end > role.width)
                || [role.inputs, role.outputs].iter().any(|&(min, max)| min > max || max > 32)
                || self.roles[..index].iter().any(|other| other.type_hash == role.type_hash)
                || matches!(&role.correspondence, Correspondence::Merge(key) if key.len() != role.key_width)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

impl CodeGenerator {
    pub(super) fn selection_frame(&self, dest: Option<&IrVar>) -> Result<usize> {
        dest.map_or(self.exact_selection_offset, |dest| self.exact_selection_offsets.get(&dest.id).copied())
            .ok_or_else(|| CompileError::without_span("exact-role verification lacks its owned scratch frame"))
    }

    pub(super) fn source_selection_contract(&self, request: &crate::cell_selection::Contract) -> Result<ExactSelection> {
        request.validate().map_err(CompileError::without_span)?;
        let mut roles = Vec::new();
        for role in &request.roles {
            let invalid = || {
                CompileError::without_span(format!("exact role '{}' lacks an admitted fixed or bounded Cell layout", role.type_name))
            };
            if !self.cell_type_names.contains(&role.type_name) {
                return Err(invalid());
            }
            let fields = self.type_layouts.get(&role.type_name).ok_or_else(invalid)?;
            let nested = self.nested_cell_layout(&role.type_name)?;
            for field in fields.values() {
                let admitted = matches!(
                    field.ty,
                    IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64 | IrType::U128 | IrType::Hash | IrType::Address
                ) || matches!(&field.ty, IrType::Array(inner, len) if **inner == IrType::U8 && *len > 0);
                if nested.is_none() && (!admitted || field.fixed_size.is_none()) {
                    return Err(invalid());
                }
            }
            let key = fields.get(&role.identity_field).ok_or_else(invalid)?;
            roles.push(ExactRole {
                type_hash: role.type_hash,
                lock_hash: role.lock_hash,
                schema: *self.exact_selection_schema_hashes.get(&role.type_name).ok_or_else(invalid)?,
                width: match &nested {
                    Some(layout) => layout.capacity,
                    None => *self.type_fixed_sizes.get(&role.type_name).ok_or_else(invalid)?,
                },
                key_offset: if nested.is_some() { 0 } else { key.offset },
                key_width: key.fixed_size.ok_or_else(invalid)?,
                inputs: (role.inputs.0 as usize, role.inputs.1 as usize),
                outputs: (role.outputs.0 as usize, role.outputs.1 as usize),
                local_type: role.local_type,
                correspondence: match &role.correspondence {
                    crate::cell_selection::Correspondence::Preserve => Correspondence::Preserve,
                    crate::cell_selection::Correspondence::Create => Correspondence::Create,
                    crate::cell_selection::Correspondence::Burn => Correspondence::Burn,
                    crate::cell_selection::Correspondence::Merge(key) => Correspondence::Merge(key.clone()),
                },
                nested: nested.map(|layout| NestedRole { layout, key_field: key.index }),
            });
        }
        let contract = ExactSelection {
            roles,
            input_scan: request.input_scan as usize,
            output_scan: request.output_scan as usize,
            selected_per_side: request.selected_per_side as usize,
            aggregate_bytes: request.aggregate_bytes as usize,
        };
        contract.validate()?;
        if contract.roles.iter().any(|role| role.nested.is_some()) != (request.profile == crate::cell_selection::BOUNDED_PROFILE) {
            return Err(CompileError::without_span("role profile differs from its resolved Cell layouts"));
        }
        Ok(contract)
    }

    pub(super) fn emit_source_exact_role_verification(
        &mut self,
        dest: Option<&IrVar>,
        request: &crate::cell_selection::Contract,
    ) -> Result<()> {
        let contract = self.source_selection_contract(request)?;
        let offset = self.selection_frame(dest)?;
        // Temporarily address the reserved subregion as SP. No call or normal
        // return occurs until the entry's original SP is restored.
        self.emit_large_addi("sp", "sp", offset as i64);
        self.emit_exact_role_selection(&contract)?;
        self.emit_large_addi("sp", "sp", -(offset as i64));
        Ok(())
    }

    /// Read immutable records after their complete selection. The source
    /// catalog binds the literal role; the ordinal is role-local, not global.
    /// Only t0..t6 and a0 are scratch; no table contents or SP are changed.
    pub(super) fn emit_selected_role_read(
        &mut self,
        dest: &IrVar,
        selection: &IrOperand,
        role: u32,
        read: crate::cell_selection::Read,
        index: Option<&IrOperand>,
    ) -> Result<()> {
        if role >= 8 || read.indexed() != index.is_some() {
            return Err(CompileError::without_span("invalid selected-role read"));
        }
        // Materialize potentially large local addresses before retaining the
        // table pointer. Operand helpers may use t6, but not t4 or a0.
        if let Some(index) = index {
            self.emit_operand_to_register("a0", index);
        }
        self.emit_operand_to_register("t4", selection);
        let count = ROLE_COUNTS + role as usize * 16 + if read.input() { 0 } else { 8 };
        self.emit(format!("ld t0, {count}(t4)"));
        let bounds = self.fresh_label("selected_read_bounds");
        let done = self.fresh_label("selected_read_done");
        if read.indexed() {
            let scan = self.fresh_label("selected_read_scan");
            let next = self.fresh_label("selected_read_next");
            let found = self.fresh_label("selected_read_found");
            self.emit(format!("bgeu a0, t0, {bounds}"));
            self.emit(format!("ld t5, {}(t4)", if read.input() { SELECTED_INPUTS } else { SELECTED_OUTPUTS }));
            self.emit("li t0, 0");
            self.emit_label(&scan);
            self.emit(format!("bgeu t0, t5, {bounds}"));
            self.emit(format!("li t1, {}", if read.input() { INPUT_TABLE } else { OUTPUT_TABLE }));
            self.emit("add t3, t4, t1");
            self.emit("slli t2, t0, 6");
            self.emit("add t3, t3, t2");
            self.emit("ld t1, 16(t3)");
            self.emit(format!("li t2, {role}"));
            self.emit(format!("bne t1, t2, {next}"));
            self.emit(format!("beqz a0, {found}"));
            self.emit("addi a0, a0, -1");
            self.emit_label(&next);
            self.emit("addi t0, t0, 1");
            self.emit(format!("j {scan}"));
            self.emit_label(&found);
            let partner = read == crate::cell_selection::Read::Successor;
            self.emit(format!("ld t0, {}(t3)", if partner { 56 } else { 0 }));
            if partner {
                let present = self.fresh_label("selected_partner_present");
                self.emit("li t1, -1");
                self.emit(format!("bne t0, t1, {present}"));
                self.emit_process_failure(CellScriptRuntimeError::FlowTransitionMismatch);
                self.emit_label(&present);
            }
            self.emit(format!(
                "li t1, {}",
                if read == crate::cell_selection::Read::Input { CKB_SOURCE_VIEW_INPUT } else { CKB_SOURCE_VIEW_OUTPUT }
            ));
            self.emit("slli t1, t1, 32");
            self.emit("or t0, t0, t1");
        }
        self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
        self.emit(format!("j {done}"));
        self.emit_label(&bounds);
        self.emit_process_failure(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&done);
        Ok(())
    }

    /// Caller reserves contract.frame_bytes() at the current SP. No calls,
    /// heap allocation or persistent pointers are introduced. All registers
    /// except SP/RA/callee-saved registers are scratch. Records contain global
    /// index, optional group index (u64::MAX if absent), nominal role ordinal,
    /// key bytes, and partner global index at +56 (u64::MAX if absent).
    /// Source/side and schema are fixed by the table and admitted role catalog.
    pub(super) fn emit_exact_role_selection(&mut self, contract: &ExactSelection) -> Result<()> {
        contract.validate()?;
        let invalid = self.fresh_label("selection_syscall_invalid");
        let bounds = self.fresh_label("selection_bounds");
        let authority = self.fresh_label("selection_authority");
        let lock = self.fresh_label("selection_lock");
        let layout = self.fresh_label("selection_layout");
        let pairing = self.fresh_label("selection_pairing");
        let nested_invalid = self.fresh_label("selection_nested_invalid");
        let nested_bounds = self.fresh_label("selection_nested_bounds");
        let done = self.fresh_label("selection_done");
        self.emit_selection_current_role(&invalid, &authority);
        for role in &contract.roles {
            self.emit(format!("# exact-role schema commitment: {}", hex::encode(role.schema)));
            if role.local_type {
                self.emit_stack_load("t0", CURRENT_ROLE);
                self.emit("li t1, 2");
                self.emit(format!("bne t0, t1, {authority}"));
                self.emit_selection_hash_equal(CURRENT_HASH, &role.type_hash, &authority);
            }
        }
        // Preflight both complete sources before inspecting protected data;
        // a late overflow must not be masked by an earlier Cell's bad layout.
        for (source, limit, count) in
            [(CKB_SOURCE_INPUT, contract.input_scan, INPUTS), (CKB_SOURCE_OUTPUT, contract.output_scan, OUTPUTS)]
        {
            self.emit(format!("li t0, {source}"));
            self.emit_stack_store("t0", SOURCE);
            self.emit_cell_source_count_scan(
                cell_scan::CellScanFrame { source: SOURCE, index: INDEX, length: LENGTH, buffer: SCRATCH },
                Some(limit),
                &invalid,
                &bounds,
            );
            self.emit_stack_store("a0", count);
        }
        for offset in [SELECTED_INPUTS, SELECTED_OUTPUTS, BYTES] {
            self.emit_stack_store("zero", offset);
        }
        for offset in (ROLE_COUNTS..ROLE_COUNTS + 16 * contract.roles.len()).step_by(8) {
            self.emit_stack_store("zero", offset);
        }
        for (side, source, count, selected, table) in [
            (0, CKB_SOURCE_INPUT, INPUTS, SELECTED_INPUTS, INPUT_TABLE),
            (1, CKB_SOURCE_OUTPUT, OUTPUTS, SELECTED_OUTPUTS, OUTPUT_TABLE),
        ] {
            let scan = self.fresh_label("selection_scan");
            let next = self.fresh_label("selection_next");
            let end = self.fresh_label("selection_end");
            let no_type = self.fresh_label("selection_no_type");
            let type_ready = self.fresh_label("selection_type_ready");
            self.emit_stack_store("zero", INDEX);
            self.emit_stack_store("zero", GROUP_COUNT);
            self.emit_label(&scan);
            self.emit_stack_load("t0", INDEX);
            self.emit_stack_load("t1", count);
            self.emit(format!("bgeu t0, t1, {end}"));
            self.emit_selection_field(source, CKB_CELL_FIELD_TYPE_HASH, TYPE_HASH, 32);
            self.emit(format!("li t0, {CKB_ITEM_MISSING}"));
            self.emit(format!("beq a0, t0, {no_type}"));
            self.emit_selection_exact_load(32, &invalid);
            self.emit("li t0, 1");
            self.emit_stack_store("t0", HAS_TYPE);
            self.emit(format!("j {type_ready}"));
            self.emit_label(&no_type);
            self.emit_stack_store("zero", HAS_TYPE);
            self.emit_label(&type_ready);
            self.emit_selection_field(source, CKB_CELL_FIELD_LOCK_HASH, LOCK_HASH, 32);
            self.emit_selection_exact_load(32, &invalid);
            self.emit_selection_group_index(side == 0);
            self.emit_stack_load("t0", HAS_TYPE);
            self.emit(format!("beqz t0, {next}"));
            for (ordinal, role) in contract.roles.iter().enumerate() {
                let unmatched = self.fresh_label("selection_role_unmatched");
                self.emit_selection_hash_equal(TYPE_HASH, &role.type_hash, &unmatched);
                self.emit_selection_hash_equal(LOCK_HASH, &role.lock_hash, &lock);
                // This primitive owns a separate frame. Do not consult the
                // enclosing action's Cell-buffer map for this scratch offset.
                let data = if role.nested.is_some() { SELECTION_FRAME_BYTES } else { DATA };
                self.emit_store_data_args_at(role.width, LENGTH, data);
                self.emit_stack_load("a3", INDEX);
                self.emit(format!("li a4, {source}"));
                self.emit(format!("li a7, {}", self.runtime_abi().load_cell_data));
                self.emit("ecall");
                self.emit(format!("bnez a0, {invalid}"));
                if let Some(nested) = &role.nested {
                    let codec = self.fresh_label("selection_payload_codec");
                    self.emit_label(&codec);
                    self.emit_nested_table_validation(LENGTH, data, &nested.layout, &codec, Some((&nested_invalid, &nested_bounds)));
                    // Full table validation makes this offset and its fixed
                    // key width safe, including keys after dynamic fields.
                    self.emit_sp_addi("t4", data);
                    self.emit_u32_le_from_base_to("t5", "t4", 4 * (nested.key_field + 1), "t2");
                    self.emit("add t4, t4, t5");
                    for byte in 0..role.key_width {
                        self.emit(format!("lbu t0, {byte}(t4)"));
                        self.emit_stack_store_byte("t0", DATA + byte);
                    }
                } else {
                    self.emit_stack_load("t0", LENGTH);
                    self.emit(format!("li t1, {}", role.width));
                    self.emit(format!("bne t0, t1, {layout}"));
                }
                self.emit_stack_load("t0", selected);
                self.emit(format!("li t1, {}", contract.selected_per_side));
                self.emit(format!("bgeu t0, t1, {bounds}"));
                self.emit_stack_load("t0", BYTES);
                if role.nested.is_some() {
                    self.emit_stack_load("t2", LENGTH);
                    self.emit("add t0, t0, t2");
                } else {
                    self.emit_large_addi("t0", "t0", role.width as i64);
                }
                self.emit(format!("li t1, {}", contract.aggregate_bytes));
                self.emit(format!("bltu t1, t0, {bounds}"));
                self.emit_stack_store("t0", BYTES);
                self.emit_selection_unique_key(role, ordinal, selected, table, &bounds);
                self.emit_stack_load("t0", selected);
                self.emit_selection_record_pointer(table, "t0", "t3");
                self.emit_stack_load("t1", INDEX);
                self.emit("sd t1, 0(t3)");
                self.emit_stack_load("t1", GROUP_INDEX);
                self.emit("sd t1, 8(t3)");
                self.emit(format!("li t1, {ordinal}"));
                self.emit("sd t1, 16(t3)");
                for byte in 0..role.key_width {
                    self.emit_stack_load_byte("t1", DATA + role.key_offset + byte);
                    self.emit(format!("sb t1, {}(t3)", 24 + byte));
                }
                self.emit("li t1, -1");
                self.emit("sd t1, 56(t3)");
                self.emit_stack_load("t0", selected);
                self.emit("addi t0, t0, 1");
                self.emit_stack_store("t0", selected);
                let role_count = ROLE_COUNTS + ordinal * 16 + side * 8;
                self.emit_stack_load("t0", role_count);
                self.emit("addi t0, t0, 1");
                self.emit_stack_store("t0", role_count);
                self.emit(format!("j {next}"));
                self.emit_label(&unmatched);
            }
            self.emit_label(&next);
            self.emit_stack_load("t0", INDEX);
            self.emit("addi t0, t0, 1");
            self.emit_stack_store("t0", INDEX);
            self.emit(format!("j {scan}"));
            self.emit_label(&end);
        }
        for (ordinal, role) in contract.roles.iter().enumerate() {
            for (side, (min, max)) in [role.inputs, role.outputs].into_iter().enumerate() {
                self.emit_stack_load("t0", ROLE_COUNTS + ordinal * 16 + side * 8);
                self.emit(format!("li t1, {min}"));
                self.emit(format!("bltu t0, t1, {bounds}"));
                self.emit(format!("li t1, {max}"));
                self.emit(format!("bltu t1, t0, {bounds}"));
            }
            self.emit_selection_correspondence(role, ordinal, &pairing);
        }
        self.emit(format!("j {done}"));
        for (label, error) in [
            (&invalid, CellScriptRuntimeError::CkbSourceViewInvalid),
            (&bounds, CellScriptRuntimeError::CollectionBoundsInvalid),
            (&authority, CellScriptRuntimeError::ScriptRoleMismatch),
            (&lock, CellScriptRuntimeError::ScriptIdentityMismatch),
            (&layout, CellScriptRuntimeError::ExactSizeMismatch),
            (&pairing, CellScriptRuntimeError::FlowTransitionMismatch),
        ] {
            self.emit_label(label);
            self.emit_process_failure(error);
        }
        if contract.roles.iter().any(|role| role.nested.is_some()) {
            for (label, error) in [
                (&nested_invalid, CellScriptRuntimeError::DynamicFieldBoundsInvalid),
                (&nested_bounds, CellScriptRuntimeError::BoundsCheckFailed),
            ] {
                self.emit_label(label);
                self.emit_process_failure(error);
            }
        }
        self.emit_label(&done);
        Ok(())
    }

    fn emit_selection_field(&mut self, source: u64, field: u64, buffer: usize, width: usize) {
        self.emit_stack_load("t3", INDEX);
        self.emit_load_cell_by_field_syscall_to_offsets_dynamic_index("exact_role_field", source, "t3", field, LENGTH, buffer, width);
    }

    fn emit_selection_exact_load(&mut self, width: usize, invalid: &str) {
        self.emit(format!("bnez a0, {invalid}"));
        self.emit_stack_load("t0", LENGTH);
        self.emit(format!("li t1, {width}"));
        self.emit(format!("bne t0, t1, {invalid}"));
    }

    fn emit_selection_hash_equal(&mut self, buffer: usize, hash: &[u8; 32], unequal: &str) {
        for (word, bytes) in hash.chunks_exact(8).enumerate() {
            self.emit_stack_load("t0", buffer + word * 8);
            self.emit(format!("li t1, {}", i64::from_le_bytes(bytes.try_into().unwrap())));
            self.emit(format!("bne t0, t1, {unequal}"));
        }
    }

    fn emit_selection_memory_hash_equal(&mut self, left: usize, right: usize, unequal: &str) {
        for word in 0..4 {
            self.emit_stack_load("t0", left + word * 8);
            self.emit_stack_load("t1", right + word * 8);
            self.emit(format!("bne t0, t1, {unequal}"));
        }
    }

    fn emit_selection_record_pointer(&mut self, table: usize, index: &str, dest: &str) {
        // t6 is dead; emit_sp_addi may use it for large table offsets.
        self.emit_sp_addi(dest, table);
        self.emit(format!("slli t2, {index}, {}", RECORD_BYTES.trailing_zeros()));
        self.emit(format!("add {dest}, {dest}, t2"));
    }

    fn emit_selection_group_index(&mut self, input: bool) {
        let is_type = self.fresh_label("selection_group_type");
        let matched = self.fresh_label("selection_group_matched");
        let done = self.fresh_label("selection_group_done");
        self.emit("li t0, -1");
        self.emit_stack_store("t0", GROUP_INDEX);
        self.emit_stack_load("t0", CURRENT_ROLE);
        self.emit("li t1, 2");
        self.emit(format!("beq t0, t1, {is_type}"));
        if input {
            self.emit_selection_memory_hash_equal(LOCK_HASH, CURRENT_HASH, &done);
            self.emit(format!("j {matched}"));
        } else {
            self.emit(format!("j {done}"));
        }
        self.emit_label(&is_type);
        self.emit_stack_load("t0", HAS_TYPE);
        self.emit(format!("beqz t0, {done}"));
        self.emit_selection_memory_hash_equal(TYPE_HASH, CURRENT_HASH, &done);
        self.emit_label(&matched);
        self.emit_stack_load("t0", GROUP_COUNT);
        self.emit_stack_store("t0", GROUP_INDEX);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", GROUP_COUNT);
        self.emit_label(&done);
    }

    fn emit_selection_current_role(&mut self, invalid: &str, authority: &str) {
        self.emit("li t0, 32");
        self.emit_stack_store("t0", LENGTH);
        self.emit_sp_addi("a0", CURRENT_HASH);
        self.emit_sp_addi("a1", LENGTH);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", self.runtime_abi().load_script_hash));
        self.emit("ecall");
        self.emit_selection_exact_load(32, invalid);
        self.emit_stack_store("zero", INDEX);
        let output = self.fresh_label("selection_role_output");
        let finish = self.fresh_label("selection_role_ready");
        self.emit_selection_field(CKB_SOURCE_GROUP_INPUT, CKB_CELL_FIELD_CAPACITY, SCRATCH, 8);
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {output}"));
        self.emit_selection_exact_load(8, invalid);
        for (source, label) in [(CKB_SOURCE_GROUP_INPUT, None), (CKB_SOURCE_GROUP_OUTPUT, Some(output))] {
            if let Some(label) = label {
                self.emit_label(&label);
            }
            self.emit_stack_store("zero", CURRENT_ROLE);
            let lock_other = self.fresh_label("selection_current_lock_other");
            let no_type = self.fresh_label("selection_current_no_type");
            self.emit_selection_field(source, CKB_CELL_FIELD_LOCK_HASH, LOCK_HASH, 32);
            self.emit_selection_exact_load(32, invalid);
            self.emit_selection_memory_hash_equal(LOCK_HASH, CURRENT_HASH, &lock_other);
            self.emit("li t0, 1");
            self.emit_stack_store("t0", CURRENT_ROLE);
            self.emit_label(&lock_other);
            self.emit_selection_field(source, CKB_CELL_FIELD_TYPE_HASH, TYPE_HASH, 32);
            self.emit(format!("li t0, {CKB_ITEM_MISSING}"));
            self.emit(format!("beq a0, t0, {no_type}"));
            self.emit_selection_exact_load(32, invalid);
            self.emit_selection_memory_hash_equal(TYPE_HASH, CURRENT_HASH, &no_type);
            self.emit_stack_load("t0", CURRENT_ROLE);
            self.emit("addi t0, t0, 2");
            self.emit_stack_store("t0", CURRENT_ROLE);
            self.emit_label(&no_type);
            self.emit_stack_load("t0", CURRENT_ROLE);
            self.emit(format!("beqz t0, {authority}"));
            self.emit("li t1, 3");
            self.emit(format!("beq t0, t1, {authority}"));
            if source == CKB_SOURCE_GROUP_OUTPUT {
                self.emit("li t1, 2");
                self.emit(format!("bne t0, t1, {authority}"));
            }
            self.emit(format!("j {finish}"));
        }
        self.emit_label(&finish);
    }

    fn emit_selection_unique_key(&mut self, role: &ExactRole, ordinal: usize, selected: usize, table: usize, duplicate: &str) {
        let scan = self.fresh_label("selection_unique_scan");
        let next = self.fresh_label("selection_unique_next");
        let done = self.fresh_label("selection_unique_done");
        self.emit_stack_store("zero", CURSOR);
        self.emit_label(&scan);
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", selected);
        self.emit(format!("bgeu t0, t1, {done}"));
        self.emit_selection_record_pointer(table, "t0", "t3");
        self.emit("ld t0, 16(t3)");
        self.emit(format!("li t1, {ordinal}"));
        self.emit(format!("bne t0, t1, {next}"));
        for byte in 0..role.key_width {
            self.emit(format!("lbu t0, {}(t3)", 24 + byte));
            self.emit_stack_load_byte("t1", DATA + role.key_offset + byte);
            self.emit(format!("bne t0, t1, {next}"));
        }
        self.emit(format!("j {duplicate}"));
        self.emit_label(&next);
        self.emit_stack_load("t0", CURSOR);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", CURSOR);
        self.emit(format!("j {scan}"));
        self.emit_label(&done);
    }

    fn emit_selection_correspondence(&mut self, role: &ExactRole, ordinal: usize, invalid: &str) {
        self.emit_stack_load("t0", ROLE_COUNTS + ordinal * 16);
        self.emit_stack_load("t1", ROLE_COUNTS + ordinal * 16 + 8);
        match &role.correspondence {
            Correspondence::Create => {
                self.emit(format!("bnez t0, {invalid}"));
                self.emit(format!("beqz t1, {invalid}"));
                return;
            }
            Correspondence::Burn => {
                self.emit(format!("beqz t0, {invalid}"));
                self.emit(format!("bnez t1, {invalid}"));
                return;
            }
            Correspondence::Preserve => self.emit(format!("bne t0, t1, {invalid}")),
            Correspondence::Merge(_) => {
                self.emit("li t2, 2");
                self.emit(format!("bltu t0, t2, {invalid}"));
                self.emit("li t2, 1");
                self.emit(format!("bne t1, t2, {invalid}"));
            }
        }
        let scan = self.fresh_label("selection_pair_scan");
        let next = self.fresh_label("selection_pair_next");
        let search = self.fresh_label("selection_pair_search");
        let other = self.fresh_label("selection_pair_other");
        let done = self.fresh_label("selection_pair_done");
        self.emit_stack_store("zero", CURSOR);
        self.emit_label(&scan);
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", SELECTED_INPUTS);
        self.emit(format!("bgeu t0, t1, {done}"));
        self.emit_selection_record_pointer(INPUT_TABLE, "t0", "t3");
        self.emit("ld t0, 16(t3)");
        self.emit(format!("li t1, {ordinal}"));
        self.emit(format!("bne t0, t1, {next}"));
        self.emit_stack_store("zero", PARTNER);
        self.emit_label(&search);
        self.emit_stack_load("t0", PARTNER);
        self.emit_stack_load("t1", SELECTED_OUTPUTS);
        self.emit(format!("bgeu t0, t1, {invalid}"));
        self.emit_selection_record_pointer(OUTPUT_TABLE, "t0", "t4");
        self.emit("ld t0, 16(t4)");
        self.emit(format!("li t1, {ordinal}"));
        self.emit(format!("bne t0, t1, {other}"));
        for byte in 0..role.key_width {
            if let Correspondence::Merge(target) = &role.correspondence {
                self.emit(format!("li t0, {}", target[byte]));
            } else {
                self.emit(format!("lbu t0, {}(t3)", 24 + byte));
            }
            self.emit(format!("lbu t1, {}(t4)", 24 + byte));
            self.emit(format!("bne t0, t1, {other}"));
        }
        self.emit("ld t0, 0(t4)");
        self.emit("sd t0, 56(t3)");
        if matches!(role.correspondence, Correspondence::Preserve) {
            self.emit("ld t0, 0(t3)");
            self.emit("sd t0, 56(t4)");
        }
        self.emit(format!("j {next}"));
        self.emit_label(&other);
        self.emit_stack_load("t0", PARTNER);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", PARTNER);
        self.emit(format!("j {search}"));
        self.emit_label(&next);
        self.emit_stack_load("t0", CURSOR);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", CURSOR);
        self.emit(format!("j {scan}"));
        self.emit_label(&done);
    }
}

#[cfg(test)]
mod tests;
