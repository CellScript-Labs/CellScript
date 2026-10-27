//! Exact finite inline byte copies into independently known private storage.
//! This proves write confinement, not the provenance of the copied source.
use super::*;

impl Proof<'_> {
    pub(super) fn private_byte_copy(
        &self,
        state: &mut State,
        pc: u64,
        floor: i64,
        frame: i64,
        protected: &[(i64, i64)],
    ) -> Result<Option<u64>, CheckerError> {
        if !self.words.get(&pc).is_some_and(|word| word & 0x707f == 0x4003 && word >> 20 == 0) {
            return Ok(None);
        }
        let Some(code) = (0..6).map(|index| self.words.get(&(pc + index * 4)).copied()).collect::<Option<Vec<_>>>() else {
            return Ok(None);
        };
        if code[0] & 0x707f != 0x4003 || code[0] >> 20 != 0 || code[1] & 0x707f != 0x23 {
            return Ok(None);
        }
        let temp = code[0] >> 7 & 31;
        let source = code[0] >> 15 & 31;
        let destination = code[1] >> 15 & 31;
        let counter = code[4] >> 7 & 31;
        let registers = BTreeSet::from([temp, source, destination, counter]);
        if registers.len() != 4 || registers.contains(&0) || registers.contains(&2) {
            return Ok(None);
        }
        let addi = |register: u32, delta: i32| ((delta as u32 & 0xfff) << 20) | (register << 15) | (register << 7) | 0x13;
        if code[1] != ((temp << 20) | (destination << 15) | 0x23)
            || code[2] != addi(source, 1)
            || code[3] != addi(destination, 1)
            || code[4] != addi(counter, -1)
            || code[5] & 0x01fff07f != ((counter << 15) | 0x1063)
            || self.targets.get(&(pc + 20)) != Some(&pc)
        {
            return Ok(None);
        }
        let Value::Constant(count) = state.registers[counter as usize] else { return Ok(None) };
        if count <= 0 || count > frame {
            return Err(invalid("inline copy has no positive private-frame bound"));
        }
        if self.elf.control_flow.iter().any(|edge| pc < edge.target && edge.target < pc + 24) || self.owner(pc) != self.owner(pc + 20)
        {
            return Err(invalid("control flow enters a partial inline copy"));
        }
        let start = write(state.registers[destination as usize], count, floor, frame - 16, protected, pc)?;
        state.forget(start, count);
        state.registers[destination as usize] = Value::Stack(start + count);
        state.registers[source as usize] = state.registers[source as usize].add(Value::Constant(count));
        state.registers[counter as usize] = Value::Constant(0);
        state.registers[temp as usize] = Value::Unknown;
        Ok(Some(pc + 24))
    }
}
