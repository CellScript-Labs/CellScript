//! Bounded selection's instruction grammar. The checker owns these offsets and
//! derives widths, identities and limits from typed evidence, not codegen notes.
use super::*;
use cursor::Cursor;

const Z: u32 = 0;
const SP: u32 = 2;
const T0: u32 = 5;
const T1: u32 = 6;
const T2: u32 = 7;
const T3: u32 = 28;
const T4: u32 = 29;
const T5: u32 = 30;
const A0: u32 = 10;
const A1: u32 = 11;
const A2: u32 = 12;
const A3: u32 = 13;
const A4: u32 = 14;
const A5: u32 = 15;
const A7: u32 = 17;
const EQ: u32 = 0;
const NE: u32 = 1;
const LTU: u32 = 6;
const GEU: u32 = 7;
const SOURCE: u32 = 0;
const INDEX: u32 = 8;
const LENGTH: u32 = 16;
const INPUTS: u32 = 24;
const OUTPUTS: u32 = 32;
const SELECTED_INPUTS: u32 = 40;
const SELECTED_OUTPUTS: u32 = 48;
const BYTES: u32 = 56;
const GROUP_COUNT: u32 = 64;
const GROUP_INDEX: u32 = 72;
const CURRENT_ROLE: u32 = 80;
const CURSOR: u32 = 88;
const PARTNER: u32 = 96;
const HAS_TYPE: u32 = 104;
const CURRENT_HASH: u32 = 128;
const TYPE_HASH: u32 = 160;
const LOCK_HASH: u32 = 192;
const SCRATCH: u32 = 224;
const DATA: u32 = 256;
const COUNTS: u32 = 768;
const IN_TABLE: u32 = 896;
const OUT_TABLE: u32 = 2944;
const GROUP_INPUT: u64 = 0x0100_0000_0000_0001;
const GROUP_OUTPUT: u64 = 0x0100_0000_0000_0002;

impl Cursor<'_> {
    fn data_args(&mut self, buffer: u32, width: u32) -> Result<(), CheckerError> {
        self.constant(T0, u64::from(width))?;
        self.stack_store(T0, LENGTH)?;
        self.stack_address(A0, buffer)?;
        self.stack_address(A1, LENGTH)?;
        self.constant(A2, 0)
    }

    fn field(&mut self, source: u64, field: u64, buffer: u32, width: u32) -> Result<(), CheckerError> {
        self.stack_load(T3, INDEX)?;
        self.data_args(buffer, width)?;
        self.addi(A3, T3, 0)?;
        self.constant(A4, source)?;
        self.constant(A5, field)?;
        self.constant(A7, 2081)?;
        self.syscall()
    }

    fn exact_load(&mut self, width: u32, fail: usize) -> Result<(), CheckerError> {
        self.branch(NE, A0, Z, fail)?;
        self.stack_load(T0, LENGTH)?;
        self.constant(T1, u64::from(width))?;
        self.branch(NE, T0, T1, fail)
    }

    fn hash_literal(&mut self, buffer: u32, expected: &[u8; 32], fail: usize) -> Result<(), CheckerError> {
        for (index, word) in expected.chunks_exact(8).enumerate() {
            self.stack_load(T0, buffer + index as u32 * 8)?;
            self.constant(T1, u64::from_le_bytes(word.try_into().unwrap()))?;
            self.branch(NE, T0, T1, fail)?;
        }
        Ok(())
    }

    fn hash_buffers(&mut self, left: u32, right: u32, fail: usize) -> Result<(), CheckerError> {
        for index in 0..4 {
            self.stack_load(T0, left + index * 8)?;
            self.stack_load(T1, right + index * 8)?;
            self.branch(NE, T0, T1, fail)?;
        }
        Ok(())
    }

    fn record_pointer(&mut self, table: u32, dest: u32) -> Result<(), CheckerError> {
        self.stack_address(dest, table)?;
        self.immediate(0x13, 1, T2, T0, 6)?;
        self.add(dest, dest, T2)
    }

    fn current_role(&mut self, invalid: usize, authority: usize) -> Result<(), CheckerError> {
        self.data_args(CURRENT_HASH, 32)?;
        self.constant(A7, 2062)?;
        self.syscall()?;
        self.exact_load(32, invalid)?;
        self.stack_store(Z, INDEX)?;
        let output = self.label();
        let finish = self.label();
        self.field(GROUP_INPUT, 0, SCRATCH, 8)?;
        self.constant(T0, 1)?;
        self.branch(EQ, A0, T0, output)?;
        self.exact_load(8, invalid)?;
        for source in [GROUP_INPUT, GROUP_OUTPUT] {
            if source == GROUP_OUTPUT {
                self.bind(output)?;
            }
            self.stack_store(Z, CURRENT_ROLE)?;
            let lock_other = self.label();
            let no_type = self.label();
            self.field(source, 3, LOCK_HASH, 32)?;
            self.exact_load(32, invalid)?;
            self.hash_buffers(LOCK_HASH, CURRENT_HASH, lock_other)?;
            self.constant(T0, 1)?;
            self.stack_store(T0, CURRENT_ROLE)?;
            self.bind(lock_other)?;
            self.field(source, 5, TYPE_HASH, 32)?;
            self.constant(T0, 2)?;
            self.branch(EQ, A0, T0, no_type)?;
            self.exact_load(32, invalid)?;
            self.hash_buffers(TYPE_HASH, CURRENT_HASH, no_type)?;
            self.stack_load(T0, CURRENT_ROLE)?;
            self.addi(T0, T0, 2)?;
            self.stack_store(T0, CURRENT_ROLE)?;
            self.bind(no_type)?;
            self.stack_load(T0, CURRENT_ROLE)?;
            self.branch(EQ, T0, Z, authority)?;
            self.constant(T1, 3)?;
            self.branch(EQ, T0, T1, authority)?;
            if source == GROUP_OUTPUT {
                self.constant(T1, 2)?;
                self.branch(NE, T0, T1, authority)?;
            }
            self.jump(finish)?;
        }
        self.bind(finish)
    }

    fn count_source(&mut self, source: u64, limit: u32, count: u32, invalid: usize, bounds: usize) -> Result<(), CheckerError> {
        self.constant(T0, source)?;
        self.stack_store(T0, SOURCE)?;
        self.stack_store(Z, INDEX)?;
        let scan = self.label();
        let end = self.label();
        self.bind(scan)?;
        self.data_args(SCRATCH, 8)?;
        self.stack_load(A3, INDEX)?;
        self.stack_load(A4, SOURCE)?;
        self.constant(A5, 0)?;
        self.constant(A7, 2081)?;
        self.syscall()?;
        self.constant(T0, 1)?;
        self.branch(EQ, A0, T0, end)?;
        self.exact_load(8, invalid)?;
        self.stack_load(T0, INDEX)?;
        self.constant(T1, u64::from(limit))?;
        self.branch(GEU, T0, T1, bounds)?;
        self.addi(T0, T0, 1)?;
        self.branch(EQ, T0, Z, invalid)?;
        self.stack_store(T0, INDEX)?;
        self.jump(scan)?;
        self.bind(end)?;
        self.stack_load(A0, INDEX)?;
        self.stack_store(A0, count)
    }

    fn group_index(&mut self, input: bool) -> Result<(), CheckerError> {
        let is_type = self.label();
        let matched = self.label();
        let done = self.label();
        self.constant(T0, u64::MAX)?;
        self.stack_store(T0, GROUP_INDEX)?;
        self.stack_load(T0, CURRENT_ROLE)?;
        self.constant(T1, 2)?;
        self.branch(EQ, T0, T1, is_type)?;
        if input {
            self.hash_buffers(LOCK_HASH, CURRENT_HASH, done)?;
            self.jump(matched)?;
        } else {
            self.jump(done)?;
        }
        self.bind(is_type)?;
        self.stack_load(T0, HAS_TYPE)?;
        self.branch(EQ, T0, Z, done)?;
        self.hash_buffers(TYPE_HASH, CURRENT_HASH, done)?;
        self.bind(matched)?;
        self.stack_load(T0, GROUP_COUNT)?;
        self.stack_store(T0, GROUP_INDEX)?;
        self.addi(T0, T0, 1)?;
        self.stack_store(T0, GROUP_COUNT)?;
        self.bind(done)
    }

    fn unique_key(&mut self, layout: &Layout, ordinal: u32, selected: u32, table: u32, fail: usize) -> Result<(), CheckerError> {
        let scan = self.label();
        let next = self.label();
        let done = self.label();
        self.stack_store(Z, CURSOR)?;
        self.bind(scan)?;
        self.stack_load(T0, CURSOR)?;
        self.stack_load(T1, selected)?;
        self.branch(GEU, T0, T1, done)?;
        self.record_pointer(table, T3)?;
        self.load(T0, T3, 16, false)?;
        self.constant(T1, u64::from(ordinal))?;
        self.branch(NE, T0, T1, next)?;
        for byte in 0..layout.key_width {
            self.load(T0, T3, 24 + byte, true)?;
            self.load(T1, SP, DATA + layout.key_offset + byte, true)?;
            self.branch(NE, T0, T1, next)?;
        }
        self.jump(fail)?;
        self.bind(next)?;
        self.stack_load(T0, CURSOR)?;
        self.addi(T0, T0, 1)?;
        self.stack_store(T0, CURSOR)?;
        self.jump(scan)?;
        self.bind(done)
    }

    fn correspondence(&mut self, role: &Role, layout: &Layout, ordinal: u32, fail: usize) -> Result<(), CheckerError> {
        self.stack_load(T0, COUNTS + ordinal * 16)?;
        self.stack_load(T1, COUNTS + ordinal * 16 + 8)?;
        match &role.correspondence {
            Correspondence::Create => {
                self.branch(NE, T0, Z, fail)?;
                return self.branch(EQ, T1, Z, fail);
            }
            Correspondence::Burn => {
                self.branch(EQ, T0, Z, fail)?;
                return self.branch(NE, T1, Z, fail);
            }
            Correspondence::Preserve => self.branch(NE, T0, T1, fail)?,
            Correspondence::Merge(_) => {
                self.constant(T2, 2)?;
                self.branch(LTU, T0, T2, fail)?;
                self.constant(T2, 1)?;
                self.branch(NE, T1, T2, fail)?;
            }
        }
        let scan = self.label();
        let next = self.label();
        let search = self.label();
        let other = self.label();
        let done = self.label();
        self.stack_store(Z, CURSOR)?;
        self.bind(scan)?;
        self.stack_load(T0, CURSOR)?;
        self.stack_load(T1, SELECTED_INPUTS)?;
        self.branch(GEU, T0, T1, done)?;
        self.record_pointer(IN_TABLE, T3)?;
        self.load(T0, T3, 16, false)?;
        self.constant(T1, u64::from(ordinal))?;
        self.branch(NE, T0, T1, next)?;
        self.stack_store(Z, PARTNER)?;
        self.bind(search)?;
        self.stack_load(T0, PARTNER)?;
        self.stack_load(T1, SELECTED_OUTPUTS)?;
        self.branch(GEU, T0, T1, fail)?;
        self.record_pointer(OUT_TABLE, T4)?;
        self.load(T0, T4, 16, false)?;
        self.constant(T1, u64::from(ordinal))?;
        self.branch(NE, T0, T1, other)?;
        for byte in 0..layout.key_width {
            if let Correspondence::Merge(key) = &role.correspondence {
                self.constant(T0, u64::from(key[byte as usize]))?;
            } else {
                self.load(T0, T3, 24 + byte, true)?;
            }
            self.load(T1, T4, 24 + byte, true)?;
            self.branch(NE, T0, T1, other)?;
        }
        self.load(T0, T4, 0, false)?;
        self.store(T0, T3, 56, false)?;
        if matches!(role.correspondence, Correspondence::Preserve) {
            self.load(T0, T3, 0, false)?;
            self.store(T0, T4, 56, false)?;
        }
        self.jump(next)?;
        self.bind(other)?;
        self.stack_load(T0, PARTNER)?;
        self.addi(T0, T0, 1)?;
        self.stack_store(T0, PARTNER)?;
        self.jump(search)?;
        self.bind(next)?;
        self.stack_load(T0, CURSOR)?;
        self.addi(T0, T0, 1)?;
        self.stack_store(T0, CURSOR)?;
        self.jump(scan)?;
        self.bind(done)
    }

    pub(super) fn selection(
        &mut self,
        contract: &Contract,
        layouts: &[Layout],
        offset: u32,
        abort: u64,
        record: &VerifiedLoweringRecord,
    ) -> Result<(), CheckerError> {
        self.stack_adjust(i64::from(offset))?;
        let invalid = self.label();
        let bounds = self.label();
        let authority = self.label();
        let custody = self.label();
        let layout_error = self.label();
        let pairing = self.label();
        let done = self.label();
        self.current_role(invalid, authority)?;
        for role in &contract.roles {
            if role.local_type {
                self.stack_load(T0, CURRENT_ROLE)?;
                self.constant(T1, 2)?;
                self.branch(NE, T0, T1, authority)?;
                self.hash_literal(CURRENT_HASH, &role.type_hash, authority)?;
            }
        }
        self.count_source(1, contract.input_scan, INPUTS, invalid, bounds)?;
        self.count_source(2, contract.output_scan, OUTPUTS, invalid, bounds)?;
        for slot in [SELECTED_INPUTS, SELECTED_OUTPUTS, BYTES] {
            self.stack_store(Z, slot)?;
        }
        for slot in (COUNTS..COUNTS + contract.roles.len() as u32 * 16).step_by(8) {
            self.stack_store(Z, slot)?;
        }
        for (side, source, count, selected, table) in
            [(0, 1, INPUTS, SELECTED_INPUTS, IN_TABLE), (1, 2, OUTPUTS, SELECTED_OUTPUTS, OUT_TABLE)]
        {
            let scan = self.label();
            let next = self.label();
            let end = self.label();
            let no_type = self.label();
            let type_ready = self.label();
            self.stack_store(Z, INDEX)?;
            self.stack_store(Z, GROUP_COUNT)?;
            self.bind(scan)?;
            self.stack_load(T0, INDEX)?;
            self.stack_load(T1, count)?;
            self.branch(GEU, T0, T1, end)?;
            self.field(source, 5, TYPE_HASH, 32)?;
            self.constant(T0, 2)?;
            self.branch(EQ, A0, T0, no_type)?;
            self.exact_load(32, invalid)?;
            self.constant(T0, 1)?;
            self.stack_store(T0, HAS_TYPE)?;
            self.jump(type_ready)?;
            self.bind(no_type)?;
            self.stack_store(Z, HAS_TYPE)?;
            self.bind(type_ready)?;
            self.field(source, 3, LOCK_HASH, 32)?;
            self.exact_load(32, invalid)?;
            self.group_index(side == 0)?;
            self.stack_load(T0, HAS_TYPE)?;
            self.branch(EQ, T0, Z, next)?;
            for (ordinal, (role, layout)) in contract.roles.iter().zip(layouts).enumerate() {
                let ordinal = ordinal as u32;
                let unmatched = self.label();
                self.hash_literal(TYPE_HASH, &role.type_hash, unmatched)?;
                self.hash_literal(LOCK_HASH, &role.lock_hash, custody)?;
                self.data_args(if layout.nested.is_some() { FRAME_BYTES } else { DATA }, layout.width)?;
                self.stack_load(A3, INDEX)?;
                self.constant(A4, source)?;
                self.constant(A7, 2092)?;
                self.syscall()?;
                self.branch(NE, A0, Z, invalid)?;
                if let Some(nested) = &layout.nested {
                    self.pc = crate::nested_machine::cells::check_selected_table(
                        record,
                        self.pc,
                        self.words,
                        self.targets,
                        &nested.fields,
                        layout.width,
                        LENGTH,
                        FRAME_BYTES,
                        layout::frame_bytes(layouts),
                    )?;
                    self.stack_address(T4, FRAME_BYTES)?;
                    let key_offset = 4 * (nested.key_field + 1);
                    self.load(T5, T4, key_offset, true)?;
                    for byte in 1..4 {
                        self.load(T2, T4, key_offset + byte, true)?;
                        self.immediate(0x13, 1, T2, T2, byte as i32 * 8)?;
                        self.bit_or(T5, T5, T2)?;
                    }
                    self.add(T4, T4, T5)?;
                    for byte in 0..layout.key_width {
                        self.load(T0, T4, byte, true)?;
                        self.stack_byte(T0, DATA + byte, true)?;
                    }
                } else {
                    self.stack_load(T0, LENGTH)?;
                    self.constant(T1, u64::from(layout.width))?;
                    self.branch(NE, T0, T1, layout_error)?;
                }
                self.stack_load(T0, selected)?;
                self.constant(T1, u64::from(contract.selected_per_side))?;
                self.branch(GEU, T0, T1, bounds)?;
                self.stack_load(T0, BYTES)?;
                if layout.nested.is_some() {
                    self.stack_load(T2, LENGTH)?;
                    self.add(T0, T0, T2)?;
                } else {
                    self.addi(T0, T0, layout.width as i32)?;
                }
                self.constant(T1, u64::from(contract.aggregate_bytes))?;
                self.branch(LTU, T1, T0, bounds)?;
                self.stack_store(T0, BYTES)?;
                self.unique_key(layout, ordinal, selected, table, bounds)?;
                self.stack_load(T0, selected)?;
                self.record_pointer(table, T3)?;
                for (slot, record) in [(INDEX, 0), (GROUP_INDEX, 8)] {
                    self.stack_load(T1, slot)?;
                    self.store(T1, T3, record, false)?;
                }
                self.constant(T1, u64::from(ordinal))?;
                self.store(T1, T3, 16, false)?;
                for byte in 0..layout.key_width {
                    self.load(T1, SP, DATA + layout.key_offset + byte, true)?;
                    self.store(T1, T3, 24 + byte, true)?;
                }
                self.constant(T1, u64::MAX)?;
                self.store(T1, T3, 56, false)?;
                for slot in [selected, COUNTS + ordinal * 16 + side * 8] {
                    self.stack_load(T0, slot)?;
                    self.addi(T0, T0, 1)?;
                    self.stack_store(T0, slot)?;
                }
                self.jump(next)?;
                self.bind(unmatched)?;
            }
            self.bind(next)?;
            self.stack_load(T0, INDEX)?;
            self.addi(T0, T0, 1)?;
            self.stack_store(T0, INDEX)?;
            self.jump(scan)?;
            self.bind(end)?;
        }
        for (ordinal, (role, layout)) in contract.roles.iter().zip(layouts).enumerate() {
            for (side, (min, max)) in [role.inputs, role.outputs].into_iter().enumerate() {
                self.stack_load(T0, COUNTS + ordinal as u32 * 16 + side as u32 * 8)?;
                self.constant(T1, u64::from(min))?;
                self.branch(LTU, T0, T1, bounds)?;
                self.constant(T1, u64::from(max))?;
                self.branch(LTU, T1, T0, bounds)?;
            }
            self.correspondence(role, layout, ordinal as u32, pairing)?;
        }
        self.jump(done)?;
        for (label, code) in [(invalid, 44), (bounds, 21), (authority, 47), (custody, 41), (layout_error, 4), (pairing, 7)] {
            self.bind(label)?;
            self.constant(A0, code)?;
            if self.jump_target()? != abort {
                return Err(invalid_error("failure bypasses the checked terminal sink"));
            }
        }
        if layouts.iter().any(|layout| layout.nested.is_some()) {
            for code in [16, 2] {
                if !record.verifier_failure_exits.iter().any(|exit| exit.address == self.pc && exit.code == code) {
                    return Err(invalid_error("nested failure has no checked terminal record"));
                }
                self.constant(A0, code as u64)?;
                if self.jump_target()? != abort {
                    return Err(invalid_error("nested failure bypasses the checked terminal sink"));
                }
            }
        }
        self.bind(done)?;
        self.stack_adjust(-i64::from(offset))?;
        Ok(())
    }
}

// The selection method's `invalid` label is deliberately separate from errors.
fn invalid_error(message: &str) -> CheckerError {
    super::invalid(message)
}
