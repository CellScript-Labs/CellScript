//! A bounded recognizer over decoded RV64 words, never compiler assembly text.
use super::*;

pub(crate) struct Cursor<'a> {
    pub pc: u64,
    pub end: u64,
    pub(super) words: &'a BTreeMap<u64, u32>,
    pub(super) targets: &'a BTreeMap<u64, u64>,
    labels: Vec<Option<u64>>,
    edges: Vec<(usize, u64)>,
}

impl<'a> Cursor<'a> {
    pub fn new(pc: u64, end: u64, words: &'a BTreeMap<u64, u32>, targets: &'a BTreeMap<u64, u64>) -> Self {
        Self { pc, end, words, targets, labels: Vec::new(), edges: Vec::new() }
    }

    fn word(&mut self) -> Result<u32, CheckerError> {
        if self.pc >= self.end {
            return Err(invalid("truncated verification body"));
        }
        let word = *self.words.get(&self.pc).ok_or_else(|| invalid("missing instruction"))?;
        self.pc += 4;
        Ok(word)
    }

    pub fn exact(&mut self, expected: u32) -> Result<(), CheckerError> {
        let at = self.pc;
        let actual = self.word()?;
        if actual != expected {
            return Err(invalid(format!("instruction at {at:#x}: expected {expected:#010x}, got {actual:#010x}")));
        }
        Ok(())
    }

    pub fn immediate(&mut self, opcode: u32, kind: u32, rd: u32, base: u32, value: i32) -> Result<(), CheckerError> {
        if !(-2048..=2047).contains(&value) {
            return Err(invalid("instruction immediate outside the finite profile"));
        }
        self.exact(((value as u32 & 0xfff) << 20) | (base << 15) | (kind << 12) | (rd << 7) | opcode)
    }

    pub fn addi(&mut self, rd: u32, base: u32, value: i32) -> Result<(), CheckerError> {
        self.immediate(0x13, 0, rd, base, value)
    }

    pub fn add(&mut self, rd: u32, left: u32, right: u32) -> Result<(), CheckerError> {
        self.exact((right << 20) | (left << 15) | (rd << 7) | 0x33)
    }

    pub fn sub(&mut self, rd: u32, left: u32, right: u32) -> Result<(), CheckerError> {
        self.exact((0x20 << 25) | (right << 20) | (left << 15) | (rd << 7) | 0x33)
    }

    pub fn u32_le(&mut self, rd: u32, base: u32, offset: u32, scratch: u32) -> Result<(), CheckerError> {
        self.load(rd, base, offset, true)?;
        for byte in 1..4 {
            self.load(scratch, base, offset + byte, true)?;
            self.immediate(0x13, 1, scratch, scratch, byte as i32 * 8)?;
            self.bit_or(rd, rd, scratch)?;
        }
        Ok(())
    }

    pub fn bit_or(&mut self, rd: u32, left: u32, right: u32) -> Result<(), CheckerError> {
        self.exact((right << 20) | (left << 15) | (6 << 12) | (rd << 7) | 0x33)
    }

    /// Only a bounded, register-local constant program can satisfy a policy
    /// literal. In particular, incoming register contents are not constants.
    pub fn constant(&mut self, register: u32, expected: u64) -> Result<(), CheckerError> {
        if (-2048..=2047).contains(&(expected as i64)) {
            return self.addi(register, 0, expected as i32);
        }
        let mut value = None;
        for _ in 0..16 {
            let word = self.word()?;
            if word >> 7 & 31 != register {
                return Err(invalid("constant writes a different register"));
            }
            let base = word >> 15 & 31;
            let source: u64 = if word & 0x7f == 0x37 || base == 0 {
                0
            } else if base == register {
                value.ok_or_else(|| invalid("constant reads an uninitialized register"))?
            } else {
                return Err(invalid("constant depends on a runtime register"));
            };
            let actual = match word & 0x707f {
                0x13 => source.wrapping_add(((word as i32) >> 20) as u64),
                0x1b => source.wrapping_add(((word as i32) >> 20) as u64) as i32 as i64 as u64,
                0x1013 if word >> 26 == 0 => source << (word >> 20 & 63),
                0x5013 if word >> 26 == 0 => source >> (word >> 20 & 63),
                _ if word & 0x7f == 0x37 => (word & 0xfffff000) as i32 as i64 as u64,
                _ => return Err(invalid("unsupported constant materialization")),
            };
            if actual == expected {
                return Ok(());
            }
            value = Some(actual);
        }
        Err(invalid("constant instruction budget exceeded"))
    }

    pub fn load(&mut self, rd: u32, base: u32, offset: u32, byte: bool) -> Result<(), CheckerError> {
        self.immediate(3, if byte { 4 } else { 3 }, rd, base, offset as i32)
    }

    pub fn store(&mut self, src: u32, base: u32, offset: u32, byte: bool) -> Result<(), CheckerError> {
        if offset > 2047 {
            return Err(invalid("store outside the direct addressing profile"));
        }
        self.exact(((offset >> 5) << 25) | (src << 20) | (base << 15) | (if byte { 0 } else { 3 } << 12) | ((offset & 31) << 7) | 0x23)
    }

    pub fn stack_load(&mut self, rd: u32, offset: u32) -> Result<(), CheckerError> {
        if offset > 2047 {
            let scratch = if rd == 31 { 30 } else { 31 };
            self.stack_address(scratch, offset)?;
            return self.load(rd, scratch, 0, false);
        }
        let load = (offset << 20) | (2 << 15) | (3 << 12) | (rd << 7) | 3;
        if self.words.get(&self.pc) != Some(&load) && !self.targets.values().any(|target| *target == self.pc) {
            // O1–O3 can forward one immediately preceding SD. Prove that store
            // from decoded bytes and forbid alternate entry at the reload.
            if let Some(previous) = self.pc.checked_sub(4).and_then(|pc| self.words.get(&pc)).copied() {
                let displacement = ((previous >> 25) << 5) | (previous >> 7 & 31);
                if previous & 0x707f == 0x3023 && previous >> 15 & 31 == 2 && displacement == offset {
                    let source = previous >> 20 & 31;
                    if source == rd {
                        return Ok(());
                    }
                    return self.addi(rd, source, 0);
                }
            }
        }
        self.load(rd, 2, offset, false)
    }

    pub fn stack_store(&mut self, src: u32, offset: u32) -> Result<(), CheckerError> {
        if offset <= 2047 {
            self.store(src, 2, offset, false)
        } else {
            let scratch = if src == 31 { 30 } else { 31 };
            self.stack_address(scratch, offset)?;
            self.store(src, scratch, 0, false)
        }
    }

    pub fn stack_byte(&mut self, register: u32, offset: u32, store: bool) -> Result<(), CheckerError> {
        let (base, offset) = if offset <= 2047 {
            (2, offset)
        } else {
            let scratch = if register == 31 { 30 } else { 31 };
            self.stack_address(scratch, offset)?;
            (scratch, 0)
        };
        if store {
            self.store(register, base, offset, true)
        } else {
            self.load(register, base, offset, true)
        }
    }

    pub fn stack_address(&mut self, rd: u32, offset: u32) -> Result<(), CheckerError> {
        if offset <= 2047 {
            self.addi(rd, 2, offset as i32)
        } else {
            self.constant(rd, u64::from(offset))?;
            self.add(rd, 2, rd)
        }
    }

    pub fn stack_adjust(&mut self, delta: i64) -> Result<(), CheckerError> {
        if (-2048..=2047).contains(&delta) {
            self.addi(2, 2, delta as i32)
        } else {
            self.constant(31, delta as u64)?;
            self.add(2, 2, 31)
        }
    }

    pub fn label(&mut self) -> usize {
        let index = self.labels.len();
        self.labels.push(None);
        index
    }

    pub fn target(&mut self, address: u64) -> usize {
        let index = self.labels.len();
        self.labels.push(Some(address));
        index
    }

    pub fn bind(&mut self, label: usize) -> Result<(), CheckerError> {
        if self.labels[label].replace(self.pc).is_some() {
            return Err(invalid("duplicate logical instruction boundary"));
        }
        Ok(())
    }

    pub fn branch(&mut self, kind: u32, left: u32, right: u32, label: usize) -> Result<(), CheckerError> {
        let at = self.pc;
        let word = self.word()?;
        if word & 0x7f != 0x63 || word >> 12 & 7 != kind || word >> 15 & 31 != left || word >> 20 & 31 != right {
            return Err(invalid(format!("branch predicate differs at {at:#x}")));
        }
        let target = *self.targets.get(&at).ok_or_else(|| invalid("missing decoded branch target"))?;
        self.edges.push((label, target));
        Ok(())
    }

    pub fn jump_target(&mut self) -> Result<u64, CheckerError> {
        let at = self.pc;
        if self.word()? & 0xfff != 0x6f {
            return Err(invalid("expected a direct non-call jump"));
        }
        self.targets.get(&at).copied().ok_or_else(|| invalid("missing jump target"))
    }

    pub fn jump(&mut self, label: usize) -> Result<(), CheckerError> {
        let target = self.jump_target()?;
        self.edges.push((label, target));
        Ok(())
    }

    pub fn syscall(&mut self) -> Result<(), CheckerError> {
        self.exact(0x73)
    }

    pub fn call(&mut self, expected: u64) -> Result<(), CheckerError> {
        let start = self.pc;
        let word = self.word()?;
        let call = if word & 0xfff == 0xef {
            start
        } else if word & 0xfff == 0x97 {
            let low = self.word()?;
            // The ELF decoder has already resolved this exact adjacent pair.
            // Relaxed-branch normalization relocates its decoded edge, while
            // keeping the original PC-relative immediate bits unchanged.
            if low & 0xfffff != 0x080e7 {
                return Err(invalid("call address materialization differs"));
            }
            start + 4
        } else {
            return Err(invalid("expected a linked direct call"));
        };
        if self.targets.get(&call) != Some(&expected) {
            return Err(invalid("call target differs"));
        }
        Ok(())
    }

    pub fn finish(&self) -> Result<(), CheckerError> {
        if self.pc != self.end || self.edges.iter().any(|(label, target)| self.labels[*label] != Some(*target)) {
            return Err(invalid("verification extent or control-flow target differs"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant_program(program: &[u32], expected: u64) -> Result<(), CheckerError> {
        let words = program.iter().enumerate().map(|(index, word)| (index as u64 * 4, *word)).collect();
        let targets = BTreeMap::new();
        let mut cursor = Cursor::new(0, program.len() as u64 * 4, &words, &targets);
        cursor.constant(6, expected)?;
        cursor.finish()
    }

    #[test]
    fn low_masks_require_initialized_register_local_logical_shifts() {
        // ADDI t1,zero,-1; SRLI t1,t1,n independently materializes low masks.
        for shift in 1..=52 {
            let logical = (shift << 20) | 0x35313;
            let expected = u64::MAX >> shift;
            assert!(constant_program(&[0xfff00313, logical], expected).is_ok());
            assert!(constant_program(&[logical], expected).is_err());
            assert!(constant_program(&[0xfff00313, logical ^ (1 << 15)], expected).is_err());
            assert!(constant_program(&[0xfff00313, logical | (1 << 30)], expected).is_err());
            assert!(constant_program(&[0xfff00313, logical | (1 << 26)], expected).is_err());
            assert!(constant_program(&[0xfff00313, logical], expected ^ 1).is_err());
        }
    }
}
