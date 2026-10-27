//! Machine initialization and arena primitives of typed collection owners.
//! Reserve/drop descriptor transitions compose checked arena primitives;
//! element reads/writes compose those transitions and ordered operations. Index
//! and scalar/fixed-value result transport compose with scalar and pointer-backed
//! fixed-value inputs, short-array representations and sorted input transport.
//! Scalar definition contracts cover local arithmetic/copies. Complete source
//! provenance, parameter decoding and machine lifetime remain separate obligations.
use super::{Operation, TypedSemanticOperationDetail};
use crate::{CheckerError, CheckerRejectionCode, ParsedElf, VerifiedLoweringRecord};
use std::collections::{BTreeMap, BTreeSet};

const PREFIX: &str = ".Lallocated_init_";

mod allocation;

pub(crate) type FlowSites = BTreeMap<(String, u32, u32), (u64, u64, u32)>;

#[derive(Default)]
pub(crate) struct Checked {
    pub constructors: BTreeMap<String, BTreeMap<u64, Constructor>>,
    pub borrowed_reads: BTreeMap<String, BTreeMap<u64, BorrowedRead>>,
    pub owned_primitives: BTreeMap<String, BTreeMap<u64, OwnedPrimitive>>,
    pub copied_results: BTreeMap<String, BTreeMap<u64, CopiedResult>>,
    pub flow_sites: FlowSites,
}

/// A complete checked publication of an immutable collection-read temporary.
/// Mutable source bindings copy this value into their own storage.
pub(crate) struct CopiedResult {
    pub end: u64,
    pub buffer: u32,
    pub bytes: u32,
    pub slot: u32,
}

impl CopiedResult {
    pub(crate) fn regions(&self) -> [(i64, i64); 2] {
        let buffer = i64::from(self.buffer);
        let slot = i64::from(self.slot);
        [(buffer, buffer + i64::from(self.bytes)), (slot, slot + 8)]
    }
}

pub(crate) struct OwnedPrimitive {
    pub end: u64,
    pub offset: i64,
    pub extent: u32,
}

pub(crate) struct BorrowedRead {
    pub pointer: u32,
    pub source: u64,
    pub extent_slot: Option<u32>,
}

pub(crate) struct Constructor {
    pub end: u64,
    pub local: u32,
    pub source: u64,
    pub offset: i64,
    /// Arena header, backing budget and descriptors; excludes private scratch.
    pub extent: u32,
    pub owner_type: String,
}

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("allocated initialization machine contract: {}", message.into()),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    Unknown,
    Constant(i64),
    Stack(i64),
}

fn add(left: Value, right: Value) -> Value {
    match (left, right) {
        (Value::Constant(a), Value::Constant(b)) => Value::Constant(a.wrapping_add(b)),
        (Value::Stack(a), Value::Constant(b)) | (Value::Constant(b), Value::Stack(a)) => {
            a.checked_add(b).map_or(Value::Unknown, Value::Stack)
        }
        _ => Value::Unknown,
    }
}

fn writes(start: u64, end: u64, words: &BTreeMap<u64, u32>) -> Result<Vec<(i64, Value)>, CheckerError> {
    if start >= end || end - start > 16_384 || !(end - start).is_multiple_of(4) {
        return Err(invalid("invalid or excessive initialization extent"));
    }
    let mut registers = [Value::Unknown; 32];
    registers[0] = Value::Constant(0);
    registers[2] = Value::Stack(0);
    let mut stores = Vec::new();
    for pc in (start..end).step_by(4) {
        let word = *words.get(&pc).ok_or_else(|| invalid("initialization leaves decoded code"))?;
        let rd = ((word >> 7) & 31) as usize;
        let rs1 = ((word >> 15) & 31) as usize;
        let rs2 = ((word >> 20) & 31) as usize;
        let funct3 = word >> 12 & 7;
        let immediate = i64::from((word as i32) >> 20);
        if word & 0x7f == 0x23 && funct3 == 3 {
            let imm = (((word >> 7) & 31) | ((word >> 25) << 5)) as i32;
            let Value::Stack(offset) = add(registers[rs1], Value::Constant(i64::from((imm << 20) >> 20))) else {
                return Err(invalid("initialization store has no private stack origin"));
            };
            if registers[rs2] == Value::Unknown {
                return Err(invalid("initialization writes an unknown value"));
            }
            stores.push((offset, registers[rs2]));
            continue;
        }
        // Initializers may materialize addresses/constants in scratch registers;
        // no call, load, branch, SP adjustment or callee-save clobber is allowed.
        if !matches!(rd, 0 | 5 | 6 | 7 | 28 | 29 | 30 | 31) {
            return Err(invalid("initialization changes a non-scratch register"));
        }
        registers[rd] = match word & 0x7f {
            0x13 if funct3 == 0 => add(registers[rs1], Value::Constant(immediate)),
            0x1b if funct3 == 0 => match add(registers[rs1], Value::Constant(immediate)) {
                Value::Constant(value) => Value::Constant(i64::from(value as i32)),
                _ => Value::Unknown,
            },
            0x37 => Value::Constant(i64::from((word & 0xfffff000) as i32)),
            0x33 if funct3 == 0 && word >> 25 == 0 => add(registers[rs1], registers[rs2]),
            0x13 if funct3 == 1 && word >> 26 == 0 => match registers[rs1] {
                Value::Constant(value) => Value::Constant(value.wrapping_shl((word >> 20) & 63)),
                _ => Value::Unknown,
            },
            _ => return Err(invalid("unproved instruction in owner initialization")),
        };
        registers[0] = Value::Constant(0);
    }
    Ok(stores)
}

pub(crate) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, sink: Option<&str>) -> Result<Checked, CheckerError> {
    let mut result = Checked::default();
    let has_operations = record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .any(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { .. }));
    if !has_operations
        && record.blocks.iter().all(|block| {
            block.allocated_init_labels.is_empty()
                && block.allocated_allocation_labels.is_empty()
                && block.allocated_lifecycle_labels.is_empty()
        })
    {
        return Ok(result);
    }
    let normalized = crate::policy_machine::normalize_relaxed_branches(record, elf).map_err(invalid)?;
    let (record, elf) = normalized.as_ref().map_or((record, elf), |(record, elf)| (record, elf));
    let abort = record
        .blocks
        .iter()
        .find(|block| Some(block.id.as_str()) == sink)
        .map(|block| block.range.start)
        .ok_or_else(|| invalid("missing checked fatal sink"))?;
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect::<BTreeMap<_, _>>();
    let mut labels = BTreeMap::new();
    for block in &record.blocks {
        for label in &block.allocated_init_labels {
            let parts =
                label.strip_prefix(PREFIX).ok_or_else(|| invalid("unknown initialization marker"))?.split('_').collect::<Vec<_>>();
            if parts.len() != 3 || !matches!(parts[1], "begin" | "end") || parts[2].parse::<u64>().is_err() {
                return Err(invalid("invalid initialization marker"));
            }
            let source = parts[0].parse::<u64>().map_err(|_| invalid("invalid constructor local"))?;
            if labels.insert((block.owner_entry.as_str(), source, parts[1]), block.range.start).is_some() {
                return Err(invalid("duplicate initialization boundary"));
            }
        }
    }
    let mut checked = BTreeSet::new();
    let mut sites = crate::cell_selection::machine::flow::Markers::new();
    for entry in &record.typed_semantics.entries {
        let constructors = entry
            .blocks
            .iter()
            .flat_map(|block| block.operations.iter().map(move |op| (block, op)))
            .filter_map(|(block, op)| match &op.detail {
                TypedSemanticOperationDetail::AllocatedValue { contract, operation: Operation::New, .. } => {
                    Some((block, op, contract))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if constructors.is_empty() {
            continue;
        }
        let lowering = record.entries.iter().find(|lowering| lowering.id == entry.id).ok_or_else(|| invalid("missing callable"))?;
        let frame = i64::from(crate::nested_machine::owned_frame(record, elf, lowering)?);
        let scalar_end = entry
            .locals
            .iter()
            .map(|local| local.source_id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .and_then(|count| count.checked_mul(8))
            .and_then(|bytes| i64::try_from(bytes).ok())
            .ok_or_else(|| invalid("scalar extent overflows"))?;
        let mut regions = Vec::new();
        for (block, op, contract) in constructors {
            let destination = op
                .destinations
                .first()
                .and_then(|id| entry.locals.iter().find(|local| local.id == *id))
                .ok_or_else(|| invalid("constructor has no typed destination"))?;
            let source = destination.source_id;
            let begin_key = (entry.id.as_str(), source, "begin");
            let end_key = (entry.id.as_str(), source, "end");
            let start = *labels.get(&begin_key).ok_or_else(|| invalid("missing constructor start"))?;
            let end = *labels.get(&end_key).ok_or_else(|| invalid("missing constructor end"))?;
            if !checked.insert(begin_key) || !checked.insert(end_key) {
                return Err(invalid("constructor boundary reused"));
            }
            let actual = writes(start, end, &words)?;
            let offset = actual.first().map(|(offset, _)| *offset).ok_or_else(|| invalid("empty initialization"))?;
            let extent = 32 + i64::from(contract.bytes) + contract.members.len() as i64 * 32 + 128;
            let region_end = offset.checked_add(extent).ok_or_else(|| invalid("owner extent overflows"))?;
            if offset < scalar_end
                || offset % 16 != 0
                || region_end > frame - 16
                || regions.iter().any(|(begin, end)| offset < *end && *begin < region_end)
            {
                return Err(invalid("owner overlaps scalar slots, another owner or frame bookkeeping"));
            }
            regions.push((offset, region_end));
            let mut expected = (0..4).map(|field| (offset + field * 8, Value::Constant(0))).collect::<Vec<_>>();
            for index in 0..contract.members.len() {
                let descriptor = offset + 32 + i64::from(contract.bytes) + index as i64 * 32;
                expected.extend((0..3).map(|field| (descriptor + field * 8, Value::Constant(0))));
                expected.push((descriptor + 24, Value::Constant(1)));
            }
            let slot =
                source.checked_mul(8).and_then(|slot| i64::try_from(slot).ok()).ok_or_else(|| invalid("owner slot overflows"))?;
            expected.push((slot, Value::Stack(offset)));
            if actual != expected {
                return Err(invalid("cursor, allocation count, descriptor or owner pointer initialization differs"));
            }
            for flow in &elf.control_flow {
                if start < flow.target && flow.target < end {
                    return Err(invalid("control flow enters a partial initialization"));
                }
            }
            sites.insert((entry.id.as_str(), block.id, op.index), (start, end, offset as u32));
            result.constructors.entry(entry.id.clone()).or_default().insert(
                start,
                Constructor {
                    end,
                    local: destination.id,
                    source,
                    offset,
                    extent: u32::try_from(extent - 128).map_err(|_| invalid("arena argument extent overflows"))?,
                    owner_type: contract.owner.clone(),
                },
            );
        }
    }
    if checked.len() != labels.len() {
        return Err(invalid("machine initializer has no typed constructor"));
    }
    let allocation = allocation::validate(record, elf, &sites, abort, &result.constructors)?;
    result.borrowed_reads = allocation.borrowed_reads;
    result.owned_primitives = allocation.owned_primitives;
    result.copied_results = allocation.copied_results;
    result.flow_sites = allocation.flow_sites;
    Ok(result)
}

/// Source-level owner identity shared by allocation sites and helper calls.
/// This does not establish machine pointer provenance; the CFG proof does that.
pub(crate) fn owner_aliases(
    entry: &crate::TypedSemanticEntry,
    owner_types: &BTreeSet<&str>,
) -> Result<BTreeMap<u32, BTreeSet<u32>>, CheckerError> {
    let mut aliases = BTreeMap::new();
    for op in entry.blocks.iter().flat_map(|block| &block.operations) {
        if (op.opcode == "move" || matches!(&op.detail, TypedSemanticOperationDetail::UnaryOperator { operator } if operator == "ref"))
            && let (Some(dest), Some(source)) = (op.destinations.first(), op.operands.first().and_then(|arg| arg.local))
        {
            aliases.entry(*dest).or_insert_with(BTreeSet::new).insert(source);
        }
    }
    let mut named_locals = BTreeMap::new();
    for local in &entry.locals {
        named_locals.entry((local.name.as_str(), local.ty.trim_start_matches('&'))).or_insert(local.id);
    }
    for borrow in &entry.borrows {
        let ty = borrow.root_type.trim_start_matches('&');
        if !owner_types.contains(ty) {
            continue;
        }
        let root = named_locals.get(&(borrow.root.as_str(), ty)).ok_or_else(|| invalid("loan has no owner local"))?;
        let binding = named_locals
            .get(&(borrow.binding.as_str(), borrow.view_type.trim_start_matches('&')))
            .ok_or_else(|| invalid("loan has no alias local"))?;
        aliases.entry(*binding).or_insert_with(|| BTreeSet::from([*root]));
    }
    Ok(aliases)
}
