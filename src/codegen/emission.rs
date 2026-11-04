//! Structured memory/address operations. Render assembly only after selecting
//! address lowering and checking its register contract; never parse it back.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum LoadOp {
    ByteUnsigned,
    Double,
}

impl LoadOp {
    const fn mnemonic(self) -> &'static str {
        match self {
            Self::ByteUnsigned => "lbu",
            Self::Double => "ld",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum StoreOp {
    Byte,
    Half,
    Word,
    Double,
}

impl StoreOp {
    const fn mnemonic(self) -> &'static str {
        match self {
            Self::Byte => "sb",
            Self::Half => "sh",
            Self::Word => "sw",
            Self::Double => "sd",
        }
    }

    pub(super) const fn width(self) -> usize {
        match self {
            Self::Byte => 1,
            Self::Half => 2,
            Self::Word => 4,
            Self::Double => 8,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum MemoryOp {
    Load(LoadOp),
    Store(StoreOp),
}

impl From<LoadOp> for MemoryOp {
    fn from(op: LoadOp) -> Self {
        Self::Load(op)
    }
}

impl From<StoreOp> for MemoryOp {
    fn from(op: StoreOp) -> Self {
        Self::Store(op)
    }
}

impl CodeGenerator {
    fn emission_immediate(&mut self, value: impl TryInto<i64>) -> Option<i64> {
        let value = value.try_into().ok();
        if value.is_none() {
            self.emission_error.get_or_insert_with(|| "memory/address immediate exceeds the signed 64-bit address range".into());
        }
        value
    }

    fn emission_registers(&mut self, value: &str, base: &str) -> Option<(u8, u8)> {
        match (parse_register(value), parse_register(base)) {
            (Ok(value), Ok(base)) => Some((value, base)),
            _ => {
                self.emission_error.get_or_insert_with(|| format!("invalid memory/address register operand: {value}, {base}"));
                None
            }
        }
    }

    pub(super) fn emit_load(&mut self, op: LoadOp, dst: &str, base: &str, offset: impl TryInto<i64>) {
        let Some(offset) = self.emission_immediate(offset) else { return };
        let Some((destination, address)) = self.emission_registers(dst, base) else { return };
        if small_signed_immediate(offset) {
            self.assembly.push(format!("    {} {dst}, {offset}({base})", op.mnemonic()));
        } else if destination != address && !matches!(destination, 0 | 2) {
            // This operation already owns its destination; no other live
            // register is scratch, including when ABI aliases are used.
            self.emit(format!("li {dst}, {offset}"));
            self.emit(format!("add {dst}, {base}, {dst}"));
            self.assembly.push(format!("    {} {dst}, 0({dst})", op.mnemonic()));
        } else {
            self.reject_implicit_scratch(format!("{} {dst}, {offset}({base})", op.mnemonic()));
        }
    }

    pub(super) fn emit_store(&mut self, op: StoreOp, src: &str, base: &str, offset: impl TryInto<i64>) {
        let Some(offset) = self.emission_immediate(offset) else { return };
        if self.emission_registers(src, base).is_none() {
            return;
        }
        // Guard the original structured address before any address expansion.
        // Guard spills must not recursively guard themselves. Stack helpers
        // explicitly suspend this scope while materializing their own frame.
        if let Some(guard) = self.allocated_write_guard.take() {
            let guarded = self.emit_allocated_guarded_store(op, src, base, offset, &guard);
            if !guarded {
                self.emit_store(op, src, base, offset);
            }
            self.allocated_write_guard = Some(guard);
            return;
        }
        if small_signed_immediate(offset) {
            self.assembly.push(format!("    {} {src}, {offset}({base})", op.mnemonic()));
        } else {
            self.reject_implicit_scratch(format!("{} {src}, {offset}({base})", op.mnemonic()));
        }
    }

    pub(super) fn emit_add_immediate(&mut self, dst: &str, base: &str, value: impl TryInto<i64>) {
        let Some(value) = self.emission_immediate(value) else { return };
        let Some((destination, source)) = self.emission_registers(dst, base) else { return };
        if small_signed_immediate(value) {
            self.assembly.push(format!("    addi {dst}, {base}, {value}"));
        } else if destination != source && !matches!(destination, 0 | 2) {
            self.emit(format!("li {dst}, {value}"));
            self.emit(format!("add {dst}, {base}, {dst}"));
        } else {
            self.reject_implicit_scratch(format!("addi {dst}, {base}, {value}"));
        }
    }

    pub(super) fn emit_memory(&mut self, op: impl Into<MemoryOp>, register: &str, base: &str, offset: impl TryInto<i64>) {
        match op.into() {
            MemoryOp::Load(op) => self.emit_load(op, register, base, offset),
            MemoryOp::Store(op) => self.emit_store(op, register, base, offset),
        }
    }

    fn reject_implicit_scratch(&mut self, instruction: String) {
        self.emission_error.get_or_insert_with(|| format!(
            "large-offset instruction requires an explicit live-register contract: {instruction}; use a structured address/access emitter"
        ));
    }
}

#[cfg(test)]
mod tests;
