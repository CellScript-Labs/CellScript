//! Complete typed table loads with an owned buffer for each field result.
use super::*;
use crate::nested_machine::cells;

// The marker coordinates, typed operation and decoded machine are independent inputs.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    entry: &TypedSemanticEntry,
    operation: &TypedSemanticOperation,
    mut c: Cursor<'_>,
    output: u32,
    abort: u64,
    storage: Storage,
    mut effect: Effect,
) -> Result<Effect, CheckerError> {
    let schema = operation.operands[0]
        .ty
        .strip_prefix("CellDataView<")
        .and_then(|ty| ty.strip_suffix('>'))
        .ok_or_else(|| invalid("nested data field has no schema"))?;
    let types = record.typed_semantics.types.iter().map(|ty| (ty.name.as_str(), ty)).collect();
    let ty = record.typed_semantics.types.iter().find(|ty| ty.name == schema).ok_or_else(|| invalid("unknown data schema"))?;
    let (fields, capacity) = cells::layout(ty, &types)?;
    if capacity > 32768 || ty.encoded_size.is_some() {
        return Err(invalid("nested data table exceeds its profile"));
    }
    let TypedSemanticOperationDetail::Field { name } = &operation.detail else { return Err(invalid("missing field name")) };
    let field = ty.fields.iter().find(|field| field.name == *name).ok_or_else(|| invalid("unknown table field"))?;
    let index = field.molecule_field_index.ok_or_else(|| invalid("missing table field index"))?;
    let field_kind = fields.get(index as usize).ok_or_else(|| invalid("field index exceeds table"))?;
    let lowering = record.entries.iter().find(|lowering| lowering.id == entry.id).ok_or_else(|| invalid("missing data owner"))?;
    let frame = crate::nested_machine::owned_frame(record, elf, lowering)?;
    let scalar_end = entry
        .locals
        .iter()
        .map(|local| local.source_id.checked_add(1).and_then(|id| id.checked_mul(8)))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid("data scalar frame overflows"))?
        .into_iter()
        .max()
        .unwrap_or(0);
    let size = storage.size;
    let buffer = size.checked_add(8).ok_or_else(|| invalid("data buffer overflows"))?;
    let end = buffer.checked_add(capacity).ok_or_else(|| invalid("data capacity overflows"))?;
    if u64::from(size) < scalar_end || end > frame.saturating_sub(16) || !size.is_multiple_of(8) {
        return Err(invalid("nested data buffer overlaps scalar or saved storage"));
    }
    effect.writes.push((size, 8 + capacity));
    match field_kind {
        cells::Field::Values(_) => {
            let length = storage.length;
            if output != 0
                || u64::from(length) < scalar_end
                || length.checked_add(8).is_none_or(|end| end > size)
                || !length.is_multiple_of(8)
            {
                return Err(invalid("nested data result has an invalid length slot"));
            }
            effect.writes.push((length, 8));
            effect.nested = Some(NestedResult { root: u64::from(effect.destination / 8), length, buffer: (size, 8 + capacity) });
        }
        cells::Field::Fixed(width) => {
            if storage.length != 0 {
                return Err(invalid("fixed data field has an unexpected length slot"));
            }
            if *width > 8 {
                if u64::from(output) < scalar_end || output.checked_add(*width).is_none_or(|end| end > size) {
                    return Err(invalid("fixed data result overlaps its input buffer or scalar storage"));
                }
                effect.writes.push((output, *width));
                effect.result_buffer = Some((output, *width));
            } else if output != 0 {
                return Err(invalid("scalar table field has an unexpected result buffer"));
            }
        }
    }
    let invalid_source = c.label();
    let direct = c.label();
    let decoded = c.label();
    let failed = c.label();
    let done = c.label();
    c.stack_load(10, effect.source)?;
    c.immediate(0x13, 5, 5, 10, 32)?;
    c.immediate(0x13, 1, 6, 10, 32)?;
    c.immediate(0x13, 5, 6, 6, 32)?;
    c.branch(0, 5, 0, invalid_source)?;
    c.constant(30, 4)?;
    c.branch(7, 30, 5, direct)?;
    c.constant(30, 6)?;
    c.branch(6, 30, 5, invalid_source)?;
    c.addi(7, 5, -4)?;
    c.constant(30, 1 << 56)?;
    c.bit_or(7, 7, 30)?;
    c.jump(decoded)?;
    c.bind(direct)?;
    c.addi(7, 5, 0)?;
    c.bind(decoded)?;
    for tag in [3, 4] {
        c.constant(5, tag)?;
        c.branch(0, 7, 5, invalid_source)?;
    }
    c.addi(13, 6, 0)?;
    c.addi(14, 7, 0)?;
    c.constant(5, u64::from(capacity))?;
    c.stack_store(5, size)?;
    c.stack_address(10, buffer)?;
    c.stack_address(11, size)?;
    c.constant(12, 0)?;
    c.constant(17, 2092)?;
    c.exact(0x73)?;
    c.branch(1, 10, 0, failed)?;
    c.pc = cells::check_selected_table(record, c.pc, c.words, c.targets, &fields, capacity, size, buffer, frame)?;
    c.stack_address(29, buffer)?;
    if matches!(field_kind, cells::Field::Values(_)) && index as usize + 1 == fields.len() {
        c.stack_load(11, size)?;
    }
    c.u32_le(30, 29, 4 * (index + 1), 7)?;
    match field_kind {
        cells::Field::Values(_) => {
            if index as usize + 1 < fields.len() {
                c.u32_le(31, 29, 4 * (index + 2), 7)?;
            } else {
                c.addi(31, 11, 0)?;
            }
            c.sub(6, 31, 30)?;
            c.stack_store(6, storage.length)?;
            c.add(5, 29, 30)?;
        }
        cells::Field::Fixed(width) => {
            c.add(29, 29, 30)?;
            if *width > 8 {
                for byte in 0..*width {
                    c.load(5, 29, byte, true)?;
                    c.stack_byte(5, output + byte, true)?;
                }
                c.stack_address(5, output)?;
            } else {
                c.constant(5, 0)?;
                for byte in 0..*width {
                    c.load(6, 29, byte, true)?;
                    if byte > 0 {
                        c.immediate(0x13, 1, 6, 6, (byte * 8) as i32)?;
                    }
                    c.bit_or(5, 5, 6)?;
                }
            }
        }
    }
    c.stack_store(5, effect.destination)?;
    c.jump(done)?;
    for (label, code) in [(invalid_source, 44), (failed, 3)] {
        c.bind(label)?;
        c.constant(10, code)?;
        if c.jump_target()? != abort {
            return Err(invalid("nested data load bypasses its fatal sink"));
        }
    }
    c.bind(done)?;
    c.finish()?;
    Ok(effect)
}
