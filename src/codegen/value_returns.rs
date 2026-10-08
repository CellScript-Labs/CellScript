//! Fixed ordinary struct results use an explicit caller-owned output buffer.
//! The hidden pointer follows all ordinary machine arguments (including their
//! lengths/type hashes), so normal outgoing-stack ownership applies. No pointer
//! into a callee frame or register tuple escapes as a nominal struct value.

use super::*;

impl CodeGenerator {
    pub(super) fn fixed_struct_return_width(&self, ty: &IrType) -> Option<usize> {
        let IrType::Named(name) = ty else { return None };
        if self.cell_type_names.contains(name) || self.enum_fixed_sizes.contains_key(name) || !self.type_layouts.contains_key(name) {
            return None;
        }
        self.type_fixed_sizes.get(name).copied()
    }

    pub(super) fn current_return_buffer_bytes(&self) -> Option<usize> {
        self.current_function.as_ref().and_then(|name| self.callable_abis.get(name)).and_then(|abi| abi.return_buffer_bytes)
    }

    pub(super) fn emit_value_return(&mut self, operand: &IrOperand) -> Result<bool> {
        let Some(width) = self.current_return_buffer_bytes() else { return Ok(false) };
        let offset = self
            .return_buffer_pointer_offset
            .ok_or_else(|| CompileError::without_span("fixed struct return lacks its saved output pointer"))?;
        self.emit(format!("# cellscript abi: fixed struct return copies {width} bytes into caller-owned storage"));
        if width != 0 {
            let source = self
                .expected_fixed_byte_source(operand, width)
                .ok_or_else(|| CompileError::without_span("fixed struct return source has no bounded storage"))?;
            self.emit_prepare_fixed_byte_source(&source, width, "fixed struct return");
            if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &source) {
                return Err(CompileError::without_span("fixed struct return source pointer is unavailable"));
            }
            // a0 is live while the saved destination is loaded; memcpy then
            // owns a0/a1/a2.
            self.emit_stack_load_with_avoid("a1", offset, &["a0"]);
            self.emit(format!("li a2, {width}"));
            self.emit("call __cellscript_memcpy_fixed");
        }
        self.emit("li a0, 0");
        self.emit_epilogue();
        Ok(true)
    }
}
