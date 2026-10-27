//! Bind typed Input/Output view properties to their coordinate, finite runtime
//! body and published scalar. Runtime recognition is independent of codegen.
use super::*;
use std::collections::BTreeSet;

mod hashes;

#[derive(Clone, Copy)]
pub(crate) struct CallSource {
    pub source: i64,
    pub start: u64,
    pub end: u64,
    pub hash_region: Option<(i64, i64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Property {
    Capacity,
    Occupied,
    Unoccupied,
    DataSize,
    OutputIndex,
    DataHash,
    LockHash,
    TypeHash,
}

impl Property {
    fn name(self) -> &'static str {
        match self {
            Self::Capacity => "__ckb_cell_capacity",
            Self::Occupied => "__ckb_cell_occupied_capacity",
            Self::Unoccupied => "__ckb_cell_unoccupied_capacity",
            Self::DataSize => "__ckb_cell_data_size",
            Self::OutputIndex => "__ckb_cell_output_index",
            Self::DataHash => "__ckb_cell_data_hash_field",
            Self::LockHash => "__ckb_cell_lock_hash",
            Self::TypeHash => "__ckb_cell_type_hash",
        }
    }

    fn hash_field(self) -> Option<u32> {
        match self {
            Self::DataHash => Some(1),
            Self::LockHash => Some(3),
            Self::TypeHash => Some(5),
            _ => None,
        }
    }
}

pub(super) fn property(op: &TypedSemanticOperation) -> Option<Property> {
    let call = op.call.as_ref()?;
    if !op.operands.iter().any(|operand| operand.ty.starts_with("InputView<") || operand.ty.starts_with("OutputView<")) {
        return None;
    }
    [
        Property::Capacity,
        Property::Occupied,
        Property::Unoccupied,
        Property::DataSize,
        Property::OutputIndex,
        Property::DataHash,
        Property::LockHash,
        Property::TypeHash,
    ]
    .into_iter()
    .find(|property| property.name() == call.target)
}

/// Required live coordinates at actual call PCs. This inventory is consumed
/// before scalar validation by the lifetime pass. It grants no write effect or
/// atomic body: the scalar pass must still recognize every byte and path of
/// each marked call, as well as the complete runtime implementation.
pub(crate) fn call_sources(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
) -> Result<BTreeMap<String, BTreeMap<u64, CallSource>>, CheckerError> {
    if !record
        .typed_semantics
        .entries
        .iter()
        .filter(|entry| covered_entry(entry))
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .any(|op| property(op).is_some())
    {
        return Ok(BTreeMap::new());
    }
    let mut markers = BTreeMap::new();
    for block in &record.blocks {
        for label in &block.allocated_lifecycle_labels {
            let Some(suffix) = label.strip_prefix(PREFIX) else { continue };
            let parts = suffix.split('_').collect::<Vec<_>>();
            if parts.len() != 4 || !matches!(parts[3], "begin" | "end") {
                return Err(invalid("invalid scalar property boundary"));
            }
            let source_block = parts[0].parse::<u32>().map_err(|_| invalid("invalid property block"))?;
            let operation = parts[1].parse::<u32>().map_err(|_| invalid("invalid property operation"))?;
            if markers.insert((block.owner_entry.as_str(), source_block, operation, parts[3]), block.range.start).is_some() {
                return Err(invalid("duplicate scalar property boundary"));
            }
        }
    }
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect::<BTreeMap<_, _>>();
    let calls = elf
        .control_flow
        .iter()
        .filter(|edge| words.get(&edge.address).is_some_and(|word| word >> 7 & 31 == 1 && matches!(word & 0x7f, 0x6f | 0x67)))
        .map(|edge| (edge.address, edge.target))
        .collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut sites = BTreeMap::<String, BTreeMap<u64, CallSource>>::new();
    for entry in record.typed_semantics.entries.iter().filter(|entry| covered_entry(entry)) {
        let locals = entry.locals.iter().map(|local| (local.id, local)).collect::<BTreeMap<_, _>>();
        let frame = record
            .entries
            .iter()
            .find(|machine| machine.id == entry.id)
            .ok_or_else(|| invalid("property owner has no frame"))?
            .frame_size_bytes;
        let mut hash_regions = Vec::new();
        for block in &entry.blocks {
            for op in &block.operations {
                let Some(property) = property(op) else { continue };
                let boundary = |edge| {
                    markers
                        .get(&(entry.id.as_str(), block.id, op.index, edge))
                        .copied()
                        .ok_or_else(|| invalid("missing typed property boundary"))
                };
                let (start, end) = (boundary("begin")?, boundary("end")?);
                if start >= end || end - start > 4096 {
                    return Err(invalid("invalid property call extent"));
                }
                let mut matches = calls.range(start..end);
                let (&pc, _) = matches.next().ok_or_else(|| invalid("property has no machine call"))?;
                if matches.next().is_some() {
                    return Err(invalid("property has multiple machine calls"));
                }
                let source = op
                    .operands
                    .first()
                    .and_then(|operand| operand.local)
                    .and_then(|id| locals.get(&id))
                    .ok_or_else(|| invalid("property has no coordinate local"))?;
                let slot = source
                    .source_id
                    .checked_mul(8)
                    .and_then(|slot| i64::try_from(slot).ok())
                    .ok_or_else(|| invalid("property coordinate slot overflows"))?;
                let hash_region = if property.hash_field().is_some() {
                    let mut cursor = Cursor::new(start, end, &words, &targets);
                    let size = hashes::size(&mut cursor, &words)?;
                    hashes::private_storage(size, &locals, frame)?;
                    let region = (i64::from(size), i64::from(size) + 40);
                    if hash_regions.iter().any(|&(start, end)| region.0 < end && start < region.1) {
                        return Err(invalid("distinct hash property results overlap"));
                    }
                    hash_regions.push(region);
                    Some(region)
                } else {
                    None
                };
                if sites
                    .entry(entry.id.clone())
                    .or_default()
                    .insert(pc, CallSource { source: slot, start, end, hash_region })
                    .is_some()
                {
                    return Err(invalid("property call belongs to multiple source operations"));
                }
            }
        }
    }
    Ok(sites)
}

pub(super) fn call(
    c: &mut Cursor<'_>,
    machine: &Machine<'_>,
    op: &TypedSemanticOperation,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
    frame: u32,
    property: Property,
) -> Result<(), CheckerError> {
    let ([source], [destination]) = (op.operands.as_slice(), op.destinations.as_slice()) else {
        return Err(invalid("Cell property has an invalid call shape"));
    };
    let source_local = source.local.and_then(|id| locals.get(&id)).ok_or_else(|| invalid("Cell property has no coordinate"))?;
    let destination = locals.get(destination).ok_or_else(|| invalid("Cell property has no destination"))?;
    if source_local.ty != source.ty
        || destination.ty != if property.hash_field().is_some() { "hash" } else { "u64" }
        || (property == Property::OutputIndex && !source.ty.starts_with("OutputView<"))
    {
        return Err(invalid(format!(
            "Cell property source or result type differs: {}/{}, result {}",
            source_local.ty, source.ty, destination.ty
        )));
    }
    if property.hash_field().is_some() {
        return hashes::call(c, machine, source_local, destination, locals, frame, property);
    }
    c.stack_load(10, slot(source_local, frame)?)?;
    c.call(*machine.properties.get(&property).ok_or_else(|| invalid("Cell property runtime was not checked"))?)?;
    if property != Property::DataSize {
        let ok = c.label();
        c.branch(0, 11, 0, ok)?;
        c.addi(10, 11, 0)?;
        let abort = c.target(machine.abort);
        c.jump(abort)?;
        c.bind(ok)?;
    }
    c.stack_store(10, slot(destination, frame)?)?;
    c.finish()
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    words: &BTreeMap<u64, u32>,
    targets: &BTreeMap<u64, u64>,
    abort: u64,
) -> Result<BTreeMap<Property, u64>, CheckerError> {
    let mut required = record
        .typed_semantics
        .entries
        .iter()
        .filter(|entry| covered_entry(entry))
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .filter_map(property)
        .collect::<BTreeSet<_>>();
    if required.contains(&Property::Unoccupied) {
        required.extend([Property::Capacity, Property::Occupied]);
    }
    let mut ranges = BTreeMap::new();
    for property in required {
        let owner = format!("runtime:{}", property.name());
        let blocks = record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
        let start = blocks.iter().map(|block| block.range.start).min().ok_or_else(|| invalid("Cell property runtime missing"))?;
        let end = blocks.iter().map(|block| block.range.end).max().unwrap();
        if end - start > 2048
            || (start..end).step_by(4).any(|pc| !blocks.iter().any(|block| block.range.contains(pc)))
            || elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address))
        {
            return Err(invalid("Cell property runtime has a foreign entry, gap or excessive extent"));
        }
        ranges.insert(property, (start, end));
    }
    for (&property, &(start, end)) in &ranges {
        if let Some(field) = property.hash_field() {
            crate::nested_machine::validate_hash_property_runtime(record, elf, &format!("runtime:{}", property.name()), field)?;
            continue;
        }
        let mut c = Cursor::new(start, end, words, targets);
        runtime(&mut c, property, &ranges, abort).map_err(|error| invalid(format!("{}: {}", property.name(), error.message)))?;
        c.finish()?;
    }
    Ok(ranges.into_iter().map(|(property, (start, _))| (property, start)).collect())
}

fn decode(c: &mut Cursor<'_>, invalid: usize) -> Result<(), CheckerError> {
    let direct = c.label();
    let done = c.label();
    c.immediate(0x13, 5, 5, 10, 32)?;
    c.immediate(0x13, 1, 6, 10, 32)?;
    c.immediate(0x13, 5, 6, 6, 32)?;
    c.branch(0, 5, 0, invalid)?;
    c.constant(30, 4)?;
    c.branch(7, 30, 5, direct)?;
    c.constant(30, 6)?;
    c.branch(6, 30, 5, invalid)?;
    c.addi(7, 5, -4)?;
    c.constant(30, 0x0100_0000_0000_0000)?;
    c.bit_or(7, 7, 30)?;
    c.jump(done)?;
    c.bind(direct)?;
    c.addi(7, 5, 0)?;
    c.bind(done)
}

fn status(c: &mut Cursor<'_>, code: u64) -> Result<(), CheckerError> {
    c.constant(10, code)?;
    c.addi(11, 10, 0)
}

fn runtime(c: &mut Cursor<'_>, property: Property, ranges: &BTreeMap<Property, (u64, u64)>, abort: u64) -> Result<(), CheckerError> {
    if property == Property::OutputIndex {
        return output_index(c);
    }
    c.addi(2, 2, -48)?;
    c.store(1, 2, 40, false)?;
    if property == Property::Unoccupied {
        unoccupied(c, ranges)?;
    } else {
        field(c, property, abort)?;
    }
    c.load(1, 2, 40, false)?;
    c.addi(2, 2, 48)?;
    c.exact(0x00008067)
}

fn field(c: &mut Cursor<'_>, property: Property, abort: u64) -> Result<(), CheckerError> {
    let invalid = c.label();
    let failed = c.label();
    let done = c.label();
    let malformed = c.label();
    decode(c, invalid)?;
    if property == Property::Occupied {
        c.store(6, 2, 0, false)?;
        c.store(7, 2, 8, false)?;
    }
    c.constant(5, if property == Property::DataSize { 0 } else { 8 })?;
    let size = if property == Property::Occupied { 16 } else { 8 };
    c.store(5, 2, size, false)?;
    c.addi(10, 2, size as i32 + 8)?;
    c.addi(11, 2, size as i32)?;
    c.constant(12, 0)?;
    if property == Property::Occupied {
        c.load(13, 2, 0, false)?;
        c.load(14, 2, 8, false)?;
    } else {
        c.addi(13, 6, 0)?;
        c.addi(14, 7, 0)?;
    }
    if property != Property::DataSize {
        c.constant(15, if property == Property::Occupied { 6 } else { 0 })?;
    }
    c.constant(17, if property == Property::DataSize { 2092 } else { 2081 })?;
    c.syscall()?;
    if property == Property::DataSize {
        let ok = c.label();
        c.constant(5, 3)?;
        c.sub(6, 10, 5)?;
        c.branch(0, 6, 0, ok)?;
        c.branch(0, 10, 0, ok)?;
        c.jump(failed)?;
        c.bind(ok)?;
    } else {
        if property == Property::Occupied {
            c.load(5, 2, 16, false)?;
        }
        c.branch(1, 10, 0, failed)?;
        if property == Property::Occupied {
            c.constant(6, 8)?;
            c.sub(7, 5, 6)?;
            c.branch(1, 7, 0, malformed)?;
        }
    }
    c.load(10, 2, if property == Property::DataSize { size } else { size + 8 }, false)?;
    c.constant(11, 0)?;
    c.jump(done)?;
    c.bind(invalid)?;
    if property == Property::DataSize {
        let abort = c.target(abort);
        c.constant(10, 44)?;
        c.jump(abort)?;
        c.bind(failed)?;
        c.constant(10, 3)?;
        c.jump(abort)?;
    } else {
        status(c, 44)?;
        c.jump(done)?;
        c.bind(failed)?;
        status(c, 44)?;
        if property == Property::Occupied {
            c.jump(done)?;
            c.bind(malformed)?;
            status(c, 34)?;
        }
    }
    c.bind(done)
}

fn unoccupied(c: &mut Cursor<'_>, ranges: &BTreeMap<Property, (u64, u64)>) -> Result<(), CheckerError> {
    let failed = c.label();
    let status_ok = c.label();
    let underflow = c.label();
    let done = c.label();
    c.store(10, 2, 32, false)?;
    c.call(ranges[&Property::Capacity].0)?;
    c.branch(1, 11, 0, failed)?;
    c.store(10, 2, 24, false)?;
    c.load(10, 2, 32, false)?;
    c.call(ranges[&Property::Occupied].0)?;
    c.branch(1, 11, 0, failed)?;
    c.load(5, 2, 24, false)?;
    c.exact((10 << 20) | (5 << 15) | (3 << 12) | (6 << 7) | 0x33)?;
    c.branch(1, 6, 0, underflow)?;
    c.sub(10, 5, 10)?;
    c.constant(11, 0)?;
    c.jump(done)?;
    c.bind(failed)?;
    c.addi(10, 11, 0)?;
    c.branch(1, 10, 0, status_ok)?;
    c.constant(10, 44)?;
    c.bind(status_ok)?;
    c.addi(11, 10, 0)?;
    c.jump(done)?;
    c.bind(underflow)?;
    status(c, 20)?;
    c.bind(done)
}

fn output_index(c: &mut Cursor<'_>) -> Result<(), CheckerError> {
    let invalid = c.label();
    let output = c.label();
    let done = c.label();
    c.constant(31, 1 << 32)?;
    c.exact((1 << 25) | (31 << 20) | (10 << 15) | (4 << 12) | (5 << 7) | 0x33)?;
    c.exact((1 << 25) | (31 << 20) | (10 << 15) | (6 << 12) | (6 << 7) | 0x33)?;
    for tag in [2, 6] {
        c.constant(30, tag)?;
        c.sub(29, 5, 30)?;
        c.branch(0, 29, 0, output)?;
    }
    c.jump(invalid)?;
    c.bind(output)?;
    c.addi(10, 6, 0)?;
    c.constant(11, 0)?;
    c.jump(done)?;
    c.bind(invalid)?;
    status(c, 44)?;
    c.bind(done)?;
    c.exact(0x00008067)
}
