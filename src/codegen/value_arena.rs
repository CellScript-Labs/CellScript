//! Experimental allocation primitive for #47. One invocation owns a bounded
//! arena in its reserved frame. This is distinct from one fixed buffer per Vec.
//! Experimental local source operations use these routines; independent
//! allocation-machine and ownership proofs remain mandatory admission blockers.
use super::*;

// Cursor, allocation count, and two private resize scratch words. Allocation
// records are [rounded bytes, requested bytes, live flag, allocation ordinal].
const STATE: usize = 32;
const RECORD: usize = 32;
const ALIGNMENT: usize = 16;

mod vector;

#[derive(Clone, Copy)]
pub(super) struct Arena {
    offset: usize,
    bytes: usize,
    allocations: usize,
    // A read-only helper receives the caller's arena address in a spilled
    // parameter. Offsets are then relative to that address, not this frame.
    pointer_slot: Option<usize>,
}

impl Arena {
    // Internal frame layout shared by source operations and primitive VM tests.
    pub(super) fn new(offset: usize, bytes: usize, allocations: usize) -> Result<Self> {
        if !offset.is_multiple_of(ALIGNMENT)
            || !bytes.is_multiple_of(ALIGNMENT)
            || !(64..=65536).contains(&bytes)
            || !(1..=256).contains(&allocations)
            || offset.checked_add(STATE + bytes).is_none_or(|end| end > 1024 * 1024)
        {
            return Err(CompileError::without_span(
                "value arena requires an aligned frame region, 64–65536 bytes and 1–256 allocation events within a 1 MiB frame",
            ));
        }
        Ok(Self { offset, bytes, allocations, pointer_slot: None })
    }

    fn borrowed(pointer_slot: usize, bytes: usize, allocations: usize) -> Result<Self> {
        Ok(Self { pointer_slot: Some(pointer_slot), ..Self::new(0, bytes, allocations)? })
    }
}

impl CodeGenerator {
    /// Initialize once at arena scope entry. Reinitializing a live arena is not
    /// an admitted operation. The caller must reserve its entire disjoint frame.
    pub(super) fn emit_value_arena_init(&mut self, arena: Arena) {
        self.emit(format!(
            "# cellscript internal value arena: offset={} bytes={} events={}",
            arena.offset, arena.bytes, arena.allocations
        ));
        for field in [0, 8, 16, 24] {
            self.emit_stack_store("zero", arena.offset + field);
        }
    }

    /// a0=requested bytes -> a0=zeroed allocation or null for zero bytes.
    /// Clobbers t0..t6; preserves a1..a6. Failure terminates the invocation.
    /// All budget/overflow checks precede mutation. Retired spans remain charged
    /// until scope exit, so repeated growth has deterministic exhaustion.
    pub(super) fn emit_value_arena_allocate(&mut self, arena: Arena) {
        let marker = self.allocated_write_guard.is_some().then(|| self.fresh_label(&format!("allocated_allocation_{}", arena.offset)));
        if let Some(marker) = &marker {
            self.emit_label(&format!("{marker}_begin"));
        }
        let done = self.fresh_label("arena_allocate_done");
        let zero = self.fresh_label("arena_allocate_zero");
        let bad = self.fresh_label("arena_allocate_invalid");
        self.emit(format!("beqz a0, {done}"));
        self.emit(format!("li t2, {}", arena.bytes - RECORD));
        self.emit(format!("bltu t2, a0, {bad}"));
        // a0 <= 65504, so this alignment addition cannot overflow.
        self.emit("addi t2, a0, 15");
        self.emit("andi t2, t2, -16");
        self.emit_sp_addi("t4", arena.offset);
        self.emit("ld t0, 0(t4)");
        self.emit("ld t1, 8(t4)");
        self.emit("andi t3, t0, 15");
        self.emit(format!("bnez t3, {bad}"));
        self.emit(format!("li t3, {}", arena.bytes));
        self.emit(format!("bltu t3, t0, {bad}"));
        self.emit("sub t3, t3, t0");
        self.emit(format!("addi t5, t2, {RECORD}"));
        self.emit(format!("bltu t3, t5, {bad}"));
        self.emit(format!("li t3, {}", arena.allocations));
        self.emit(format!("bgeu t1, t3, {bad}"));
        self.emit(format!("addi t6, t4, {STATE}"));
        self.emit("add t6, t6, t0");
        self.emit("add t0, t0, t5");
        self.emit("addi t1, t1, 1");
        self.emit("sd t0, 0(t4)");
        self.emit("sd t1, 8(t4)");
        self.emit("sd t2, 0(t6)");
        self.emit("sd a0, 8(t6)");
        self.emit("li t3, 1");
        self.emit("sd t3, 16(t6)");
        self.emit("sd t1, 24(t6)");
        self.emit(format!("addi a0, t6, {RECORD}"));
        self.emit("mv t0, a0");
        self.emit_label(&zero);
        self.emit("sb zero, 0(t0)");
        self.emit("addi t0, t0, 1");
        self.emit("addi t2, t2, -1");
        self.emit(format!("bnez t2, {zero}"));
        self.emit(format!("j {done}"));
        self.emit_label(&bad);
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&done);
        if let Some(marker) = marker {
            self.emit_label(&format!("{marker}_end"));
        }
    }

    /// a0 must be a live non-null allocation from this arena. Walk real record
    /// boundaries instead of accepting an aligned interior pointer. Returns
    /// t5=record and t1=requested bytes; a0/a1 remain live. No memory is written.
    fn emit_value_arena_find(&mut self, arena: Arena) {
        let scan = self.fresh_label("arena_find_scan");
        let found = self.fresh_label("arena_find_found");
        let done = self.fresh_label("arena_find_done");
        let bad = self.fresh_label("arena_find_invalid");
        if let Some(slot) = arena.pointer_slot {
            self.emit_stack_load("t4", slot);
        } else {
            self.emit_sp_addi("t4", arena.offset);
        }
        self.emit("ld t0, 0(t4)");
        self.emit(format!("li t2, {}", arena.bytes));
        self.emit(format!("bltu t2, t0, {bad}"));
        self.emit("andi t2, t0, 15");
        self.emit(format!("bnez t2, {bad}"));
        self.emit("ld t3, 8(t4)");
        self.emit(format!("li t2, {}", arena.allocations));
        self.emit(format!("bltu t2, t3, {bad}"));
        self.emit(format!("addi t5, t4, {STATE}"));
        self.emit("add t6, t5, t0");
        self.emit("li t4, 1");
        self.emit_label(&scan);
        self.emit(format!("bltu t3, t4, {bad}"));
        self.emit(format!("bgeu t5, t6, {bad}"));
        self.emit("sub t2, t6, t5");
        self.emit(format!("li t0, {RECORD}"));
        self.emit(format!("bltu t2, t0, {bad}"));
        self.emit(format!("addi t2, t2, -{RECORD}"));
        self.emit("ld t0, 0(t5)");
        self.emit(format!("beqz t0, {bad}"));
        self.emit(format!("bltu t2, t0, {bad}"));
        self.emit("andi t2, t0, 15");
        self.emit(format!("bnez t2, {bad}"));
        self.emit("ld t1, 8(t5)");
        self.emit(format!("beqz t1, {bad}"));
        self.emit(format!("bltu t0, t1, {bad}"));
        self.emit("addi t2, t1, 15");
        self.emit("andi t2, t2, -16");
        self.emit(format!("bne t2, t0, {bad}"));
        self.emit("ld t2, 24(t5)");
        self.emit(format!("bne t2, t4, {bad}"));
        self.emit("ld t2, 16(t5)");
        self.emit("srli t2, t2, 1");
        self.emit(format!("bnez t2, {bad}"));
        self.emit(format!("addi t2, t5, {RECORD}"));
        self.emit(format!("beq t2, a0, {found}"));
        self.emit("add t5, t2, t0");
        self.emit("addi t4, t4, 1");
        self.emit(format!("j {scan}"));
        self.emit_label(&found);
        self.emit("ld t2, 16(t5)");
        self.emit("li t0, 1");
        self.emit(format!("bne t2, t0, {bad}"));
        self.emit(format!("j {done}"));
        self.emit_label(&bad);
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&done);
    }

    /// a0=old pointer, a1=new bytes -> a0=new allocation. Preserve min(old,new)
    /// bytes, zero additional capacity, then retire the old record. A zero size
    /// retires without allocating; a null old pointer behaves as allocate.
    /// Clobbers t0..t6; preserves a1..a6. The arena's two resize slots are private.
    pub(super) fn emit_value_arena_resize(&mut self, arena: Arena) {
        let marker = self.allocated_write_guard.is_some().then(|| self.fresh_label(&format!("allocated_resize_{}", arena.offset)));
        if let Some(marker) = &marker {
            self.emit_label(&format!("{marker}_begin"));
        }
        let fresh = self.fresh_label("arena_resize_fresh");
        let release = self.fresh_label("arena_resize_release");
        let copy = self.fresh_label("arena_resize_copy");
        let bounded = self.fresh_label("arena_resize_bounded");
        let done = self.fresh_label("arena_resize_done");
        self.emit(format!("beqz a0, {fresh}"));
        self.emit_value_arena_find(arena);
        self.emit(format!("beqz a1, {release}"));
        self.emit_stack_store("a0", arena.offset + 16);
        self.emit_stack_store("t1", arena.offset + 24);
        self.emit("mv a0, a1");
        self.emit_value_arena_allocate(arena);
        self.emit_stack_load("t0", arena.offset + 16);
        self.emit_stack_load("t1", arena.offset + 24);
        self.emit(format!("bltu t1, a1, {bounded}"));
        self.emit("mv t1, a1");
        self.emit_label(&bounded);
        self.emit("mv t2, a0");
        self.emit_label(&copy);
        self.emit("lbu t3, 0(t0)");
        self.emit("sb t3, 0(t2)");
        self.emit("addi t0, t0, 1");
        self.emit("addi t2, t2, 1");
        self.emit("addi t1, t1, -1");
        self.emit(format!("bnez t1, {copy}"));
        self.emit_stack_load("t0", arena.offset + 16);
        self.emit("sd zero, -16(t0)");
        self.emit(format!("j {done}"));
        self.emit_label(&release);
        self.emit("sd zero, 16(t5)");
        self.emit("li a0, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&fresh);
        self.emit("mv a0, a1");
        self.emit_value_arena_allocate(arena);
        self.emit_label(&done);
        if let Some(marker) = marker {
            self.emit_label(&format!("{marker}_end"));
        }
    }

    /// Retire a live allocation. Null is a no-op; stale/double release fails.
    /// No bytes are reclaimed until the owner arena's lifetime ends.
    pub(super) fn emit_value_arena_release(&mut self, arena: Arena) {
        let marker = self.allocated_write_guard.is_some().then(|| self.fresh_label(&format!("allocated_release_{}", arena.offset)));
        if let Some(marker) = &marker {
            self.emit_label(&format!("{marker}_begin"));
        }
        let done = self.fresh_label("arena_release_done");
        self.emit(format!("beqz a0, {done}"));
        self.emit_value_arena_find(arena);
        self.emit("sd zero, 16(t5)");
        self.emit_label(&done);
        if let Some(marker) = marker {
            self.emit_label(&format!("{marker}_end"));
        }
    }
}

#[cfg(test)]
mod tests;

mod write_guard;
pub(super) use write_guard::WriteGuard;
