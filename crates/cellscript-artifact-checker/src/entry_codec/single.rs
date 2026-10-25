//! Independent recognition of the complete fixed single-entry witness decoder.
//! Every instruction and edge from entry through argument reception is checked;
//! names only locate blocks and never substitute for executable evidence.
use super::*;
mod nested;
mod spawn;
pub(crate) use nested::verify as verify_nested;
pub(crate) use spawn::verify as verify_spawn;

impl Cursor<'_> {
    fn word(&self) -> Result<u32, CheckerError> {
        self.elf
            .instructions
            .binary_search_by_key(&self.address, |i| i.address)
            .map(|i| self.elf.instructions[i].word)
            .map_err(|_| invalid("missing single-entry instruction"))
    }
    fn i(&mut self, rd: u32, rs: u32, imm: i32) -> Result<(), CheckerError> {
        self.expect(|w| is_addi(w, rd, rs, imm)).map(|_| ())
    }
    fn r(&mut self, rd: u32, a: u32, b: u32, function: u32) -> Result<(), CheckerError> {
        self.expect(|w| w == (function | b << 20 | a << 15 | rd << 7 | 0x33)).map(|_| ())
    }
    fn shift(&mut self, rd: u32, rs: u32, bits: u32, right: bool) -> Result<(), CheckerError> {
        self.expect(|w| w == (bits << 20 | rs << 15 | (if right { 5 } else { 1 }) << 12 | rd << 7 | 0x13)).map(|_| ())
    }
    fn load(&mut self, rd: u32, base: u32, offset: i32, byte: bool) -> Result<(), CheckerError> {
        self.expect(|w| if byte { is_lbu(w, rd, base, offset) } else { is_ld(w, rd, base, offset) }).map(|_| ())
    }
    fn store(&mut self, src: u32, base: u32, offset: i32, byte: bool) -> Result<(), CheckerError> {
        self.expect(|w| {
            if byte {
                w == (((offset as u32 & 0xfe0) << 20) | src << 20 | base << 15 | (offset as u32 & 31) << 7 | 0x23)
            } else {
                is_sd(w, src, base, offset)
            }
        })
        .map(|_| ())
    }
    fn b(&mut self, a: u32, b: u32, function: u32, target: u64) -> Result<(), CheckerError> {
        self.branch(|w| w & 0x01fff07f == (b << 20 | a << 15 | function << 12 | 0x63), target)
    }
    fn jump(&mut self, target: u64) -> Result<(), CheckerError> {
        self.branch(is_jal_zero, target)
    }
    /// Evaluate a bounded, side-effect-free immediate materialization. This
    /// accepts instruction selection changes without trusting producer values.
    fn li(&mut self, rd: u32, expected: u64) -> Result<(), CheckerError> {
        let mut value = None;
        for _ in 0..16 {
            let word = self.word()?;
            if word >> 7 & 31 != rd {
                break;
            }
            let rs = word >> 15 & 31;
            let imm = (word as i32 >> 20) as i64 as u64;
            let next = match (word & 0x7f, word >> 12 & 7) {
                (0x37, _) => Some((word as i32 & !4095) as i64 as u64),
                (0x13, 0) if rs == 0 => Some(imm),
                (0x13, 0) if rs == rd => value.map(|v: u64| v.wrapping_add(imm)),
                (0x13, 1) if rs == rd && word >> 26 == 0 => value.map(|v| v << (word >> 20 & 63)),
                (0x13, 5) if rs == rd && word >> 26 == 0 => value.map(|v| v >> (word >> 20 & 63)),
                _ => None,
            };
            let Some(next) = next else { break };
            value = Some(next);
            self.address += 4;
            if next == expected {
                return Ok(());
            }
        }
        Err(invalid("single-entry immediate differs or has unproven side effects"))
    }
    fn adjust(&mut self, delta: i32) -> Result<(), CheckerError> {
        self.li(31, delta as i64 as u64)?;
        self.r(2, 2, 31, 0)
    }
    fn ra(&mut self, restore: bool) -> Result<(), CheckerError> {
        self.li(31, 5368)?;
        self.r(31, 2, 31, 0)?;
        if restore {
            self.load(1, 31, 0, false)
        } else {
            self.store(1, 31, 0, false)
        }
    }
    fn syscall(&mut self, source: u64, capacity_probe: bool) -> Result<(), CheckerError> {
        self.li(5, if capacity_probe { 8 } else { 4096 })?;
        self.store(5, 2, 0, false)?;
        self.i(10, 2, 8)?;
        self.i(11, 2, 0)?;
        self.li(12, 0)?;
        self.li(13, 0)?;
        self.li(14, source)?;
        if capacity_probe {
            self.li(15, 0)?;
        }
        self.li(17, if capacity_probe { 2081 } else { 2074 })?;
        self.expect(|w| w == 0x73)?;
        Ok(())
    }
    fn u32(&mut self, dest: u32, base: u32, scratch: u32) -> Result<(), CheckerError> {
        self.load(dest, base, 0, true)?;
        for index in 1..4 {
            self.load(scratch, base, index, true)?;
            self.shift(scratch, scratch, index as u32 * 8, false)?;
            self.r(dest, dest, scratch, 0x6000)?;
        }
        Ok(())
    }
    fn at(&self, address: u64) -> Result<(), CheckerError> {
        if self.address == address {
            Ok(())
        } else {
            Err(invalid("unchecked gap in single-entry decoder"))
        }
    }
}

pub(super) fn verify(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<BTreeMap<u32, Variant>, CheckerError> {
    let contract = &record.typed_semantics.foundation.entry_contract;
    if !matches!(contract.script_role.as_str(), "lock" | "type") {
        return Err(invalid("single-entry decoder requires a concrete Lock or Type entry"));
    }
    let entry = record
        .typed_semantics
        .entries
        .iter()
        .find(|e| e.id == contract.exact_entry)
        .ok_or_else(|| invalid("single entry has no exact typed owner"))?;
    let params = parameters(entry, &record.typed_semantics)?;
    let wrapper =
        record.entries.iter().find(|e| e.name == "_cellscript_entry").ok_or_else(|| invalid("missing single-entry wrapper"))?;
    let start = entry_start(record, &wrapper.id)?;
    let mut c = Cursor { elf, address: start };
    let owned = record.blocks.iter().filter(|b| b.owner_entry == wrapper.id).collect::<Vec<_>>();
    if elf.control_flow.iter().any(|f| {
        owned.iter().any(|b| b.range.contains(f.target)) && !owned.iter().any(|b| b.range.contains(f.address)) && f.target != start
    }) {
        return Err(invalid("incoming edge bypasses single-entry guards"));
    }
    let label = |name: &str| unique_policy_block(record, &wrapper.id, name).map(|b| b.range.start);
    if params.is_empty() {
        c.jump(entry_start(record, &entry.id)?)?;
    } else {
        if wrapper.frame_size_bytes != 5376 || wrapper.outgoing_argument_bytes != 0 {
            return Err(invalid("single-entry frame differs from the bounded witness profile"));
        }
        c.adjust(-5376)?;
        c.ra(false)?;
        let fail = label(".Lentry_witness_fail_")?;
        let width = params.iter().map(|p| p.width_bytes).sum::<u32>();
        if width > 0 {
            if contract.entry_payload_abi != "cellscript-entry-witness-v1"
                || contract.witness_placement_abi != "cellscript-witnessargs-input-type-v2"
                || contract.witness_placement_field != "input_type"
                || contract.witness_placement_source != "group-input-0-then-group-output-0"
            {
                return Err(invalid("single-entry witness placement differs"));
            }
            load_witness(&mut c, fail, &label)?;
            // The preceding certified store puts t1 into size[sp]; local
            // store/load forwarding may retain that exact value in t0.
            c.expect(|w| is_ld(w, 5, 2, 0) || is_addi(w, 5, 6, 0))?;
            c.li(6, u64::from(width + 8))?;
            c.r(7, 5, 6, 0x3000)?;
            let size = label(".Lentry_witness_size_ok_")?;
            c.b(7, 0, 0, size)?;
            c.jump(fail)?;
            c.at(size)?;
            for (index, byte) in b"CSARGv1\0".iter().enumerate() {
                c.load(5, 2, 8 + index as i32, true)?;
                c.li(6, u64::from(*byte))?;
                c.r(7, 5, 6, 0x40000000)?;
                c.b(7, 0, 1, fail)?;
            }
            c.load(5, 2, 0, false)?;
            c.li(6, u64::from(width + 8))?;
            c.r(7, 5, 6, 0x40000000)?;
            let exact = label(".Lentry_witness_exact_size_ok_")?;
            c.b(7, 0, 0, exact)?;
            c.jump(fail)?;
            c.at(exact)?;
        }
        for param in &params {
            let dest = 10 + param.abi_index;
            if param.transport == Transport::RuntimeBoundNull {
                for index in 0..param.abi_arguments {
                    c.li(dest + index, 0)?;
                }
            } else if param.width_bytes > 8 {
                c.i(dest, 2, 16 + param.payload_offset.unwrap() as i32)?;
                c.li(dest + 1, u64::from(param.width_bytes))?;
            } else {
                c.li(dest, 0)?;
                for index in 0..param.width_bytes {
                    c.load(5, 2, 16 + (param.payload_offset.unwrap() + index) as i32, true)?;
                    if index != 0 {
                        c.shift(5, 5, index * 8, false)?;
                    }
                    c.r(dest, dest, 5, 0x6000)?;
                }
                if param.ty == "i32" {
                    c.shift(dest, dest, 32, false)?;
                    c.expect(|w| w == (0x42005013 | dest << 15 | dest << 7))?;
                }
            }
        }
        c.expect(|w| is_auipc(w, 1))?;
        c.branch(is_jalr_call, entry_start(record, &entry.id)?)?;
        let done = label(".Lentry_witness_done_")?;
        c.jump(done)?;
        c.at(fail)?;
        c.li(10, 25)?;
        let abort = record.entries.iter().find(|e| e.name == "__cellscript_abort").ok_or_else(|| invalid("missing abort target"))?;
        c.jump(entry_start(record, &abort.id)?)?;
        c.at(done)?;
        c.ra(true)?;
        c.adjust(5376)?;
        c.expect(|w| w == 0x00008067)?;
    }
    if owned.iter().any(|b| b.range.start < start || b.range.end > c.address) {
        return Err(invalid("unchecked code in single-entry wrapper"));
    }
    Ok(BTreeMap::from([(0, Variant { action: entry.name.clone(), parameters: params })]))
}

fn normalize(c: &mut Cursor<'_>, fail: u64, label: &impl Fn(&str) -> Result<u64, CheckerError>) -> Result<(), CheckerError> {
    c.load(5, 2, 0, false)?;
    c.li(6, 16)?;
    c.b(5, 6, 6, fail)?;
    c.i(28, 2, 8)?;
    c.load(14, 28, 0, false)?;
    c.shift(6, 14, 32, false)?;
    c.shift(6, 6, 32, true)?;
    c.b(6, 5, 1, fail)?;
    c.shift(29, 14, 32, true)?;
    c.li(6, 16)?;
    c.b(29, 6, 1, fail)?;
    c.load(14, 28, 8, false)?;
    c.shift(30, 14, 32, false)?;
    c.shift(30, 30, 32, true)?;
    c.b(30, 29, 6, fail)?;
    c.shift(31, 14, 32, true)?;
    c.b(31, 30, 6, fail)?;
    c.b(5, 31, 6, fail)?;
    c.li(29, 16)?;
    c.li(15, 0)?;
    let validate = label(".Lentry_witness_v2_validate_loop_")?;
    let end = label(".Lentry_witness_v2_field_end_ready_")?;
    let next = label(".Lentry_witness_v2_field_done_")?;
    c.at(validate)?;
    c.i(16, 30, 0)?;
    c.b(15, 0, 0, end)?;
    c.i(16, 31, 0)?;
    c.li(10, 1)?;
    c.b(15, 10, 0, end)?;
    c.i(16, 5, 0)?;
    c.at(end)?;
    c.r(11, 16, 29, 0x40000000)?;
    c.b(11, 0, 0, next)?;
    c.li(10, 4)?;
    c.b(11, 10, 6, fail)?;
    c.r(12, 28, 29, 0)?;
    c.u32(6, 12, 7)?;
    c.i(11, 11, -4)?;
    c.b(6, 11, 1, fail)?;
    c.at(next)?;
    c.i(29, 16, 0)?;
    c.i(15, 15, 1)?;
    c.li(10, 3)?;
    c.b(15, 10, 6, validate)?;
    c.r(6, 31, 30, 0x40000000)?;
    c.b(6, 0, 0, fail)?;
    c.i(6, 6, -4)?;
    c.r(29, 28, 30, 0)?;
    c.i(29, 29, 4)?;
    c.i(30, 2, 8)?;
    c.li(7, 0)?;
    let copy = label(".Lentry_witness_v2_copy_loop_")?;
    let copied = label(".Lentry_witness_v2_copy_done_")?;
    c.at(copy)?;
    c.r(31, 7, 6, 0x3000)?;
    c.b(31, 0, 0, copied)?;
    c.r(28, 29, 7, 0)?;
    c.load(31, 28, 0, true)?;
    c.r(28, 30, 7, 0)?;
    c.store(31, 28, 0, true)?;
    c.i(7, 7, 1)?;
    c.jump(copy)?;
    c.at(copied)?;
    c.store(6, 2, 0, false)
}

fn load_witness(c: &mut Cursor<'_>, fail: u64, label: &impl Fn(&str) -> Result<u64, CheckerError>) -> Result<(), CheckerError> {
    let loaded = label(".Lentry_witness_loaded_")?;
    let fallback = label(".Lentry_witness_try_group_output_")?;
    c.syscall(0x0100000000000001, false)?;
    c.b(10, 0, 0, loaded)?;
    c.syscall(0x0100000000000001, true)?;
    c.li(5, 1)?;
    c.b(10, 5, 0, fallback)?;
    c.jump(fail)?;
    c.at(fallback)?;
    c.syscall(0x0100000000000002, false)?;
    c.b(10, 0, 0, loaded)?;
    c.jump(fail)?;
    c.at(loaded)?;
    c.load(5, 2, 0, false)?;
    c.li(6, 4097)?;
    c.r(7, 5, 6, 0x3000)?;
    let buffer = label(".Lentry_witness_buffer_ok_")?;
    c.b(7, 0, 1, buffer)?;
    c.jump(fail)?;
    c.at(buffer)?;
    normalize(c, fail, label)?;
    Ok(())
}
