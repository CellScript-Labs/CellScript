//! Independent complete instruction contract for the existing four-way byte
//! cache. Lifetime analysis separately proves initialization, caller ownership,
//! protected storage and the invariant of retained pointers before every call.
use super::*;

pub(super) const ENTRY_BYTES: i64 = 312;
pub(super) const WAYS: i64 = 4;

pub(super) struct Checked {
    pub hot: bool,
    pub wrappers: BTreeSet<String>,
}

impl Checked {
    pub fn header(&self) -> i64 {
        if self.hot {
            72
        } else {
            24
        }
    }
    pub fn bytes(&self) -> i64 {
        self.header() + WAYS * ENTRY_BYTES
    }
}

fn wrapper(owner: &str) -> Option<(u32, u32)> {
    Some(match owner {
        "runtime:__ckb_cell_data_u8" => (1, 0),
        "runtime:__ckb_cell_data_u32_le" => (4, 0),
        "runtime:__ckb_cell_data_u64_le" => (8, 0),
        "runtime:__ckb_witness_u8" => (1, 1),
        "runtime:__ckb_witness_u32_le" => (4, 1),
        "runtime:__ckb_witness_u64_le" => (8, 1),
        "runtime:__ckb_cell_lock_u8" => (1, 2),
        "runtime:__ckb_cell_type_u8" => (1, 3),
        _ => return None,
    })
}

fn extent(record: &VerifiedLoweringRecord, owner: &str) -> Result<(u64, u64), CheckerError> {
    let mut ranges = record.blocks.iter().filter(|block| block.owner_entry == owner).map(|block| &block.range).collect::<Vec<_>>();
    ranges.sort_by_key(|range| range.start);
    if ranges.is_empty() || ranges.windows(2).any(|pair| pair[0].end != pair[1].start) {
        return Err(invalid("cache helper has missing or non-contiguous machine ownership"));
    }
    Ok((ranges[0].start, ranges.last().unwrap().end))
}

fn finish(c: &Decoder<'_>, elf: &ParsedElf, start: u64, end: u64) -> Result<(), CheckerError> {
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("exact-read cache extent or control-flow target differs"));
    }
    if elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address)) {
        return Err(invalid("control flow enters a partial cache helper"));
    }
    Ok(())
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, abort: Option<u64>) -> Result<Option<Checked>, CheckerError> {
    let wrappers = record.entries.iter().filter(|entry| wrapper(&entry.id).is_some()).collect::<Vec<_>>();
    if wrappers.is_empty() {
        return Ok(None);
    }
    let abort = abort.ok_or_else(|| invalid("cache has no independently checked fatal sink"))?;
    let (start, end) = extent(record, "runtime:__cellscript_exact_read_cached")?;
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect::<BTreeMap<_, _>>();
    let hot = words.get(&start).is_some_and(|word| word & 0x7f == 0x63);
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    c.cache_body(hot, abort)?;
    finish(&c, elf, start, end)?;
    let mut checked = Checked { hot, wrappers: BTreeSet::new() };
    for entry in wrappers {
        let (width, kind) = wrapper(&entry.id).unwrap();
        let (begin, end) = extent(record, &entry.id)?;
        let mut c = Decoder { pc: begin, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
        c.cache_wrapper(width, kind, hot, start)?;
        finish(&c, elf, begin, end)?;
        checked.wrappers.insert(entry.id.clone());
    }
    Ok(Some(checked))
}

impl Decoder<'_> {
    fn cache_store(&mut self, source: u32, base: u32, offset: u32) -> Result<(), CheckerError> {
        self.exact(((offset >> 5) << 25) | (source << 20) | (base << 15) | (3 << 12) | ((offset & 31) << 7) | 0x23)
    }
    fn cache_load(&mut self, register: u32, base: u32, offset: i32) -> Result<(), CheckerError> {
        self.immediate(3, 3, register, base, offset)
    }
    fn cache_jump(&mut self, label: u8) -> Result<(), CheckerError> {
        let target = self.jump()?;
        self.branches.push((Label::Cache(label), target));
        Ok(())
    }
    fn cache_branch(&mut self, kind: u32, left: u32, right: u32, label: u8) -> Result<(), CheckerError> {
        self.branch(kind, left, right, Label::Cache(label))
    }
    fn cache_label(&mut self, label: u8) {
        self.label(Label::Cache(label));
    }
    fn cache_hot_hit(&mut self, miss: u8, width_register: u32) -> Result<(), CheckerError> {
        self.cache_branch(0, 26, 0, miss)?;
        self.cache_branch(1, 25, 10, miss)?;
        self.cache_branch(1, 24, 14, miss)?;
        self.cache_branch(6, 11, 23, miss)?;
        self.sub(29, 11, 23)?;
        self.cache_branch(6, 22, 29, miss)?;
        self.sub(6, 22, 29)?;
        self.cache_branch(6, 6, width_register, miss)
    }
    fn cache_hit(&mut self, miss: u8) -> Result<(), CheckerError> {
        self.cache_load(5, 16, 24)?;
        self.cache_branch(0, 5, 0, miss)?;
        self.cache_load(5, 16, 0)?;
        self.cache_branch(1, 5, 10, miss)?;
        self.cache_load(5, 16, 32)?;
        self.cache_branch(1, 5, 14, miss)?;
        self.cache_load(5, 16, 8)?;
        self.cache_branch(6, 11, 5, miss)?;
        self.sub(29, 11, 5)?;
        self.cache_load(6, 16, 16)?;
        self.cache_branch(6, 6, 29, miss)?;
        self.sub(6, 6, 29)?;
        self.cache_branch(6, 6, 13, miss)
    }
    fn cache_refresh(&mut self) -> Result<(), CheckerError> {
        self.addi(26, 16, 0)?;
        self.cache_load(25, 16, 0)?;
        self.cache_load(24, 16, 32)?;
        self.cache_load(23, 16, 8)?;
        self.cache_load(22, 16, 16)?;
        self.addi(21, 16, 56)
    }
    fn cache_decode_source(&mut self, invalid: u8) -> Result<(), CheckerError> {
        self.immediate(0x13, 5, 5, 10, 32)?;
        self.immediate(0x13, 1, 6, 10, 32)?;
        self.immediate(0x13, 5, 6, 6, 32)?;
        self.cache_branch(0, 5, 0, invalid)?;
        self.li(30, 4)?;
        self.cache_branch(7, 30, 5, 40)?;
        self.li(30, 6)?;
        self.cache_branch(6, 30, 5, invalid)?;
        self.addi(7, 5, -4)?;
        self.constant(30, 0x0100_0000_0000_0000)?;
        self.register(6, 0, 7, 7, 30)?;
        self.cache_jump(41)?;
        self.cache_label(40);
        self.addi(7, 5, 0)?;
        self.cache_label(41);
        Ok(())
    }
    fn cache_failure(&mut self, code: u32, abort: u64) -> Result<(), CheckerError> {
        self.li(10, code)?;
        if self.jump()? != abort {
            return Err(invalid("cache failure bypasses checked fatal sink"));
        }
        Ok(())
    }
    fn cache_body(&mut self, hot: bool, abort: u64) -> Result<(), CheckerError> {
        // Labels 0..19 are the fixed CFG; 20..23 are the four way misses;
        // 40..41 belong to the independent source-tag decoder.
        if hot {
            self.cache_hot_hit(0, 13)?;
            self.addi(16, 26, 0)?;
            self.cache_jump(12)?;
        } else {
            self.cache_load(16, 12, 8)?;
            self.cache_branch(0, 16, 0, 0)?;
            self.cache_hit(0)?;
            self.cache_jump(12)?;
        }
        let header = if hot { 72 } else { 24 };
        self.cache_label(0);
        for way in 0..4 {
            self.addi(16, 12, header + way * 312)?;
            self.cache_hit(20 + way as u8)?;
            self.cache_store(16, 12, 8)?;
            if hot {
                self.addi(26, 16, 0)?;
                self.addi(25, 10, 0)?;
                self.addi(24, 14, 0)?;
                self.cache_load(23, 16, 8)?;
                self.cache_load(22, 16, 16)?;
                self.addi(21, 16, 56)?;
            }
            self.cache_jump(12)?;
            self.cache_label(20 + way as u8);
        }
        self.cache_jump(1)?;
        self.cache_label(1);
        self.cache_load(5, 12, 0)?;
        self.addi(6, 5, 1)?;
        self.li(7, 3)?;
        self.register(7, 0, 6, 6, 7)?;
        self.cache_store(6, 12, 0)?;
        self.li(7, 312)?;
        self.register(0, 1, 6, 5, 7)?;
        self.add(16, 12, 6)?;
        self.addi(16, 16, header)?;
        for (register, offset) in [(0, 24), (10, 0), (11, 8), (14, 32), (13, 40), (12, 48)] {
            self.cache_store(register, 16, offset)?;
        }
        self.cache_decode_source(16)?;
        for source in [1, 2, 0x0100_0000_0000_0001, 0x0100_0000_0000_0002] {
            self.constant(5, source)?;
            self.cache_branch(0, 5, 7, 2)?;
        }
        self.li(5, 3)?;
        self.cache_branch(1, 5, 7, 16)?;
        self.cache_load(5, 16, 32)?;
        self.li(28, 1)?;
        self.cache_branch(0, 5, 28, 16)?;
        self.cache_label(2);
        self.li(5, 256)?;
        self.cache_store(5, 16, 16)?;
        self.addi(10, 16, 56)?;
        self.addi(11, 16, 16)?;
        self.cache_load(12, 16, 8)?;
        self.addi(13, 6, 0)?;
        self.addi(14, 7, 0)?;
        self.cache_load(5, 16, 32)?;
        self.cache_branch(0, 5, 0, 3)?;
        for (kind, label) in [(1, 4), (2, 5), (3, 6)] {
            self.li(28, kind)?;
            self.cache_branch(0, 5, 28, label)?;
        }
        self.cache_jump(16)?;
        self.cache_label(3);
        self.constant(17, 2092)?;
        self.cache_jump(7)?;
        self.cache_label(4);
        self.constant(17, 2074)?;
        self.cache_jump(7)?;
        self.cache_label(5);
        self.li(15, 2)?;
        self.constant(17, 2081)?;
        self.cache_jump(7)?;
        self.cache_label(6);
        self.li(15, 4)?;
        self.constant(17, 2081)?;
        self.cache_label(7);
        self.exact(0x73)?;
        self.cache_branch(0, 10, 0, 8)?;
        self.li(5, 3)?;
        self.cache_branch(1, 10, 5, 17)?;
        self.cache_label(8);
        self.cache_load(5, 16, 16)?;
        self.li(6, 256)?;
        self.cache_branch(6, 5, 6, 9)?;
        self.addi(5, 6, 0)?;
        self.cache_label(9);
        self.cache_store(5, 16, 16)?;
        self.cache_load(13, 16, 40)?;
        self.cache_branch(6, 5, 13, 17)?;
        self.li(6, 1)?;
        self.cache_store(6, 16, 24)?;
        self.cache_load(7, 16, 48)?;
        self.cache_store(16, 7, 8)?;
        if hot {
            self.cache_refresh()?;
        }
        self.li(29, 0)?;
        self.cache_label(12);
        self.addi(5, 16, 56)?;
        self.add(5, 5, 29)?;
        self.li(6, 1)?;
        self.cache_branch(0, 13, 6, 13)?;
        self.li(6, 4)?;
        self.cache_branch(0, 13, 6, 14)?;
        self.cache_jump(15)?;
        self.cache_label(13);
        self.immediate(3, 4, 10, 5, 0)?;
        self.cache_jump(19)?;
        for (label, width) in [(14, 4), (15, 8)] {
            self.cache_label(label);
            self.li(10, 0)?;
            for byte in 0..width {
                self.immediate(3, 4, 6, 5, byte)?;
                if byte != 0 {
                    self.immediate(0x13, 1, 6, 6, byte * 8)?;
                }
                self.register(6, 0, 10, 10, 6)?;
            }
            self.cache_jump(19)?;
        }
        self.cache_label(19);
        self.li(11, 0)?;
        self.exact(0x00008067)?;
        self.cache_label(16);
        self.cache_failure(44, abort)?;
        self.cache_label(17);
        self.cache_load(5, 16, 32)?;
        self.cache_branch(1, 5, 0, 18)?;
        self.cache_failure(3, abort)?;
        self.cache_label(18);
        self.cache_failure(1, abort)
    }
    fn cache_wrapper(&mut self, width: u32, kind: u32, hot: bool, target: u64) -> Result<(), CheckerError> {
        if hot && width <= 4 {
            self.cache_branch(0, 26, 0, 0)?;
            self.cache_branch(1, 25, 10, 0)?;
            self.li(5, kind)?;
            self.cache_branch(1, 24, 5, 0)?;
            self.cache_branch(6, 11, 23, 0)?;
            self.sub(29, 11, 23)?;
            self.cache_branch(6, 22, 29, 0)?;
            self.sub(6, 22, 29)?;
            self.li(7, width)?;
            self.cache_branch(6, 6, 7, 0)?;
            self.add(5, 21, 29)?;
            self.immediate(3, 4, 10, 5, 0)?;
            for byte in 1..width as i32 {
                self.immediate(3, 4, 6, 5, byte)?;
                self.immediate(0x13, 1, 6, 6, byte * 8)?;
                self.register(6, 0, 10, 10, 6)?;
            }
            self.li(11, 0)?;
            self.exact(0x00008067)?;
            self.cache_label(0);
        }
        self.li(13, width)?;
        self.li(14, kind)?;
        if self.jump()? != target {
            return Err(invalid("exact-read wrapper tail target differs"));
        }
        Ok(())
    }
}
