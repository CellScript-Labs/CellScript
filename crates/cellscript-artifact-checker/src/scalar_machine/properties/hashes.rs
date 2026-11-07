//! Hash property transport and private, distinct 32-byte result storage.
use super::*;

/// Discover a proposed large stack displacement from a finite constant program.
/// `Cursor::stack_store` subsequently verifies the complete addressing sequence;
/// discovery alone grants no write or instruction claim.
fn proposed_offset(words: &BTreeMap<u64, u32>, mut pc: u64) -> Result<u32, CheckerError> {
    let mut value = None::<u64>;
    for _ in 0..16 {
        let word = *words.get(&pc).ok_or_else(|| invalid("hash size address is truncated"))?;
        if word == (31 << 20) | (2 << 15) | (31 << 7) | 0x33 {
            return value.and_then(|value| u32::try_from(value).ok()).ok_or_else(|| invalid("hash size offset overflows"));
        }
        if word >> 7 & 31 != 31 {
            return Err(invalid("hash size constant writes another register"));
        }
        value = Some(
            crate::machine::constant_step(word, 31, value, crate::machine::ConstantInstructions::LogicalShifts)
                .ok_or_else(|| invalid("unsupported or uninitialized hash size constant"))?,
        );
        pc += 4;
    }
    Err(invalid("hash size address exceeds its instruction budget"))
}

pub(super) fn size(c: &mut Cursor<'_>, words: &BTreeMap<u64, u32>) -> Result<u32, CheckerError> {
    c.constant(5, 32)?;
    let word = *words.get(&c.pc).ok_or_else(|| invalid("hash size store missing"))?;
    let offset = if word & 0x707f == 0x3023 && word >> 15 & 31 == 2 {
        ((word >> 25) << 5) | (word >> 7 & 31)
    } else {
        proposed_offset(words, c.pc)?
    };
    c.stack_store(5, offset)?;
    Ok(offset)
}

pub(super) fn private_storage(size: u32, locals: &BTreeMap<u32, &TypedSemanticLocal>, frame: u32) -> Result<(), CheckerError> {
    let scalar_end = locals
        .values()
        .map(|local| local.source_id.checked_add(1).and_then(|id| id.checked_mul(8)))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid("hash scalar region overflows"))?
        .into_iter()
        .max()
        .unwrap_or(0);
    if !size.is_multiple_of(8) || u64::from(size) < scalar_end || size.checked_add(40).is_none_or(|end| end > frame.saturating_sub(16))
    {
        return Err(invalid("hash property storage overlaps scalars or leaves its private frame"));
    }
    Ok(())
}

pub(super) fn call(
    c: &mut Cursor<'_>,
    machine: &Machine<'_>,
    source: &TypedSemanticLocal,
    destination: &TypedSemanticLocal,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
    frame: u32,
    property: Property,
) -> Result<(), CheckerError> {
    let size = size(c, machine.words)?;
    private_storage(size, locals, frame)?;
    c.stack_load(10, slot(source, frame)?)?;
    c.stack_address(11, size + 8)?;
    c.stack_address(12, size)?;
    c.call(*machine.properties.get(&property).ok_or_else(|| invalid("hash property runtime was not checked"))?)?;
    let ok = c.label();
    c.branch(0, 10, 0, ok)?;
    let abort = c.target(machine.abort);
    c.jump(abort)?;
    c.bind(ok)?;
    c.stack_address(5, size + 8)?;
    c.stack_store(5, slot(destination, frame)?)?;
    c.finish()
}
