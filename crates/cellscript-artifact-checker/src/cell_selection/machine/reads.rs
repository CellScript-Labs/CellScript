//! Independent record-read grammar. The input pointer and ordinal are loaded
//! from the exact typed operands; no marker-supplied register or slot is trusted.
//! Proving those operands' definitions and enclosing memory lifetime is separate.
use super::{cursor::Cursor, invalid};
use crate::cell_selection::Read;
use crate::{CheckerError, TypedSemanticConstant, TypedSemanticEntry, TypedSemanticOperand, TypedSemanticOperation};

pub(super) fn slot(entry: &TypedSemanticEntry, id: u32) -> Result<u32, CheckerError> {
    entry
        .locals
        .iter()
        .find(|local| local.id == id)
        .and_then(|local| local.source_id.checked_mul(8))
        .and_then(|offset| u32::try_from(offset).ok())
        .ok_or_else(|| invalid("selection local has no bounded scalar slot"))
}

impl Cursor<'_> {
    fn selected_operand(
        &mut self,
        entry: &TypedSemanticEntry,
        operand: &TypedSemanticOperand,
        register: u32,
    ) -> Result<(), CheckerError> {
        if let Some(local) = operand.local {
            self.stack_load(register, slot(entry, local)?)
        } else if let Some(TypedSemanticConstant::U64(value)) = &operand.constant {
            self.constant(register, value.parse().map_err(|_| invalid("invalid selected-read ordinal"))?)
        } else {
            Err(invalid("selection operand must be a local or unsigned ordinal"))
        }
    }

    fn selected_failure(&mut self, code: u64, abort: u64) -> Result<(), CheckerError> {
        self.constant(10, code)?;
        if self.jump_target()? != abort {
            return Err(invalid("selected-read failure bypasses the checked fatal sink"));
        }
        Ok(())
    }

    pub(super) fn selected_use(
        &mut self,
        entry: &TypedSemanticEntry,
        operation: &TypedSemanticOperation,
        abort: u64,
    ) -> Result<(), CheckerError> {
        let destination = *operation.destinations.first().ok_or_else(|| invalid("selection use has no destination"))?;
        let selected = operation.operands.first().ok_or_else(|| invalid("selection use has no input"))?;
        if selected.local.is_none() || !selected.ty.starts_with("ExactCellSelection<") {
            return Err(invalid("selection handle is not an invocation-local value"));
        }
        if matches!(operation.opcode.as_str(), "move" | "field-access") {
            self.selected_operand(entry, selected, 5)?;
            self.stack_store(5, slot(entry, destination)?)?;
            return self.finish();
        }
        let crate::TypedSemanticOperationDetail::ExactRoleRead { role, read } = operation.detail else {
            return Err(invalid("selected read has no typed role"));
        };
        if read.indexed() {
            let index = operation.operands.get(1).ok_or_else(|| invalid("selected read has no ordinal"))?;
            self.selected_operand(entry, index, 10)?;
        }
        self.selected_operand(entry, selected, 29)?;
        self.load(5, 29, 768 + role * 16 + if read.input() { 0 } else { 8 }, false)?;
        let bounds = self.label();
        let done = self.label();
        if read.indexed() {
            let scan = self.label();
            let next = self.label();
            let found = self.label();
            self.branch(7, 10, 5, bounds)?;
            self.load(30, 29, if read.input() { 40 } else { 48 }, false)?;
            self.constant(5, 0)?;
            self.bind(scan)?;
            self.branch(7, 5, 30, bounds)?;
            self.constant(6, if read.input() { 896 } else { 2944 })?;
            self.add(28, 29, 6)?;
            self.immediate(0x13, 1, 7, 5, 6)?;
            self.add(28, 28, 7)?;
            self.load(6, 28, 16, false)?;
            self.constant(7, u64::from(role))?;
            self.branch(1, 6, 7, next)?;
            self.branch(0, 10, 0, found)?;
            self.addi(10, 10, -1)?;
            self.bind(next)?;
            self.addi(5, 5, 1)?;
            self.jump(scan)?;
            self.bind(found)?;
            self.load(5, 28, if read == Read::Successor { 56 } else { 0 }, false)?;
            if read == Read::Successor {
                let present = self.label();
                self.constant(6, u64::MAX)?;
                self.branch(1, 5, 6, present)?;
                self.selected_failure(7, abort)?;
                self.bind(present)?;
            }
            self.constant(6, if read == Read::Input { 1 } else { 2 })?;
            self.immediate(0x13, 1, 6, 6, 32)?;
            // or t0, t0, t1: the source-view tag occupies the upper word.
            self.bit_or(5, 5, 6)?;
        }
        self.stack_store(5, slot(entry, destination)?)?;
        self.jump(done)?;
        self.bind(bounds)?;
        self.selected_failure(21, abort)?;
        self.bind(done)?;
        self.finish()
    }
}
