//! Optional, deliberately partial machine evidence for fixed Cell-data reads.
//! This proves read arguments and immediate status/length gates, not field
//! decoding, lifecycle authorization, helper-call closure or full H1 admission.
use crate::checker::{
    block_for_address, flow_targets, is_addi, is_beq, is_ld, is_sd, is_sub, jump_targets_runtime_error,
    last_register_definition_before, register_constant_before, stack_address_offset,
};
use crate::interface::{CheckedModuleProjection, SourceType};
use crate::{
    CellBindingSource, CheckerBudgets, CheckerError, CheckerRejectionCode, ParsedElf, TypedSemanticRecord, VerifiedLoweringRecord,
};
use serde::Serialize;

#[derive(Debug)]
pub struct CheckedFixedCellReads {
    module: CheckedModuleProjection,
    record: Record,
    identity: String,
}
impl CheckedFixedCellReads {
    pub(crate) fn reads(&self) -> &[Read] {
        &self.record.reads
    }
    pub fn module_projection(&self) -> &CheckedModuleProjection {
        &self.module
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
    module_contract: String,
    bundle_hashes: [String; 4],
    reads: Vec<Read>,
}
#[derive(Debug, Serialize)]
pub(crate) struct Read {
    pub(crate) entry: String,
    pub(crate) binding: String,
    ty: String,
    pub(crate) source: CellBindingSource,
    pub(crate) ordinal: u32,
    width_bytes: u32,
    capacity_bytes: u32,
    pub(crate) size_offset: u32,
    pub(crate) buffer_offset: u32,
    pub(crate) setup_start: u64,
    pub(crate) syscall_address: u64,
    pub(crate) guarded_end: u64,
    status_error: i32,
}
fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("fixed Cell read: {}", message.into()))
}
/// Independently inspect all ECALLs in typed action/helper bodies. Every actual
/// LOAD_CELL_DATA there must match this compact, constant-index, fixed-length
/// profile; dynamic indexes, unknown syscall registers and incomplete gates
/// reject. Runtime helper ECALLs are outside this partial certificate's scope.
pub fn check_fixed_cell_reads(
    artifact: &[u8],
    metadata: &[u8],
    lowering: &[u8],
    source_map: &[u8],
    budgets: &CheckerBudgets,
) -> Result<CheckedFixedCellReads, CheckerError> {
    let module = crate::interface::project_bundle(artifact, metadata, lowering, source_map, budgets)?;
    let record: VerifiedLoweringRecord = serde_json::from_slice(lowering).map_err(|error| invalid(error.to_string()))?;
    let elf = crate::parse_elf(artifact, budgets.instructions).map_err(|error| invalid(error.to_string()))?;
    let reads = if let Some(logical) =
        crate::policy_machine::normalize_relaxed_branches_with_addresses(&record, &elf).map_err(invalid)?
    {
        let mut reads = verify(&logical.record, &logical.elf)?;
        for read in &mut reads {
            for address in [&mut read.setup_start, &mut read.syscall_address, &mut read.guarded_end] {
                *address =
                    *logical.actual_addresses.get(address).ok_or_else(|| invalid("read range has no actual instruction address"))?;
            }
        }
        reads
    } else {
        verify(&record, &elf)?
    };
    if reads.is_empty() {
        return Err(invalid("no fixed reads in typed bodies"));
    }
    let bundle_hashes = [artifact, metadata, lowering, source_map].map(|bytes| crate::hex_encode(&crate::ckb_blake2b256(bytes)));
    let record =
        Record { schema: "cellscript-fixed-cell-read-gates-v1", module_contract: module.identity().into(), bundle_hashes, reads };
    let identity = crate::canonical_hash("cellscript-fixed-cell-read-gates-id-v1", &record)?;
    Ok(CheckedFixedCellReads { module, record, identity })
}
fn source_value(source: CellBindingSource) -> u64 {
    match source {
        CellBindingSource::Input => 1,
        CellBindingSource::Output => 2,
        CellBindingSource::CellDep => 3,
        CellBindingSource::GroupInput => 0x0100_0000_0000_0001,
        CellBindingSource::GroupOutput => 0x0100_0000_0000_0002,
    }
}
fn width(value: &str, typed: &TypedSemanticRecord, depth: usize, visits: &mut usize) -> Result<u32, CheckerError> {
    width_type(&crate::interface::parse_source_type(&crate::checker::canonical_abi_type(value))?, typed, depth, visits)
}
fn width_type(ty: &SourceType, typed: &TypedSemanticRecord, depth: usize, visits: &mut usize) -> Result<u32, CheckerError> {
    *visits += 1;
    if depth > 32 || *visits > 4096 {
        return Err(invalid("fixed layout traversal exceeds its bound"));
    }
    let result = match ty {
        SourceType::Named(name, args) if args.is_empty() => match name.as_str() {
            "unit" => 0,
            "u8" => 1,
            "u16" => 2,
            "u32" | "i32" => 4,
            "u64" => 8,
            "u128" => 16,
            "address" | "hash" => 32,
            "bool" => return Err(invalid("boolean canonicality requires a separate profile")),
            _ => {
                let layout =
                    typed.types.iter().find(|layout| layout.name == *name).ok_or_else(|| invalid("fixed Cell layout is absent"))?;
                if !matches!(layout.kind.as_str(), "struct" | "resource") || !layout.variants.is_empty() {
                    return Err(invalid("bool, enum and dynamic canonicality require a separate profile"));
                }
                let mut size = 0u32;
                for field in &layout.fields {
                    let field_width = width(&field.ty, typed, depth + 1, visits)?;
                    if field.offset != size || field.width_bytes != Some(field_width) {
                        return Err(invalid("fixed field layout differs"));
                    }
                    size = size.checked_add(field_width).ok_or_else(|| invalid("fixed layout overflow"))?;
                }
                if layout.encoded_size != Some(size) {
                    return Err(invalid("fixed layout total differs"));
                }
                size
            }
        },
        SourceType::Array(inner, count) => width_type(inner, typed, depth + 1, visits)?
            .checked_mul(u32::try_from(*count).map_err(|_| invalid("fixed array count overflow"))?)
            .ok_or_else(|| invalid("fixed array overflow"))?,
        SourceType::Tuple(fields) => fields.iter().try_fold(0u32, |total, field| {
            total.checked_add(width_type(field, typed, depth + 1, visits)?).ok_or_else(|| invalid("fixed tuple overflow"))
        })?,
        _ => return Err(invalid("generic or dynamic Cell layout lacks this profile")),
    };
    if result > 1536 {
        return Err(invalid("fixed Cell width exceeds 1536 bytes"));
    }
    Ok(result)
}
fn word(elf: &ParsedElf, address: u64) -> Result<u32, CheckerError> {
    elf.instructions
        .binary_search_by_key(&address, |instruction| instruction.address)
        .map(|index| elf.instructions[index].word)
        .map_err(|_| invalid("missing read instruction"))
}
fn verify(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<Vec<Read>, CheckerError> {
    let mut reads = Vec::new();
    for &address in &elf.syscall_addresses {
        if !record.text_range.contains(address) {
            continue;
        }
        let block = block_for_address(record, address).ok_or_else(|| invalid("unowned ECALL"))?;
        let Some(entry) = record.typed_semantics.entries.iter().find(|entry| entry.id == block.owner_entry) else { continue };
        if block.range.end - block.range.start > 4096 * 4 {
            return Err(invalid("read classification block exceeds 4096 instructions"));
        }
        let syscall = register_constant_before(elf, block, address, 17).ok_or_else(|| invalid("opaque syscall in typed body"))?;
        // This initial profile permits only the identity/field observations
        // accompanying fixed reads; they do not acquire codec evidence here.
        if syscall != 2092 {
            if !matches!(syscall, 2062 | 2081) {
                return Err(invalid("syscall is outside the fixed-read profile"));
            }
            continue;
        }
        if reads.len() == 64 {
            return Err(invalid("more than 64 fixed read sites"));
        }
        let source = register_constant_before(elf, block, address, 14).ok_or_else(|| invalid("opaque Cell source"))?;
        let ordinal = register_constant_before(elf, block, address, 13)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| invalid("dynamic Cell index lacks this profile"))?;
        let mut bindings =
            entry.cell_bindings.iter().filter(|binding| source_value(binding.source) == source && binding.ordinal == ordinal);
        let binding = bindings.next().ok_or_else(|| invalid("read has no typed Cell location"))?;
        if bindings.next().is_some() {
            return Err(invalid("read has ambiguous typed Cell locations"));
        }
        let width = width(&binding.ty, &record.typed_semantics, 0, &mut 0)?;
        let (a0_address, a0_word) =
            last_register_definition_before(elf, block, address, 10).ok_or_else(|| invalid("missing read buffer"))?;
        let (_, a1_word) = last_register_definition_before(elf, block, address, 11).ok_or_else(|| invalid("missing read size"))?;
        let buffer = stack_address_offset(a0_word, 10).ok_or_else(|| invalid("noncompact read buffer"))?;
        let size = stack_address_offset(a1_word, 11).ok_or_else(|| invalid("noncompact size pointer"))?;
        let frame =
            record.entries.iter().find(|owner| owner.id == entry.id).ok_or_else(|| invalid("missing read frame"))?.frame_size_bytes;
        let start =
            a0_address.checked_sub(8).filter(|start| *start >= block.range.start).ok_or_else(|| invalid("incomplete setup"))?;
        let capacity = register_constant_before(elf, block, a0_address - 4, 5)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| invalid("unknown capacity"))?;
        if size < 0
            || size % 8 != 0
            || buffer != size + 8
            || capacity != 512
            || width > capacity
            || frame > 2047
            || (buffer as u32).checked_add(capacity).is_none_or(|end| end > frame.saturating_sub(16))
            || !is_addi(word(elf, start)?, 5, 0, capacity as i32)
            || !is_sd(word(elf, start + 4)?, 5, 2, size)
            || !is_addi(word(elf, start + 8)?, 10, 2, buffer)
            || !is_addi(word(elf, start + 12)?, 11, 2, size)
            || !is_addi(word(elf, start + 16)?, 12, 0, 0)
            || ordinal > 2047
            || !is_addi(word(elf, start + 20)?, 13, 0, ordinal as i32)
        {
            return Err(invalid("read setup, capacity or frame bounds differ"));
        }
        // Only the source's canonical constant materializer may intervene.
        let source_start = start + 24;
        if address < source_start + 12 || address > source_start + 40 {
            return Err(invalid("noncanonical source materializer length"));
        }
        for pc in (source_start..address - 8).step_by(4) {
            let instruction = word(elf, pc)?;
            let opcode = instruction & 0x7f;
            let rd = (instruction >> 7) & 31;
            let rs1 = (instruction >> 15) & 31;
            let function = (instruction >> 12) & 7;
            if rd != 14
                || !(opcode == 0x37
                    || (opcode == 0x13
                        && ((function == 0 && matches!(rs1, 0 | 14)) || (function == 1 && rs1 == 14 && instruction >> 26 == 0))))
            {
                return Err(invalid("nonconstant source setup instruction"));
            }
        }
        if word(elf, address - 8)? != 0x0000_18b7 || !is_addi(word(elf, address - 4)?, 17, 17, 2092 - 4096) {
            return Err(invalid("changed syscall materializer"));
        }
        let status_error = [1, 3, 45]
            .into_iter()
            .find(|code| jump_targets_runtime_error(record, elf, address + 8, *code))
            .ok_or_else(|| invalid("status has no checked error exit"))?;
        let end = address + 32;
        if !is_beq(word(elf, address + 4)?, 10, 0)
            || !flow_targets(elf, address + 4, address + 12)
            || !is_ld(word(elf, address + 12)?, 10, 2, size)
            || !is_addi(word(elf, address + 16)?, 11, 0, width as i32)
            || !is_sub(word(elf, address + 20)?, 10, 10, 11)
            || !is_beq(word(elf, address + 24)?, 10, 0)
            || !flow_targets(elf, address + 24, end)
            || !jump_targets_runtime_error(record, elf, address + 28, 4)
        {
            return Err(invalid("success or exact-length guard differs"));
        }
        if elf
            .control_flow
            .iter()
            .any(|flow| start < flow.target && flow.target < end && !(start <= flow.address && flow.address < end))
        {
            return Err(invalid("external flow bypasses read setup or gates"));
        }
        reads.push(Read {
            entry: entry.id.clone(),
            binding: binding.binding.clone(),
            ty: binding.ty.clone(),
            source: binding.source,
            ordinal,
            width_bytes: width,
            capacity_bytes: capacity,
            size_offset: size as u32,
            buffer_offset: buffer as u32,
            setup_start: start,
            syscall_address: address,
            guarded_end: end,
            status_error,
        });
    }
    for entry in &record.typed_semantics.entries {
        for binding in &entry.cell_bindings {
            if reads
                .iter()
                .filter(|read| {
                    read.entry == entry.id
                        && read.binding == binding.binding
                        && read.source == binding.source
                        && read.ordinal == binding.ordinal
                })
                .count()
                != 1
            {
                return Err(invalid("each typed fixed Cell binding requires exactly one checked read"));
            }
        }
    }
    Ok(reads)
}
