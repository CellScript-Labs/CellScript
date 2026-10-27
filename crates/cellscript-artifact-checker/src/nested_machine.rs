//! Independent instruction contract for bounded nested witness decoders.
//! This binds canonical list/element decoders to typed parameter bounds. It
//! binds decoder inputs to incoming ABI words and an unavoidable entry prefix.
//! Single-entry and policy witness transports are checked separately below.
//! Typed reads are checked in `uses`, witness/Cell CFG effects in `lifetime`.
//! Typed Cell loads and complete tables are checked in `cells`; `cell_args`
//! binds borrowed Cell, whole-value and derived-view helper calls, including both
//! ABI words' source identity. `outputs` checks nested successor comparisons;
//! `scalar_machine` composes typed scalar definitions, calls, branches and results.
//! This bounded contract does not claim arbitrary whole-language equivalence.
use crate::bounded_nested::{ElementShape, NestedValueContract};
use crate::{CheckerError, CheckerRejectionCode, ParsedElf, VerifiedLoweringRecord};
use std::collections::{BTreeMap, BTreeSet};

mod arena_calls;
mod cell_args;
mod cell_read_hashes;
pub(crate) mod cells;
mod context_hashes;
mod exact_cache;
mod guarded_writes;
mod hash_fields;
mod lifetime;
mod out_point_hash;
mod outputs;
mod parameters;
mod scalar_status;
mod script_hash_fields;
mod uses;
mod view_calls;
mod view_values;
mod witness_exact_hash;

pub(crate) use cell_read_hashes::validate_all as validate_cell_data_reads;
pub(crate) use hash_fields::validate as validate_hash_property_runtime;
pub(crate) use scalar_status::validate_all as validate_scalar_status_calls;

const PREFIX: &str = ".Lnested_param_codec_";

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("nested value machine contract: {}", message.into()),
    )
}

pub(crate) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    terminal_sink: Option<&str>,
    selections: &crate::cell_selection::machine::Checked,
    allocated: &crate::allocated_values::machine::Checked,
) -> Result<BTreeMap<u64, u64>, CheckerError> {
    let has_values = record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.locals)
        .any(|local| crate::checker::is_nested_source_type(&local.ty));
    let has_markers = record.blocks.iter().any(|block| {
        block.machine_label.as_deref().is_some_and(|label| label.starts_with(PREFIX) || label.starts_with(uses::PREFIX))
            || !block.nested_read_labels.is_empty()
            || !block.nested_cell_labels.is_empty()
            || !block.nested_output_labels.is_empty()
            || !block.cell_view_value_labels.is_empty()
            || block.allocated_lifecycle_labels.iter().any(|label| label.starts_with(".Lallocated_call_"))
    });
    let has_cell_values =
        record.typed_semantics.types.iter().any(|ty| ty.fields.iter().any(|field| crate::checker::is_nested_source_type(&field.ty)));
    let has_view_parameters = record.typed_semantics.entries.iter().any(|entry| {
        entry.kind == "helper"
            && entry.params.iter().any(|param| param.ty.starts_with("InputView<") || param.ty.starts_with("OutputView<"))
    });
    let has_coordinates =
        record.typed_semantics.entries.iter().flat_map(|entry| &entry.locals).any(|local| view_values::is_coordinate(&local.ty));
    let owners = crate::allocated_values::owner_types(&record.typed_semantics);
    let has_allocated_parameters = record.typed_semantics.entries.iter().any(|entry| {
        entry.kind == "helper" && entry.params.iter().any(|param| param.ty.strip_prefix('&').is_some_and(|ty| owners.contains(ty)))
    });
    let has_bounded_helpers =
        record.typed_semantics.entries.iter().any(|entry| entry.kind == "helper" && crate::scalar_machine::covered_entry(entry));
    if !has_values
        && !has_markers
        && !has_cell_values
        && !has_view_parameters
        && !has_coordinates
        && !has_allocated_parameters
        && !has_bounded_helpers
        && allocated.constructors.is_empty()
        && selections.sites.is_empty()
        && selections.uses.is_empty()
        && selections.data.is_empty()
    {
        return Ok(BTreeMap::new());
    }
    let normalized = crate::policy_machine::normalize_relaxed_branches(record, elf).map_err(invalid)?;
    let (record, elf) = normalized.as_ref().map_or((record, elf), |(record, elf)| (record, elf));
    let cell_buffers = cells::validate(record, elf)?;
    let types = record.typed_semantics.types.iter().map(|ty| (ty.name.as_str(), ty)).collect();
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect::<BTreeMap<_, _>>();
    let owners = record.blocks.iter().map(|block| (block.range.start, block)).collect::<BTreeMap<_, _>>();
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for flow in &elf.control_flow {
        incoming.entry(flow.target).or_default().push(flow.address);
    }
    let mut markers = BTreeMap::new();
    for block in &record.blocks {
        if let Some(suffix) = block.machine_label.as_deref().and_then(|label| label.strip_prefix(PREFIX)) {
            let (source, ordinal) = suffix.split_once('_').ok_or_else(|| invalid("invalid decoder marker"))?;
            let source = source.parse::<u64>().map_err(|_| invalid("invalid decoder source local"))?;
            ordinal.parse::<u64>().map_err(|_| invalid("invalid decoder ordinal"))?;
            if markers.insert((block.owner_entry.as_str(), source), block).is_some() {
                return Err(invalid("duplicate parameter decoder"));
            }
        }
    }
    let mut checked = BTreeSet::new();
    let mut root_sizes = BTreeMap::new();
    let mut lifetime_seeds = BTreeMap::new();
    for entry in &record.typed_semantics.entries {
        if !entry.params.iter().any(|param| crate::checker::is_nested_source_type(&param.ty)) {
            continue;
        }
        let lowering = record.entries.iter().find(|candidate| candidate.id == entry.id).ok_or_else(|| invalid("missing entry"))?;
        let entry_start = record
            .blocks
            .iter()
            .find(|block| block.id == lowering.entry_block)
            .ok_or_else(|| invalid("missing entry block"))?
            .range
            .start;
        // A caller must enter through the prologue. Calling a later block would
        // bypass validation even if every decoder instruction were authentic.
        for flow in &elf.control_flow {
            let target = owners.range(..=flow.target).next_back().map(|(_, block)| *block);
            let source = owners.range(..=flow.address).next_back().map(|(_, block)| *block);
            if target.is_some_and(|block| block.owner_entry == entry.id && block.range.contains(flow.target))
                && !source.is_some_and(|block| block.owner_entry == entry.id && block.range.contains(flow.address))
                && flow.target != entry_start
            {
                return Err(invalid("external control flow bypasses the entry prologue"));
            }
        }
        let frame = owned_frame(record, elf, lowering)?;
        let mut prefix = PrefixMachine::new(entry_start, frame);
        let mut previous_decoder_start = None;
        let mut abi_index = 0u32;
        let mut spans = BTreeMap::new();
        for param in &entry.params {
            let parameter_index = abi_index;
            abi_index = abi_index
                .checked_add(crate::fixed_results::parameter_word_width(record, entry, param))
                .ok_or_else(|| invalid("argument expansion overflows"))?;
            if !crate::checker::is_nested_source_type(&param.ty) {
                continue;
            }
            let (contract, derived) = crate::checker::nested_parameter_contract(&param.ty, &types)
                .ok_or_else(|| invalid("parameter has no finite typed nested contract"))?;
            if derived && entry.kind != "helper" {
                return Err(invalid("derived nested views cannot enter through an external ABI"));
            }
            let local = entry
                .locals
                .iter()
                .find(|local| local.id == param.binding_id)
                .ok_or_else(|| invalid("parameter has no source local"))?;
            let start = markers
                .get(&(entry.id.as_str(), local.source_id))
                .ok_or_else(|| invalid(format!("missing decoder for {}.{}", entry.name, param.name)))?;
            if !checked.insert(start.id.as_str()) {
                return Err(invalid("duplicate parameter decoder"));
            }
            let prefix_start = prefix.pc;
            prefix.execute_until(start.range.start, &words)?;
            if incoming.range(prefix_start..=start.range.start).any(|(target, sources)| {
                sources.iter().any(|source| {
                    if *target != prefix_start {
                        return true;
                    }
                    if let Some(previous) = previous_decoder_start {
                        return *source < previous || *source >= prefix_start;
                    }
                    owners.range(..=*source).next_back().is_some_and(|(_, block)| block.owner_entry == entry.id)
                })
            }) {
                return Err(invalid("control flow enters parameter spill/reload instructions"));
            }
            if prefix.registers[10] != Origin::Incoming(parameter_index)
                || prefix.registers[11] != Origin::Incoming(parameter_index + 1)
            {
                return Err(invalid("decoder pointer/length do not match the declared parameter ABI"));
            }
            let pointer_slot = i64::try_from(local.source_id.checked_mul(8).ok_or_else(|| invalid("nested source slot overflows"))?)
                .map_err(|_| invalid("nested source slot exceeds i64"))?
                - i64::from(frame);
            if prefix.memory.get(&pointer_slot) != Some(&Origin::Incoming(parameter_index)) {
                return Err(invalid("nested source local differs from the actual received pointer"));
            }
            let sizes = prefix.memory.iter().filter(|(_, value)| **value == Origin::Incoming(parameter_index + 1)).collect::<Vec<_>>();
            let [(size, _)] = sizes.as_slice() else { return Err(invalid("nested parameter has no unique received size slot")) };
            let size = u32::try_from(**size + i64::from(frame)).map_err(|_| invalid("nested size slot outside frame"))?;
            root_sizes.insert((entry.id.clone(), local.source_id), size);
            spans.insert(parameter_index, local.source_id);
            let mut machine =
                Decoder { pc: start.range.start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
            let failure = machine.check_parameter(&contract, derived)?;
            if !record.verifier_failure_exits.iter().any(|exit| exit.address == failure && exit.code == 16) {
                return Err(invalid("malformed value does not jump to the checked fatal error 16"));
            }
            for (address, _) in words.range(prefix_start..machine.pc) {
                if !owners
                    .range(..=*address)
                    .next_back()
                    .is_some_and(|(_, block)| block.owner_entry == entry.id && block.range.contains(*address))
                {
                    return Err(invalid("decoder crosses its owning entry"));
                }
            }
            if incoming
                .range(start.range.start + 4..machine.pc)
                .any(|(_, sources)| sources.iter().any(|address| *address < start.range.start || *address >= machine.pc))
            {
                return Err(invalid("external control flow enters the middle of a decoder"));
            }
            // The fully checked decoder preserves a0/a1, SP/FP and all memory;
            // all other registers it writes lose their incoming-word identity.
            prefix.pc = machine.pc;
            previous_decoder_start = Some(start.range.start);
            for register in [5, 6, 7, 28, 12, 13, 14, 15] {
                prefix.registers[register] = Origin::Unknown;
            }
        }
        let roots = root_sizes.keys().filter(|(owner, _)| owner == &entry.id).map(|(_, source)| *source).collect();
        lifetime_seeds.insert(
            entry.id.clone(),
            lifetime::Seed {
                machine: prefix,
                roots,
                spans,
                view_slots: Vec::new(),
                allocated_slots: Vec::new(),
                allocated_spans: BTreeMap::new(),
            },
        );
    }
    if markers.values().any(|block| !checked.contains(block.id.as_str())) {
        return Err(invalid("machine decoder has no typed parameter contract"));
    }
    let view_calls = view_calls::validate(record, elf, &mut lifetime_seeds)?;
    parameters::allocated(record, elf, &mut lifetime_seeds)?;
    parameters::scalars(record, elf, &lifetime_seeds)?;
    crate::entry_codec::verify_nested(record, elf)?;
    for (owner, sites) in &selections.data {
        for result in sites.values().filter_map(|site| site.nested.as_ref()) {
            if root_sizes.insert((owner.clone(), result.root), result.length).is_some() {
                return Err(invalid("nested data result aliases another root"));
            }
        }
    }
    let terminal_sink = terminal_sink.and_then(|id| record.blocks.iter().find(|block| block.id == id)).map(|block| block.range.start);
    uses::validate(
        record,
        elf,
        &root_sizes,
        &lifetime_seeds,
        lifetime::CellEffects { cells: &cell_buffers, selections, view_calls: &view_calls, allocated },
        terminal_sink,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    Unknown,
    Constant(i64),
    Stack(i64),
    Incoming(u32),
    ReturnAddress,
    ParentFrame,
    CalleeSaved(u32),
}

// frame_size_bytes also accumulates outgoing-call adjustments. Derive private
// ownership from the decoded prologue, and cross-check the fixed-result ABI
// when one exists (fixed_results::validate already checked that ABI).
pub(crate) fn owned_frame(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    entry: &crate::LoweringEntry,
) -> Result<u32, CheckerError> {
    let block =
        record.blocks.iter().find(|block| block.id == entry.entry_block).ok_or_else(|| invalid("missing owned-frame prologue"))?;
    let adjustment = elf.stack_adjustments.iter().find(|adjustment| block.range.contains(adjustment.address));
    let frame = match adjustment {
        Some(adjustment) if adjustment.delta < 0 => {
            u32::try_from(adjustment.delta.unsigned_abs()).map_err(|_| invalid("owned frame overflows"))?
        }
        None if entry.frame_size_bytes == 0 => 0,
        _ => return Err(invalid("owned frame has no allocating prologue")),
    };
    if !frame.is_multiple_of(16) || frame > entry.frame_size_bytes {
        return Err(invalid("owned frame exceeds its bounded declaration"));
    }
    let fixed = entry
        .fixed_result_abi
        .as_ref()
        .map(|abi| abi.owned_frame_bytes)
        .or_else(|| entry.fixed_result_calls.first().map(|call| call.owned_frame_bytes));
    if fixed.is_some_and(|fixed| fixed != frame) {
        return Err(invalid("owned frame differs from its checked fixed-result ABI"));
    }
    Ok(frame)
}

impl Origin {
    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Constant(left), Self::Constant(right)) => Self::Constant(left.wrapping_add(right)),
            (Self::Stack(offset), Self::Constant(delta)) | (Self::Constant(delta), Self::Stack(offset)) => {
                offset.checked_add(delta).map_or(Self::Unknown, Self::Stack)
            }
            (value, Self::Constant(0)) | (Self::Constant(0), value) => value,
            _ => Self::Unknown,
        }
    }
}

/// Straight-line prologue and spill interpreter. Addresses are relative to
/// incoming SP; negative addresses belong to this frame, nonnegative words to
/// caller stack arguments. No unknown store, control flow or memory alias is
/// accepted. The canonical decoder itself is summarized only after checking.
#[derive(Clone)]
struct PrefixMachine {
    pc: u64,
    frame: i64,
    registers: [Origin; 32],
    memory: BTreeMap<i64, Origin>,
}

impl PrefixMachine {
    fn new(pc: u64, frame: u32) -> Self {
        let mut registers = [Origin::Unknown; 32];
        registers[0] = Origin::Constant(0);
        registers[1] = Origin::ReturnAddress;
        registers[2] = Origin::Stack(0);
        registers[8] = Origin::ParentFrame;
        for register in [9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27] {
            registers[register] = Origin::CalleeSaved(register as u32);
        }
        for index in 0..8 {
            registers[10 + index] = Origin::Incoming(index as u32);
        }
        Self { pc, frame: i64::from(frame), registers, memory: BTreeMap::new() }
    }

    fn execute_until(&mut self, end: u64, words: &BTreeMap<u64, u32>) -> Result<(), CheckerError> {
        if end < self.pc {
            return Err(invalid("parameter decoders are not in declaration order"));
        }
        while self.pc < end {
            let word = *words.get(&self.pc).ok_or_else(|| invalid("missing parameter prefix instruction"))?;
            let rd = ((word >> 7) & 31) as usize;
            let base = ((word >> 15) & 31) as usize;
            let right = ((word >> 20) & 31) as usize;
            let kind = (word >> 12) & 7;
            let immediate = i64::from((word as i32) >> 20);
            match (word & 0x7f, kind) {
                (0x13, 0) => self.registers[rd] = self.registers[base].add(Origin::Constant(immediate)),
                (0x1b, 0) => {
                    self.registers[rd] = match self.registers[base] {
                        Origin::Constant(value) => Origin::Constant(i64::from(value.wrapping_add(immediate) as i32)),
                        _ => Origin::Unknown,
                    };
                }
                (0x37, _) => self.registers[rd] = Origin::Constant(i64::from((word & 0xfffff000) as i32)),
                (0x33, 0) if word >> 25 == 0 => self.registers[rd] = self.registers[base].add(self.registers[right]),
                (0x03, 3) => {
                    let Origin::Stack(offset) = self.registers[base].add(Origin::Constant(immediate)) else {
                        return Err(invalid("parameter reload has no stack provenance"));
                    };
                    if offset % 8 != 0 {
                        return Err(invalid("unaligned parameter reload"));
                    }
                    self.registers[rd] = if offset >= 0 {
                        let index = u32::try_from(offset / 8)
                            .ok()
                            .and_then(|index| index.checked_add(8))
                            .ok_or_else(|| invalid("caller argument index overflows"))?;
                        Origin::Incoming(index)
                    } else {
                        *self.memory.get(&offset).ok_or_else(|| invalid("parameter reload reads an uninitialized frame slot"))?
                    };
                }
                (0x23, 3) => {
                    let immediate = i64::from((((word >> 7) & 31) | ((word >> 25) << 5)) as i32) << 52 >> 52;
                    let Origin::Stack(offset) = self.registers[base].add(Origin::Constant(immediate)) else {
                        return Err(invalid("parameter spill has no stack provenance"));
                    };
                    if offset % 8 != 0 || offset < -self.frame || offset > -8 {
                        return Err(invalid("parameter spill leaves the owned frame"));
                    }
                    self.memory.insert(offset, self.registers[right]);
                }
                _ => return Err(invalid(format!("unsupported or branching parameter prefix at {:#x}", self.pc))),
            }
            self.registers[0] = Origin::Constant(0);
            self.pc += 4;
        }
        if self.registers[2] != Origin::Stack(-self.frame) || self.registers[8] != Origin::Stack(0) {
            return Err(invalid("parameter prefix changed the declared frame"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Label {
    Invalid,
    Done,
    Item,
    EndReady,
    Ready,
    HashDirect,
    HashDecoded,
    HashBadOutput,
    HashFailed,
    HashCopy,
    HashCopied,
    Cache(u8),
}

struct Decoder<'a> {
    pc: u64,
    words: &'a BTreeMap<u64, u32>,
    targets: &'a BTreeMap<u64, u64>,
    labels: BTreeMap<Label, u64>,
    branches: Vec<(Label, u64)>,
}

impl Decoder<'_> {
    fn word(&mut self) -> Result<u32, CheckerError> {
        let word = *self.words.get(&self.pc).ok_or_else(|| invalid("truncated decoder"))?;
        self.pc += 4;
        Ok(word)
    }
    fn exact(&mut self, expected: u32) -> Result<(), CheckerError> {
        let at = self.pc;
        let actual = self.word()?;
        if actual != expected {
            return Err(invalid(format!("instruction differs at {at:#x}: expected {expected:#010x}, found {actual:#010x}")));
        }
        Ok(())
    }
    fn immediate(&mut self, opcode: u32, kind: u32, rd: u32, base: u32, value: i32) -> Result<(), CheckerError> {
        self.exact(((value as u32 & 0xfff) << 20) | (base << 15) | (kind << 12) | (rd << 7) | opcode)
    }
    fn addi(&mut self, rd: u32, base: u32, value: i32) -> Result<(), CheckerError> {
        self.immediate(0x13, 0, rd, base, value)
    }
    fn li(&mut self, rd: u32, value: u32) -> Result<(), CheckerError> {
        // The finite profile uses only nonnegative constants <= 4096.
        if value <= 2047 {
            return self.addi(rd, 0, value as i32);
        }
        if value > 4096 {
            return Err(invalid("constant exceeds decoder profile"));
        }
        let high = (value + 2048) >> 12;
        self.exact((high << 12) | (rd << 7) | 0x37)?;
        let low = value as i32 - ((high as i32) << 12);
        if low != 0 {
            self.addi(rd, rd, low)?;
        }
        Ok(())
    }
    /// A bounded constant program must initialize its register from x0 or LUI;
    /// an incoming machine register is never implicitly zero.
    fn constant(&mut self, register: u32, expected: u64) -> Result<(), CheckerError> {
        let mut value: Option<u64> = None;
        for _ in 0..16 {
            let word = self.word()?;
            if word >> 7 & 31 != register {
                return Err(invalid("constant writes another register"));
            }
            let base = word >> 15 & 31;
            let source = if word & 0x7f == 0x37 || base == 0 {
                0
            } else if base == register {
                value.ok_or_else(|| invalid("constant reads an uninitialized register"))?
            } else {
                return Err(invalid("constant reads another register"));
            };
            let actual = match word & 0x707f {
                0x13 => source.wrapping_add(((word as i32) >> 20) as u64),
                0x1b => source.wrapping_add(((word as i32) >> 20) as u64) as i32 as i64 as u64,
                0x1013 if word >> 26 == 0 => source << (word >> 20 & 63),
                _ if word & 0x7f == 0x37 => (word & 0xfffff000) as i32 as i64 as u64,
                _ => return Err(invalid("unchecked constant instruction")),
            };
            if actual == expected {
                return Ok(());
            }
            value = Some(actual);
        }
        Err(invalid("constant exceeds instruction budget"))
    }
    fn register(&mut self, kind: u32, high: u32, rd: u32, left: u32, right: u32) -> Result<(), CheckerError> {
        self.exact((high << 25) | (right << 20) | (left << 15) | (kind << 12) | (rd << 7) | 0x33)
    }
    fn add(&mut self, rd: u32, left: u32, right: u32) -> Result<(), CheckerError> {
        self.register(0, 0, rd, left, right)
    }
    fn sub(&mut self, rd: u32, left: u32, right: u32) -> Result<(), CheckerError> {
        self.register(0, 32, rd, left, right)
    }
    fn u32_le(&mut self, rd: u32, base: u32, offset: i32, scratch: u32) -> Result<(), CheckerError> {
        self.immediate(3, 4, rd, base, offset)?;
        for byte in 1..4 {
            self.immediate(3, 4, scratch, base, offset + byte)?;
            self.immediate(0x13, 1, scratch, scratch, byte * 8)?;
            self.register(6, 0, rd, rd, scratch)?;
        }
        Ok(())
    }
    fn branch(&mut self, kind: u32, left: u32, right: u32, label: Label) -> Result<(), CheckerError> {
        let at = self.pc;
        let word = self.word()?;
        if word & 0x7f != 0x63 || (word >> 12) & 7 != kind || (word >> 15) & 31 != left || (word >> 20) & 31 != right {
            return Err(invalid(format!("condition differs at {at:#x}")));
        }
        let target = *self.targets.get(&at).ok_or_else(|| invalid("branch has no decoded target"))?;
        self.branches.push((label, target));
        Ok(())
    }
    fn jump(&mut self) -> Result<u64, CheckerError> {
        let at = self.pc;
        if self.word()? & 0xfff != 0x6f {
            return Err(invalid("completion must be an unconditional non-call jump"));
        }
        self.targets.get(&at).copied().ok_or_else(|| invalid("jump target is missing"))
    }
    fn label(&mut self, label: Label) {
        self.labels.insert(label, self.pc);
    }

    fn body(&mut self, contract: &NestedValueContract) -> Result<(), CheckerError> {
        use Label::*;
        // a0/a1 are the immutable slice; a2 is the result count. All temporary
        // registers are explicit. No write, syscall or call can fit this body.
        self.li(12, 0)?;
        self.li(5, 4)?;
        self.branch(6, 11, 5, Invalid)?;
        self.li(5, contract.max_total_bytes)?;
        self.branch(6, 5, 11, Invalid)?;
        self.add(5, 10, 11)?;
        self.branch(6, 5, 10, Invalid)?;
        self.u32_le(5, 10, 0, 6)?;
        self.branch(1, 5, 11, Invalid)?;
        self.li(5, 4)?;
        self.branch(0, 11, 5, Done)?;
        self.li(5, 8)?;
        self.branch(6, 11, 5, Invalid)?;
        self.u32_le(14, 10, 4, 6)?;
        self.branch(6, 14, 5, Invalid)?;
        self.branch(6, 11, 14, Invalid)?;
        self.immediate(0x13, 7, 5, 14, 3)?;
        self.branch(1, 5, 0, Invalid)?;
        self.immediate(0x13, 5, 12, 14, 2)?;
        self.addi(12, 12, -1)?;
        self.li(5, contract.max_items)?;
        self.branch(6, 5, 12, Invalid)?;
        self.li(13, 0)?;
        self.label(Item);
        self.addi(5, 13, 1)?;
        self.addi(15, 11, 0)?;
        self.branch(0, 5, 12, EndReady)?;
        self.immediate(0x13, 1, 5, 5, 2)?;
        self.addi(5, 5, 4)?;
        self.add(5, 10, 5)?;
        self.u32_le(15, 5, 0, 6)?;
        self.label(EndReady);
        self.branch(6, 15, 14, Invalid)?;
        self.branch(6, 11, 15, Invalid)?;
        self.sub(7, 15, 14)?;
        self.add(5, 10, 14)?;
        self.element(&contract.element, contract.max_payload_bytes)?;
        self.addi(14, 15, 0)?;
        self.addi(13, 13, 1)?;
        self.branch(6, 13, 12, Item)?;
        self.label(Done);
        Ok(())
    }

    fn element(&mut self, element: &ElementShape, payload_bound: u32) -> Result<(), CheckerError> {
        use Label::Invalid;
        if let ElementShape::Record { fixed_field_widths } = element {
            let header = (fixed_field_widths.len() as u32 + 2) * 4;
            let payload = header + fixed_field_widths.iter().map(|width| u32::from(*width)).sum::<u32>();
            self.li(6, payload + 4)?;
            self.branch(6, 7, 6, Invalid)?;
            self.u32_le(6, 5, 0, 28)?;
            self.branch(1, 6, 7, Invalid)?;
            let mut offset = header;
            for field in 0..=fixed_field_widths.len() {
                self.u32_le(6, 5, 4 * (field as i32 + 1), 28)?;
                self.li(28, offset)?;
                self.branch(1, 6, 28, Invalid)?;
                if let Some(width) = fixed_field_widths.get(field) {
                    offset += u32::from(*width);
                }
            }
            self.add(5, 5, 28)?;
            self.sub(7, 7, 28)?;
        }
        self.li(6, 4)?;
        self.branch(6, 7, 6, Invalid)?;
        self.u32_le(6, 5, 0, 28)?;
        self.li(28, payload_bound)?;
        self.branch(6, 28, 6, Invalid)?;
        self.addi(6, 6, 4)?;
        self.branch(1, 6, 7, Invalid)?;
        Ok(())
    }

    fn view_body(&mut self, contract: &NestedValueContract) -> Result<(), CheckerError> {
        use Label::Invalid;
        let header = match &contract.element {
            ElementShape::Bytes {} => 4,
            ElementShape::Record { fixed_field_widths } => {
                (fixed_field_widths.len() as u32 + 2) * 4 + fixed_field_widths.iter().map(|width| u32::from(*width)).sum::<u32>() + 4
            }
        };
        self.li(5, 4)?;
        self.branch(6, 11, 5, Invalid)?;
        self.li(5, header + contract.max_payload_bytes)?;
        self.branch(6, 5, 11, Invalid)?;
        self.add(5, 10, 11)?;
        self.branch(6, 5, 10, Invalid)?;
        self.addi(5, 10, 0)?;
        self.addi(7, 11, 0)?;
        self.element(&contract.element, contract.max_payload_bytes)
    }

    fn check_parameter(&mut self, contract: &NestedValueContract, derived: bool) -> Result<u64, CheckerError> {
        use Label::*;
        if derived {
            self.view_body(contract)?;
        } else {
            self.body(contract)?;
        }
        let ready = self.jump()?;
        self.branches.push((Ready, ready));
        self.label(Invalid);
        let failure = self.jump()?;
        self.label(Ready);
        for (label, target) in &self.branches {
            if self.labels.get(label) != Some(target) {
                return Err(invalid("decoder branch target or loop structure differs"));
            }
        }
        Ok(failure)
    }
}
