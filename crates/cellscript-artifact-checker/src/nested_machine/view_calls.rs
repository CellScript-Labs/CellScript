//! Typed transaction-coordinate transport through helper calls. This binds
//! caller loads and callee spills. `view_values` checks local producers and
//! aliases; `lifetime` requires those coordinates to remain live at each call.
//! Export the fully checked prefix and immutable coordinate slots to the
//! enclosing memory-effect proof; later body writes must preserve them.
//! Complete source/value reconstruction retains its experimental blocker.
use super::*;
use crate::{TypedSemanticEntry, TypedSemanticOperation};

pub(super) type Calls = BTreeMap<String, BTreeMap<u64, Vec<i64>>>;

fn is_view(ty: &str) -> bool {
    ty.starts_with("InputView<") || ty.starts_with("OutputView<")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(register: u32, offset: u32, kind: u32) -> u32 {
        ((offset >> 5) << 25) | (register << 20) | (2 << 15) | (kind << 12) | ((offset & 31) << 7) | 0x23
    }

    #[test]
    fn retained_coordinate_values_expire_on_overwrites_calls_and_joins() {
        let mut machine = Transport::new();
        machine.step(0, store(10, 32, 3)).unwrap();
        assert_eq!(machine.registers[10], Atom::Slot(32));
        machine.step(4, 0x00050593).unwrap(); // ADDI a1,a0,0
        assert_eq!(machine.registers[11], Atom::Slot(32));
        machine.step(8, store(12, 32, 3)).unwrap();
        assert_eq!(machine.registers[10], Atom::Unknown);
        assert_eq!(machine.registers[11], Atom::Unknown);
        assert_eq!(machine.registers[12], Atom::Slot(32));
        machine.step(12, store(13, 39, 0)).unwrap();
        assert_eq!(machine.registers[12], Atom::Unknown);

        machine.step(16, store(10, 32, 3)).unwrap();
        machine.step(20, 0x00150513).unwrap(); // ADDI a0,a0,1 changes the coordinate.
        assert_ne!(machine.registers[10], Atom::Slot(32));
        machine.step(24, store(10, 32, 3)).unwrap();
        machine.step(28, 0x000000ef).unwrap(); // A call invalidates volatile registers.
        assert_eq!(machine.registers[10], Atom::Unknown);
        machine.step(32, store(10, 32, 3)).unwrap();
        machine.boundary().unwrap();
        assert_eq!(machine.registers[10], Atom::Unknown);
    }

    #[test]
    fn partial_stores_cannot_establish_full_coordinate_transport() {
        for kind in 0..3 {
            let mut machine = Transport::new();
            machine.step(0, store(10, 32, kind)).unwrap();
            assert_eq!(machine.registers[10], Atom::Unknown);
        }
        let mut machine = Transport::new();
        machine.step(0, store(10, 33, 3)).unwrap();
        assert_eq!(machine.registers[10], Atom::Unknown);
        machine.step(4, store(2, 32, 3)).unwrap();
        assert_eq!(machine.registers[2], Atom::Stack(0));
    }
}

pub(super) fn outgoing_bytes(record: &VerifiedLoweringRecord, entry: &TypedSemanticEntry) -> Result<u32, CheckerError> {
    let mut words = entry.params.iter().try_fold(0u32, |words, param| {
        words
            .checked_add(crate::fixed_results::parameter_word_width(record, entry, param))
            .ok_or_else(|| invalid("view helper argument expansion overflows"))
    })?;
    if record.entries.iter().any(|lowering| lowering.id == entry.id && lowering.fixed_result_abi.is_some()) {
        words = words.checked_add(1).ok_or_else(|| invalid("view helper result argument overflows"))?;
    }
    words
        .saturating_sub(8)
        .checked_mul(8)
        .and_then(|bytes| bytes.checked_next_multiple_of(16))
        .ok_or_else(|| invalid("view helper outgoing frame overflows"))
}

/// The enclosing write proof needs the expanded, aligned outgoing extent for
/// every typed helper called by this owner, including ordinary scalar/array
/// helpers beside bounded calls. Caller transport remains a separate proof.
pub(super) fn outgoing_for_owner(record: &VerifiedLoweringRecord, owner: &str) -> Result<u32, CheckerError> {
    let Some(entry) = record.typed_semantics.entries.iter().find(|entry| entry.id == owner) else { return Ok(0) };
    let mut bytes = 0;
    for call in entry.blocks.iter().flat_map(|block| &block.operations).filter_map(|operation| operation.call.as_ref()) {
        if let Some(callee) = record.typed_semantics.entries.iter().find(|entry| entry.kind == "helper" && entry.name == call.target) {
            bytes = bytes.max(outgoing_bytes(record, callee)?);
        }
    }
    Ok(bytes)
}

struct Parameter {
    position: usize,
    word: u32,
    slot: i64,
    ty: String,
}

fn slot(entry: &TypedSemanticEntry, local: u32) -> Result<i64, CheckerError> {
    entry
        .locals
        .iter()
        .find(|value| value.id == local)
        .and_then(|value| value.source_id.checked_mul(8))
        .and_then(|offset| i64::try_from(offset).ok())
        .ok_or_else(|| invalid("view transport has no bounded typed local slot"))
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    seeds: &mut BTreeMap<String, lifetime::Seed>,
) -> Result<Calls, CheckerError> {
    let mut helpers = BTreeMap::new();
    let mut outgoing = BTreeMap::new();
    for entry in &record.typed_semantics.entries {
        if entry.kind != "helper" {
            continue;
        }
        let mut word = 0u32;
        let mut parameters = Vec::new();
        for (position, param) in entry.params.iter().enumerate() {
            if is_view(&param.ty) {
                parameters.push(Parameter { position, word, slot: slot(entry, param.binding_id)?, ty: param.ty.clone() });
            }
            word = word
                .checked_add(crate::fixed_results::parameter_word_width(record, entry, param))
                .ok_or_else(|| invalid("view helper argument expansion overflows"))?;
        }
        if !parameters.is_empty() {
            let bytes = outgoing_bytes(record, entry)?;
            outgoing.insert(entry.id.as_str(), bytes);
            helpers.insert(entry.id.as_str(), parameters);
        }
    }
    if helpers.is_empty() {
        return Ok(BTreeMap::new());
    }
    let normalized = crate::policy_machine::normalize_relaxed_branches(record, elf).map_err(invalid)?;
    let (record, elf) = normalized.as_ref().map_or((record, elf), |(record, elf)| (record, elf));
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect::<BTreeMap<_, _>>();
    let entries = record.typed_semantics.entries.iter().map(|entry| (entry.id.as_str(), entry)).collect::<BTreeMap<_, _>>();
    let names = entries
        .values()
        .filter(|entry| helpers.contains_key(entry.id.as_str()))
        .map(|entry| (entry.name.as_str(), entry.id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut starts = BTreeMap::new();
    for (id, params) in &helpers {
        let entry = record.entries.iter().find(|entry| entry.id == *id).ok_or_else(|| invalid("view helper has no machine entry"))?;
        let start = record
            .blocks
            .iter()
            .find(|block| block.id == entry.entry_block)
            .ok_or_else(|| invalid("view helper has no prologue"))?
            .range
            .start;
        let machine = parameters::finish_prefix(record, elf, entry, seeds.get(*id), &words)?;
        let frame = machine.frame;
        for param in params {
            if param.slot < 0
                || param.slot.checked_add(8).is_none_or(|end| end > frame.saturating_sub(16))
                || machine.memory.get(&(param.slot - frame)) != Some(&Origin::Incoming(param.word))
            {
                return Err(invalid("view helper parameter spill differs from its typed incoming argument"));
            }
        }
        let mut seed = seeds.remove(*id).unwrap_or_else(|| lifetime::Seed {
            machine: machine.clone(),
            roots: BTreeSet::new(),
            spans: BTreeMap::new(),
            view_slots: Vec::new(),
            allocated_slots: Vec::new(),
            allocated_spans: BTreeMap::new(),
        });
        seed.machine = machine;
        seed.view_slots = params.iter().map(|param| (param.slot, param.slot + 8)).collect();
        seeds.insert((*id).to_string(), seed);
        starts.insert(start, *id);
    }
    let mut typed_at = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut ordered_blocks = record.blocks.iter().collect::<Vec<_>>();
    ordered_blocks.sort_by_key(|block| block.range.start);
    for block in &ordered_blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        typed_at.insert(block.range.start, (block.owner_entry.as_str(), current.get(block.owner_entry.as_str()).copied()));
    }
    let mut expected = BTreeMap::<(&str, u32), Vec<&TypedSemanticOperation>>::new();
    for entry in entries.values() {
        for block in &entry.blocks {
            for op in &block.operations {
                if op.call.as_ref().is_some_and(|call| names.contains_key(call.target.as_str())) {
                    expected.entry((entry.id.as_str(), block.id)).or_default().push(op);
                }
            }
        }
    }
    let mut observed = BTreeMap::<(&str, u32), usize>::new();
    let mut calls = Calls::new();
    let boundaries = elf.control_flow.iter().map(|flow| flow.target).collect::<BTreeSet<_>>();
    let mut total_steps = 0usize;
    let mut flows = elf.control_flow.iter().collect::<Vec<_>>();
    flows.sort_by_key(|flow| flow.address);
    for flow in flows {
        let Some(callee) = starts.get(&flow.target).copied() else { continue };
        let word = *words.get(&flow.address).ok_or_else(|| invalid("view call instruction missing"))?;
        if !matches!(word & 0x7f, 0x67 | 0x6f) || word >> 7 & 31 != 1 {
            return Err(invalid("view helper must be entered through a call"));
        }
        let &(owner, Some(block)) = typed_at.range(..=flow.address).next_back().ok_or_else(|| invalid("view call owner missing"))?.1
        else {
            return Err(invalid("view call has no typed block"));
        };
        let ordinal = observed.entry((owner, block)).or_default();
        let op = expected
            .get(&(owner, block))
            .and_then(|ops| ops.get(*ordinal))
            .ok_or_else(|| invalid("machine view call has no typed operation"))?;
        *ordinal += 1;
        if op.call.as_ref().and_then(|call| names.get(call.target.as_str())).copied() != Some(callee) {
            return Err(invalid("view call target or order differs from typed source"));
        }
        let start = record
            .blocks
            .iter()
            .filter(|candidate| candidate.owner_entry == owner && candidate.lowering_block_id == Some(block))
            .map(|block| block.range.start)
            .min()
            .ok_or_else(|| invalid("view call body missing"))?;
        let caller =
            record.entries.iter().find(|entry| entry.id == owner).ok_or_else(|| invalid("view caller has no machine entry"))?;
        let frame = i64::from(owned_frame(record, elf, caller)?);
        let mut machine = Transport::new();
        let mut pc = start;
        while pc < flow.address {
            if boundaries.contains(&pc) {
                machine.boundary()?;
            }
            let next = if let Some(sequence) = guarded_writes::sequence(record, elf, &words, pc)? {
                if sequence.end > flow.address {
                    return Err(invalid("view call enters a partial guarded write"));
                }
                machine.guarded_write(pc, &sequence, &words, frame)?;
                sequence.end
            } else {
                machine.step(pc, *words.get(&pc).ok_or_else(|| invalid("view transport instruction missing"))?)?;
                pc + 4
            };
            total_steps += ((next - pc) / 4) as usize;
            if total_steps > 262144 {
                return Err(invalid("view call transport exceeds its aggregate proof budget"));
            }
            pc = next;
        }
        if boundaries.contains(&flow.address) {
            machine.boundary()?;
        }
        if machine.registers[2] != Atom::Stack(-i64::from(outgoing[callee])) {
            return Err(invalid("view call outgoing frame differs from its typed ABI"));
        }
        for parameter in &helpers[callee] {
            let operand = op.operands.get(parameter.position).ok_or_else(|| invalid("typed view argument missing"))?;
            if operand.ty != parameter.ty {
                return Err(invalid("view call changes source kind or nominal schema"));
            }
            let source = slot(entries[owner], operand.local.ok_or_else(|| invalid("view argument is not a local"))?)?;
            if source.checked_add(8).is_none_or(|end| end > frame.saturating_sub(16)) {
                return Err(invalid("view call source slot exceeds the caller's owned frame"));
            }
            if machine.argument(parameter.word) != Atom::Slot(source) {
                return Err(invalid(format!(
                    "view call argument does not load its typed source slot: {owner} at {:#x}, word {}, slot {source}",
                    flow.address, parameter.word
                )));
            }
            calls.entry(owner.to_string()).or_default().entry(flow.address).or_default().push(source);
        }
    }
    if expected.iter().any(|(key, ops)| observed.get(key).copied().unwrap_or(0) != ops.len()) {
        return Err(invalid("typed view call lacks machine coverage"));
    }
    Ok(calls)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Atom {
    Unknown,
    Constant(i64),
    Stack(i64),
    Slot(i64),
}

fn add(left: Atom, right: Atom) -> Atom {
    match (left, right) {
        (Atom::Constant(a), Atom::Constant(b)) => Atom::Constant(a.wrapping_add(b)),
        (Atom::Stack(a), Atom::Constant(b)) | (Atom::Constant(b), Atom::Stack(a)) => {
            a.checked_add(b).map_or(Atom::Unknown, Atom::Stack)
        }
        (value, Atom::Constant(0)) | (Atom::Constant(0), value) => value,
        _ => Atom::Unknown,
    }
}

struct Transport {
    registers: [Atom; 32],
    outgoing: BTreeMap<i64, Atom>,
    steps: usize,
}

impl Transport {
    fn new() -> Self {
        let mut registers = [Atom::Unknown; 32];
        registers[0] = Atom::Constant(0);
        registers[2] = Atom::Stack(0);
        Self { registers, outgoing: BTreeMap::new(), steps: 0 }
    }
    fn argument(&self, word: u32) -> Atom {
        if word < 8 {
            self.registers[10 + word as usize]
        } else if let Atom::Stack(sp) = self.registers[2] {
            sp.checked_add(i64::from(word - 8) * 8).and_then(|offset| self.outgoing.get(&offset)).copied().unwrap_or(Atom::Unknown)
        } else {
            Atom::Unknown
        }
    }
    fn guarded_write(
        &mut self,
        pc: u64,
        sequence: &guarded_writes::Sequence,
        words: &BTreeMap<u64, u32>,
        frame: i64,
    ) -> Result<(), CheckerError> {
        if self.registers[2] != Atom::Stack(0) {
            return Err(invalid("view transport guarded write has no private stack base"));
        }
        // The exact sequence saves every setup register and restores SP. Prove
        // its guarded destination cannot overwrite those temporary spills;
        // the enclosing memory proof additionally protects retained values.
        let mut setup = Self { registers: self.registers, outgoing: BTreeMap::new(), steps: 0 };
        setup.registers[2] = Atom::Stack(-32);
        for at in (pc + 16..sequence.guard).step_by(4) {
            setup.step(at, words[&at])?;
        }
        let lower = words[&sequence.guard];
        let upper = words[&(sequence.guard + 4)];
        let store = words[&(sequence.guard + 8)];
        let (Atom::Stack(start), Atom::Stack(last)) =
            (setup.registers[(lower >> 20 & 31) as usize], setup.registers[(upper >> 15 & 31) as usize])
        else {
            return Err(invalid("view transport write guards lack private-frame bounds"));
        };
        let width = 1i64 << (store >> 12 & 7);
        if start < 0 || last < start || last.checked_add(width).is_none_or(|end| end > frame - 16) {
            return Err(invalid("view transport guarded write overlaps spills or frame linkage"));
        }
        // No retained slot fact may survive an unknown dynamic destination.
        // Subsequent argument setup must load the coordinate again.
        self.outgoing.clear();
        for value in &mut self.registers {
            if matches!(value, Atom::Slot(_)) {
                *value = Atom::Unknown;
            }
        }
        self.steps += ((sequence.end - pc) / 4) as usize;
        if self.steps > 65536 {
            return Err(invalid("view call transport proof exceeds its instruction budget"));
        }
        Ok(())
    }
    // Facts before a control-flow join cannot establish argument setup after
    // it. Every admitted path reloads the coordinate in the final straight-line
    // suffix; branching with a reserved outgoing frame is not admitted.
    fn boundary(&mut self) -> Result<(), CheckerError> {
        if self.registers[2] != Atom::Stack(0) {
            return Err(invalid("view argument control flow changes its owned stack base"));
        }
        self.registers = [Atom::Unknown; 32];
        self.registers[0] = Atom::Constant(0);
        self.registers[2] = Atom::Stack(0);
        self.outgoing.clear();
        Ok(())
    }
    fn step(&mut self, _pc: u64, word: u32) -> Result<(), CheckerError> {
        self.steps += 1;
        if self.steps > 65536 {
            return Err(invalid("view call transport proof exceeds its instruction budget"));
        }
        let rd = (word >> 7 & 31) as usize;
        let left = (word >> 15 & 31) as usize;
        let right = (word >> 20 & 31) as usize;
        let kind = word >> 12 & 7;
        let imm = i64::from((word as i32) >> 20);
        match word & 0x7f {
            0x13 if kind == 0 => self.registers[rd] = add(self.registers[left], Atom::Constant(imm)),
            0x1b if kind == 0 => {
                self.registers[rd] = match add(self.registers[left], Atom::Constant(imm)) {
                    Atom::Constant(value) => Atom::Constant(i64::from(value as i32)),
                    _ => Atom::Unknown,
                }
            }
            0x37 => self.registers[rd] = Atom::Constant(i64::from((word & 0xfffff000) as i32)),
            0x33 if kind == 0 && word >> 25 == 0 => self.registers[rd] = add(self.registers[left], self.registers[right]),
            0x03 => {
                self.registers[rd] = match (kind, add(self.registers[left], Atom::Constant(imm))) {
                    (3, Atom::Stack(offset)) if offset >= 0 && offset % 8 == 0 => Atom::Slot(offset),
                    _ => Atom::Unknown,
                }
            }
            0x23 => {
                let imm = i64::from(((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20) >> 20);
                if let Atom::Stack(offset) = add(self.registers[left], Atom::Constant(imm)) {
                    let width = 1i64.checked_shl(kind).ok_or_else(|| invalid("view call store width overflows"))?;
                    let end = offset.checked_add(width).ok_or_else(|| invalid("view call store address overflows"))?;
                    self.outgoing.retain(|slot, _| *slot + 8 <= offset || *slot >= end);
                    // A saved register/outgoing word cannot stand for the
                    // current value of a local after that local is overwritten.
                    let stale = |value: Atom| matches!(value, Atom::Slot(slot) if slot < end && slot.checked_add(8).is_none_or(|slot_end| slot_end > offset));
                    for value in &mut self.registers {
                        if stale(*value) {
                            *value = Atom::Unknown;
                        }
                    }
                    self.outgoing.retain(|_, value| !stale(*value));
                    if kind == 3 && offset < 0 {
                        self.outgoing.insert(offset, self.registers[right]);
                    } else if kind == 3
                        && offset >= 0
                        && offset % 8 == 0
                        && matches!(self.registers[right], Atom::Unknown | Atom::Slot(_))
                    {
                        // A store followed by a redundant reload may keep the
                        // value in its register. It is now exactly the current
                        // value of this slot; the lifetime proof separately
                        // requires that slot to hold the typed live coordinate.
                        self.registers[right] = Atom::Slot(offset);
                    }
                } else {
                    self.outgoing.clear();
                    for value in &mut self.registers {
                        if matches!(value, Atom::Slot(_)) {
                            *value = Atom::Unknown;
                        }
                    }
                }
            }
            0x67 | 0x6f if rd == 1 => {
                for register in [1, 5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 28, 29, 30, 31] {
                    self.registers[register] = Atom::Unknown;
                }
                self.outgoing.clear();
            }
            0x63 | 0x67 | 0x6f | 0x73 => self.boundary()?,
            _ => self.registers[rd] = Atom::Unknown,
        }
        self.registers[0] = Atom::Constant(0);
        Ok(())
    }
}
