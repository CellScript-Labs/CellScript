//! Canonical bounded nested-value validation. Source admission is deliberately
//! separate: this emitter only proves byte structure within a caller-owned
//! immutable, readable slice. It grants no Cell or lifecycle authority.
use super::*;
use cellscript_artifact_checker::bounded_nested::{ElementShape, NestedValueContract};

impl CodeGenerator {
    /// Validate all elements before exposing any view.
    ///
    /// ABI: a0 = readable immutable bytes, a1 = encoded byte length. On success
    /// these registers are preserved and a2 is the element count. Clobbers
    /// a3–a5 and t0–t3 only; no calls, stack use, allocation or data stores.
    /// The caller owns the slice lifetime and branches at `invalid` to a
    /// terminal error, never to a value-returning success continuation.
    ///
    /// This prerequisite remains unreachable from DSL lowering until its
    /// source bounds and independent artifact obligations are bound together.
    #[cfg_attr(
        not(all(test, feature = "vm-runner")),
        expect(dead_code, reason = "nested source/metadata admission is not enabled yet")
    )]
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
        if let ElementShape::Record { fixed_field_widths } = &contract.element {
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
        self.emit(format!("li t3, {}", contract.max_payload_bytes));
        self.emit(format!("bltu t3, t1, {invalid}"));
        self.emit("addi t1, t1, 4");
        self.emit(format!("bne t1, t2, {invalid}"));
        self.emit("addi a4, a5, 0");
        self.emit("addi a3, a3, 1");
        self.emit(format!("bltu a3, a2, {item}"));
        self.emit_label(&done);
        Ok(())
    }
}

#[cfg(all(test, feature = "vm-runner"))]
mod tests;
