//! Allocation-backed vector operations for experimental local source execution.
//! Independent machine and ownership admission remain pending.
use super::*;

// Pointer, initialized length, capacity, live owner flag.
const DESCRIPTOR: usize = 32;

mod ordered;

#[derive(Clone, Copy)]
pub(super) struct Vector {
    arena: Arena,
    offset: usize,
    width: usize,
    elements: usize,
}

impl Vector {
    pub(super) fn new(arena: Arena, offset: usize, width: usize, elements: usize) -> Result<Self> {
        let end = offset.checked_add(DESCRIPTOR);
        if !offset.is_multiple_of(16)
            || end.is_none_or(|end| end > 1024 * 1024 || (offset < arena.offset + STATE + arena.bytes && end > arena.offset))
            || !(1..=64).contains(&width)
            || !(1..=1024).contains(&elements)
            || width.checked_mul(elements).is_none_or(|bytes| bytes > arena.bytes - RECORD)
        {
            return Err(CompileError::without_span(
                "allocated vector requires a disjoint aligned descriptor, 1–64 byte fixed elements and 1–1024 elements fitting its arena",
            ));
        }
        Ok(Self { arena, offset, width, elements })
    }

    fn buffer(self, offset: usize) -> Result<()> {
        let end = offset.checked_add(self.width);
        if end.is_none_or(|end| {
            end > 1024 * 1024
                || (self.arena.pointer_slot.is_none()
                    && ((offset < self.offset + DESCRIPTOR && end > self.offset)
                        || (offset < self.arena.offset + STATE + self.arena.bytes && end > self.arena.offset)))
        }) {
            return Err(CompileError::without_span("allocated vector copy buffer must be disjoint from its descriptor and arena"));
        }
        Ok(())
    }
}

impl CodeGenerator {
    fn begin_value_element_marker(&mut self, vector: Vector, operation: &str) -> Option<String> {
        let marker = self
            .allocated_write_guard
            .as_ref()
            .map(|guard| {
                if let Some(pointer) = vector.arena.pointer_slot {
                    format!("allocated_borrowed_{operation}_{pointer}_{}_{}", vector.offset, guard.start)
                } else {
                    format!("allocated_{operation}_{}_{}", vector.arena.offset, vector.offset)
                }
            })
            .map(|prefix| self.fresh_label(&prefix));
        if let Some(marker) = &marker {
            self.emit_label(&format!("{marker}_begin"));
        }
        marker
    }

    fn end_value_element_marker(&mut self, marker: Option<String>) {
        if let Some(marker) = marker {
            self.emit_label(&format!("{marker}_end"));
        }
    }

    /// Load a descriptor word without changing other live descriptor registers.
    /// t6 is scratch for large offsets; a1..a6 are preserved.
    fn emit_value_vector_load(&mut self, dest: &str, vector: Vector, field: usize) {
        if let Some(slot) = vector.arena.pointer_slot {
            self.emit_stack_load(dest, slot);
            self.emit_large_addi(dest, dest, (vector.offset + field) as i64);
            self.emit(format!("ld {dest}, 0({dest})"));
        } else {
            self.emit_stack_load(dest, vector.offset + field);
        }
    }

    /// Validate the owner and backing allocation; return t0=pointer, t1=length,
    /// t2=capacity. Preserve a1..a6. This is not source lifetime/alias analysis.
    fn emit_value_vector_check(&mut self, vector: Vector) {
        let bad = self.fresh_label("value_vector_invalid");
        let zero = self.fresh_label("value_vector_empty");
        let done = self.fresh_label("value_vector_checked");
        self.emit_value_vector_load("t0", vector, 24);
        self.emit("li t1, 1");
        self.emit(format!("bne t0, t1, {bad}"));
        self.emit_value_vector_load("t1", vector, 8);
        self.emit_value_vector_load("t2", vector, 16);
        self.emit(format!("li t3, {}", vector.elements));
        self.emit(format!("bltu t2, t1, {bad}"));
        self.emit(format!("bltu t3, t2, {bad}"));
        self.emit_value_vector_load("a0", vector, 0);
        self.emit(format!("beqz t2, {zero}"));
        self.emit_value_arena_find(vector.arena);
        self.emit_value_vector_load("t2", vector, 16);
        self.emit(format!("li t3, {}", vector.width));
        self.emit("mul t3, t2, t3");
        self.emit(format!("bne t1, t3, {bad}"));
        self.emit_value_vector_load("t1", vector, 8);
        self.emit("mv t0, a0");
        self.emit(format!("j {done}"));
        self.emit_label(&zero);
        self.emit(format!("bnez a0, {bad}"));
        self.emit("li t0, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&bad);
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&done);
    }

    /// Initialize an empty live owner exactly once; source must enforce this.
    pub(super) fn emit_value_vector_init(&mut self, vector: Vector) {
        for field in [0, 8, 16] {
            self.emit_stack_store("zero", vector.offset + field);
        }
        self.emit("li t0, 1");
        self.emit_stack_store("t0", vector.offset + 24);
    }

    /// a1=minimum capacity. Growth is max(minimum, twice current), clipped to
    /// the declared element limit. Retired backing remains charged to the arena.
    /// Clobbers a0..a2/t0..t6; preserves a3..a6. No descriptor change on failure.
    pub(super) fn emit_value_vector_reserve(&mut self, vector: Vector) {
        let marker = self
            .allocated_write_guard
            .is_some()
            .then(|| self.fresh_label(&format!("allocated_reserve_{}_{}", vector.arena.offset, vector.offset)));
        if let Some(marker) = &marker {
            self.emit_label(&format!("{marker}_begin"));
        }
        self.emit_value_vector_check(vector);
        let bad = self.fresh_label("value_vector_capacity_invalid");
        let done = self.fresh_label("value_vector_capacity_done");
        let enough = self.fresh_label("value_vector_growth_enough");
        let bounded = self.fresh_label("value_vector_growth_bounded");
        self.emit(format!("li t3, {}", vector.elements));
        self.emit(format!("bltu t3, a1, {bad}"));
        self.emit(format!("bgeu t2, a1, {done}"));
        self.emit("slli a2, t2, 1");
        self.emit(format!("bgeu a2, a1, {enough}"));
        self.emit("mv a2, a1");
        self.emit_label(&enough);
        self.emit(format!("bgeu t3, a2, {bounded}"));
        self.emit("mv a2, t3");
        self.emit_label(&bounded);
        self.emit("mv a0, t0");
        self.emit(format!("li a1, {}", vector.width));
        self.emit("mul a1, a2, a1");
        self.emit_value_arena_resize(vector.arena);
        self.emit_stack_store("a0", vector.offset);
        self.emit_stack_store("a2", vector.offset + 16);
        self.emit(format!("j {done}"));
        self.emit_label(&bad);
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&done);
        if let Some(marker) = &marker {
            self.emit_label(&format!("{marker}_end"));
        }
    }

    /// Copy one fixed element from a disjoint caller frame buffer. No borrowed
    /// pointer may cross growth; the typed caller must materialize first.
    pub(super) fn emit_value_vector_push(&mut self, vector: Vector, source: usize) -> Result<()> {
        vector.buffer(source)?;
        let marker = self.begin_value_element_marker(vector, "push");
        self.emit_value_vector_check(vector);
        self.emit("addi a1, t1, 1");
        self.emit_value_vector_reserve(vector);
        self.emit_stack_load("t0", vector.offset);
        self.emit_stack_load("t1", vector.offset + 8);
        self.emit(format!("li t2, {}", vector.width));
        self.emit("mul t2, t1, t2");
        self.emit("add t0, t0, t2");
        self.emit_sp_addi("t3", source);
        self.emit(format!("li t2, {}", vector.width));
        self.emit_value_vector_copy();
        self.emit("addi t1, t1, 1");
        self.emit_stack_store("t1", vector.offset + 8);
        self.end_value_element_marker(marker);
        Ok(())
    }

    /// a1=index, write a value copy to a disjoint output buffer. This does not
    /// expose a backing pointer or permit reference serialization.
    pub(super) fn emit_value_vector_get(&mut self, vector: Vector, output: usize) -> Result<()> {
        vector.buffer(output)?;
        let marker = self.begin_value_element_marker(vector, "read");
        self.emit_value_vector_index(vector);
        self.emit("mv t3, t0");
        self.emit_sp_addi("t0", output);
        self.emit(format!("li t2, {}", vector.width));
        self.emit_value_vector_copy();
        self.end_value_element_marker(marker);
        Ok(())
    }

    /// a1=index; replace one existing element without allocation or reordering.
    pub(super) fn emit_value_vector_set(&mut self, vector: Vector, source: usize) -> Result<()> {
        vector.buffer(source)?;
        let marker = self.begin_value_element_marker(vector, "set");
        self.emit_value_vector_index(vector);
        self.emit_sp_addi("t3", source);
        self.emit(format!("li t2, {}", vector.width));
        self.emit_value_vector_copy();
        self.end_value_element_marker(marker);
        Ok(())
    }

    /// a1=index in 0..=length; insert with stable order. Save the index in a4,
    /// which the arena and reserve primitives preserve across possible growth.
    pub(super) fn emit_value_vector_insert(&mut self, vector: Vector, source: usize) -> Result<()> {
        vector.buffer(source)?;
        let marker = self.begin_value_element_marker(vector, "insert");
        self.emit_value_vector_check(vector);
        let valid = self.fresh_label("value_vector_insert_valid");
        self.emit(format!("bgeu t1, a1, {valid}"));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&valid);
        self.emit("mv a4, a1");
        self.emit("addi a1, t1, 1");
        self.emit_value_vector_reserve(vector);
        self.emit_stack_load("t0", vector.offset);
        self.emit_stack_load("t1", vector.offset + 8);
        self.emit(format!("li t2, {}", vector.width));
        self.emit("mul t3, t1, t2");
        self.emit("add t3, t0, t3");
        self.emit("add t0, t3, t2");
        self.emit("sub t2, t1, a4");
        self.emit(format!("li t4, {}", vector.width));
        self.emit("mul t2, t2, t4");
        let place = self.fresh_label("value_vector_insert_place");
        let shift = self.fresh_label("value_vector_insert_shift");
        self.emit(format!("beqz t2, {place}"));
        self.emit_label(&shift);
        self.emit("addi t0, t0, -1");
        self.emit("addi t3, t3, -1");
        self.emit("lbu t4, 0(t3)");
        self.emit("sb t4, 0(t0)");
        self.emit("addi t2, t2, -1");
        self.emit(format!("bnez t2, {shift}"));
        self.emit_label(&place);
        self.emit("mv t0, t3");
        self.emit_sp_addi("t3", source);
        self.emit(format!("li t2, {}", vector.width));
        self.emit_value_vector_copy();
        self.emit("addi t1, t1, 1");
        self.emit_stack_store("t1", vector.offset + 8);
        self.end_value_element_marker(marker);
        Ok(())
    }

    /// a1=index; stable-order removal, capacity unchanged. Zero the vacated
    /// trailing element, including for width > 8; no uninitialized spare bytes.
    pub(super) fn emit_value_vector_remove(&mut self, vector: Vector) {
        let marker = self.begin_value_element_marker(vector, "remove");
        self.emit_value_vector_index(vector);
        self.emit("addi t1, t1, -1");
        self.emit("sub t2, t1, a1");
        self.emit(format!("li t3, {}", vector.width));
        self.emit("mul t2, t2, t3");
        self.emit("add t3, t0, t3");
        let clear = self.fresh_label("value_vector_remove_clear");
        self.emit(format!("beqz t2, {clear}"));
        self.emit_value_vector_copy();
        self.emit_label(&clear);
        self.emit(format!("li t2, {}", vector.width));
        let zero = self.fresh_label("value_vector_remove_zero");
        self.emit_label(&zero);
        self.emit("sb zero, 0(t0)");
        self.emit("addi t0, t0, 1");
        self.emit("addi t2, t2, -1");
        self.emit(format!("bnez t2, {zero}"));
        self.emit_stack_store("t1", vector.offset + 8);
        self.end_value_element_marker(marker);
    }

    pub(super) fn emit_value_vector_drop(&mut self, vector: Vector) {
        let marker = self
            .allocated_write_guard
            .is_some()
            .then(|| self.fresh_label(&format!("allocated_drop_{}_{}", vector.arena.offset, vector.offset)));
        if let Some(marker) = &marker {
            self.emit_label(&format!("{marker}_begin"));
        }
        self.emit_value_vector_check(vector);
        self.emit("mv a0, t0");
        self.emit_value_arena_release(vector.arena);
        for field in [0, 8, 16, 24] {
            self.emit_stack_store("zero", vector.offset + field);
        }
        if let Some(marker) = &marker {
            self.emit_label(&format!("{marker}_end"));
        }
    }

    /// a1=index -> t0=element pointer, t1=length; preserves a1.
    fn emit_value_vector_index(&mut self, vector: Vector) {
        self.emit_value_vector_check(vector);
        let valid = self.fresh_label("value_vector_index_valid");
        self.emit(format!("bltu a1, t1, {valid}"));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&valid);
        self.emit(format!("li t2, {}", vector.width));
        self.emit("mul t2, a1, t2");
        self.emit("add t0, t0, t2");
    }

    /// t0=destination, t3=source, t2=positive byte count. Preserve t1/a1..a6.
    /// Copies forward; also valid for leftward overlapping stable removal.
    fn emit_value_vector_copy(&mut self) {
        let copy = self.fresh_label("value_vector_copy");
        self.emit_label(&copy);
        self.emit("lbu t4, 0(t3)");
        self.emit("sb t4, 0(t0)");
        self.emit("addi t0, t0, 1");
        self.emit("addi t3, t3, 1");
        self.emit("addi t2, t2, -1");
        self.emit(format!("bnez t2, {copy}"));
    }
}

#[cfg(test)]
mod tests;
