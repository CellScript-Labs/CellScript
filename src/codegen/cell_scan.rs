//! Complete capacity-field scans shared by source cardinality and bounded
//! selection lowering. A missing Type field must never terminate this scan.

use super::*;

/// Caller-owned, disjoint eight-byte slots; `source` contains a validated CKB
/// Cell source. The caller owns the frame and terminal failure labels.
pub(super) struct CellScanFrame {
    pub source: usize,
    pub index: usize,
    pub length: usize,
    pub buffer: usize,
}

impl CodeGenerator {
    /// Start at global/source index zero and return the exact count in a0.
    /// Only INDEX_OUT_OF_BOUND terminates successfully. With a bound, the
    /// capacity probe at that bound is mandatory: an existing Cell rejects.
    /// Clobbers caller-saved registers; all live scan state is in `frame`.
    /// The optional bound does not change the existing unbounded cell_count API.
    pub(super) fn emit_cell_source_count_scan(&mut self, frame: CellScanFrame, bound: Option<usize>, invalid: &str, over_bound: &str) {
        if let Some(bound) = bound {
            assert!((1..=64).contains(&bound), "exact-role scan bounds must be validated before codegen");
        }
        let scan = self.fresh_label("cell_source_scan");
        let end = self.fresh_label("cell_source_scan_end");
        self.emit_stack_store("zero", frame.index);
        self.emit_label(&scan);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", frame.length);
        self.emit_sp_addi("a0", frame.buffer);
        self.emit_sp_addi("a1", frame.length);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", frame.index);
        self.emit_stack_load("a4", frame.source);
        self.emit(format!("li a5, {CKB_CELL_FIELD_CAPACITY}"));
        self.emit(format!("li a7, {}", self.runtime_abi().load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {end}"));
        self.emit(format!("bnez a0, {invalid}"));
        self.emit_stack_load("t0", frame.length);
        self.emit("li t1, 8");
        self.emit(format!("bne t0, t1, {invalid}"));
        self.emit_stack_load("t0", frame.index);
        if let Some(bound) = bound {
            self.emit(format!("li t1, {bound}"));
            self.emit(format!("bgeu t0, t1, {over_bound}"));
        }
        self.emit("addi t0, t0, 1");
        self.emit(format!("beqz t0, {invalid}"));
        self.emit_stack_store("t0", frame.index);
        self.emit(format!("j {scan}"));
        self.emit_label(&end);
        self.emit_stack_load("a0", frame.index);
    }
}

#[cfg(test)]
mod tests;
