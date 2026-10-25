//! Linear construction from a checked nondecreasing array. Duplicates retain
//! the last map value, matching repeated insert; sets retain one key.
use super::*;

impl CodeGenerator {
    /// Compare keys at a3/a4. Preserve a1..a6; clobber a0/t3..t6. Callers
    /// validate both spans before entry. Numeric keys compare most-significant
    /// bytes first; byte-string keys retain lexicographic wire order.
    fn emit_sorted_key_compare(&mut self, ordered: Ordered, less: &str, equal: &str, greater: &str) {
        self.emit("mv t3, a3");
        self.emit("mv t4, a4");
        let step = match ordered.order {
            KeyOrder::UnsignedLittleEndian => {
                self.emit(format!("addi t3, t3, {}", ordered.key_bytes - 1));
                self.emit(format!("addi t4, t4, {}", ordered.key_bytes - 1));
                -1
            }
            KeyOrder::LexicographicBytes => 1,
        };
        let repeat = self.fresh_label("ordered_sorted_compare");
        self.emit(format!("li a0, {}", ordered.key_bytes));
        self.emit_label(&repeat);
        self.emit("lbu t5, 0(t3)");
        self.emit("lbu t6, 0(t4)");
        self.emit(format!("bltu t5, t6, {less}"));
        self.emit(format!("bltu t6, t5, {greater}"));
        self.emit("addi a0, a0, -1");
        self.emit(format!("beqz a0, {equal}"));
        self.emit(format!("addi t3, t3, {step}"));
        self.emit(format!("addi t4, t4, {step}"));
        self.emit(format!("j {repeat}"));
    }

    /// The caller supplies an exact-width immutable input pointer in a local
    /// slot. Reject overlap with the destination arena/descriptor before any
    /// write. This supports owned local collections only, never borrowed ones.
    pub(super) fn emit_value_ordered_load_sorted(&mut self, ordered: Ordered, source_slot: usize, count: usize) -> Result<()> {
        let vector = ordered.vector;
        if count > 1024 || vector.arena.pointer_slot.is_some() {
            return Err(CompileError::without_span(
                "sorted construction requires an owned destination and at most 1024 input entries",
            ));
        }
        let marker = self.begin_value_element_marker(vector, &format!("sorted_{count}"));
        self.emit_value_vector_check(vector);
        let empty = self.fresh_label("ordered_sorted_empty");
        self.emit(format!("beqz t1, {empty}"));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&empty);
        if count == 0 {
            self.end_value_element_marker(marker);
            return Ok(());
        }
        let bad = self.fresh_label("ordered_sorted_invalid");
        let done = self.fresh_label("ordered_sorted_done");
        self.emit_stack_load("a4", source_slot);
        self.emit(format!("li t0, {}", count * vector.width));
        self.emit("add t1, a4, t0");
        self.emit(format!("bltu t1, a4, {bad}"));
        for (start, end) in
            [(vector.offset, vector.offset + DESCRIPTOR), (vector.arena.offset, vector.arena.offset + STATE + vector.arena.bytes)]
        {
            let disjoint = self.fresh_label("ordered_sorted_disjoint");
            self.emit_sp_addi("t2", start);
            self.emit(format!("bgeu t2, t1, {disjoint}"));
            self.emit_sp_addi("t2", end);
            self.emit(format!("bltu a4, t2, {bad}"));
            self.emit_label(&disjoint);
        }
        self.emit("li a6, 1");
        if count > 1 {
            let scan = self.fresh_label("ordered_sorted_scan");
            let unique = self.fresh_label("ordered_sorted_unique");
            let next = self.fresh_label("ordered_sorted_next");
            self.emit("mv a3, a4");
            self.emit(format!("addi a4, a4, {}", vector.width));
            self.emit(format!("li a5, {}", count - 1));
            self.emit_label(&scan);
            self.emit_sorted_key_compare(ordered, &unique, &next, &bad);
            self.emit_label(&unique);
            self.emit("addi a6, a6, 1");
            self.emit_label(&next);
            self.emit("mv a3, a4");
            self.emit(format!("addi a4, a4, {}", vector.width));
            self.emit("addi a5, a5, -1");
            self.emit(format!("bnez a5, {scan}"));
        }
        self.emit("mv a1, a6");
        self.emit_value_vector_reserve(vector);
        self.emit_stack_store("a6", vector.offset + 8);
        self.emit_stack_load("a3", vector.offset);
        self.emit_stack_load("a4", source_slot);
        self.emit(format!("li a5, {count}"));
        let copy = self.fresh_label("ordered_sorted_copy");
        let advance = self.fresh_label("ordered_sorted_advance");
        self.emit(format!("j {copy}"));
        self.emit_label(&advance);
        // a3 is one past the last output. Equal keys overwrite that entry;
        // unequal keys restore the next free output address before copying.
        self.emit(format!("addi a3, a3, -{}", vector.width));
        let fresh = self.fresh_label("ordered_sorted_fresh");
        self.emit_sorted_key_compare(ordered, &fresh, &copy, &bad);
        self.emit_label(&fresh);
        self.emit(format!("addi a3, a3, {}", vector.width));
        self.emit_label(&copy);
        self.emit("mv t0, a3");
        self.emit("mv t3, a4");
        self.emit(format!("li t2, {}", vector.width));
        self.emit_value_vector_copy();
        self.emit(format!("addi a3, a3, {}", vector.width));
        self.emit(format!("addi a4, a4, {}", vector.width));
        self.emit("addi a5, a5, -1");
        self.emit(format!("bnez a5, {advance}"));
        self.emit(format!("j {done}"));
        self.emit_label(&bad);
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&done);
        self.end_value_element_marker(marker);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
