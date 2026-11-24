//! Complete fixed Cell data field bodies and their bounded write effect.
use super::{cursor::Cursor, invalid, reads};
use crate::{
    CheckerError, ParsedElf, TypedSemanticConstant, TypedSemanticEntry, TypedSemanticOperation, TypedSemanticOperationDetail,
    VerifiedLoweringRecord,
};

mod nested;

#[derive(Clone, Copy)]
pub(super) struct Storage {
    pub size: u32,
    pub length: u32,
}

pub(crate) struct NestedResult {
    pub root: u64,
    pub length: u32,
    pub buffer: (u32, u32),
}

pub(crate) struct Effect {
    pub end: u64,
    pub source: u32,
    pub destination: u32,
    pub projection: bool,
    pub array_bytes: Option<u32>,
    pub writes: Vec<(u32, u32)>,
    pub result_buffer: Option<(u32, u32)>,
    pub nested: Option<NestedResult>,
}

pub(super) fn is_operation(operation: &TypedSemanticOperation) -> bool {
    operation.opcode == "field-access"
        && operation.operands.first().is_some_and(|operand| {
            operand.ty.starts_with("CellDataView<")
                || (matches!(&operation.detail, TypedSemanticOperationDetail::Field { name } if name == "data")
                    && (operand.ty.starts_with("InputView<") || operand.ty.starts_with("OutputView<")))
        })
}

// Marker storage is checked against the owned frame and the complete machine body.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    entry: &TypedSemanticEntry,
    operation: &TypedSemanticOperation,
    mut c: Cursor<'_>,
    output: u32,
    abort: u64,
    storage: Option<Storage>,
) -> Result<Effect, CheckerError> {
    let operand = operation.operands.first().ok_or_else(|| invalid("Cell data read lacks its typed input"))?;
    let source = reads::slot(entry, operand.local.ok_or_else(|| invalid("Cell data read requires a local view"))?)?;
    let destination = reads::slot(entry, *operation.destinations.first().ok_or_else(|| invalid("Cell data read has no result"))?)?;
    let mut effect = Effect {
        end: c.end,
        source,
        destination,
        projection: !operand.ty.starts_with("CellDataView<"),
        array_bytes: None,
        writes: Vec::new(),
        result_buffer: None,
        nested: None,
    };
    if effect.projection {
        if output != 0 || storage.is_some() {
            return Err(invalid("Cell data projection cannot supply a result buffer"));
        }
        c.stack_load(5, source)?;
        c.stack_store(5, destination)?;
        c.finish()?;
        return Ok(effect);
    }
    let name = operand
        .ty
        .strip_prefix("CellDataView<")
        .and_then(|name| name.strip_suffix('>'))
        .ok_or_else(|| invalid("Cell data read lacks its schema"))?;
    let ty = record.typed_semantics.types.iter().find(|ty| ty.name == name).ok_or_else(|| invalid("missing Cell data schema"))?;
    let TypedSemanticOperationDetail::Field { name } = &operation.detail else { return Err(invalid("missing Cell data field")) };
    let field = ty.fields.iter().find(|field| field.name == *name).ok_or_else(|| invalid("unknown Cell data field"))?;
    if let Some(storage) = storage {
        return nested::validate(record, elf, entry, operation, c, output, abort, storage, effect);
    }
    let total = ty.encoded_size.filter(|total| (1..=512).contains(total)).ok_or_else(|| invalid("unbounded Cell data layout"))?;
    let width = field
        .width_bytes
        .filter(|width| *width > 0 && field.offset.checked_add(*width).is_some_and(|end| end <= total))
        .ok_or_else(|| invalid("Cell data field exceeds schema"))?;
    let (frame, scalar_end) = owned_frame(record, elf, entry)?;
    // The emitter's two 512-byte scratch slots each have an eight-byte length.
    // Recognize that physical contract from the allocated frame and every use.
    let size = frame.checked_sub(16 + 2 * (8 + 512)).ok_or_else(|| invalid("Cell data scratch exceeds frame"))?;
    let buffer = size + 8;
    if size < scalar_end || destination + 8 > scalar_end {
        return Err(invalid("Cell data scratch overlaps scalar slots"));
    }
    effect.writes.push((size, 8 + total));
    let types = record.typed_semantics.types.iter().map(|ty| (ty.name.as_str(), ty)).collect();
    let records = crate::cell_data_view::record_array_layout(&field.ty, &types).is_some();
    if width > 8 || records {
        let end = output.checked_add(width).ok_or_else(|| invalid("Cell data result overflows"))?;
        if output < scalar_end || end > size {
            return Err(invalid("Cell data result overlaps scalar, scratch or saved storage"));
        }
        effect.writes.push((output, width));
        effect.result_buffer = Some((output, width));
    } else if output != 0 {
        return Err(invalid("scalar Cell data result has an unexpected buffer"));
    }
    let invalid_source = c.label();
    let direct = c.label();
    let decoded = c.label();
    let failed = c.label();
    let wrong_size = c.label();
    let done = c.label();
    c.stack_load(10, source)?;
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
    c.constant(5, u64::from(total))?;
    c.stack_store(5, size)?;
    c.stack_address(10, buffer)?;
    c.stack_address(11, size)?;
    c.constant(12, 0)?;
    c.constant(17, 2092)?;
    c.exact(0x73)?;
    c.branch(1, 10, 0, failed)?;
    c.stack_load(5, size)?;
    c.constant(6, u64::from(total))?;
    c.branch(1, 5, 6, wrong_size)?;
    // Older artifacts copy and decode one byte at a time. Recognize the
    // aligned word form only when its complete first load is present.
    let word_load = width >= 8 && field.offset.is_multiple_of(8) && c.clone().stack_load(5, buffer + field.offset).is_ok();
    if width > 8 || records {
        let words = if word_load { width / 8 } else { 0 };
        if words > 0 && !output.is_multiple_of(8) {
            return Err(invalid("fixed record array storage is not aligned"));
        }
        for word in 0..words {
            c.stack_load(5, buffer + field.offset + word * 8)?;
            c.stack_store(5, output + word * 8)?;
        }
        for byte in words * 8..width {
            c.stack_byte(5, buffer + field.offset + byte, false)?;
            c.stack_byte(5, output + byte, true)?;
        }
        c.stack_address(5, output)?;
    } else if word_load {
        c.stack_load(5, buffer + field.offset)?;
    } else {
        c.constant(5, 0)?;
        for byte in 0..width {
            c.stack_byte(6, buffer + field.offset + byte, false)?;
            if byte != 0 {
                c.immediate(0x13, 1, 6, 6, (byte * 8) as i32)?;
            }
            c.bit_or(5, 5, 6)?;
        }
    }
    c.stack_store(5, destination)?;
    c.jump(done)?;
    for (label, code) in [(invalid_source, 44), (failed, 3), (wrong_size, 16)] {
        c.bind(label)?;
        c.constant(10, code)?;
        if c.jump_target()? != abort {
            return Err(invalid("Cell data failure bypasses the fatal sink"));
        }
    }
    c.bind(done)?;
    c.finish()?;
    Ok(effect)
}

fn owned_frame(record: &VerifiedLoweringRecord, elf: &ParsedElf, entry: &TypedSemanticEntry) -> Result<(u32, u32), CheckerError> {
    let lowering = record.entries.iter().find(|lowering| lowering.id == entry.id).ok_or_else(|| invalid("missing Cell data owner"))?;
    let prologue =
        record.blocks.iter().find(|block| block.id == lowering.entry_block).ok_or_else(|| invalid("missing Cell data prologue"))?;
    let frame = elf
        .stack_adjustments
        .iter()
        .find(|adjustment| prologue.range.contains(adjustment.address))
        .filter(|adjustment| adjustment.delta < 0)
        .and_then(|adjustment| u32::try_from(adjustment.delta.unsigned_abs()).ok())
        .filter(|frame| *frame <= lowering.frame_size_bytes)
        .ok_or_else(|| invalid("Cell data read has no owned frame"))?;
    let scalar_end = match entry.locals.iter().map(|local| local.source_id).max() {
        Some(last) => last
            .checked_add(1)
            .and_then(|id| id.checked_mul(8))
            .and_then(|size| u32::try_from(size).ok())
            .ok_or_else(|| invalid("Cell data scalar slots overflow"))?,
        None => 0,
    };
    Ok((frame, scalar_end))
}

/// The array's stride and count are reconstructed from the nominal record.
/// The enclosing memory checker proves the array pointer's live frame extent.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate_record_index(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    entry: &TypedSemanticEntry,
    operation: &TypedSemanticOperation,
    mut c: Cursor<'_>,
    output: u32,
    abort: u64,
    (count, width, bytes): (u32, u32, u32),
) -> Result<Effect, CheckerError> {
    let [array, index] = operation.operands.as_slice() else { return Err(invalid("record index must have two operands")) };
    let source = reads::slot(entry, array.local.ok_or_else(|| invalid("record index requires a local array"))?)?;
    let destination = reads::slot(entry, *operation.destinations.first().ok_or_else(|| invalid("record index has no result"))?)?;
    let (frame, scalar_end) = owned_frame(record, elf, entry)?;
    if output < scalar_end
        || !output.is_multiple_of(8)
        || output.checked_add(width).is_none_or(|end| end > frame.saturating_sub(16 + 2 * (8 + 512)))
        || destination + 8 > scalar_end
    {
        return Err(invalid("indexed record result overlaps scalar, scratch or saved storage"));
    }
    c.stack_load(29, source)?;
    if !matches!(index.ty.as_str(), "u8" | "u16" | "u32" | "u64") {
        return Err(invalid("record index is not an unsigned machine word"));
    }
    if let Some(local) = index.local {
        c.stack_load(6, reads::slot(entry, local)?)?;
    } else if let Some(
        TypedSemanticConstant::U8(value)
        | TypedSemanticConstant::U16(value)
        | TypedSemanticConstant::U32(value)
        | TypedSemanticConstant::U64(value),
    ) = &index.constant
    {
        c.constant(6, value.parse().map_err(|_| invalid("invalid record index literal"))?)?;
    } else {
        return Err(invalid("missing record index value"));
    }
    c.constant(7, u64::from(count))?;
    let in_bounds = c.label();
    c.branch(6, 6, 7, in_bounds)?;
    c.constant(10, 2)?;
    if c.jump_target()? != abort {
        return Err(invalid("record index failure bypasses the fatal sink"));
    }
    c.bind(in_bounds)?;
    c.constant(7, u64::from(width))?;
    // mul t1, t1, t2
    c.exact((1 << 25) | (7 << 20) | (6 << 15) | (6 << 7) | 0x33)?;
    c.add(29, 29, 6)?;
    if width.is_multiple_of(8) {
        for offset in (0..width).step_by(8) {
            c.load(5, 29, offset, false)?;
            c.stack_store(5, output + offset)?;
        }
    } else {
        for byte in 0..width {
            c.load(5, 29, byte, true)?;
            c.stack_byte(5, output + byte, true)?;
        }
    }
    c.stack_address(5, output)?;
    c.stack_store(5, destination)?;
    c.finish()?;
    Ok(Effect {
        end: c.end,
        source,
        destination,
        projection: false,
        array_bytes: Some(bytes),
        writes: vec![(output, width)],
        result_buffer: Some((output, width)),
        nested: None,
    })
}

/// Copied fields have private storage for the invocation. Check every data
/// producer together: recognizing each copy in isolation cannot rule out a
/// coherent marker/instruction rewrite that aliases another result or load.
pub(super) fn validate_result_storage(
    effect: &std::collections::BTreeMap<String, std::collections::BTreeMap<u64, Effect>>,
) -> Result<(), CheckerError> {
    for sites in effect.values() {
        let mut regions = sites
            .values()
            .flat_map(|effect| {
                effect.writes.iter().map(|&(start, bytes)| (start, start + bytes, effect.result_buffer == Some((start, bytes))))
            })
            .collect::<Vec<_>>();
        regions.sort_unstable();
        let mut previous_end = 0;
        let mut copied_end = 0;
        for (start, end, copied) in regions {
            // Reusing transient load scratch is allowed. A copied result must
            // not overlap any other data producer's write footprint, even if
            // that producer has not executed yet or lies on another branch.
            if start < copied_end || (copied && start < previous_end) {
                return Err(invalid("copied Cell data results overlap another data read's storage"));
            }
            previous_end = previous_end.max(end);
            if copied {
                copied_end = copied_end.max(end);
            }
        }
    }
    Ok(())
}
