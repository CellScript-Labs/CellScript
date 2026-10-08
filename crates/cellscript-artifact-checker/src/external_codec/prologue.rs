//! Actual initial reception of every scalar source parameter.
use super::*;
use crate::checker::{entry_start, is_addi, is_sd};
use crate::{ParsedElf, TypedSemanticEntry, TypedSemanticRecord, VerifiedLoweringRecord};

fn word(elf: &ParsedElf, address: u64) -> Result<u32, CheckerError> {
    elf.instructions
        .binary_search_by_key(&address, |instruction| instruction.address)
        .map(|index| elf.instructions[index].word)
        .map_err(|_| invalid("missing actual ABI reception instruction"))
}

pub(super) fn check(
    entry: &TypedSemanticEntry,
    typed: &TypedSemanticRecord,
    lowering: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    fields: Option<&CheckedFixedCellScalarFields>,
) -> Result<(), CheckerError> {
    let parameters = crate::entry_codec::parameters(entry, typed)?;
    let frame = lowering
        .entries
        .iter()
        .find(|owner| owner.id == entry.id)
        .ok_or_else(|| invalid("missing external machine owner"))?
        .frame_size_bytes;
    let start = entry_start(lowering, &entry.id)?;
    if !(16..=2047).contains(&frame)
        || !is_addi(word(elf, start)?, 2, 2, -(frame as i32))
        || !is_sd(word(elf, start + 4)?, 1, 2, frame as i32 - 8)
        || !is_sd(word(elf, start + 8)?, 8, 2, frame as i32 - 16)
        || !is_addi(word(elf, start + 12)?, 8, 2, frame as i32)
    {
        return Err(invalid("external entry lacks its actual compact frame"));
    }
    let cells = fields.map(|fields| fields.storage().cells()).unwrap_or_default();
    let scalar_end = cells.iter().filter(|cell| cell.entry == entry.id).map(|cell| cell.size_offset).min().unwrap_or(frame - 16);
    let mut source_ids = BTreeSet::new();
    if entry.locals.len() > 256
        || entry.locals.iter().any(|local| {
            !source_ids.insert(local.source_id)
                || local.source_id.checked_mul(8).and_then(|slot| slot.checked_add(8)).is_none_or(|end| end > u64::from(scalar_end))
        })
    {
        return Err(invalid("external local source IDs duplicate or escape their scalar frame"));
    }
    let mut expected = Vec::new();
    let mut slots = BTreeSet::new();
    for (parameter, transport) in entry.params.iter().zip(&parameters) {
        let local = entry
            .locals
            .iter()
            .find(|local| local.id == parameter.binding_id)
            .ok_or_else(|| invalid("parameter lacks a checked source local"))?;
        let slot = local.source_id.checked_mul(8).ok_or_else(|| invalid("source slot overflow"))?;
        if !slots.insert(slot) {
            return Err(invalid("parameter source slots overlap"));
        }
        expected.push((10 + transport.abi_index, slot));
        if transport.payload_offset.is_none() {
            if transport.abi_arguments != 2 {
                return Err(invalid("additional runtime pointer ABI lacks this profile"));
            }
            let cell = cells
                .iter()
                .find(|cell| cell.entry == entry.id && cell.local_id == parameter.binding_id)
                .ok_or_else(|| invalid("runtime parameter lacks direct fixed Cell reception"))?;
            if !slots.insert(u64::from(cell.size_offset)) {
                return Err(invalid("Cell size slot overlaps another ABI parameter"));
            }
            expected.push((11 + transport.abi_index, u64::from(cell.size_offset)));
        } else if parameter.reference
            || !matches!(parameter.ty.as_str(), "u8" | "u16" | "u32" | "u64" | "i32")
            || transport.abi_arguments != 1
        {
            return Err(invalid("this initial external reception profile requires scalar witness values"));
        }
    }
    if expected.len() > 8 {
        return Err(invalid("external reception exceeds eight registers"));
    }
    for (index, (register, slot)) in expected.iter().enumerate() {
        if !is_sd(word(elf, start + 16 + 4 * index as u64)?, *register, 2, *slot as i32) {
            return Err(invalid("actual ABI register spill differs from its source parameter slot"));
        }
    }
    // No hidden result or extra register spill may overlap the exact unit ABI.
    let following = word(elf, start + 16 + 4 * expected.len() as u64)?;
    if following & 0x7f == 0x23 && ((following >> 15) & 31) == 2 && (10..=17).contains(&((following >> 20) & 31)) {
        return Err(invalid("unexpected extra external argument spill"));
    }
    let owned = lowering.blocks.iter().filter(|block| block.owner_entry == entry.id).collect::<Vec<_>>();
    let prologue_end = start + 16 + 4 * expected.len() as u64;
    if elf.control_flow.iter().any(|flow| {
        owned.iter().any(|block| block.range.contains(flow.target))
            && ((start < flow.target && flow.target < prologue_end)
                || (flow.target == start && owned.iter().any(|block| block.range.contains(flow.address))))
    }) {
        return Err(invalid("incoming flow bypasses or repeats external parameter reception"));
    }
    Ok(())
}
