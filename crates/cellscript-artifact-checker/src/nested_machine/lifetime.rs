//! Must-initialization and memory-effect proof for decoded witness and Cell views.
//! Cell roots enter only through independently checked load/codec summaries.
use super::*;
use crate::checker::is_nested_source_type;
use std::collections::VecDeque;

mod copies;

#[derive(Clone)]
pub(super) struct Seed {
    pub machine: PrefixMachine,
    pub roots: BTreeSet<u64>,
    /// ABI pointer word -> source local, after the complete decoder proof.
    pub spans: BTreeMap<u32, u64>,
    /// Immutable transaction-coordinate parameter slots, after all incoming
    /// words and any mixed nested decoders have been independently checked.
    pub view_slots: Vec<(i64, i64)>,
    /// Checked incoming arena pointer/extent spills. Protect their bytes without
    /// treating them as transaction coordinates or proving caller provenance.
    pub allocated_slots: Vec<(i64, i64)>,
    /// Checked incoming pointer ABI word -> arena parameter source identity.
    pub allocated_spans: BTreeMap<u32, u64>,
}

pub(super) struct Use {
    pub end: u64,
    pub source: u64,
    pub destination: u64,
    pub nested_result: bool,
    pub alias: bool,
    pub stores: Vec<u32>,
    pub cell_source_size: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    Unknown,
    Constant(i64),
    Stack(i64),
    // The independently checked hidden result argument, relative to its start.
    // It never aliases this entry's private frame; callers prove its extent.
    Result(i64),
    BorrowedCell(u64),
    CellLength(u64),
    NestedPointer(u64),
    NestedLength(u64),
    ArenaPointer(u64),
    ArenaExtent(u64),
    // One evaluated typed operand, valid until its consuming call. Full source
    // computation is separate from this checked load and transport fact.
    ScalarArgument(u64, u32),
    // Slot-specific evidence: only a checked producer/alias/projection can
    // establish a different destination's coordinate. Ordinary copies retain
    // the source identity, and overlapping writes invalidate stored facts.
    Coordinate(i64),
    ReturnAddress,
    ParentFrame,
    CalleeSaved(u32),
}

impl Value {
    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Constant(a), Self::Constant(b)) => Self::Constant(a.wrapping_add(b)),
            (Self::Stack(a), Self::Constant(b)) | (Self::Constant(b), Self::Stack(a)) => {
                a.checked_add(b).map_or(Self::Unknown, Self::Stack)
            }
            (Self::Result(a), Self::Constant(b)) | (Self::Constant(b), Self::Result(a)) => {
                a.checked_add(b).map_or(Self::Unknown, Self::Result)
            }
            (value, Self::Constant(0)) | (Self::Constant(0), value) => value,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    registers: [Value; 32],
    memory: BTreeMap<i64, Value>,
    initialized: BTreeSet<u64>,
    loaded_cells: BTreeSet<u64>,
    compared: BTreeSet<outputs::Obligation>,
    caches: BTreeSet<i64>,
    cache_regions: BTreeSet<i64>,
    hot_cache: Option<CacheBase>,
    selections: BTreeSet<i64>,
    selection_regions: BTreeSet<(i64, i64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CacheBase {
    Owned(i64),
    Borrowed,
}

impl State {
    fn new(frame: i64, result: Option<&crate::FixedResultAbi>, params: &[cell_args::Parameter]) -> Self {
        let mut registers = [Value::Unknown; 32];
        registers[0] = Value::Constant(0);
        registers[1] = Value::ReturnAddress;
        registers[2] = Value::Stack(frame);
        registers[8] = Value::ParentFrame;
        for register in [9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27] {
            registers[register] = Value::CalleeSaved(register as u32);
        }
        let mut memory = BTreeMap::new();
        if let Some(abi) = result {
            if abi.hidden_argument_index < 8 {
                registers[10 + abi.hidden_argument_index as usize] = Value::Result(0);
            } else {
                memory.insert(frame + i64::from(abi.hidden_argument_index - 8) * 8, Value::Result(0));
            }
        }
        for param in params {
            let (pointer, length) = if param.nested {
                (Value::NestedPointer(param.source), Value::NestedLength(param.source))
            } else {
                (Value::BorrowedCell(param.source), Value::CellLength(param.source))
            };
            for (index, value) in [(param.word, pointer), (param.word + 1, length)] {
                if index < 8 {
                    registers[10 + index as usize] = value;
                } else {
                    memory.insert(frame + i64::from(index - 8) * 8, value);
                }
            }
        }
        Self {
            registers,
            memory,
            initialized: params.iter().map(|param| param.source).collect(),
            loaded_cells: BTreeSet::new(),
            compared: BTreeSet::new(),
            caches: BTreeSet::new(),
            cache_regions: BTreeSet::new(),
            hot_cache: Some(CacheBase::Borrowed),
            selections: BTreeSet::new(),
            selection_regions: BTreeSet::new(),
        }
    }

    fn seed(seed: &Seed, result: Option<&crate::FixedResultAbi>, params: &[cell_args::Parameter]) -> Self {
        let value = |origin| match origin {
            Origin::Constant(value) => Value::Constant(value),
            Origin::Stack(offset) => Value::Stack(offset + seed.machine.frame),
            Origin::ReturnAddress => Value::ReturnAddress,
            Origin::ParentFrame => Value::ParentFrame,
            Origin::CalleeSaved(register) => Value::CalleeSaved(register),
            Origin::Incoming(index) if result.is_some_and(|abi| abi.hidden_argument_index == index) => Value::Result(0),
            Origin::Incoming(index) if seed.allocated_spans.contains_key(&index) => Value::ArenaPointer(seed.allocated_spans[&index]),
            Origin::Incoming(index) if index > 0 && seed.allocated_spans.contains_key(&(index - 1)) => {
                Value::ArenaExtent(seed.allocated_spans[&(index - 1)])
            }
            Origin::Incoming(index) if seed.spans.contains_key(&index) => Value::NestedPointer(seed.spans[&index]),
            Origin::Incoming(index) if index > 0 && seed.spans.contains_key(&(index - 1)) => {
                Value::NestedLength(seed.spans[&(index - 1)])
            }
            Origin::Incoming(index) => params
                .iter()
                .find_map(|param| {
                    if index == param.word {
                        Some(Value::BorrowedCell(param.source))
                    } else if index == param.word + 1 {
                        Some(Value::CellLength(param.source))
                    } else {
                        None
                    }
                })
                .unwrap_or(Value::Unknown),
            _ => Value::Unknown,
        };
        Self {
            registers: seed.machine.registers.map(value),
            memory: seed
                .machine
                .memory
                .iter()
                .filter_map(|(offset, origin)| {
                    let value = value(*origin);
                    (value != Value::Unknown).then_some((offset + seed.machine.frame, value))
                })
                .chain(seed.view_slots.iter().map(|&(slot, _)| (slot, Value::Coordinate(slot))))
                .collect(),
            initialized: seed.roots.iter().copied().chain(params.iter().map(|param| param.source)).collect(),
            loaded_cells: BTreeSet::new(),
            compared: BTreeSet::new(),
            caches: BTreeSet::new(),
            cache_regions: BTreeSet::new(),
            hot_cache: Some(CacheBase::Borrowed),
            selections: BTreeSet::new(),
            selection_regions: BTreeSet::new(),
        }
    }

    fn merge(&mut self, other: &Self) -> bool {
        let before = self.clone();
        for (value, incoming) in self.registers.iter_mut().zip(other.registers) {
            if *value != incoming {
                *value = Value::Unknown;
            }
        }
        self.memory.retain(|offset, value| other.memory.get(offset) == Some(value));
        self.initialized.retain(|local| other.initialized.contains(local));
        // A possible live Cell buffer must remain protected at a join even
        // when its definition is not definite on every incoming path.
        self.loaded_cells.extend(&other.loaded_cells);
        self.compared.retain(|field| other.compared.contains(field));
        self.caches.retain(|cache| other.caches.contains(cache));
        self.cache_regions.extend(&other.cache_regions);
        self.selections.retain(|base| other.selections.contains(base));
        self.selection_regions.extend(&other.selection_regions);
        if self.hot_cache != other.hot_cache {
            self.hot_cache = None;
        }
        *self != before
    }

    fn cells(&self) -> usize {
        32 + self.memory.len()
            + self.initialized.len()
            + self.loaded_cells.len()
            + self.compared.len()
            + self.caches.len()
            + self.cache_regions.len()
            + self.selections.len()
            + self.selection_regions.len()
    }

    fn forget(&mut self, start: i64, width: i64) {
        if width == 0 {
            return;
        }
        self.memory.retain(|offset, _| *offset + 8 <= start || *offset >= start + width);
    }
}

pub(super) struct CellEffects<'a> {
    pub cells: &'a cells::Checked,
    pub selections: &'a crate::cell_selection::machine::Checked,
    pub view_calls: &'a view_calls::Calls,
    pub allocated: &'a crate::allocated_values::machine::Checked,
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    seeds: &BTreeMap<String, Seed>,
    sizes: &BTreeMap<String, BTreeMap<u64, u32>>,
    uses: &BTreeMap<String, BTreeMap<u64, Use>>,
    effects: CellEffects<'_>,
    terminal_sink: Option<u64>,
) -> Result<BTreeMap<u64, u64>, CheckerError> {
    let cells = effects.cells;
    let view_values = view_values::validate(record, elf, terminal_sink, effects.selections)?;
    let property_sources = crate::scalar_machine::property_call_sources(record, elf)?;
    let cell_arguments = cell_args::validate(record, elf)?;
    let arena_arguments = arena_calls::validate(record, elf, seeds, effects.allocated)?;
    let outputs = outputs::validate(record, elf, cells, sizes)?;
    let cache = exact_cache::validate(record, elf, terminal_sink)?;
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    let targets = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect();
    let owners = record.blocks.iter().map(|block| (block.range.start, block)).collect();
    let entries = record.entries.iter().map(|entry| (entry.id.as_str(), entry)).collect();
    let mut proof = Proof {
        record,
        elf,
        words,
        targets,
        owners,
        entries,
        seeds,
        sizes,
        uses,
        cells,
        selections: effects.selections,
        view_values: &view_values,
        view_calls: effects.view_calls,
        property_sources: &property_sources,
        outputs: &outputs,
        cell_arguments: &cell_arguments,
        arena_arguments: &arena_arguments,
        allocated: effects.allocated,
        terminal_sink,
        done: BTreeSet::new(),
        active: BTreeSet::new(),
        resident_cells: 0,
        total_steps: 0,
        memcpy_checked: false,
        cache,
        cache_borrowers: BTreeSet::new(),
        returning_syscalls: BTreeMap::new(),
    };
    for owner in seeds
        .keys()
        .chain(cells.loads.keys())
        .chain(cell_arguments.parameters.keys())
        .chain(effects.selections.sites.keys())
        .chain(effects.selections.uses.keys())
        .chain(effects.selections.data.keys())
        .chain(effects.view_calls.keys())
        .chain(property_sources.keys())
        .chain(effects.allocated.constructors.keys())
        .chain(arena_arguments.calls.values().map(|call| &call.owner))
    {
        proof.entry(owner)?;
    }
    // Borrowing through s11 is a private helper contract. Check actual callers
    // transitively until a public entry establishes an owned cache.
    loop {
        let mut callers = BTreeSet::new();
        for borrower in &proof.cache_borrowers {
            let entry = proof.entries[borrower.as_str()];
            if matches!(entry.kind, crate::EntryKind::Action | crate::EntryKind::Lock | crate::EntryKind::Wrapper) {
                return Err(invalid("public entry borrows an uninitialized exact-read cache"));
            }
            let start = record
                .blocks
                .iter()
                .find(|block| block.id == entry.entry_block)
                .ok_or_else(|| invalid("missing cache borrower entry"))?
                .range
                .start;
            for edge in elf.control_flow.iter().filter(|edge| edge.target == start) {
                let caller = proof.owner(edge.address).ok_or_else(|| invalid("cache borrower caller has no owner"))?;
                if caller != borrower && !proof.done.contains(caller) {
                    callers.insert(caller.to_string());
                }
            }
        }
        if callers.is_empty() {
            break;
        }
        for caller in callers {
            proof.entry(&caller)?;
        }
    }
    // Publish atomic bodies only after their instruction grammars and complete
    // lifetime/CFG effects have passed. Scalar coverage may compose these
    // successful fallthroughs, including Cell loads' inline syscalls.
    let ranges = cells
        .loads
        .values()
        .flat_map(|sites| sites.iter().map(|(start, site)| (*start, site.end)))
        .chain(uses.values().flat_map(|sites| sites.iter().map(|(start, site)| (*start, site.end))))
        .chain(view_values.values().flat_map(|sites| sites.iter().map(|(start, site)| (*start, site.end))))
        .chain(outputs.sites.values().flat_map(|sites| sites.iter().map(|(start, site)| (*start, site.end))))
        .chain(proof.returning_syscalls.iter().filter_map(|(&pc, &returns)| returns.then_some((pc, pc + 4))));
    let mut bodies = BTreeMap::new();
    for (start, end) in ranges {
        if bodies.insert(start, end).is_some_and(|previous| previous != end) {
            return Err(invalid("checked bodies disagree on their complete extent"));
        }
    }
    Ok(bodies)
}

struct Proof<'a> {
    record: &'a VerifiedLoweringRecord,
    elf: &'a ParsedElf,
    words: BTreeMap<u64, u32>,
    targets: BTreeMap<u64, u64>,
    owners: BTreeMap<u64, &'a crate::LoweringBlock>,
    entries: BTreeMap<&'a str, &'a crate::LoweringEntry>,
    seeds: &'a BTreeMap<String, Seed>,
    sizes: &'a BTreeMap<String, BTreeMap<u64, u32>>,
    uses: &'a BTreeMap<String, BTreeMap<u64, Use>>,
    cells: &'a cells::Checked,
    selections: &'a crate::cell_selection::machine::Checked,
    view_values: &'a view_values::Checked,
    view_calls: &'a view_calls::Calls,
    property_sources: &'a BTreeMap<String, BTreeMap<u64, crate::scalar_machine::PropertyCallSource>>,
    outputs: &'a outputs::Checked,
    cell_arguments: &'a cell_args::Checked,
    arena_arguments: &'a arena_calls::Checked,
    allocated: &'a crate::allocated_values::machine::Checked,
    done: BTreeSet<String>,
    active: BTreeSet<String>,
    terminal_sink: Option<u64>,
    resident_cells: usize,
    total_steps: usize,
    memcpy_checked: bool,
    cache: Option<exact_cache::Checked>,
    cache_borrowers: BTreeSet<String>,
    // All analyzed states at a site must prove a returning observation with
    // bounded, non-overlapping writes. Sidecar syscall numbers are insufficient.
    returning_syscalls: BTreeMap<u64, bool>,
}

impl Proof<'_> {
    fn owner(&self, pc: u64) -> Option<&str> {
        self.owners.range(..=pc).next_back().filter(|(_, block)| block.range.contains(pc)).map(|(_, block)| block.owner_entry.as_str())
    }

    fn entry(&mut self, owner: &str) -> Result<(), CheckerError> {
        if self.done.contains(owner) {
            return Ok(());
        }
        if self.active.len() >= 64 || !self.active.insert(owner.to_string()) {
            return Err(invalid("recursive or over-depth nested memory effects"));
        }
        let entry = *self.entries.get(owner).ok_or_else(|| invalid("missing memory-effect owner"))?;
        let frame = i64::from(owned_frame(self.record, self.elf, entry)?);
        let mut outgoing =
            entry.fixed_result_calls.iter().map(|call| call.outgoing_stack_bytes).fold(entry.outgoing_argument_bytes, u32::max);
        outgoing = self
            .cell_arguments
            .calls
            .values()
            .filter(|call| call.owner == owner)
            .map(|call| call.outgoing_stack_bytes)
            .fold(outgoing, u32::max);
        outgoing = outgoing.max(view_calls::outgoing_for_owner(self.record, owner)?);
        outgoing =
            self.arena_arguments.calls.values().filter(|call| call.owner == owner).map(|call| call.outgoing).fold(outgoing, u32::max);
        // Dynamic collection stores preserve three live registers in a
        // temporary aligned subframe. This is private outgoing storage, not
        // permission to write the caller's frame or a borrowed arena.
        if self.record.typed_semantics.entries.iter().find(|entry| entry.id == owner).is_some_and(|entry| {
            entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .any(|op| matches!(op.detail, crate::TypedSemanticOperationDetail::AllocatedValue { .. }))
        }) {
            outgoing = outgoing.max(32);
        }
        let floor = -i64::from(outgoing);
        let seed = self.seeds.get(owner);
        let params = self.cell_arguments.parameters.get(owner).map_or(&[][..], Vec::as_slice);
        let checked_roots = seed.is_some()
            || self.cells.loads.contains_key(owner)
            || !params.is_empty()
            || self.selections.sites.contains_key(owner)
            || self.selections.uses.contains_key(owner)
            || self.property_sources.contains_key(owner);
        let checked_roots = checked_roots || self.selections.data.contains_key(owner);
        let checked_roots = checked_roots
            || self.allocated.constructors.contains_key(owner)
            || self.arena_arguments.calls.values().any(|call| call.owner == owner);
        let entry_start = self
            .record
            .blocks
            .iter()
            .find(|block| block.id == entry.entry_block)
            .ok_or_else(|| invalid("missing effect entry block"))?
            .range
            .start;
        if checked_roots
            && self.elf.control_flow.iter().any(|flow| {
                self.owner(flow.target) == Some(owner) && self.owner(flow.address) != Some(owner) && flow.target != entry_start
            })
        {
            return Err(invalid("external control flow bypasses the nested lifetime entry"));
        }
        let view_slots = seed.map_or(&[][..], |seed| seed.view_slots.as_slice());
        let mut protected = view_slots.to_vec();
        let allocated_slots = seed.map_or(&[][..], |seed| seed.allocated_slots.as_slice());
        protected.extend_from_slice(allocated_slots);
        let owned_arenas = self
            .allocated
            .constructors
            .get(owner)
            .into_iter()
            .flat_map(|sites| sites.values())
            .map(|constructor| (constructor.offset, constructor.offset + i64::from(constructor.extent)))
            .collect::<BTreeSet<_>>();
        let mut selection_slots = Vec::new();
        let mut data_regions = Vec::new();
        for result in
            self.selections.data.get(owner).into_iter().flat_map(|sites| sites.values()).filter_map(|site| site.nested.as_ref())
        {
            let start = i64::from(result.buffer.0);
            let end = start + i64::from(result.buffer.1);
            if data_regions.iter().any(|&(a, b)| start < b && a < end) {
                return Err(invalid("independent nested data results share a retained buffer"));
            }
            data_regions.push((start, end));
        }
        protected.extend(&data_regions);
        let copied_data_regions = self
            .selections
            .data
            .get(owner)
            .into_iter()
            .flat_map(|sites| sites.values())
            .filter_map(|site| site.result_buffer)
            .map(|(start, bytes)| (i64::from(start), i64::from(start) + i64::from(bytes)))
            .collect::<Vec<_>>();
        let copied_value_sites = self.allocated.copied_results.get(owner);
        let copied_value_regions =
            copied_value_sites.into_iter().flat_map(|sites| sites.values()).flat_map(|site| site.regions()).collect::<BTreeSet<_>>();
        if checked_roots {
            let typed = self
                .record
                .typed_semantics
                .entries
                .iter()
                .find(|entry| entry.id == owner)
                .ok_or_else(|| invalid("missing lifetime types"))?;
            let mut physical_locals = BTreeMap::new();
            for local in &typed.locals {
                let selection = local.ty.starts_with("ExactCellSelection<") || view_values::is_coordinate(&local.ty);
                if physical_locals.insert(local.source_id, selection).is_some_and(|previous| previous || selection) {
                    return Err(invalid("selection pointer slot aliases another typed local"));
                }
            }
            for local in typed.locals.iter().filter(|local| is_nested_source_type(&local.ty)) {
                let slot = local
                    .source_id
                    .checked_mul(8)
                    .and_then(|slot| i64::try_from(slot).ok())
                    .ok_or_else(|| invalid("pointer slot overflows"))?;
                protected.push((slot, slot + 8));
            }
            for local in typed.locals.iter().filter(|local| local.ty.starts_with("ExactCellSelection<")) {
                let slot = local
                    .source_id
                    .checked_mul(8)
                    .and_then(|slot| i64::try_from(slot).ok())
                    .ok_or_else(|| invalid("selection pointer slot overflows"))?;
                selection_slots.push((slot, slot + 8));
            }
            if let Some(sizes) = self.sizes.get(owner) {
                protected.extend(sizes.values().map(|offset| (i64::from(*offset), i64::from(*offset) + 8)));
            }
            if protected.iter().chain(&selection_slots).any(|(start, end)| *start < 0 || *end > frame - 16) {
                return Err(invalid("protected nested slot aliases frame bookkeeping"));
            }
        }
        let start = match seed {
            Some(seed) => seed.machine.pc,
            None => entry_start,
        };
        // fixed_results::validate has already derived the ABI from the typed
        // declaration and checked every call/save/copy. Start at the actual
        // incoming argument, never fabricate provenance at a later reload.
        let result = entry.fixed_result_abi.as_ref();
        let initial = seed.map_or_else(|| State::new(frame, result, params), |seed| State::seed(seed, result, params));
        self.charge(initial.cells())?;
        let mut states = BTreeMap::from([(start, initial)]);
        let mut pending = VecDeque::from([start]);
        let checkpoints = self.checkpoints(owner, start)?;
        let mut steps = 0usize;
        while let Some(pc) = pending.pop_front() {
            // Only branch targets and summarized-range exits can merge
            // independent predecessors. Consume straight-line states after
            // use; loop targets retain their must/may facts until convergence.
            let retained = checkpoints.contains(&pc);
            let Some(mut state) = (if retained { states.get(&pc).cloned() } else { states.remove(&pc) }) else {
                // A repeated queue item was already processed with its latest
                // incoming state. No independent predecessor can enter here.
                continue;
            };
            let transient_cells = if retained { 0 } else { state.cells() };
            steps += 1;
            self.total_steps += 1;
            if steps > 262144 || states.len() > 8192 || self.total_steps > 2097152 {
                return Err(invalid("nested memory-effect CFG budget exceeded"));
            }
            if self.owner(pc) != Some(owner) {
                return Err(invalid(format!("nested lifetime leaves {owner} at {pc:#x}")));
            }
            if let Some(stages) = self.arena_arguments.scalar_ends.get(&pc) {
                for stage in stages {
                    if stage.owner != owner || state.registers[2] != Value::Stack(0) {
                        return Err(invalid("scalar call preparation changed its owning frame"));
                    }
                    let value = Value::ScalarArgument(stage.call, stage.word);
                    if stage.word < 8 {
                        state.registers[10 + stage.word as usize] = value;
                    } else {
                        let slot = i64::from(stage.word - 8) * 8 - i64::from(stage.outgoing);
                        state.forget(slot, 8);
                        state.memory.insert(slot, value);
                    }
                }
            }
            let mut protected = protected.clone();
            // An arena's state, allocation records, elements and descriptors
            // may change only inside its independently checked constructor or
            // primitive. Ordinary stores and unrelated helpers cannot repair
            // a dropped descriptor or change a previously checked capacity.
            let allowed_arena = self
                .allocated
                .constructors
                .get(owner)
                .and_then(|sites| sites.get(&pc))
                .map(|site| (site.offset, site.offset + i64::from(site.extent)))
                .or_else(|| {
                    self.allocated
                        .owned_primitives
                        .get(owner)
                        .and_then(|sites| sites.range(..=pc).next_back())
                        .filter(|(_, site)| pc < site.end)
                        .map(|(_, site)| (site.offset, site.offset + i64::from(site.extent)))
                });
            let arena_protected = owned_arenas.iter().copied().filter(|region| Some(*region) != allowed_arena);
            protected.extend(arena_protected.clone());
            // Only the recognized producer at its exact entry may write its
            // copied result. Other stores/calls/syscalls must preserve it.
            // Keep other protected regions even if their extents coincide.
            let own_copy = self
                .selections
                .data
                .get(owner)
                .and_then(|sites| sites.get(&pc))
                .and_then(|site| site.result_buffer)
                .map(|(start, bytes)| (i64::from(start), i64::from(start) + i64::from(bytes)));
            let copied_protected = copied_data_regions.iter().copied().filter(|region| Some(*region) != own_copy);
            protected.extend(copied_protected.clone());
            // Collection reads publish immutable value temporaries. Only the
            // complete checked publication may write its bytes/pointer; later
            // ordinary stores and helper effects must preserve both regions.
            let own_value_copy = copied_value_sites
                .and_then(|sites| sites.range(..=pc).next_back())
                .filter(|(_, site)| pc < site.end)
                .map(|(_, site)| site.regions());
            let copied_value_protected =
                copied_value_regions.iter().copied().filter(|region| own_value_copy.is_none_or(|regions| !regions.contains(region)));
            protected.extend(copied_value_protected.clone());
            // Only a fully recognized hash-property call body may initialize
            // or fill its own size/result storage. All other effects preserve it.
            protected.extend(
                self.property_sources
                    .get(owner)
                    .into_iter()
                    .flat_map(|sites| sites.values())
                    .filter(|site| !(site.start..site.end).contains(&pc))
                    .filter_map(|site| site.hash_region),
            );
            for load in self.cells.loads.get(owner).into_iter().flat_map(|loads| loads.values()) {
                if state.loaded_cells.contains(&load.source) {
                    protected.extend([
                        (i64::from(load.buffer.start), i64::from(load.buffer.end)),
                        (i64::from(load.buffer.size), i64::from(load.buffer.size) + 8),
                    ]);
                    if load.initializes_pointer {
                        let slot = i64::try_from(load.source.checked_mul(8).ok_or_else(|| invalid("Cell pointer slot overflows"))?)
                            .map_err(|_| invalid("Cell pointer slot exceeds i64"))?;
                        protected.push((slot, slot + 8));
                    }
                }
            }
            let mut selection_protected = selection_slots.clone();
            selection_protected.extend(&state.selection_regions);
            // Constructor/alias summaries alone may write their typed handle
            // destination. Every other effect must preserve these scalar slots.
            let mut selection_writes = protected.clone();
            selection_writes.extend(&state.selection_regions);
            protected.extend(&selection_protected);
            self.discover_cache(owner, &mut state, frame, &protected, pc)?;
            let non_cache_protected = protected.clone();
            let cache_regions = self.cache_regions(&state);
            protected.extend(&cache_regions);
            selection_writes.extend(&cache_regions);
            let mut auxiliary_protected = cache_regions.clone();
            auxiliary_protected.extend_from_slice(view_slots);
            auxiliary_protected.extend_from_slice(allocated_slots);
            auxiliary_protected.extend(arena_protected);
            auxiliary_protected.extend(&selection_protected);
            auxiliary_protected.extend(&data_regions);
            auxiliary_protected.extend(copied_protected);
            auxiliary_protected.extend(copied_value_protected);
            let mut next = vec![pc + 4];
            if let Some(read) = self.allocated.borrowed_reads.get(owner).and_then(|sites| sites.get(&pc))
                && (state.registers[2] != Value::Stack(0)
                    || state.memory.get(&i64::from(read.pointer)) != Some(&Value::ArenaPointer(read.source))
                    || read
                        .extent_slot
                        .is_some_and(|slot| state.memory.get(&i64::from(slot)) != Some(&Value::ArenaExtent(read.source))))
            {
                return Err(invalid("borrowed collection read lost its incoming arena pointer or extent"));
            }
            if let Some(constructor) = self.allocated.constructors.get(owner).and_then(|sites| sites.get(&pc)) {
                if state.registers[2] != Value::Stack(0) {
                    return Err(invalid("arena constructor changes its private frame"));
                }
                let slot = i64::try_from(constructor.source.checked_mul(8).ok_or_else(|| invalid("arena pointer slot overflows"))?)
                    .map_err(|_| invalid("arena pointer slot exceeds i64"))?;
                let bytes = i64::from(constructor.extent) + 128;
                write(Value::Stack(constructor.offset), bytes, 0, frame - 16, &protected, pc)?;
                write(Value::Stack(slot), 8, 0, frame - 16, &protected, pc)?;
                state.forget(constructor.offset, bytes);
                state.forget(slot, 8);
                // A loop's next construction invalidates facts retained from
                // the preceding use of this source owner's private storage.
                let stale = |value: Value| matches!(value, Value::ArenaPointer(source) | Value::ArenaExtent(source) if source == constructor.source);
                for value in &mut state.registers {
                    if stale(*value) {
                        *value = Value::Unknown;
                    }
                }
                state.memory.retain(|_, value| !stale(*value));
                for (_, word) in self.words.range(pc..constructor.end) {
                    if matches!(word & 0x7f, 0x13 | 0x1b | 0x33 | 0x37) && word >> 7 & 31 != 0 {
                        state.registers[(word >> 7 & 31) as usize] = Value::Unknown;
                    }
                }
                state.memory.insert(slot, Value::ArenaPointer(constructor.source));
                next[0] = constructor.end;
            } else if let Some(value) = self.view_values.get(owner).and_then(|sites| sites.get(&pc)) {
                if state.registers[2] != Value::Stack(0) {
                    return Err(invalid("Cell view value changes its private frame"));
                }
                if let Some(source) = value.source
                    && state.memory.get(&source) != Some(&Value::Coordinate(source))
                {
                    return Err(invalid("Cell view alias has no live checked coordinate source"));
                }
                write(Value::Stack(value.destination), 8, 0, frame - 16, &protected, pc)?;
                state.forget(value.destination, 8);
                state.memory.insert(value.destination, Value::Coordinate(value.destination));
                for (_, word) in self.words.range(pc..value.end) {
                    if matches!(word & 0x7f, 3 | 0x13 | 0x1b | 0x33 | 0x37 | 0x3b) {
                        state.registers[(word >> 7 & 31) as usize] = Value::Unknown;
                    }
                }
                if value.call {
                    for register in [1, 5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 28, 29, 30, 31] {
                        state.registers[register] = Value::Unknown;
                    }
                }
                next[0] = value.end;
            } else if let Some(data) = self.selections.data.get(owner).and_then(|sites| sites.get(&pc)) {
                if state.registers[2] != Value::Stack(0) {
                    return Err(invalid("Cell data read does not use the proved private frame"));
                }
                let source = state.memory.get(&i64::from(data.source)).copied().unwrap_or(Value::Unknown);
                if source != Value::Coordinate(i64::from(data.source)) {
                    return Err(invalid("Cell data read has no live checked coordinate source"));
                }
                let mut data_protected = protected.clone();
                if let Some(result) = &data.nested {
                    let allowed = [
                        (i64::from(data.destination), i64::from(data.destination) + 8),
                        (i64::from(result.length), i64::from(result.length) + 8),
                        (i64::from(result.buffer.0), i64::from(result.buffer.0) + i64::from(result.buffer.1)),
                    ];
                    data_protected.retain(|region| !allowed.contains(region));
                    // Re-executing a producer in a loop invalidates every
                    // prior derived view before publishing its new root.
                    let mut invalidated = BTreeSet::from([result.root]);
                    if let Some(uses) = self.uses.get(owner) {
                        for _ in 0..=uses.len() {
                            let previous = invalidated.len();
                            for site in uses.values() {
                                if site.nested_result && invalidated.contains(&site.source) {
                                    invalidated.insert(site.destination);
                                }
                            }
                            if invalidated.len() == previous {
                                break;
                            }
                        }
                    }
                    state.initialized.retain(|root| !invalidated.contains(root));
                }
                for &(offset, bytes) in &data.writes {
                    let start = write(Value::Stack(i64::from(offset)), i64::from(bytes), 0, frame - 16, &data_protected, pc)?;
                    state.forget(start, i64::from(bytes));
                }
                let slot = write(Value::Stack(i64::from(data.destination)), 8, 0, frame - 16, &data_protected, pc)?;
                state.forget(slot, 8);
                if data.projection {
                    state.registers[5] = Value::Unknown;
                    state.memory.insert(slot, Value::Coordinate(slot));
                } else {
                    if let Some(result) = &data.nested {
                        state.initialized.insert(result.root);
                        state.memory.insert(slot, Value::NestedPointer(result.root));
                        state.memory.insert(i64::from(result.length), Value::NestedLength(result.root));
                    }
                    if let Some((pointer, _)) = data.result_buffer {
                        state.memory.insert(slot, Value::Stack(i64::from(pointer)));
                    }
                    for register in [5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 28, 29, 30, 31] {
                        state.registers[register] = Value::Unknown;
                    }
                }
                next[0] = data.end;
            } else if let Some(selection) = self.selections.sites.get(owner).and_then(|sites| sites.get(&pc)) {
                if state.registers[2] != Value::Stack(0) {
                    return Err(invalid("exact-role verification does not use the proved private frame"));
                }
                // The selection recognizer proves every write remains in this
                // private region and restores SP, RA and all callee-saved regs.
                // Compare its full footprint with live Cell buffers and nested
                // pointer/length slots; no metadata-only effect is trusted.
                let start =
                    write(Value::Stack(i64::from(selection.offset)), i64::from(selection.bytes), 0, frame - 16, &protected, pc)?;
                state.forget(start, i64::from(selection.bytes));
                if let Some(destination) = selection.destination {
                    let slot = write(Value::Stack(i64::from(destination)), 8, 0, frame - 16, &selection_writes, pc)?;
                    state.forget(slot, 8);
                    state.memory.insert(slot, Value::Stack(start));
                    state.selections.insert(start);
                    state.selection_regions.insert((start, start + i64::from(selection.bytes)));
                }
                for register in [5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 28, 29, 30, 31] {
                    state.registers[register] = Value::Unknown;
                }
                next[0] = selection.end;
            } else if let Some(site) = self.selections.uses.get(owner).and_then(|sites| sites.get(&pc)) {
                if state.registers[2] != Value::Stack(0) {
                    return Err(invalid("selection read does not use its proved private frame"));
                }
                let Some(Value::Stack(base)) = state.memory.get(&i64::from(site.source)).copied() else {
                    return Err(invalid("selection read has no constructed handle provenance"));
                };
                if !state.selections.contains(&base) {
                    return Err(invalid("selection read is reachable before its table definition"));
                }
                let forbidden = if site.alias { &selection_writes } else { &protected };
                let slot = write(Value::Stack(i64::from(site.destination)), 8, 0, frame - 16, forbidden, pc)?;
                state.forget(slot, 8);
                if site.alias {
                    state.memory.insert(slot, Value::Stack(base));
                } else if site.coordinate {
                    state.memory.insert(slot, Value::Coordinate(slot));
                }
                for (_, word) in self.words.range(pc..site.end) {
                    if matches!(word & 0x7f, 3 | 0x13 | 0x1b | 0x33 | 0x37 | 0x3b) {
                        state.registers[(word >> 7 & 31) as usize] = Value::Unknown;
                    }
                }
                next[0] = site.end;
            } else if let Some(load) = self.cells.loads.get(owner).and_then(|loads| loads.get(&pc)) {
                if state.registers[2] != Value::Stack(0) {
                    return Err(invalid(format!(
                        "Cell load does not use its checked private frame: {owner} at {pc:#x}, sp {:?}, frame {frame}",
                        state.registers[2]
                    )));
                }
                write(
                    Value::Stack(i64::from(load.buffer.start)),
                    i64::from(load.buffer.end - load.buffer.start),
                    0,
                    frame - 16,
                    &auxiliary_protected,
                    pc,
                )?;
                write(Value::Stack(i64::from(load.buffer.size)), 8, 0, frame - 16, &auxiliary_protected, pc)?;
                state.forget(i64::from(load.buffer.start), i64::from(load.buffer.end - load.buffer.start));
                state.forget(i64::from(load.buffer.size), 8);
                state.memory.insert(i64::from(load.buffer.size), Value::CellLength(load.source));
                state.loaded_cells.insert(load.source);
                state.compared.retain(|(output, ..)| *output != load.source);
                if load.initializes_pointer {
                    let slot = i64::try_from(load.source.checked_mul(8).ok_or_else(|| invalid("Cell pointer slot overflows"))?)
                        .map_err(|_| invalid("Cell pointer slot exceeds i64"))?;
                    write(Value::Stack(slot), 8, 0, frame - 16, &auxiliary_protected, pc)?;
                    state.forget(slot, 8);
                    state.memory.insert(slot, Value::Stack(i64::from(load.buffer.start)));
                    state.initialized.insert(load.source);
                }
                for (_, word) in self.words.range(pc..load.end) {
                    if matches!(word & 0x7f, 3 | 0x13 | 0x1b | 0x33 | 0x37 | 0x3b) {
                        state.registers[(word >> 7 & 31) as usize] = Value::Unknown;
                    }
                }
                next[0] = load.end;
            } else if let Some(site) = self.outputs.sites.get(owner).and_then(|sites| sites.get(&pc)) {
                let buffer =
                    self.cells.buffers.get(&(owner.to_string(), site.output)).ok_or_else(|| invalid("missing successor buffer"))?;
                if state.registers[2] != Value::Stack(0)
                    || (site.nested_view && !state.initialized.contains(&site.expected))
                    || state.memory.get(&i64::from(buffer.size)) != Some(&Value::CellLength(site.output))
                {
                    return Err(invalid("output comparison precedes its loaded Cell or expected value definition"));
                }
                state.compared.insert((site.output, site.field, site.block, site.expected));
                for (_, word) in self.words.range(pc..site.end) {
                    if matches!(word & 0x7f, 3 | 0x13 | 0x1b | 0x33 | 0x37 | 0x3b) {
                        state.registers[(word >> 7 & 31) as usize] = Value::Unknown;
                    }
                }
                next[0] = site.end;
            } else if let Some(site) = self.uses.get(owner).and_then(|sites| sites.get(&pc)) {
                let source = if site.cell_source_size.is_some() {
                    let typed = self
                        .record
                        .typed_semantics
                        .entries
                        .iter()
                        .find(|entry| entry.id == owner)
                        .ok_or_else(|| invalid("Cell field has no typed owner"))?;
                    cell_args::root(typed, site.source)?
                } else {
                    site.source
                };
                if checked_roots && !state.initialized.contains(&source) {
                    return Err(invalid("nested read is reachable before its source definition"));
                }
                if let Some(size) = site.cell_source_size {
                    let pointer = self.cell_pointer(owner, source)?;
                    if state.memory.get(&((site.source * 8) as i64)) != Some(&pointer)
                        || state.memory.get(&i64::from(size)) != Some(&Value::CellLength(source))
                    {
                        return Err(invalid("Cell field pointer/length lost their checked source provenance"));
                    }
                }
                let alias_pointer = site.alias.then(|| state.memory.get(&((site.source * 8) as i64)).copied()).flatten();
                for offset in &site.stores {
                    write(Value::Stack(i64::from(*offset)), 8, 0, frame - 16, &auxiliary_protected, pc)?;
                    state.forget(i64::from(*offset), 8);
                }
                if site.nested_result {
                    state.initialized.insert(site.destination);
                    let pointer = if site.alias {
                        alias_pointer.ok_or_else(|| invalid("nested alias has no checked pointer provenance"))?
                    } else {
                        Value::NestedPointer(site.destination)
                    };
                    state.memory.insert((site.destination * 8) as i64, pointer);
                    if !site.alias {
                        let size = self
                            .sizes
                            .get(owner)
                            .and_then(|sizes| sizes.get(&site.destination))
                            .ok_or_else(|| invalid("nested result has no checked length slot"))?;
                        state.memory.insert(i64::from(*size), Value::NestedLength(site.destination));
                    }
                }
                for (_, word) in self.words.range(pc..site.end) {
                    if matches!(word & 0x7f, 3 | 0x13 | 0x1b | 0x33 | 0x37 | 0x3b) {
                        state.registers[(word >> 7 & 31) as usize] = Value::Unknown;
                    }
                }
                next[0] = site.end;
            } else if let Some(end) = self.private_byte_copy(&mut state, pc, floor, frame, &protected)? {
                next[0] = end;
            } else if let Some((end, start, bytes)) = self.guarded_store_sequence(&state, pc, frame, &protected)? {
                state.forget(start, bytes);
                // Registers and SP are restored by the checked sequence. Its
                // temporary spill contents are dead outgoing-frame storage.
                if let Value::Stack(sp) = state.registers[2] {
                    state.forget(sp - 32, 32);
                }
                next[0] = end;
            } else {
                let word = *self.words.get(&pc).ok_or_else(|| invalid("missing lifetime instruction"))?;
                let rd = (word >> 7 & 31) as usize;
                let rs1 = (word >> 15 & 31) as usize;
                let rs2 = (word >> 20 & 31) as usize;
                let kind = word >> 12 & 7;
                let immediate = i64::from((word as i32) >> 20);
                if matches!(word & 0x7f, 3 | 0x13 | 0x1b | 0x33 | 0x37 | 0x3b) && (21..=26).contains(&rd) {
                    state.hot_cache = None;
                }
                match word & 0x7f {
                    0x13 if kind == 0 => state.registers[rd] = state.registers[rs1].add(Value::Constant(immediate)),
                    0x1b if kind == 0 => {
                        state.registers[rd] = match state.registers[rs1].add(Value::Constant(immediate)) {
                            Value::Constant(value) => Value::Constant(i64::from(value as i32)),
                            _ => Value::Unknown,
                        }
                    }
                    0x37 => state.registers[rd] = Value::Constant(i64::from((word & 0xfffff000) as i32)),
                    0x17 => state.registers[rd] = Value::Constant((pc as i64).wrapping_add(i64::from((word & 0xfffff000) as i32))),
                    0x33 if kind == 0 && word >> 25 == 0 => state.registers[rd] = state.registers[rs1].add(state.registers[rs2]),
                    0x33 if kind == 0 && word >> 25 == 1 => {
                        state.registers[rd] = match (state.registers[rs1], state.registers[rs2]) {
                            (Value::Constant(left), Value::Constant(right)) => Value::Constant(left.wrapping_mul(right)),
                            _ => Value::Unknown,
                        };
                    }
                    0x03 => {
                        state.registers[rd] = match (kind, state.registers[rs1].add(Value::Constant(immediate))) {
                            (3, Value::Stack(offset)) => state.memory.get(&offset).copied().unwrap_or(Value::Unknown),
                            _ => Value::Unknown,
                        };
                    }
                    0x23 => {
                        if kind > 3 {
                            return Err(invalid("unsupported memory write width"));
                        }
                        let immediate = i64::from((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20 >> 20);
                        let address = state.registers[rs1].add(Value::Constant(immediate));
                        let width = 1i64 << kind;
                        let offset = write(address, width, floor, frame, &protected, pc)?;
                        state.forget(offset, width);
                        if width == 8 && state.registers[rs2] != Value::Unknown {
                            state.memory.insert(offset, state.registers[rs2]);
                        }
                    }
                    0x63 => {
                        if let Some((start, bytes)) = self.guarded_store(&state, pc, frame, &protected)? {
                            state.forget(start, bytes);
                            next[0] = pc + 12;
                        } else {
                            next.push(*self.targets.get(&pc).ok_or_else(|| invalid("missing lifetime branch target"))?);
                        }
                    }
                    0x67 | 0x6f if rd == 1 => {
                        if !matches!(state.registers[2], Value::Stack(offset) if offset <= 0) {
                            return Err(invalid("callee frame may overlap its caller's protected storage"));
                        }
                        let target = *self.targets.get(&pc).ok_or_else(|| invalid("unresolved nested memory-effect call"))?;
                        let callee = self.owner(target).ok_or_else(|| invalid("call has no memory-effect owner"))?.to_string();
                        let callee_entry = self.entries[callee.as_str()];
                        if self.arena_arguments.helpers.contains(&callee) {
                            let call =
                                self.arena_arguments.calls.get(&pc).ok_or_else(|| invalid("bounded helper call is not bound"))?;
                            if call.owner != owner
                                || call.callee != callee
                                || state.registers[2] != Value::Stack(-i64::from(call.outgoing))
                            {
                                return Err(invalid("bounded helper call target or outgoing frame differs from its typed ABI"));
                            }
                            let incoming = |index: u32| {
                                if index < 8 {
                                    state.registers[10 + index as usize]
                                } else if let Value::Stack(sp) = state.registers[2] {
                                    state.memory.get(&(sp + i64::from(index - 8) * 8)).copied().unwrap_or(Value::Unknown)
                                } else {
                                    Value::Unknown
                                }
                            };
                            for argument in &call.arguments {
                                let (source, extent) = match argument.root {
                                    arena_calls::Root::Owned { source, extent } => (source, Value::Constant(i64::from(extent))),
                                    arena_calls::Root::Borrowed { source } => (source, Value::ArenaExtent(source)),
                                };
                                if incoming(argument.word) != Value::ArenaPointer(source) || incoming(argument.word + 1) != extent {
                                    return Err(invalid(format!(
                                        "arena call pointer/extent lack their typed owner's provenance: {owner} at {pc:#x}, word {}",
                                        argument.word
                                    )));
                                }
                            }
                            for argument in &call.scalars {
                                if incoming(argument.word) != Value::ScalarArgument(pc, argument.word) {
                                    return Err(invalid("scalar call argument lost its evaluated typed source"));
                                }
                            }
                        }
                        if self.cell_arguments.parameters.contains_key(&callee) {
                            let call = self.cell_arguments.calls.get(&pc).ok_or_else(|| invalid("borrowed call is not bound"))?;
                            if call.callee != callee {
                                return Err(invalid("borrowed callee changed"));
                            }
                            for argument in &call.arguments {
                                if !state.initialized.contains(&argument.root) {
                                    return Err(invalid("borrowed call precedes its Cell definition"));
                                }
                                let incoming = |index: u32| {
                                    if index < 8 {
                                        state.registers[10 + index as usize]
                                    } else if let Value::Stack(sp) = state.registers[2] {
                                        state.memory.get(&(sp + i64::from(index - 8) * 8)).copied().unwrap_or(Value::Unknown)
                                    } else {
                                        Value::Unknown
                                    }
                                };
                                let (pointer, length) = if argument.nested {
                                    (Value::NestedPointer(argument.root), Value::NestedLength(argument.root))
                                } else {
                                    (self.cell_pointer(owner, argument.root)?, Value::CellLength(argument.root))
                                };
                                if incoming(argument.word) != pointer || incoming(argument.word + 1) != length {
                                    return Err(invalid("borrowed call pointer/length do not match their typed source"));
                                }
                            }
                        }
                        if !self.record.blocks.iter().any(|block| block.id == callee_entry.entry_block && block.range.start == target)
                        {
                            return Err(invalid("memory-effect call bypasses callee prologue"));
                        }
                        if let Some(sources) = self.view_calls.get(owner).and_then(|calls| calls.get(&pc)) {
                            for source in sources {
                                if state.memory.get(source) != Some(&Value::Coordinate(*source)) {
                                    return Err(invalid("Cell view helper argument has no live checked coordinate source"));
                                }
                            }
                        }
                        if let Some(site) = self.property_sources.get(owner).and_then(|calls| calls.get(&pc)) {
                            // Scalar machine recognition binds the actual load
                            // (including checked adjacent-store forwarding).
                            // Atomic coordinate producers need not retain a0
                            // facts in this memory-effect interpreter.
                            if state.memory.get(&site.source) != Some(&Value::Coordinate(site.source)) {
                                return Err(invalid("Cell property argument has no live checked coordinate source"));
                            }
                        }
                        if self.cache.as_ref().is_some_and(|cache| cache.wrappers.contains(&callee)) {
                            let pointer = state.registers[12];
                            if self.cache_call(&mut state, pointer, frame, &non_cache_protected, pc)? {
                                self.cache_borrowers.insert(owner.to_string());
                            }
                        } else if callee == "runtime:__cellscript_memcpy_fixed" {
                            if !self.memcpy_checked {
                                crate::fixed_results::validate_memcpy(self.record, self.elf)
                                    .map_err(|error| invalid(format!("copy effects: {error}")))?;
                                self.memcpy_checked = true;
                            }
                            let Value::Constant(width) = state.registers[12] else {
                                return Err(invalid("fixed copy has no bounded write length"));
                            };
                            match state.registers[11] {
                                Value::Result(offset) => {
                                    let abi = result.ok_or_else(|| invalid("copy has no caller-owned result contract"))?;
                                    if offset != 0
                                        || width != i64::from(abi.width_bytes)
                                        || !abi.copy_ranges.iter().any(|range| range.contains(pc))
                                    {
                                        return Err(invalid("copy leaves the checked caller-owned result extent"));
                                    }
                                }
                                destination => {
                                    let destination = write(destination, width, floor, frame, &protected, pc)?;
                                    state.forget(destination, width);
                                }
                            }
                        } else if witness_exact_hash::field(&callee).is_some() {
                            if !self.done.contains(&callee) {
                                witness_exact_hash::validate(self.record, self.elf, &callee)?;
                                self.entry(witness_exact_hash::RESOLVER)?;
                                self.done.insert(callee.clone());
                            }
                            let buffer = write(state.registers[11], 32, 0, frame - 16, &protected, pc)?;
                            state.forget(buffer, 32);
                        } else if context_hashes::syscall(&callee).is_some() {
                            if !self.done.contains(&callee) {
                                context_hashes::validate(self.record, self.elf, &callee)?;
                                self.done.insert(callee.clone());
                            }
                            let buffer = write(state.registers[10], 32, 0, frame - 16, &protected, pc)?;
                            let size = write(state.registers[11], 8, 0, frame - 16, &protected, pc)?;
                            if buffer < size + 8 && size < buffer + 32 {
                                return Err(invalid("context hash output and size storage overlap"));
                            }
                            state.forget(buffer, 32);
                            state.forget(size, 8);
                        } else if callee == witness_bytes::OWNER {
                            let buffer = write(state.registers[13], 32, 0, frame - 16, &protected, pc)?;
                            state.forget(buffer, 32);
                        } else if streaming_hashes::syscall(&callee).is_some() {
                            if !self.done.contains(&callee) {
                                streaming_hashes::validate(self.record, self.elf, &callee)?;
                                self.done.insert(callee.clone());
                            }
                            let buffer = write(state.registers[13], 32, 0, frame - 16, &protected, pc)?;
                            state.forget(buffer, 32);
                        } else if callee == cell_read_hashes::SLICE {
                            // The complete exact-read body is independently validated.
                            let output = 12;
                            let buffer = write(state.registers[output], 32, 0, frame - 16, &protected, pc)?;
                            let size = write(state.registers[output + 1], 8, 0, frame - 16, &protected, pc)?;
                            if buffer < size + 8 && size < buffer + 32 {
                                return Err(invalid("Cell data read output and size storage overlap"));
                            }
                            state.forget(buffer, 32);
                            state.forget(size, 8);
                        } else if hash_fields::field(&callee).is_some()
                            || script_hash_fields::kind(&callee).is_some()
                            || callee == out_point_hash::OWNER
                        {
                            if !self.done.contains(&callee) {
                                if let Some(field) = hash_fields::field(&callee) {
                                    hash_fields::validate(self.record, self.elf, &callee, field)?;
                                } else if callee == out_point_hash::OWNER {
                                    out_point_hash::validate(self.record, self.elf)?;
                                } else {
                                    script_hash_fields::validate(self.record, self.elf, &callee)?;
                                }
                                self.done.insert(callee.clone());
                            }
                            let buffer = write(state.registers[11], 32, 0, frame - 16, &protected, pc)?;
                            let size = write(state.registers[12], 8, 0, frame - 16, &protected, pc)?;
                            if buffer < size + 8 && size < buffer + 32 {
                                return Err(invalid("hash helper output and size storage overlap"));
                            }
                            state.forget(buffer, 32);
                            state.forget(size, 8);
                        } else {
                            if let Some(abi) = &callee_entry.fixed_result_abi {
                                let call = entry
                                    .fixed_result_calls
                                    .iter()
                                    .find(|call| call.callee == callee && call.setup_range.contains(pc))
                                    .ok_or_else(|| invalid("callee result has no checked caller buffer"))?;
                                let hidden = if abi.hidden_argument_index < 8 {
                                    state.registers[10 + abi.hidden_argument_index as usize]
                                } else {
                                    let Value::Stack(sp) = state.registers[2] else { unreachable!() };
                                    state
                                        .memory
                                        .get(&(sp + i64::from(abi.hidden_argument_index - 8) * 8))
                                        .copied()
                                        .unwrap_or(Value::Unknown)
                                };
                                if hidden != Value::Stack(i64::from(call.buffer_offset)) {
                                    return Err(invalid("callee result pointer differs from its checked caller buffer"));
                                }
                                let destination = write(hidden, i64::from(abi.width_bytes), 0, frame - 16, &protected, pc)?;
                                // A checked return writes unknown bytes, not the
                                // pointer or scalar facts previously held here.
                                state.forget(destination, i64::from(abi.width_bytes));
                            }
                            self.entry(&callee).map_err(|error| invalid(format!("callee {callee}: {error}")))?;
                            if self.cache_borrowers.contains(&callee) {
                                let pointer = state.registers[27];
                                if self.cache_call(&mut state, pointer, frame, &non_cache_protected, pc)? {
                                    self.cache_borrowers.insert(owner.to_string());
                                }
                            }
                        }
                        // Outgoing argument scratch is not permanent caller
                        // storage and can be reused by a subsequent callee.
                        state.memory.retain(|offset, _| *offset >= 0);
                        // A later loop iteration must prepare its own values;
                        // neither a saved register nor a copied stack fact can
                        // reuse evidence consumed by this invocation.
                        state.memory.retain(|_, value| !matches!(value, Value::ScalarArgument(call, _) if *call == pc));
                        for value in &mut state.registers {
                            if matches!(value, Value::ScalarArgument(call, _) if *call == pc) {
                                *value = Value::Unknown;
                            }
                        }
                        // Ordinary callees prove saved-register restoration.
                        // Cache wrappers update their hot registers explicitly.
                        for register in 1..32 {
                            if !matches!(register, 2 | 8 | 9 | 18..=27) {
                                state.registers[register] = Value::Unknown;
                            }
                        }
                    }
                    0x67 if word == 0x00008067 => {
                        self.require_comparisons(owner, &state)?;
                        if state.registers[2] != Value::Stack(frame)
                            || state.registers[1] != Value::ReturnAddress
                            || state.registers[8] != Value::ParentFrame
                            || [9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27].iter().any(|&register| {
                                state.registers[register] != Value::CalleeSaved(register as u32)
                                    && !((21..=26).contains(&register)
                                        && state.hot_cache == Some(CacheBase::Borrowed)
                                        && self.cache.as_ref().is_some_and(|cache| cache.hot))
                            })
                        {
                            return Err(invalid(format!("{owner} does not restore its entry frame and return address")));
                        }
                        next.clear();
                    }
                    0x67 | 0x6f if rd == 0 => {
                        next = vec![*self.targets.get(&pc).ok_or_else(|| invalid("unresolved lifetime jump"))?];
                    }
                    0x73 if word == 0x00000073 => match state.registers[17] {
                        Value::Constant(93) => {
                            self.returning_syscalls.insert(pc, false);
                            if !matches!(state.registers[10], Value::Constant(code) if code != 0) {
                                self.require_comparisons(owner, &state)?;
                            }
                            next.clear();
                        }
                        Value::Constant(2051 | 2052 | 2061 | 2062 | 2071 | 2072 | 2073 | 2074 | 2081 | 2082 | 2083 | 2092) => {
                            let size = write(state.registers[11], 8, floor, frame, &protected, pc)?;
                            let Some(Value::Constant(width)) = state.memory.get(&size).copied() else {
                                return Err(invalid("syscall has no bounded destination size"));
                            };
                            let buffer = write(state.registers[10], width, floor, frame, &protected, pc)?;
                            state.forget(buffer, width);
                            state.forget(size, 8);
                            state.registers[10] = Value::Unknown;
                            // Never turn a site seen as EXIT on another path
                            // into a returning summary.
                            self.returning_syscalls.entry(pc).or_insert(true);
                        }
                        _ => return Err(invalid("unproved syscall memory effects in nested view lifetime")),
                    },
                    0x13 | 0x1b | 0x33 | 0x3b => state.registers[rd] = Value::Unknown,
                    _ => return Err(invalid(format!("unsupported lifetime instruction at {pc:#x}"))),
                }
            }
            state.registers[0] = Value::Constant(0);
            for successor in next {
                if self.terminal_sink == Some(successor)
                    || self.record.verifier_failure_exits.iter().any(|exit| exit.address == successor)
                {
                    continue;
                }
                if self.owner(successor) != Some(owner) {
                    return Err(invalid(format!(
                        "nested lifetime control flow leaves {owner} at {pc:#x} for {successor:#x} ({:?})",
                        self.owner(successor)
                    )));
                }
                match states.get_mut(&successor) {
                    Some(previous) => {
                        let old_cells = previous.cells();
                        if previous.merge(&state) {
                            pending.push_back(successor);
                        }
                        let new_cells = previous.cells();
                        if new_cells > old_cells {
                            self.charge(new_cells - old_cells)?;
                        } else {
                            self.resident_cells -= old_cells - new_cells;
                        }
                    }
                    None => {
                        if states.len() >= 8192 {
                            return Err(invalid("nested memory-effect CFG budget exceeded"));
                        }
                        self.charge(state.cells())?;
                        states.insert(successor, state.clone());
                        pending.push_back(successor);
                    }
                }
            }
            self.resident_cells -= transient_cells;
        }
        self.resident_cells -= states.values().map(State::cells).sum::<usize>();
        self.active.remove(owner);
        self.done.insert(owner.to_string());
        Ok(())
    }

    /// Every non-fallthrough successor is retained, including exits of checked
    /// summaries. Guard recognition only locates those exits here; the actual
    /// transfer still proves its incoming frame, bounds and protected storage.
    fn checkpoints(&self, owner: &str, start: u64) -> Result<BTreeSet<u64>, CheckerError> {
        let mut points = BTreeSet::from([start]);
        points.extend(self.targets.values().copied().filter(|pc| self.owner(*pc) == Some(owner)));
        points.extend(self.allocated.constructors.get(owner).into_iter().flat_map(|sites| sites.values()).map(|site| site.end));
        points.extend(self.view_values.get(owner).into_iter().flat_map(|sites| sites.values()).map(|site| site.end));
        points.extend(self.selections.data.get(owner).into_iter().flat_map(|sites| sites.values()).map(|site| site.end));
        points.extend(self.selections.sites.get(owner).into_iter().flat_map(|sites| sites.values()).map(|site| site.end));
        points.extend(self.selections.uses.get(owner).into_iter().flat_map(|sites| sites.values()).map(|site| site.end));
        points.extend(self.cells.loads.get(owner).into_iter().flat_map(|sites| sites.values()).map(|site| site.end));
        points.extend(self.outputs.sites.get(owner).into_iter().flat_map(|sites| sites.values()).map(|site| site.end));
        points.extend(self.uses.get(owner).into_iter().flat_map(|sites| sites.values()).map(|site| site.end));
        let (begin, end) = self
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == owner)
            .fold((u64::MAX, 0), |(begin, end), block| (begin.min(block.range.start), end.max(block.range.end)));
        for (&pc, &word) in self.words.range(begin..end) {
            if self.owner(pc) != Some(owner) {
                continue;
            }
            if word & 0x7f == 0x63 {
                // The bare two-guard/store summary skips three instructions.
                points.insert(pc.checked_add(12).ok_or_else(|| invalid("guard continuation overflows"))?);
            }
            if word == 0xfe010113
                && let Some(sequence) = guarded_writes::sequence(self.record, self.elf, &self.words, pc)?
            {
                points.insert(sequence.end);
            }
        }
        Ok(points)
    }

    /// Check the complete spill/address/guard/store/restore sequence atomically.
    /// Keeping a CFG state at every register spill would multiply the enclosing
    /// proof's memory use without adding independent evidence.
    fn guarded_store_sequence(
        &self,
        state: &State,
        pc: u64,
        frame: i64,
        protected: &[(i64, i64)],
    ) -> Result<Option<(u64, i64, i64)>, CheckerError> {
        if state.registers[2] != Value::Stack(0) {
            return Ok(None);
        }
        let Some(sequence) = guarded_writes::sequence(self.record, self.elf, &self.words, pc)? else { return Ok(None) };
        let mut current = state.clone();
        current.registers[2] = Value::Stack(-32);
        for at in (pc + 16..sequence.guard).step_by(4) {
            let word = self.words[&at];
            let rd = (word >> 7 & 31) as usize;
            let rs1 = (word >> 15 & 31) as usize;
            let rs2 = (word >> 20 & 31) as usize;
            current.registers[rd] = match word & 0x7f {
                0x13 if word >> 12 & 7 == 0 => current.registers[rs1].add(Value::Constant(i64::from((word as i32) >> 20))),
                0x1b if word >> 12 & 7 == 0 => match current.registers[rs1].add(Value::Constant(i64::from((word as i32) >> 20))) {
                    Value::Constant(value) => Value::Constant(i64::from(value as i32)),
                    _ => Value::Unknown,
                },
                0x37 => Value::Constant(i64::from((word & 0xfffff000) as i32)),
                0x33 if word >> 12 & 7 == 0 && word >> 25 == 0 => current.registers[rs1].add(current.registers[rs2]),
                _ => return Ok(None),
            };
        }
        let Some((start, bytes)) = self.guarded_store(&current, sequence.guard, frame, protected)? else { return Ok(None) };
        Ok(Some((sequence.end, start, bytes)))
    }

    /// Prove one dynamic write from two actual unsigned address guards. Bounds
    /// must be independently tracked private-stack addresses; labels and
    /// collection metadata never establish an address or a trusted store.
    fn guarded_store(&self, state: &State, pc: u64, frame: i64, protected: &[(i64, i64)]) -> Result<Option<(i64, i64)>, CheckerError> {
        let Some((&lower, &upper, &store)) =
            self.words.get(&pc).zip(self.words.get(&(pc + 4))).zip(self.words.get(&(pc + 8))).map(|((a, b), c)| (a, b, c))
        else {
            return Ok(None);
        };
        if lower & 0x707f != 0x6063 || upper & 0x707f != 0x6063 || store & 0x7f != 0x23 {
            return Ok(None);
        }
        let pointer = lower >> 15 & 31;
        if upper >> 20 & 31 != pointer || store >> 15 & 31 != pointer {
            return Ok(None);
        }
        let offset = (((store >> 7 & 31) | (store >> 25 << 5)) as i32) << 20 >> 20;
        let kind = store >> 12 & 7;
        if offset != 0 || kind > 3 {
            return Ok(None);
        }
        let (Value::Stack(start), Value::Stack(last)) =
            (state.registers[(lower >> 20 & 31) as usize], state.registers[(upper >> 15 & 31) as usize])
        else {
            return Ok(None);
        };
        for branch in [pc, pc + 4] {
            if !self
                .targets
                .get(&branch)
                .is_some_and(|target| self.record.verifier_failure_exits.iter().any(|exit| exit.address == *target && exit.code == 21))
            {
                return Ok(None);
            }
        }
        if self.owner(pc) != self.owner(pc + 8)
            || self.elf.control_flow.iter().any(|flow| flow.target == pc + 4 || flow.target == pc + 8)
        {
            return Err(invalid("control flow bypasses a dynamic write address guard"));
        }
        let bytes = last
            .checked_sub(start)
            .and_then(|distance| distance.checked_add(1 << kind))
            .filter(|bytes| *bytes > 0 && last >= start)
            .ok_or_else(|| invalid("invalid guarded write interval"))?;
        write(Value::Stack(start), bytes, 0, frame - 16, protected, pc)?;
        Ok(Some((start, bytes)))
    }

    fn require_comparisons(&self, owner: &str, state: &State) -> Result<(), CheckerError> {
        if self.outputs.required.get(owner).is_some_and(|required| !required.is_subset(&state.compared)) {
            return Err(invalid("successful execution bypasses a declared nested successor comparison"));
        }
        Ok(())
    }

    fn cache_regions(&self, state: &State) -> Vec<(i64, i64)> {
        self.cache
            .as_ref()
            .map_or_else(Vec::new, |cache| state.cache_regions.iter().map(|&base| (base, base + cache.bytes())).collect())
    }

    fn discover_cache(
        &self,
        owner: &str,
        state: &mut State,
        frame: i64,
        protected: &[(i64, i64)],
        pc: u64,
    ) -> Result<(), CheckerError> {
        let Some(cache) = &self.cache else {
            return Ok(());
        };
        let Value::Stack(base) = state.registers[27] else {
            return Ok(());
        };
        if !state.caches.contains(&base) {
            let Some(end) = base.checked_add(cache.bytes()) else {
                return Err(invalid("cache extent overflows"));
            };
            if base < 0 || end > frame - 16 {
                return Ok(());
            }
            let initialized = [base, base + 8]
                .into_iter()
                .chain((0..exact_cache::WAYS).map(|way| base + cache.header() + way * exact_cache::ENTRY_BYTES + 24))
                .all(|slot| state.memory.get(&slot) == Some(&Value::Constant(0)));
            if !initialized {
                return Ok(());
            }
            let typed = self
                .record
                .typed_semantics
                .entries
                .iter()
                .find(|entry| entry.id == owner)
                .ok_or_else(|| invalid("owned cache has no typed caller frame"))?;
            let scalar_end = typed
                .locals
                .iter()
                .map(|local| local.source_id.checked_add(1).and_then(|id| id.checked_mul(8)))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| invalid("cache scalar frame overflows"))?
                .into_iter()
                .max()
                .unwrap_or(0);
            if (base as u64) < scalar_end {
                return Err(invalid("exact-read cache overlaps typed scalar slots"));
            }
            let mut forbidden = protected.to_vec();
            forbidden.extend(self.cache_regions(state));
            write(Value::Stack(base), cache.bytes(), 0, frame - 16, &forbidden, pc)?;
            state.caches.insert(base);
            state.cache_regions.insert(base);
        }
        // Clearing the fast-hit flag re-establishes coherence without relying
        // on any of the other hot registers. Every later writer invalidates it.
        if cache.hot && state.registers[26] == Value::Constant(0) {
            state.hot_cache = Some(CacheBase::Owned(base));
        }
        Ok(())
    }

    fn cache_call(
        &self,
        state: &mut State,
        pointer: Value,
        frame: i64,
        protected: &[(i64, i64)],
        pc: u64,
    ) -> Result<bool, CheckerError> {
        let cache = self.cache.as_ref().ok_or_else(|| invalid("missing checked cache helper"))?;
        if pointer == Value::CalleeSaved(27) && state.registers[27] == pointer {
            if cache.hot && state.hot_cache != Some(CacheBase::Borrowed) {
                return Err(invalid("borrowed hot-cache registers lost their invocation provenance"));
            }
            if cache.hot {
                for register in 21..=26 {
                    state.registers[register] = Value::Unknown;
                }
                state.hot_cache = Some(CacheBase::Borrowed);
            }
            return Ok(true);
        }
        let Value::Stack(base) = pointer else {
            return Err(invalid("exact-read cache has no owned caller pointer"));
        };
        if state.registers[27] != Value::Stack(base)
            || !state.caches.contains(&base)
            || (cache.hot && state.hot_cache != Some(CacheBase::Owned(base)))
        {
            return Err(invalid("exact-read cache has not been initialized or its retained pointers lost provenance"));
        }
        let mut forbidden = protected.to_vec();
        forbidden.extend(self.cache_regions(state).into_iter().filter(|&(start, _)| start != base));
        write(Value::Stack(base), cache.bytes(), 0, frame - 16, &forbidden, pc)?;
        // The grammar writes only the counter, last-entry pointer and four
        // bounded entries. Saved caller registers in the header remain intact.
        state.forget(base, 16);
        state.forget(base + cache.header(), exact_cache::WAYS * exact_cache::ENTRY_BYTES);
        if cache.hot {
            for register in 21..=26 {
                state.registers[register] = Value::Unknown;
            }
            state.hot_cache = Some(CacheBase::Owned(base));
        }
        Ok(false)
    }

    fn charge(&mut self, cells: usize) -> Result<(), CheckerError> {
        self.resident_cells = self.resident_cells.checked_add(cells).ok_or_else(|| invalid("lifetime state budget overflows"))?;
        if self.resident_cells > 262144 {
            return Err(invalid("nested memory-effect state budget exceeded"));
        }
        Ok(())
    }

    fn cell_pointer(&self, owner: &str, source: u64) -> Result<Value, CheckerError> {
        if let Some(buffer) = self.cells.buffers.get(&(owner.to_string(), source)) {
            Ok(Value::Stack(i64::from(buffer.start)))
        } else if self
            .cell_arguments
            .parameters
            .get(owner)
            .is_some_and(|params| params.iter().any(|param| !param.nested && param.source == source))
        {
            Ok(Value::BorrowedCell(source))
        } else {
            Err(invalid("Cell view has no checked load or borrowed argument"))
        }
    }
}

fn write(address: Value, width: i64, floor: i64, frame: i64, protected: &[(i64, i64)], pc: u64) -> Result<i64, CheckerError> {
    if width == 0 {
        return Ok(0); // A zero-capacity size query does not write its data pointer.
    }
    let Value::Stack(start) = address else { return Err(invalid(format!("write without private stack provenance at {pc:#x}"))) };
    let end = start.checked_add(width).ok_or_else(|| invalid("memory-effect range overflows"))?;
    if width < 0 || start < floor || end > frame {
        return Err(invalid(format!("write [{start}, {end}) leaves private frame [{floor}, {frame}) at {pc:#x}")));
    }
    if width != 0 && protected.iter().any(|(protected_start, protected_end)| start < *protected_end && end > *protected_start) {
        return Err(invalid(format!("unverified write overlaps protected runtime storage at {pc:#x}")));
    }
    Ok(start)
}
