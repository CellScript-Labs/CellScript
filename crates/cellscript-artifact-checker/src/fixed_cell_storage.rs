//! Actual fixed Cell parameter pointer reception, distinct from field codecs.
use crate::checker::{entry_start, is_addi, is_sd};
use crate::entry_codec::CheckedFixedPolicyParameterDecoders;
use crate::fixed_cell_reads::CheckedFixedCellReads;
use crate::{CellBindingRole, CheckerBudgets, CheckerError, CheckerRejectionCode, ParsedElf, VerifiedLoweringRecord};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug)]
pub struct CheckedFixedCellParameterStorage {
    reads: CheckedFixedCellReads,
    parameters: CheckedFixedPolicyParameterDecoders,
    record: Record,
    identity: String,
}
impl CheckedFixedCellParameterStorage {
    pub(crate) fn cells(&self) -> &[Storage] {
        &self.record.cells
    }
    pub fn reads(&self) -> &CheckedFixedCellReads {
        &self.reads
    }
    pub fn parameters(&self) -> &CheckedFixedPolicyParameterDecoders {
        &self.parameters
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
    read_gates: String,
    parameter_decoder: String,
    cells: Vec<Storage>,
}
#[derive(Debug, Serialize)]
pub(crate) struct Storage {
    pub(crate) entry: String,
    pub(crate) parameter: String,
    pub(crate) local_id: u32,
    source_id: u64,
    abi_register: u32,
    pub(crate) pointer_offset: u32,
    pub(crate) size_offset: u32,
    pub(crate) buffer_offset: u32,
    pub(crate) spill_address: u64,
    pub(crate) receiver_start: u64,
    receiver_end: u64,
}
fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("fixed Cell parameter storage: {}", message.into()),
    )
}
fn word(elf: &ParsedElf, address: u64) -> Result<u32, CheckerError> {
    elf.instructions
        .binary_search_by_key(&address, |instruction| instruction.address)
        .map(|index| elf.instructions[index].word)
        .map_err(|_| invalid("missing storage instruction"))
}
/// Check actual policy null-pair transport, read gates and the source-ID slot
/// profile's pointer reception. This grants no later field/provenance, alias
/// freedom, helper closure, complete codec receipt or compatible-open admission.
pub fn check_fixed_cell_parameter_storage(
    artifact: &[u8],
    metadata: &[u8],
    lowering: &[u8],
    source_map: &[u8],
    budgets: &CheckerBudgets,
) -> Result<CheckedFixedCellParameterStorage, CheckerError> {
    let reads = crate::fixed_cell_reads::check_fixed_cell_reads(artifact, metadata, lowering, source_map, budgets)?;
    let parameters = crate::entry_codec::check_fixed_policy_parameter_decoders(artifact, metadata, lowering, source_map, budgets)?;
    let record: VerifiedLoweringRecord = serde_json::from_slice(lowering).map_err(|error| invalid(error.to_string()))?;
    let elf = crate::parse_elf(artifact, budgets.instructions).map_err(|error| invalid(error.to_string()))?;
    let mut cells = Vec::new();
    for read in reads.reads() {
        let entry = record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.id == read.entry)
            .ok_or_else(|| invalid("missing typed owner"))?;
        let binding = entry
            .cell_bindings
            .iter()
            .find(|binding| binding.binding == read.binding && binding.source == read.source && binding.ordinal == read.ordinal)
            .ok_or_else(|| invalid("missing read binding"))?;
        if binding.role == CellBindingRole::Output {
            continue;
        }
        let Some(local_id) = binding.local_id else { continue };
        let local = entry.locals.iter().find(|local| local.id == local_id).ok_or_else(|| invalid("missing Cell local"))?;
        let parameter = entry
            .params
            .iter()
            .find(|param| param.binding_id == local_id && param.name == binding.binding)
            .ok_or_else(|| invalid("Cell local is not a source parameter"))?;
        let transports = crate::entry_codec::parameters(entry, &record.typed_semantics)?;
        let abi = transports
            .iter()
            .find(|param| param.name == parameter.name)
            .filter(|param| param.payload_offset.is_none())
            .ok_or_else(|| invalid("Cell parameter lacks checked null transport"))?
            .abi_index;
        // This finite profile independently requires the uncompressed slot
        // model. It never trusts a producer 'unpacked' or slot-valid bit.
        let pointer = local
            .source_id
            .checked_mul(8)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| invalid("source pointer slot overflow"))?;
        let scalar_end = reads
            .reads()
            .iter()
            .filter(|other| other.entry == read.entry)
            .map(|other| other.size_offset)
            .min()
            .ok_or_else(|| invalid("missing read storage boundary"))?;
        let mut source_ids = BTreeSet::new();
        if entry.locals.len() > 256
            || entry.locals.iter().any(|local| {
                !source_ids.insert(local.source_id)
                    || local
                        .source_id
                        .checked_mul(8)
                        .and_then(|slot| slot.checked_add(8))
                        .is_none_or(|end| end > u64::from(scalar_end))
            })
        {
            return Err(invalid("local source IDs duplicate or escape the uncompressed scalar region"));
        }
        for other in reads.reads().iter().filter(|other| other.entry == read.entry && other.setup_start != read.setup_start) {
            if read.size_offset < other.buffer_offset + 512 && other.size_offset < read.buffer_offset + 512 {
                return Err(invalid("parameter read storage overlaps another Cell read"));
            }
        }
        if pointer.checked_add(8).is_none_or(|end| end > scalar_end) || pointer > 2047 || abi > 7 {
            return Err(invalid("source pointer slot overlaps read buffers or exceeds the compact profile"));
        }
        let owner = record.entries.iter().find(|owner| owner.id == entry.id).ok_or_else(|| invalid("missing machine owner"))?;
        let frame = owner.frame_size_bytes;
        let start = entry_start(&record, &entry.id)?;
        if !(16..=2047).contains(&frame)
            || !is_addi(word(&elf, start)?, 2, 2, -(frame as i32))
            || !is_sd(word(&elf, start + 4)?, 1, 2, frame as i32 - 8)
            || !is_sd(word(&elf, start + 8)?, 8, 2, frame as i32 - 16)
            || !is_addi(word(&elf, start + 12)?, 8, 2, frame as i32)
        {
            return Err(invalid("Cell owner lacks the actual compact frame prologue"));
        }
        // At most eight register arguments can be spilled by this profile.
        let mut spill = None;
        let mut spill_slots = BTreeSet::new();
        let mut spill_registers = BTreeSet::new();
        for address in (start + 16..start + 48).step_by(4) {
            let instruction = word(&elf, address)?;
            if instruction & 0x7f != 0x23 || (instruction >> 12) & 7 != 3 || (instruction >> 15) & 31 != 2 {
                break;
            }
            let register = (instruction >> 20) & 31;
            let offset = (((instruction >> 25) << 5) | ((instruction >> 7) & 31)) as i32;
            let offset = (offset << 20) >> 20;
            let length_slot = offset >= 0
                && reads.reads().iter().any(|other| {
                    other.entry == read.entry
                        && other.size_offset == offset as u32
                        && transports.iter().any(|param| {
                            param.name == other.binding && param.payload_offset.is_none() && 11 + param.abi_index == register
                        })
                });
            if !(10..=17).contains(&register)
                || offset < 0
                || offset % 8 != 0
                || ((offset as u32).checked_add(8).is_none_or(|end| end > scalar_end) && !length_slot)
                || !spill_slots.insert(offset)
                || !spill_registers.insert(register)
            {
                return Err(invalid(format!(
                    "argument spill register={register} offset={offset} scalar_end={scalar_end} overlaps or escapes the profile"
                )));
            }
            if is_sd(instruction, 10 + abi, 2, pointer as i32) && spill.replace(address).is_some() {
                return Err(invalid("duplicate parameter pointer spill"));
            }
        }
        let spill = spill.ok_or_else(|| invalid("source pointer slot lacks its actual ABI spill"))?;
        let end = read.guarded_end + 8;
        if !is_addi(word(&elf, read.guarded_end)?, 5, 2, read.buffer_offset as i32)
            || !is_sd(word(&elf, read.guarded_end + 4)?, 5, 2, pointer as i32)
        {
            return Err(invalid("read buffer is not received into its source parameter slot"));
        }
        if elf.control_flow.iter().any(|flow| {
            read.setup_start < flow.target && flow.target < end && !(read.setup_start <= flow.address && flow.address < end)
        }) {
            return Err(invalid("external flow bypasses read gates or pointer reception"));
        }
        cells.push(Storage {
            entry: entry.id.clone(),
            parameter: parameter.name.clone(),
            local_id,
            source_id: local.source_id,
            abi_register: abi,
            pointer_offset: pointer,
            size_offset: read.size_offset,
            buffer_offset: read.buffer_offset,
            spill_address: spill,
            receiver_start: read.guarded_end,
            receiver_end: end,
        });
    }
    if cells.is_empty() {
        return Err(invalid("no eligible Cell parameter storage"));
    }
    let record = Record {
        schema: "cellscript-fixed-cell-parameter-storage-v1",
        read_gates: reads.identity().into(),
        parameter_decoder: parameters.identity().into(),
        cells,
    };
    let identity = crate::canonical_hash("cellscript-fixed-cell-parameter-storage-id-v1", &record)?;
    Ok(CheckedFixedCellParameterStorage { reads, parameters, record, identity })
}
