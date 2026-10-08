//! Independent validation of the caller-owned fixed ordinary-struct result ABI.
//! This verifies placement, lifetime, bounded copies and frame ownership. It
//! does not turn the bundle's structural claim into source equivalence.
use std::collections::{BTreeMap, BTreeSet};

use crate::checker::{canonical_abi_type, committed_state_type_width};
use crate::*;

fn error(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2407AbiOrStackInvalid, format!("fixed struct result ABI: {}", message.into()))
}

fn require(condition: bool, message: &str) -> Result<(), CheckerError> {
    if condition {
        Ok(())
    } else {
        Err(error(message))
    }
}

fn same_nominal_type(left: &str, right: &str, aliases: &BTreeMap<&str, String>) -> bool {
    crate::generic_projection::checked_source_type(left, aliases) == crate::generic_projection::checked_source_type(right, aliases)
}

fn result_width<'a>(record: &'a VerifiedLoweringRecord, entry: &TypedSemanticEntry) -> Option<(&'a str, u32)> {
    let ty = record.typed_semantics.types.iter().find(|ty| canonical_abi_type(&ty.name) == canonical_abi_type(&entry.return_type))?;
    (ty.kind == "struct").then_some((ty.name.as_str(), ty.encoded_size?))
}

fn argument_count(record: &VerifiedLoweringRecord, entry: &TypedSemanticEntry) -> Result<u32, CheckerError> {
    entry.params.iter().try_fold(0u32, |count, param| {
        let ty = canonical_abi_type(&param.ty);
        let ty = ty.strip_prefix("&mut").or_else(|| ty.strip_prefix('&')).unwrap_or(&ty);
        let temporal = matches!(
            ty,
            "BlockNumber"
                | "EpochNumber"
                | "EpochDuration"
                | "EpochLength"
                | "EncodedSince"
                | "DecodedSince"
                | "TimestampMillis"
                | "AbsoluteBlockSince"
                | "AbsoluteEpochSince"
                | "AbsoluteTimestampSince"
                | "RelativeBlockSince"
                | "RelativeEpochSince"
                | "RelativeTimestampSince"
        ) || ty.starts_with("Since<Absolute,")
            || ty.starts_with("Since<Relative,");
        let nominal = record.typed_semantics.types.iter().any(|layout| canonical_abi_type(&layout.name) == ty)
            || (!matches!(ty, "bool" | "u8" | "u16" | "u32" | "i32" | "u64" | "u128" | "hash" | "address" | "unit")
                && !ty.starts_with(['[', '(']));
        let wide = committed_state_type_width(ty, &record.typed_semantics.types).is_some_and(|width| width > 8);
        let type_hash = !ty.contains("__mono__")
            && nominal
            && entry.blocks.iter().flat_map(|block| &block.operations).any(|operation| {
                operation.opcode == "type-hash"
                    && operation.operands.iter().filter_map(|operand| operand.local).any(|local| {
                        let source = entry.locals.iter().find(|candidate| candidate.id == local).map(|candidate| candidate.source_id);
                        source.is_some()
                            && source
                                == entry
                                    .locals
                                    .iter()
                                    .find(|candidate| candidate.id == param.binding_id)
                                    .map(|candidate| candidate.source_id)
                    })
            });
        let width = if temporal {
            1
        } else if nominal {
            2 + 2 * u32::from(type_hash)
        } else if wide {
            2
        } else {
            1
        };
        count.checked_add(width).ok_or_else(|| error("argument expansion overflows"))
    })
}

pub(crate) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    // Generic projection and value-ability validation ran before this boundary.
    // Only aliases owned by the same checked instantiation may be substituted;
    // equal widths or unqualified template names do not establish equivalence.
    let aliases = crate::generic_projection::nominal_aliases(&record.typed_semantics);
    let mut checked_calls = BTreeSet::new();
    let mut uses_copy = false;
    for entry in &record.entries {
        let typed = record.typed_semantics.entries.iter().find(|typed| typed.id == entry.id);
        let expected = typed.filter(|typed| typed.kind == "helper").and_then(|typed| result_width(record, typed));
        match (expected, &entry.fixed_result_abi) {
            (None, None) => {}
            (Some((name, width)), Some(abi)) => {
                let typed = typed.expect("fixed result has a typed helper");
                validate_owned_frame(record, elf, entry, abi.owned_frame_bytes)?;
                require(
                    abi.schema == FIXED_RESULT_ABI_SCHEMA
                        && canonical_abi_type(&abi.type_name) == canonical_abi_type(name)
                        && abi.width_bytes == width
                        && abi.hidden_argument_index == argument_count(record, typed)?,
                    "type, width, schema or hidden argument index differs from the typed declaration",
                )?;
                require(
                    abi.saved_pointer_offset.is_multiple_of(8)
                        && abi.saved_pointer_offset.checked_add(8).is_some_and(|end| end <= abi.owned_frame_bytes.saturating_sub(16)),
                    "saved destination lies outside the owned frame",
                )?;
                owned_range(record, &entry.id, abi.save_range)?;
                let mut machine = Machine::incoming(abi.hidden_argument_index);
                machine.execute(elf, abi.save_range, None)?;
                require(
                    machine.stores == [(Atom::Stack(i64::from(abi.saved_pointer_offset)), Atom::Incoming(abi.hidden_argument_index))],
                    "hidden destination is saved from the wrong register or caller-stack position",
                )?;
                let returns = typed.blocks.iter().filter(|block| block.terminator == "return").count();
                require(returns == abi.copy_ranges.len() && returns > 0, "return copy coverage is incomplete")?;
                require(abi.copy_sources.len() == if width == 0 { 0 } else { returns }, "source pointer coverage is incomplete")?;
                require(
                    abi.copy_ranges.iter().map(|range| (range.start, range.end)).collect::<BTreeSet<_>>().len() == returns,
                    "duplicate return copy range",
                )?;
                let first_copy = abi.copy_ranges[0];
                let epilogue_address =
                    elf.control_flow.iter().find(|flow| flow.address == first_copy.end - 4).map_or(first_copy.end, |flow| flow.target);
                let epilogue = record
                    .blocks
                    .iter()
                    .find(|block| block.owner_entry == entry.id && block.range.start == epilogue_address)
                    .ok_or_else(|| error("missing shared epilogue"))?;
                let mut epilogue_machine = Machine::new();
                epilogue_machine.execute(elf, epilogue.range, None)?;
                require(
                    elf.control_flow
                        .iter()
                        .filter(|flow| flow.target == epilogue.range.start)
                        .filter(|flow| {
                            record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(flow.address))
                        })
                        .all(|flow| abi.copy_ranges.iter().any(|range| range.contains(flow.address))),
                    "a return bypasses the fixed result copy",
                )?;
                require(
                    epilogue_machine.registers[2] == Atom::Stack(i64::from(abi.owned_frame_bytes))
                        && epilogue_machine.returned
                        && epilogue_machine.loads.contains(&(1, Atom::Stack(i64::from(abi.owned_frame_bytes) - 8)))
                        && epilogue_machine.loads.contains(&(8, Atom::Stack(i64::from(abi.owned_frame_bytes) - 16))),
                    "frame/RA/FP teardown differs from the owned frame",
                )?;
                for range in &abi.copy_ranges {
                    owned_range(record, &entry.id, *range)?;
                    let mut machine = Machine::new();
                    if width != 0 {
                        let source = abi
                            .copy_sources
                            .iter()
                            .find(|source| source.range.end == range.start)
                            .ok_or_else(|| error("copy lacks its adjacent source pointer range"))?;
                        validate_source(record, elf, entry, typed, abi, source, &aliases)?;
                        machine.registers[10] = Atom::Source;
                    }
                    machine.saved = Some(i64::from(abi.saved_pointer_offset));
                    machine.execute(elf, *range, Some(("runtime:__cellscript_memcpy_fixed", width, record)))?;
                    require(
                        machine.calls.len() == usize::from(width != 0)
                            && machine.stores.is_empty()
                            && machine.registers[10] == Atom::Constant(0)
                            && (machine.jumps == [epilogue.range.start]
                                || (machine.jumps.is_empty() && range.end == epilogue.range.start)),
                        "return fails to copy the exact width before frame teardown",
                    )?;
                    require(width != 0 || machine.loads.is_empty(), "empty result dereferences storage")?;
                    uses_copy |= width != 0;
                }
            }
            _ => return Err(error("missing or unexpected fixed result ABI extension")),
        }
        if !entry.fixed_result_calls.is_empty() {
            let typed = typed.ok_or_else(|| error("result caller has no typed entry"))?;
            for call in &entry.fixed_result_calls {
                validate_call(record, elf, entry, typed, call, &mut checked_calls, &aliases)?;
            }
        }
        validate_protected_stack_regions(record, elf, entry)?;
    }
    // Require coverage from actual decoded call targets as well as producer
    // records. Removing a call extension cannot bypass admission.
    for flow in &elf.control_flow {
        let Some(callee) = record.entries.iter().find(|entry| {
            entry.fixed_result_abi.is_some()
                && record.blocks.iter().any(|block| block.id == entry.entry_block && block.range.start == flow.target)
        }) else {
            continue;
        };
        let word = elf
            .instructions
            .iter()
            .find(|instruction| instruction.address == flow.address)
            .map(|instruction| instruction.word)
            .unwrap_or(0);
        require(
            word & 0x7f == 0x67 && (word >> 7) & 31 == 1 && checked_calls.contains(&flow.address),
            &format!("unbound machine call to {}", callee.id),
        )?;
    }
    if uses_copy {
        validate_memcpy(record, elf)?;
    }
    Ok(())
}

fn validate_source(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    entry: &LoweringEntry,
    typed: &TypedSemanticEntry,
    abi: &FixedResultAbi,
    source: &FixedResultSource,
    aliases: &BTreeMap<&str, String>,
) -> Result<(), CheckerError> {
    owned_range(record, &entry.id, source.range)?;
    let local = typed
        .locals
        .iter()
        .find(|local| local.source_id == source.source_local)
        .ok_or_else(|| error("source pointer lacks a typed local"))?;
    let base = typed
        .locals
        .iter()
        .find(|local| local.source_id == source.base_local)
        .ok_or_else(|| error("source pointer lacks a typed base local"))?;
    require(
        same_nominal_type(&local.ty, &abi.type_name, aliases)
            && typed.blocks.iter().any(|block| {
                block.operations.iter().any(|operation| {
                    operation.opcode == "return" && operation.operands.iter().any(|operand| operand.local == Some(local.id))
                })
            }),
        "source local differs from the typed return operand",
    )?;
    if base.id == local.id {
        require(source.field_offset == 0, "direct source has a field offset")?;
    } else {
        let layout = record
            .typed_semantics
            .types
            .iter()
            .find(|layout| canonical_abi_type(&layout.name) == canonical_abi_type(&base.ty))
            .ok_or_else(|| error("source base has no fixed layout"))?;
        require(
            layout.encoded_size.is_some()
                && typed.blocks.iter().flat_map(|block| &block.operations).any(|operation| {
                    let TypedSemanticOperationDetail::Field { name } = &operation.detail else { return false };
                    operation.destinations.contains(&local.id)
                        && operation.operands.iter().any(|operand| operand.local == Some(base.id))
                        && layout.fields.iter().any(|field| {
                            &field.name == name
                                && field.offset == source.field_offset
                                && field.width_bytes == Some(abi.width_bytes)
                                && same_nominal_type(&field.ty, &local.ty, aliases)
                        })
                }),
            "source field differs from its typed layout",
        )?;
    }
    // Aggregate-bearing helpers are outside the scalar-only slot allocator:
    // their local pointer slots retain the deterministic source-id * 8 layout.
    let slot =
        source.base_local.checked_mul(8).and_then(|slot| i64::try_from(slot).ok()).ok_or_else(|| error("source slot overflows"))?;
    require(
        slot >= 0
            && slot as u64 + 8 <= u64::from(abi.owned_frame_bytes.saturating_sub(16))
            && slot != i64::from(abi.saved_pointer_offset),
        "source slot escapes its owned frame",
    )?;
    let mut machine = Machine::new();
    machine.source_slot = Some(slot);
    machine.execute(elf, source.range, None)?;
    require(
        machine.registers[10] == Atom::SourceOffset(i64::from(source.field_offset))
            && machine.loads == [(10, Atom::Stack(slot))]
            && machine.stores.is_empty()
            && machine.calls.is_empty()
            && machine.jumps.is_empty()
            && !machine.returned,
        "source pointer load or field offset changed",
    )
}

fn owned_range(record: &VerifiedLoweringRecord, owner: &str, range: MachineRange) -> Result<(), CheckerError> {
    require(
        !range.is_empty()
            && record.text_range.contains_range(range)
            && range.start.is_multiple_of(4)
            && range.end.is_multiple_of(4)
            && (range.start..range.end)
                .step_by(4)
                .all(|address| record.blocks.iter().any(|block| block.owner_entry == owner && block.range.contains(address))),
        "machine range escapes its declared owner",
    )
}

fn validate_owned_frame(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    entry: &LoweringEntry,
    width: u32,
) -> Result<(), CheckerError> {
    let block = record.blocks.iter().find(|block| block.id == entry.entry_block).ok_or_else(|| error("missing frame prologue"))?;
    require(
        width >= 16
            && width.is_multiple_of(16)
            && width <= entry.frame_size_bytes
            && elf
                .stack_adjustments
                .iter()
                .find(|adjustment| block.range.contains(adjustment.address))
                .is_some_and(|adjustment| adjustment.delta == -i64::from(width)),
        "owned frame differs from its decoded prologue",
    )
}

fn validate_call(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    owner: &LoweringEntry,
    typed: &TypedSemanticEntry,
    call: &FixedResultCall,
    checked: &mut BTreeSet<u64>,
    aliases: &BTreeMap<&str, String>,
) -> Result<(), CheckerError> {
    validate_owned_frame(record, elf, owner, call.owned_frame_bytes)?;
    let callee = record.entries.iter().find(|entry| entry.id == call.callee).ok_or_else(|| error("missing result callee"))?;
    let abi = callee.fixed_result_abi.as_ref().ok_or_else(|| error("call target has no result ABI"))?;
    let expected_outgoing = (abi.hidden_argument_index + 1)
        .saturating_sub(8)
        .checked_mul(8)
        .ok_or_else(|| error("outgoing size overflows"))?
        .next_multiple_of(16);
    require(
        call.schema == FIXED_RESULT_ABI_SCHEMA
            && call.width_bytes == abi.width_bytes
            && call.hidden_argument_index == abi.hidden_argument_index
            && call.outgoing_stack_bytes == expected_outgoing,
        "caller and callee disagree on width or expanded argument placement",
    )?;
    let local = typed
        .locals
        .iter()
        .find(|local| local.id == call.destination_local)
        .ok_or_else(|| error("missing typed result destination"))?;
    require(
        same_nominal_type(&local.ty, &abi.type_name, aliases)
            && typed.blocks.iter().flat_map(|block| &block.operations).any(|operation| {
                operation.destinations.contains(&local.id)
                    && operation.call.as_ref().is_some_and(|target| format!("helper:{}", target.target) == callee.id)
            }),
        "machine call disagrees with its typed result destination",
    )?;
    require(
        call.scalar_region_bytes.is_multiple_of(8)
            && call.scalar_region_bytes <= call.buffer_region_end
            && call.buffer_region_end <= call.owned_frame_bytes.saturating_sub(16)
            && call.pointer_slot_offset.is_multiple_of(8)
            && call.pointer_slot_offset.checked_add(8).is_some_and(|end| end <= call.scalar_region_bytes),
        "result pointer overlaps aggregate buffers or frame trailer",
    )?;
    let mut previous_end = call.scalar_region_bytes;
    let mut seen = BTreeSet::new();
    let mut selected = false;
    for buffer in &call.buffers {
        let buffer_local = typed
            .locals
            .iter()
            .find(|candidate| candidate.source_id == buffer.source_local)
            .ok_or_else(|| error("buffer lacks a typed local"))?;
        let width = committed_state_type_width(&buffer_local.ty, &record.typed_semantics.types)
            .ok_or_else(|| error("buffer layout is not fixed"))?;
        require(
            seen.insert(buffer.source_local)
                && width == u64::from(buffer.width_bytes)
                && buffer.offset.is_multiple_of(8)
                && buffer.offset >= previous_end,
            "buffer widths or ownership intervals conflict",
        )?;
        let aligned_width =
            buffer.width_bytes.checked_add(7).map(|width| width & !7).ok_or_else(|| error("buffer extent overflows"))?;
        previous_end = buffer.offset.checked_add(aligned_width).ok_or_else(|| error("buffer extent overflows"))?;
        require(previous_end <= call.buffer_region_end, "buffer exceeds aggregate region")?;
        if buffer.source_local == local.source_id {
            require(
                buffer.offset == call.buffer_offset && buffer.width_bytes == call.width_bytes,
                "result buffer differs from its typed destination extent",
            )?;
            selected = true;
        }
    }
    require(selected && call.buffer_offset >= call.scalar_region_bytes, "result buffer overlaps scalar/witness pointer slots")?;
    owned_range(record, &owner.id, call.setup_range)?;
    owned_range(record, &owner.id, call.receive_range)?;
    require(call.setup_range.end == call.receive_range.start, "result receive is detached from its call")?;
    let mut machine = Machine::new();
    machine.hidden = Some((call.hidden_argument_index, i64::from(call.buffer_offset), i64::from(call.outgoing_stack_bytes)));
    machine.execute(elf, call.setup_range, Some((&callee.id, 0, record)))?;
    require(
        machine.calls.len() == 1 && machine.registers[2] == Atom::Stack(0),
        "caller fails to restore its outgoing argument frame",
    )?;
    let expected_store = if call.hidden_argument_index >= 8 {
        vec![(
            Atom::Stack(i64::from((call.hidden_argument_index - 8) * 8) - i64::from(call.outgoing_stack_bytes)),
            Atom::Stack(i64::from(call.buffer_offset)),
        )]
    } else {
        Vec::new()
    };
    require(machine.stores == expected_store, "hidden pointer overwrites an ordinary outgoing argument")?;
    require(checked.insert(machine.calls[0]), "duplicate machine call binding")?;
    machine.stores.clear();
    machine.execute(elf, call.receive_range, None)?;
    require(
        machine.stores == [(Atom::Stack(i64::from(call.pointer_slot_offset)), Atom::Stack(i64::from(call.buffer_offset)))],
        "callee pointer escapes instead of the caller-owned buffer",
    )
}

/// Result buffers have a single machine writer: the bound callee. Direct
/// caller-frame stores and syscall/copy destinations must not overlap them.
/// Likewise, the saved hidden destination is written only by its admission
/// spill. Track constant SP address calculations, including large offsets.
fn validate_protected_stack_regions(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    entry: &LoweringEntry,
) -> Result<(), CheckerError> {
    if entry.fixed_result_abi.is_none() && entry.fixed_result_calls.is_empty() {
        return Ok(());
    }
    let check_write = |address: Atom, width: u64, pc: u64| -> Result<(), CheckerError> {
        let Atom::Stack(start) = address else { return Ok(()) };
        let end = start
            .checked_add(i64::try_from(width).map_err(|_| error("write width overflows"))?)
            .ok_or_else(|| error("write interval overflows"))?;
        for call in &entry.fixed_result_calls {
            let buffer_start = i64::from(call.buffer_offset);
            let buffer_end = buffer_start + i64::from(call.width_bytes);
            require(
                call.width_bytes == 0 || start >= buffer_end || end <= buffer_start,
                "result buffer overlaps a caller store or runtime destination",
            )?;
        }
        if let Some(abi) = &entry.fixed_result_abi {
            let saved = i64::from(abi.saved_pointer_offset);
            require(
                start >= saved + 8 || end <= saved || (start == saved && width == 8 && abi.save_range.contains(pc)),
                "saved hidden pointer is overwritten outside its admission spill",
            )?;
        }
        Ok(())
    };
    for block in record.blocks.iter().filter(|block| block.owner_entry == entry.id) {
        let mut registers = [Atom::Unknown; 32];
        registers[0] = Atom::Constant(0);
        let frame = entry
            .fixed_result_abi
            .as_ref()
            .map(|abi| abi.owned_frame_bytes)
            .or_else(|| entry.fixed_result_calls.first().map(|call| call.owned_frame_bytes))
            .expect("entry has fixed result evidence");
        registers[2] = Atom::Stack(if block.id == entry.entry_block { i64::from(frame) } else { 0 });
        let mut memory = BTreeMap::new();
        for instruction in elf.instructions.iter().filter(|instruction| block.range.contains(instruction.address)) {
            let word = instruction.word;
            let rd = ((word >> 7) & 31) as usize;
            let rs1 = ((word >> 15) & 31) as usize;
            let rs2 = ((word >> 20) & 31) as usize;
            let funct3 = (word >> 12) & 7;
            let immediate = i64::from((word as i32) >> 20);
            match word & 0x7f {
                0x13 if funct3 == 0 => registers[rd] = add(registers[rs1], Atom::Constant(immediate)),
                0x1b if funct3 == 0 => {
                    registers[rd] = match add(registers[rs1], Atom::Constant(immediate)) {
                        Atom::Constant(value) => Atom::Constant(i64::from(value as i32)),
                        _ => Atom::Unknown,
                    }
                }
                0x37 => registers[rd] = Atom::Constant(i64::from((word & 0xfffff000) as i32)),
                0x33 if funct3 == 0 && word >> 25 == 0 => registers[rd] = add(registers[rs1], registers[rs2]),
                0x23 if funct3 <= 3 => {
                    let imm = (((word >> 7) & 31) | ((word >> 25) << 5)) as i32;
                    let address = add(registers[rs1], Atom::Constant(i64::from((imm << 20) >> 20)));
                    check_write(address, 1 << funct3, instruction.address)?;
                    if let Atom::Stack(offset) = address {
                        memory.insert(offset, registers[rs2]);
                    }
                }
                0x73 if word == 0x00000073 => {
                    if let (Atom::Stack(buffer), Atom::Stack(size)) = (registers[10], registers[11]) {
                        if let Some(Atom::Constant(width)) = memory.get(&size) {
                            require(*width >= 0, "negative runtime buffer size")?;
                            check_write(Atom::Stack(buffer), *width as u64, instruction.address)?;
                        } else {
                            // At least reject a destination inside a protected
                            // region even if the syscall size is not constant.
                            check_write(Atom::Stack(buffer), 1, instruction.address)?;
                        }
                    }
                    registers[10] = Atom::Unknown;
                }
                0x67 | 0x6f if rd == 1 => {
                    if elf.control_flow.iter().any(|flow| {
                        flow.address == instruction.address
                            && record.blocks.iter().any(|target| {
                                target.owner_entry == "runtime:__cellscript_memcpy_fixed" && target.range.start == flow.target
                            })
                    }) && let Atom::Constant(width) = registers[12]
                    {
                        require(width >= 0, "negative copy size")?;
                        check_write(registers[11], width as u64, instruction.address)?;
                    }
                    for register in [1, 5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 28, 29, 30, 31] {
                        registers[register] = Atom::Unknown;
                    }
                }
                0x03 | 0x13 | 0x17 | 0x1b | 0x33 | 0x3b => registers[rd] = Atom::Unknown,
                _ => {}
            }
            registers[0] = Atom::Constant(0);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Atom {
    Unknown,
    Constant(i64),
    Stack(i64),
    ParentStack(i64),
    Incoming(u32),
    Source,
    SourceOffset(i64),
    Destination,
}

struct Machine {
    registers: [Atom; 32],
    stores: Vec<(Atom, Atom)>,
    loads: Vec<(usize, Atom)>,
    calls: Vec<u64>,
    jumps: Vec<u64>,
    saved: Option<i64>,
    source_slot: Option<i64>,
    hidden: Option<(u32, i64, i64)>,
    returned: bool,
}

impl Machine {
    fn new() -> Self {
        let mut registers = [Atom::Unknown; 32];
        registers[0] = Atom::Constant(0);
        registers[2] = Atom::Stack(0);
        Self {
            registers,
            stores: Vec::new(),
            loads: Vec::new(),
            calls: Vec::new(),
            jumps: Vec::new(),
            saved: None,
            source_slot: None,
            hidden: None,
            returned: false,
        }
    }
    fn incoming(index: u32) -> Self {
        let mut machine = Self::new();
        for register in 10..18 {
            machine.registers[register] = Atom::Incoming((register - 10) as u32);
        }
        machine.registers[8] = Atom::ParentStack(0);
        machine.hidden = Some((index, 0, 0));
        machine
    }
    fn execute(
        &mut self,
        elf: &ParsedElf,
        range: MachineRange,
        call: Option<(&str, u32, &VerifiedLoweringRecord)>,
    ) -> Result<(), CheckerError> {
        for instruction in elf.instructions.iter().filter(|instruction| range.contains(instruction.address)) {
            let word = instruction.word;
            let rd = ((word >> 7) & 31) as usize;
            let rs1 = ((word >> 15) & 31) as usize;
            let rs2 = ((word >> 20) & 31) as usize;
            let funct3 = (word >> 12) & 7;
            let immediate = i64::from((word as i32) >> 20);
            match word & 0x7f {
                0x13 if funct3 == 0 => self.registers[rd] = add(self.registers[rs1], Atom::Constant(immediate)),
                0x1b if funct3 == 0 => {
                    self.registers[rd] = match add(self.registers[rs1], Atom::Constant(immediate)) {
                        Atom::Constant(value) => Atom::Constant(i64::from(value as i32)),
                        _ => Atom::Unknown,
                    }
                }
                0x37 => self.registers[rd] = Atom::Constant(i64::from((word & 0xfffff000) as i32)),
                0x17 => self.registers[rd] = Atom::Unknown,
                0x33 if funct3 == 0 && word >> 25 == 0 => self.registers[rd] = add(self.registers[rs1], self.registers[rs2]),
                0x03 if funct3 == 3 => {
                    let address = add(self.registers[rs1], Atom::Constant(immediate));
                    self.loads.push((rd, address));
                    self.registers[rd] = match address {
                        Atom::ParentStack(offset) if offset >= 0 && offset % 8 == 0 => Atom::Incoming(8 + (offset / 8) as u32),
                        Atom::Stack(offset) if self.saved == Some(offset) => Atom::Destination,
                        Atom::Stack(offset) if self.source_slot == Some(offset) => Atom::SourceOffset(0),
                        _ => Atom::Unknown,
                    };
                }
                0x23 if funct3 == 3 => {
                    let imm = (((word >> 7) & 31) | ((word >> 25) << 5)) as i32;
                    let imm = (imm << 20) >> 20;
                    self.stores.push((add(self.registers[rs1], Atom::Constant(i64::from(imm))), self.registers[rs2]));
                }
                0x67 if rd == 1 => {
                    let (target, width, record) = call.ok_or_else(|| error("unexpected call in result ABI range"))?;
                    let target_address = record
                        .entries
                        .iter()
                        .find(|entry| entry.id == target)
                        .and_then(|entry| record.blocks.iter().find(|block| block.id == entry.entry_block))
                        .map(|block| block.range.start)
                        .ok_or_else(|| error("missing machine call target"))?;
                    require(
                        elf.control_flow.iter().any(|flow| flow.address == instruction.address && flow.target == target_address),
                        "copy/call target was substituted",
                    )?;
                    if target == "runtime:__cellscript_memcpy_fixed" {
                        require(
                            self.registers[10] == Atom::Source
                                && self.registers[11] == Atom::Destination
                                && self.registers[12] == Atom::Constant(i64::from(width)),
                            "source pointer was clobbered or destination/copy length changed",
                        )?;
                    } else {
                        let (index, offset, outgoing) = self.hidden.ok_or_else(|| error("missing hidden argument placement"))?;
                        require(self.registers[2] == Atom::Stack(-outgoing), "outgoing stack reservation changed")?;
                        if index < 8 {
                            require(
                                self.registers[10 + index as usize] == Atom::Stack(offset),
                                "hidden result register points outside caller storage",
                            )?;
                        }
                    }
                    self.calls.push(instruction.address);
                    for register in [1, 5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 28, 29, 30, 31] {
                        self.registers[register] = Atom::Unknown;
                    }
                }
                0x6f if rd == 0 => {
                    let target = elf
                        .control_flow
                        .iter()
                        .find(|flow| flow.address == instruction.address)
                        .ok_or_else(|| error("missing return jump"))?
                        .target;
                    self.jumps.push(target);
                    require(instruction.address + 4 == range.end, "instructions follow the return jump")?;
                }
                0x67 if word == 0x00008067 => {
                    self.returned = true;
                    require(instruction.address + 4 == range.end, "instructions follow return")?;
                }
                _ => return Err(error(format!("unsupported instruction in ABI range at {:#x}", instruction.address))),
            }
            self.registers[0] = Atom::Constant(0);
        }
        Ok(())
    }
}

fn add(left: Atom, right: Atom) -> Atom {
    match (left, right) {
        (Atom::Constant(left), Atom::Constant(right)) => left.checked_add(right).map_or(Atom::Unknown, Atom::Constant),
        (Atom::Stack(left), Atom::Constant(right)) | (Atom::Constant(right), Atom::Stack(left)) => {
            left.checked_add(right).map_or(Atom::Unknown, Atom::Stack)
        }
        (Atom::ParentStack(left), Atom::Constant(right)) => left.checked_add(right).map_or(Atom::Unknown, Atom::ParentStack),
        (Atom::SourceOffset(left), Atom::Constant(right)) => left.checked_add(right).map_or(Atom::Unknown, Atom::SourceOffset),
        (atom, Atom::Constant(0)) => atom,
        _ => Atom::Unknown,
    }
}

fn validate_memcpy(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let entry = record
        .entries
        .iter()
        .find(|entry| entry.id == "runtime:__cellscript_memcpy_fixed")
        .ok_or_else(|| error("missing bounded copy helper"))?;
    let base = record
        .blocks
        .iter()
        .find(|block| block.id == entry.entry_block)
        .ok_or_else(|| error("missing copy helper block"))?
        .range
        .start;
    let words = (0..9)
        .map(|index| {
            elf.instructions
                .iter()
                .find(|instruction| instruction.address == base + index * 4)
                .map(|instruction| instruction.word)
                .ok_or_else(|| error("truncated copy helper"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    require(
        words[0] & 0x01fff07f == 0x00060063
            && words[1] == 0x00054283
            && words[2] == 0x00558023
            && words[3] == 0x00150513
            && words[4] == 0x00158593
            && words[5] == 0xfff60613
            && words[6] & 0x01fff07f == 0x00061063
            && words[7] == 0x00000513
            && words[8] == 0x00008067
            && elf.control_flow.iter().any(|flow| flow.address == base && flow.target == base + 28)
            && elf.control_flow.iter().any(|flow| flow.address == base + 24 && flow.target == base + 4),
        "bounded byte copy helper changed",
    )
}
