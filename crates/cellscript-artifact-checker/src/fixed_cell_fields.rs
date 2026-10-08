//! Optional direct unsigned Cell field materialization evidence.
use crate::checker::{entry_start, is_addi, is_lbu, is_ld, is_or, is_slli};
use crate::fixed_cell_storage::CheckedFixedCellParameterStorage;
use crate::{CheckerBudgets, CheckerError, CheckerRejectionCode, ParsedElf, TypedSemanticOperationDetail, VerifiedLoweringRecord};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
mod helpers;

#[derive(Debug)]
pub struct CheckedFixedCellScalarFields {
    storage: CheckedFixedCellParameterStorage,
    record: Record,
    identity: String,
}
impl CheckedFixedCellScalarFields {
    pub fn storage(&self) -> &CheckedFixedCellParameterStorage {
        &self.storage
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        crate::canonical_bytes(&self.record)
    }
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    storage: String,
    fields: Vec<Field>,
}
#[derive(Debug, Serialize)]
struct Field {
    entry: String,
    parameter: String,
    field: String,
    ty: String,
    offset: u32,
    width: u32,
    pointer_load: u64,
    decode_end: u64,
    register: u32,
    native_load: bool,
}
fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("fixed Cell scalar fields: {}", message.into()))
}
fn word(elf: &ParsedElf, address: u64) -> Result<u32, CheckerError> {
    elf.instructions
        .binary_search_by_key(&address, |instruction| instruction.address)
        .map(|index| elf.instructions[index].word)
        .map_err(|_| invalid("missing field instruction"))
}
fn owned_instructions(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    owner: &str,
    limit: usize,
) -> Result<BTreeMap<u64, u32>, CheckerError> {
    let mut result = BTreeMap::new();
    for block in record.blocks.iter().filter(|block| block.owner_entry == owner) {
        let start = elf.instructions.partition_point(|instruction| instruction.address < block.range.start);
        let end = elf.instructions.partition_point(|instruction| instruction.address < block.range.end);
        if result.len().checked_add(end - start).is_none_or(|count| count > limit) {
            return Err(invalid(format!("owner exceeds {limit} instructions")));
        }
        for instruction in &elf.instructions[start..end] {
            if result.insert(instruction.address, instruction.word).is_some() {
                return Err(invalid("duplicate owned instruction"));
            }
        }
    }
    Ok(result)
}
fn reachable_without(
    instructions: &BTreeMap<u64, u32>,
    flows: &BTreeMap<u64, u64>,
    start: u64,
    omitted: Option<u64>,
    returning_calls: &BTreeSet<u64>,
) -> BTreeSet<u64> {
    let mut visited = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(address) = pending.pop() {
        if omitted == Some(address) || !instructions.contains_key(&address) || !visited.insert(address) {
            continue;
        }
        let word = instructions[&address];
        match word & 0x7f {
            0x63 => {
                if let Some(target) = flows.get(&address) {
                    pending.push(*target);
                }
                pending.push(address + 4);
            }
            0x6f | 0x67 if returning_calls.contains(&address) => pending.push(address + 4),
            0x6f | 0x67 if word != 0x00008067 => {
                if let Some(target) = flows.get(&address) {
                    pending.push(*target);
                }
            }
            0x67 => {}
            _ => pending.push(address + 4),
        }
    }
    visited
}
/// Certify direct unsigned field bytes at actual t0/t1 registers. This finite
/// profile rejects unproved aliases/calls, pointer overwrite and field forms;
/// it does not establish source assignments, predicates or complete receipts.
pub fn check_fixed_cell_scalar_fields(
    artifact: &[u8],
    metadata: &[u8],
    lowering: &[u8],
    source_map: &[u8],
    budgets: &CheckerBudgets,
) -> Result<CheckedFixedCellScalarFields, CheckerError> {
    let storage = crate::fixed_cell_storage::check_fixed_cell_parameter_storage(artifact, metadata, lowering, source_map, budgets)?;
    let record: VerifiedLoweringRecord = serde_json::from_slice(lowering).map_err(|error| invalid(error.to_string()))?;
    let elf = crate::parse_elf(artifact, budgets.instructions).map_err(|error| invalid(error.to_string()))?;
    let mut fields = Vec::new();
    let flows = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect::<BTreeMap<_, _>>();
    for cell in storage.cells() {
        let entry =
            record.typed_semantics.entries.iter().find(|entry| entry.id == cell.entry).ok_or_else(|| invalid("missing owner"))?;
        let parameter =
            entry.params.iter().find(|param| param.binding_id == cell.local_id).ok_or_else(|| invalid("missing parameter"))?;
        let layout =
            record.typed_semantics.types.iter().find(|ty| ty.name == parameter.ty).ok_or_else(|| invalid("missing named layout"))?;
        let instructions = owned_instructions(&record, &elf, &cell.entry, 4096)?;
        let start = entry_start(&record, &cell.entry)?;
        if elf
            .control_flow
            .iter()
            .any(|flow| instructions.contains_key(&flow.target) && flow.target != start && !instructions.contains_key(&flow.address))
        {
            return Err(invalid("external flow enters a Cell owner's interior"));
        }
        let frame =
            record.entries.iter().find(|owner| owner.id == cell.entry).ok_or_else(|| invalid("missing frame"))?.frame_size_bytes;
        let mut returning_calls = BTreeSet::new();
        for (&address, &instruction) in &instructions {
            if instruction == 0x00000073
                && !storage.reads().reads().iter().any(|read| read.entry == cell.entry && read.syscall_address == address)
            {
                helpers::check_owner_fixed_observation(&record, &elf, &instructions, &storage, &cell.entry, address, frame)?;
            }
            let opcode = instruction & 0x7f;
            let rd = (instruction >> 7) & 31;
            let rs1 = (instruction >> 15) & 31;
            let immediate = (instruction as i32) >> 20;
            if instruction == 0x00008067 {
                let restore = address.checked_sub(12).ok_or_else(|| invalid("truncated owner return"))?;
                if !is_ld(word(&elf, restore)?, 1, 2, frame as i32 - 8)
                    || !is_ld(word(&elf, restore + 4)?, 8, 2, frame as i32 - 16)
                    || !is_addi(word(&elf, restore + 8)?, 2, 2, frame as i32)
                    || elf.control_flow.iter().any(|flow| {
                        restore < flow.target && flow.target <= address && !(restore <= flow.address && flow.address < address)
                    })
                {
                    return Err(invalid("owner return bypasses saved RA/FP or stack restoration"));
                }
            } else if !matches!(opcode, 0x6f | 0x67) && !instructions.contains_key(&(address + 4)) {
                return Err(invalid("owner falls out of its instruction range"));
            }
            if matches!(opcode, 0x6f | 0x67) && rd != 0 {
                if rd != 1 {
                    return Err(invalid("unsupported call return register"));
                }
                let target = *flows.get(&address).ok_or_else(|| invalid("call lacks actual target"))?;
                helpers::check_membership_frame(&record, &elf, target)?;
                returning_calls.insert(address);
            }
            if opcode == 0x23 {
                if rs1 != 2 {
                    return Err(invalid("unproved store base could alias Cell storage"));
                }
                let offset = (((instruction >> 25) << 5) | ((instruction >> 7) & 31)) as i32;
                let offset = (offset << 20) >> 20;
                let width = 1u32 << ((instruction >> 12) & 7);
                if offset < 0 || width > 8 {
                    return Err(invalid("unsupported frame store"));
                }
                let offset = offset as u32;
                if offset + width > frame {
                    return Err(invalid("body store escapes its owned frame"));
                }
                if offset < frame && frame - 16 < offset + width && address != start + 4 && address != start + 8 {
                    return Err(invalid("body overwrites saved caller RA/FP"));
                }
                if offset < cell.buffer_offset + 512 && cell.buffer_offset < offset + width {
                    return Err(invalid("instruction overwrites checked Cell data"));
                }
                if offset < cell.pointer_offset + 8
                    && cell.pointer_offset < offset + width
                    && address != cell.spill_address
                    && address != cell.receiver_start + 4
                {
                    return Err(invalid("instruction overwrites the received Cell pointer"));
                }
            }
            if matches!(opcode, 0x03 | 0x13 | 0x17 | 0x1b | 0x33 | 0x37 | 0x3b | 0x6f | 0x67) && rd == 1 {
                let call = matches!(opcode, 0x6f | 0x67) && returning_calls.contains(&address);
                let call_prefix = opcode == 0x17
                    && instructions.contains_key(&(address + 4))
                    && word(&elf, address + 4)? & 0x7f == 0x67
                    && ((word(&elf, address + 4)? >> 7) & 31) == 1;
                let restore = is_ld(instruction, 1, 2, frame as i32 - 8)
                    && is_ld(word(&elf, address + 4)?, 8, 2, frame as i32 - 16)
                    && is_addi(word(&elf, address + 8)?, 2, 2, frame as i32)
                    && word(&elf, address + 12)? == 0x00008067;
                if !call && !call_prefix && !restore {
                    return Err(invalid("body substitutes its caller return address"));
                }
            }
            if matches!(opcode, 0x13 | 0x1b | 0x33 | 0x3b | 0x03 | 0x37 | 0x17)
                && rd == 2
                && !(address == start && is_addi(instruction, 2, 2, -(frame as i32)))
                && !(is_addi(instruction, 2, 2, frame as i32) && word(&elf, address + 4)? == 0x00008067)
            {
                return Err(invalid("body changes the checked stack base"));
            }
            if opcode == 0x13
                && ((instruction >> 12) & 7) == 0
                && rs1 == 2
                && immediate >= cell.buffer_offset as i32
                && immediate < (cell.buffer_offset + 512) as i32
                && address != cell.receiver_start
                && !storage
                    .reads()
                    .reads()
                    .iter()
                    .any(|read| read.entry == cell.entry && read.setup_start <= address && address < read.guarded_end)
            {
                return Err(invalid("unproved direct alias of the Cell buffer"));
            }
        }
        let mut required = BTreeMap::new();
        for operation in entry.blocks.iter().flat_map(|block| &block.operations) {
            if operation.opcode != "field-access" {
                continue;
            }
            let operand = operation.operands.first().ok_or_else(|| invalid("missing field owner"))?;
            if operand.local != Some(cell.local_id) {
                return Err(invalid("indirect or non-parameter field access lacks this profile"));
            }
            let TypedSemanticOperationDetail::Field { name } = &operation.detail else {
                return Err(invalid("missing field name"));
            };
            let field =
                layout.fields.iter().find(|field| field.name == *name).ok_or_else(|| invalid("missing checked field layout"))?;
            let width = match field.ty.as_str() {
                "u8" => 1,
                "u16" => 2,
                "u32" => 4,
                "u64" => 8,
                _ => return Err(invalid("signed, nested or wide field lacks this profile")),
            };
            if field.width_bytes != Some(width) {
                return Err(invalid("unsigned field width differs"));
            }
            required.insert(field.offset, (name.as_str(), field.ty.as_str(), width));
        }
        let reached = reachable_without(&instructions, &flows, start, None, &returning_calls);
        let bypass = reachable_without(&instructions, &flows, start, Some(cell.receiver_start + 4), &returning_calls);
        let mut covered = BTreeSet::new();
        for (&address, &instruction) in &instructions {
            if instruction & 0x7f != 0x03
                || ((instruction >> 15) & 31) != 2
                || (instruction as i32) >> 20 != cell.pointer_offset as i32
            {
                continue;
            }
            if !is_ld(instruction, 29, 2, cell.pointer_offset as i32) {
                return Err(invalid("Cell pointer load is not an identified field base"));
            }
            if !reached.contains(&address) || bypass.contains(&address) {
                return Err(invalid("pointer reception does not dominate a field load"));
            }
            let first = word(&elf, address + 4)?;
            let (offset, dest, native) = if first & 0x7f == 0x03 && ((first >> 12) & 7) == 3 && ((first >> 15) & 31) == 29 {
                ((first as i32) >> 20, (first >> 7) & 31, true)
            } else if is_addi(first, 5, 0, 0) || is_addi(first, 6, 0, 0) {
                let byte = word(&elf, address + 8)?;
                ((byte as i32) >> 20, (first >> 7) & 31, false)
            } else {
                return Err(invalid("unsupported field decode prefix"));
            };
            let &(name, ty, width) = required
                .get(&(offset as u32))
                .filter(|_| offset >= 0)
                .ok_or_else(|| invalid("field bytes lack their direct typed layout"))?;
            if !matches!(dest, 5 | 6) {
                return Err(invalid("unsupported field value register"));
            }
            let mut cursor = address + 4;
            if native {
                if width != 8 || offset % 8 != 0 || !is_ld(first, dest, 29, offset) {
                    return Err(invalid("native field load lacks aligned u64 layout"));
                }
                cursor += 4;
            } else {
                cursor += 4;
                for index in 0..width {
                    if !is_lbu(word(&elf, cursor)?, 7, 29, offset + index as i32) {
                        return Err(invalid("field byte source or order differs"));
                    }
                    cursor += 4;
                    if index != 0 {
                        if !is_slli(word(&elf, cursor)?, 7, 7, index * 8) {
                            return Err(invalid("field byte shift differs"));
                        }
                        cursor += 4;
                    }
                    if !is_or(word(&elf, cursor)?, dest, dest, 7) {
                        return Err(invalid("field byte combination differs"));
                    }
                    cursor += 4;
                }
            }
            if elf
                .control_flow
                .iter()
                .any(|flow| address < flow.target && flow.target < cursor && !(address <= flow.address && flow.address < cursor))
            {
                return Err(invalid("incoming flow bypasses field reconstruction"));
            }
            if fields.len() >= 256 {
                return Err(invalid("field materializations exceed 256 sites"));
            }
            covered.insert(offset as u32);
            fields.push(Field {
                entry: cell.entry.clone(),
                parameter: cell.parameter.clone(),
                field: name.into(),
                ty: ty.into(),
                offset: offset as u32,
                width,
                pointer_load: address,
                decode_end: cursor,
                register: dest,
                native_load: native,
            });
        }
        if required.keys().any(|offset| !covered.contains(offset)) {
            return Err(invalid("a required direct field lacks actual materialization"));
        }
    }
    let record = Record { schema: "cellscript-fixed-cell-scalar-fields-v1", storage: storage.identity().into(), fields };
    let identity = crate::canonical_hash("cellscript-fixed-cell-scalar-fields-id-v1", &record)?;
    Ok(CheckedFixedCellScalarFields { storage, record, identity })
}
