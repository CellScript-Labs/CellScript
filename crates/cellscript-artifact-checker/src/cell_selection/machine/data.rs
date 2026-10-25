//! Complete fixed Cell data field bodies and their bounded write effects.
use super::{cursor::Cursor, invalid, reads};
use crate::{
    CheckerError, ParsedElf, TypedSemanticEntry, TypedSemanticOperation, TypedSemanticOperationDetail, VerifiedLoweringRecord,
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
    pub writes: Vec<(u32, u32)>,
    pub result_pointer: Option<u32>,
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
        writes: Vec::new(),
        result_pointer: None,
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
    // The emitter's two 512-byte scratch slots each have an eight-byte length.
    // Recognize that physical contract from the allocated frame and every use.
    let size = frame.checked_sub(16 + 2 * (8 + 512)).ok_or_else(|| invalid("Cell data scratch exceeds frame"))?;
    let buffer = size + 8;
    let scalar_end = entry
        .locals
        .iter()
        .map(|local| local.source_id.checked_add(1).and_then(|id| id.checked_mul(8)))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid("Cell data scalar slots overflow"))?
        .into_iter()
        .max()
        .unwrap_or(0);
    if u64::from(size) < scalar_end || u64::from(destination) + 8 > scalar_end {
        return Err(invalid("Cell data scratch overlaps scalar slots"));
    }
    effect.writes.push((size, 8 + total));
    if width > 8 {
        let end = output.checked_add(width).ok_or_else(|| invalid("Cell data result overflows"))?;
        if u64::from(output) < scalar_end || end > size {
            return Err(invalid("Cell data result overlaps scalar, scratch or saved storage"));
        }
        effect.writes.push((output, width));
        effect.result_pointer = Some(output);
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
    if width > 8 {
        for byte in 0..width {
            c.stack_byte(5, buffer + field.offset + byte, false)?;
            c.stack_byte(5, output + byte, true)?;
        }
        c.stack_address(5, output)?;
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
