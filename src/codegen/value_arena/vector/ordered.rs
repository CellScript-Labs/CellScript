//! Deterministic ordered map/set machine primitives over an owned vector.
//! Keys are fixed unsigned integers or lexicographically ordered byte strings.
//! Typed ownership must prevent raw vector mutation from breaking sorted order.
use super::*;
mod sorted;
mod source;

#[derive(Clone, Copy)]
pub(super) enum KeyOrder {
    UnsignedLittleEndian,
    LexicographicBytes,
}

#[derive(Clone, Copy)]
pub(super) struct Ordered {
    vector: Vector,
    key_bytes: usize,
    order: KeyOrder,
}

impl Ordered {
    pub(super) fn new(vector: Vector, key_bytes: usize, order: KeyOrder) -> Result<Self> {
        let valid = match order {
            KeyOrder::UnsignedLittleEndian => matches!(key_bytes, 1 | 2 | 4 | 8),
            KeyOrder::LexicographicBytes => (1..=32).contains(&key_bytes),
        };
        if !valid || key_bytes > vector.width || vector.width - key_bytes > 32 {
            return Err(CompileError::without_span(
                "ordered collection requires an unsigned 1/2/4/8-byte key or 1–32 lexicographic bytes, and 0–32 fixed value bytes",
            ));
        }
        Ok(Self { vector, key_bytes, order })
    }
}

impl CodeGenerator {
    /// Read the key prefix of a disjoint full-width input buffer. Return
    /// a0=found, a1=ordinal (existing entry or insertion point). No allocation.
    /// Clobbers a0..a6/t0..t6. The owned search body is independently checked;
    /// complete source/machine preservation of sorted unique keys is pending.
    pub(super) fn emit_value_ordered_find(&mut self, ordered: Ordered, source: usize) -> Result<()> {
        ordered.vector.buffer(source)?;
        let marker = self.begin_value_element_marker(ordered.vector, "find");
        self.emit_value_vector_check(ordered.vector);
        let search = self.fresh_label("ordered_search");
        let compare = self.fresh_label("ordered_compare");
        let equal_byte = self.fresh_label("ordered_equal_byte");
        let less = self.fresh_label("ordered_less");
        let greater = self.fresh_label("ordered_greater");
        let found = self.fresh_label("ordered_found");
        let absent = self.fresh_label("ordered_absent");
        let done = self.fresh_label("ordered_done");
        self.emit("li a2, 0");
        self.emit("mv a3, t1");
        self.emit("mv a5, t0");
        self.emit_label(&search);
        self.emit(format!("bgeu a2, a3, {absent}"));
        self.emit("add a4, a2, a3");
        self.emit("srli a4, a4, 1");
        self.emit(format!("li t2, {}", ordered.vector.width));
        self.emit("mul t2, a4, t2");
        self.emit("add t3, a5, t2");
        self.emit_sp_addi("t4", source);
        self.emit(format!("li a6, {}", ordered.key_bytes));
        let step = match ordered.order {
            KeyOrder::UnsignedLittleEndian => {
                self.emit(format!("addi t3, t3, {}", ordered.key_bytes - 1));
                self.emit(format!("addi t4, t4, {}", ordered.key_bytes - 1));
                -1
            }
            KeyOrder::LexicographicBytes => 1,
        };
        self.emit_label(&compare);
        self.emit("lbu t5, 0(t3)");
        self.emit("lbu t6, 0(t4)");
        self.emit(format!("beq t5, t6, {equal_byte}"));
        self.emit(format!("bltu t5, t6, {less}"));
        self.emit(format!("j {greater}"));
        self.emit_label(&equal_byte);
        self.emit("addi a6, a6, -1");
        self.emit(format!("beqz a6, {found}"));
        self.emit(format!("addi t3, t3, {step}"));
        self.emit(format!("addi t4, t4, {step}"));
        self.emit(format!("j {compare}"));
        self.emit_label(&less);
        self.emit("addi a2, a4, 1");
        self.emit(format!("j {search}"));
        self.emit_label(&greater);
        self.emit("mv a3, a4");
        self.emit(format!("j {search}"));
        self.emit_label(&found);
        self.emit("li a0, 1");
        self.emit("mv a1, a4");
        self.emit(format!("j {done}"));
        self.emit_label(&absent);
        self.emit("li a0, 0");
        self.emit("mv a1, a2");
        self.emit_label(&done);
        self.end_value_element_marker(marker);
        Ok(())
    }

    /// Insert or replace a key/value entry; a0=1 on insertion, 0 on replacement.
    /// For sets (zero value width), duplicate insertion is an allocation-free
    /// no-op. Replacing an existing map key also works when capacity is full.
    pub(super) fn emit_value_ordered_put(&mut self, ordered: Ordered, source: usize) -> Result<()> {
        let marker = self.begin_value_element_marker(ordered.vector, "put");
        self.emit_value_ordered_find(ordered, source)?;
        let insert = self.fresh_label("ordered_insert");
        let done = self.fresh_label("ordered_put_done");
        self.emit(format!("beqz a0, {insert}"));
        if ordered.key_bytes != ordered.vector.width {
            self.emit_value_vector_set(ordered.vector, source)?;
        }
        self.emit("li a0, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&insert);
        self.emit_value_vector_insert(ordered.vector, source)?;
        self.emit("li a0, 1");
        self.emit_label(&done);
        self.end_value_element_marker(marker);
        Ok(())
    }

    /// Copy a complete entry when present; a0=found. A missing key leaves the
    /// output buffer unchanged and grants no entry view or allocation ownership.
    pub(super) fn emit_value_ordered_get(&mut self, ordered: Ordered, source: usize, output: usize) -> Result<()> {
        ordered.vector.buffer(output)?;
        let marker = self.begin_value_element_marker(ordered.vector, "lookup");
        self.emit_value_ordered_find(ordered, source)?;
        let done = self.fresh_label("ordered_get_done");
        self.emit(format!("beqz a0, {done}"));
        self.emit_value_vector_get(ordered.vector, output)?;
        self.emit("li a0, 1");
        self.emit_label(&done);
        self.end_value_element_marker(marker);
        Ok(())
    }

    /// Stable removal when present; a0=removed. Missing keys are a no-op.
    pub(super) fn emit_value_ordered_remove(&mut self, ordered: Ordered, source: usize) -> Result<()> {
        let marker = self.begin_value_element_marker(ordered.vector, "erase");
        self.emit_value_ordered_find(ordered, source)?;
        let done = self.fresh_label("ordered_remove_done");
        self.emit(format!("beqz a0, {done}"));
        self.emit_value_vector_remove(ordered.vector);
        self.emit("li a0, 1");
        self.emit_label(&done);
        self.end_value_element_marker(marker);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
