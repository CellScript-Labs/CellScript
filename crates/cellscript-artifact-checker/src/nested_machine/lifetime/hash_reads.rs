//! Read-only fixed inputs for bounded hash and C256 summaries. These facts prove
//! memory extent and caller ownership, not the source computation of the bytes.
use super::*;

pub(super) fn parameters(record: &VerifiedLoweringRecord, owner: &str) -> Result<Vec<(u32, i64)>, CheckerError> {
    let Some(entry) = record.typed_semantics.entries.iter().find(|entry| entry.id == owner) else { return Ok(Vec::new()) };
    let mut word = 0u32;
    let mut parameters = Vec::new();
    for param in &entry.params {
        let ty = crate::checker::canonical_abi_type(&param.ty);
        // u128 uses the same independently checked pointer/exact-length ABI;
        // it is never a scalar register containing an address by assumption.
        let fixed_bytes = matches!(ty.as_str(), "u128" | "hash" | "address" | "ScriptHash" | "SighashAllDigest")
            || ty.starts_with("[u8;")
            || record.typed_semantics.types.iter().any(|layout| layout.name == param.ty && layout.kind == "struct");
        let words = crate::fixed_results::parameter_word_width(record, entry, param);
        if fixed_bytes
            && words == 2
            && let Some(width) = crate::checker::committed_state_type_width(&param.ty, &record.typed_semantics.types)
            && width > 8
        {
            parameters.push((word, i64::try_from(width).map_err(|_| invalid("fixed input width overflows"))?));
        }
        word = word.checked_add(words).ok_or_else(|| invalid("fixed input ABI overflows"))?;
    }
    Ok(parameters)
}

impl Proof<'_> {
    pub(super) fn hash_read(&mut self, owner: &str, pointer: Value, width: i64, frame: i64, pc: u64) -> Result<(), CheckerError> {
        if width < 0 {
            return Err(invalid("hash read has a negative extent"));
        }
        if width == 0 {
            return Ok(());
        }
        let valid = match pointer {
            Value::Stack(start) => start >= 0 && start.checked_add(width).is_some_and(|end| end <= frame - 16),
            Value::Constant(start) => u64::try_from(start).ok().is_some_and(|start| {
                start >= self.elf.rodata.address
                    && start.checked_add(width as u64).is_some_and(|end| end <= self.elf.rodata.range().end)
            }),
            Value::FixedPointer(word, extent, offset) if offset >= 0 && offset.checked_add(width).is_some_and(|end| end <= extent) => {
                self.fixed_reads.entry(owner.to_string()).or_default().insert(word, extent);
                true
            }
            _ => false,
        };
        if !valid {
            return Err(invalid(format!("hash input {pointer:?} + {width} lacks a caller-owned read extent at {pc:#x}")));
        }
        Ok(())
    }

    pub(super) fn hash_arguments(
        &mut self,
        owner: &str,
        callee: &str,
        state: &State,
        frame: i64,
        pc: u64,
    ) -> Result<(), CheckerError> {
        let Some(arguments) = self.fixed_reads.get(callee).cloned() else { return Ok(()) };
        let incoming = |word: u32| {
            if word < 8 {
                state.registers[10 + word as usize]
            } else if let Value::Stack(sp) = state.registers[2] {
                state.memory.get(&(sp + i64::from(word - 8) * 8)).copied().unwrap_or(Value::Unknown)
            } else {
                Value::Unknown
            }
        };
        for (word, width) in arguments {
            let pointer = incoming(word);
            let length = incoming(word + 1);
            if length != Value::Constant(width)
                && !matches!((pointer, length), (Value::FixedPointer(source, extent, 0), Value::FixedLength(length_source, length_extent))
                    if source == length_source && extent == width && length_extent == width)
            {
                return Err(invalid(format!("hash borrowing call has a changed fixed input length at {pc:#x}")));
            }
            self.hash_read(owner, pointer, width, frame, pc)?;
        }
        Ok(())
    }
}
