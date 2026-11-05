//! Normal-return register obligations over decoded instructions. Memory contents,
//! buffer extents and caller guards require separate independent recognizers.
use super::*;
use runtime_contract::RuntimeReturnAbi as Abi;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct Value {
    available: bool,
    constant: Option<u64>,
    nonzero: bool,
    pending: Option<u64>,
    status: Option<u64>,
    stack: Option<i64>,
}

impl Value {
    fn unknown() -> Self {
        Self { available: true, ..Self::default() }
    }
    fn constant(value: u64) -> Self {
        Self { constant: Some(value), nonzero: value != 0, ..Self::unknown() }
    }
    fn join(self, other: Self) -> Self {
        let pending = match (self.pending, other.pending) {
            (Some(left), Some(right)) if left != right => return Self::default(),
            (left, right) => left.or(right),
        };
        Self {
            available: self.available && other.available,
            constant: (self.constant == other.constant).then_some(self.constant).flatten(),
            nonzero: self.nonzero && other.nonzero,
            pending,
            status: (self.status == other.status).then_some(self.status).flatten(),
            stack: (self.stack == other.stack).then_some(self.stack).flatten(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    registers: [Value; 32],
    written: u32,
    memory: BTreeMap<i64, (usize, Value)>,
    // Conditional or unavailable bytes survive opaque memory effects. A
    // partial store cannot establish the untouched part of a spilled value.
    uncertain: BTreeMap<i64, Value>,
}

impl State {
    fn entry() -> Self {
        let mut state = Self { registers: [Value::default(); 32], written: 0, memory: BTreeMap::new(), uncertain: BTreeMap::new() };
        state.registers[0] = Value::constant(0);
        for register in [1, 2, 3, 4, 10, 11, 12, 13, 14, 15, 16, 17] {
            state.registers[register] = Value::unknown();
        }
        state.registers[2].stack = Some(0);
        state
    }
    fn write(&mut self, register: usize, value: Value) {
        if register != 0 {
            self.registers[register] = value;
            self.written |= 1 << register;
        }
    }
    fn join(&self, other: &Self) -> Self {
        Self {
            registers: std::array::from_fn(|index| self.registers[index].join(other.registers[index])),
            written: self.written & other.written,
            memory: self
                .memory
                .iter()
                .filter_map(|(offset, (width, value))| {
                    other
                        .memory
                        .get(offset)
                        .filter(|(other, _)| other == width)
                        .map(|(_, other)| (*offset, (*width, value.join(*other))))
                })
                .collect(),
            uncertain: self
                .uncertain
                .keys()
                .chain(other.uncertain.keys())
                .map(|offset| {
                    let left = self.uncertain.get(offset).copied().unwrap_or_else(Value::unknown);
                    let right = other.uncertain.get(offset).copied().unwrap_or_else(Value::unknown);
                    (*offset, left.join(right))
                })
                .collect(),
        }
    }
    fn refine(&mut self, register: usize, zero: bool) {
        let token = self.registers[register].status;
        for value in
            self.registers.iter_mut().chain(self.memory.values_mut().map(|(_, value)| value)).chain(self.uncertain.values_mut())
        {
            if token.is_some() && value.status == token {
                value.constant = zero.then_some(0);
                value.nonzero = !zero;
            }
            if zero && token.is_some() && value.pending == token {
                value.pending = None;
            }
        }
        self.registers[register].constant = zero.then_some(0);
        self.registers[register].nonzero = !zero;
        self.uncertain.retain(|_, value| !value.available || value.pending.is_some());
    }
    fn forget(&mut self, token: u64) {
        for value in
            self.registers.iter_mut().chain(self.memory.values_mut().map(|(_, value)| value)).chain(self.uncertain.values_mut())
        {
            if value.status == Some(token) {
                value.status = None;
            }
            if value.pending == Some(token) {
                *value = Value::default();
            }
        }
    }

    fn store(&mut self, offset: i64, width: usize, value: Value) {
        self.memory
            .retain(|base, (size, _)| base.saturating_add(*size as i64) <= offset || offset.saturating_add(width as i64) <= *base);
        for byte in offset..offset.saturating_add(width as i64) {
            if !value.available || value.pending.is_some() {
                self.uncertain.insert(byte, Value { constant: None, status: None, stack: None, ..value });
            } else {
                self.uncertain.remove(&byte);
            }
        }
        self.memory.insert(offset, (width, value));
    }

    fn load(&self, offset: i64, width: usize, pointer: Value) -> Value {
        let value = self
            .memory
            .get(&offset)
            .filter(|(size, _)| *size == width)
            .map(|(_, value)| *value)
            .unwrap_or_else(|| Value { available: pointer.available && pointer.pending.is_none(), ..Value::default() });
        self.uncertain.range(offset..offset.saturating_add(width as i64)).fold(value, |value, (_, byte)| value.join(*byte))
    }
}

fn status(abi: Abi) -> Option<usize> {
    match abi {
        Abi::ScalarStatusA1 | Abi::ScalarZeroStatusOrAbort => Some(11),
        Abi::StatusA0 | Abi::ReturnedStatusA0 | Abi::BufferStatusA0 | Abi::StatusA0ValueA1 | Abi::StatusA0ValuesA1A2 => Some(10),
        Abi::ValuesA0ToA3StatusA4 => Some(14),
        _ => None,
    }
}

fn results(abi: Abi) -> &'static [usize] {
    match abi {
        Abi::ScalarValue | Abi::ScalarOrAbort | Abi::ScalarStatusA1 | Abi::ScalarZeroStatusOrAbort => &[10],
        Abi::StatusA0ValueA1 => &[11],
        Abi::StatusA0ValuesA1A2 => &[11, 12],
        Abi::ValuesA0ToA3StatusA4 => &[10, 11, 12, 13],
        _ => &[],
    }
}

fn returned(abi: Abi, state: &State) -> bool {
    if abi == Abi::NoReturn {
        return false;
    }
    let returned_status = status(abi).map(|register| state.registers[register]);
    if let Some(register) = status(abi) {
        let value = state.registers[register];
        if state.written & (1 << register) == 0 || !value.available || value.pending.is_some() {
            return false;
        }
        if abi == Abi::ScalarZeroStatusOrAbort && value.constant != Some(0) {
            return false;
        }
        if value.nonzero {
            return true;
        }
    }
    results(abi).iter().all(|&register| {
        let value = state.registers[register];
        let multi = matches!(abi, Abi::StatusA0ValueA1 | Abi::StatusA0ValuesA1A2 | Abi::ValuesA0ToA3StatusA4);
        value.available
            && (!multi || state.written & (1 << register) != 0)
            && (value.pending.is_none() || value.pending == returned_status.and_then(|value| value.status))
    })
}

fn call(state: &mut State, abi: Abi, pc: u64) {
    state.forget(pc);
    state.memory.clear(); // A register audit must not assume callee memory effects.
    for register in [1, 5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 28, 29, 30, 31] {
        state.registers[register] = Value::default();
        state.written &= !(1 << register);
    }
    for &register in results(abi) {
        state.write(
            register,
            Value { pending: status(abi).filter(|_| abi != Abi::ScalarZeroStatusOrAbort).map(|_| pc), ..Value::unknown() },
        );
    }
    if let Some(register) = status(abi) {
        state.write(
            register,
            Value { status: Some(pc), ..if abi == Abi::ScalarZeroStatusOrAbort { Value::constant(0) } else { Value::unknown() } },
        );
    }
}

fn compatible(caller: Abi, callee: Abi) -> bool {
    caller == callee
        || callee == Abi::NoReturn
        || caller == Abi::Void
        || matches!(
            (caller, callee),
            (Abi::ScalarValue | Abi::ScalarOrAbort, Abi::ScalarValue | Abi::ScalarOrAbort | Abi::ScalarZeroStatusOrAbort)
                | (Abi::ScalarStatusA1, Abi::ScalarZeroStatusOrAbort)
                | (
                    Abi::StatusA0 | Abi::ReturnedStatusA0,
                    Abi::StatusA0 | Abi::ReturnedStatusA0 | Abi::BufferStatusA0 | Abi::StatusA0ValueA1 | Abi::StatusA0ValuesA1A2
                )
                | (Abi::StatusA0ValueA1, Abi::StatusA0ValuesA1A2)
        )
}

fn audit(elf: &cellscript_artifact_checker::ParsedElf, evidence: &MachineLayoutEvidence) -> Vec<String> {
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect::<BTreeMap<_, _>>();
    let owners = evidence
        .symbols
        .iter()
        .filter_map(|(name, address)| {
            runtime_contract::return_abi(name)
                .filter(|_| (evidence.text_start..evidence.text_end).contains(address))
                .map(|abi| (*address, (name.as_str(), abi)))
        })
        .collect::<BTreeMap<_, _>>();
    let mut errors = BTreeSet::new();
    for (&start, &(name, abi)) in &owners {
        let end = owners.range(start + 1..).next().map_or(evidence.text_end, |(address, _)| *address);
        let mut pending = vec![(start, State::entry())];
        let mut incoming = BTreeMap::<(u64, u8), State>::new();
        let mut steps = 0u64;
        let budget = ((end - start) / 4).saturating_mul(128).saturating_add(1024);
        while let Some((pc, mut state)) = pending.pop() {
            steps += 1;
            if steps > budget {
                errors.insert(format!("{name}: register fixed point exceeds its work bound"));
                break;
            }
            if !(start..end).contains(&pc) {
                if !owners.get(&pc).is_some_and(|(_, callee)| compatible(abi, *callee)) {
                    errors.insert(format!("{name}: incompatible tail at {pc:#x}"));
                }
                continue;
            }
            let class = status(abi).map_or(0, |register| {
                if state.registers[register].constant == Some(0) {
                    1
                } else if state.registers[register].nonzero {
                    2
                } else {
                    0
                }
            });
            if let Some(previous) = incoming.get(&(pc, class)) {
                let joined = previous.join(&state);
                if &joined == previous {
                    continue;
                }
                state = joined;
            }
            incoming.insert((pc, class), state.clone());
            let Some(&word) = words.get(&pc) else {
                errors.insert(format!("{name}: missing instruction at {pc:#x}"));
                continue;
            };
            if word == 0x8067 {
                if !returned(abi, &state) {
                    errors.insert(format!("{name}: undefined or incompatible normal return at {pc:#x}"));
                }
                continue;
            }
            let opcode = word & 127;
            let rd = (word >> 7 & 31) as usize;
            let rs = (word >> 15 & 31) as usize;
            let rt = (word >> 20 & 31) as usize;
            let funct = word >> 12 & 7;
            if matches!(opcode, 0x6f | 0x67) {
                let Some(&target) = targets.get(&pc) else {
                    errors.insert(format!("{name}: unresolved jump at {pc:#x}"));
                    continue;
                };
                if rd == 1 {
                    let Some(&(_, callee)) = owners.get(&target) else {
                        errors.insert(format!("{name}: unregistered callee at {pc:#x}"));
                        continue;
                    };
                    if callee == Abi::NoReturn {
                        continue;
                    }
                    call(&mut state, callee, pc);
                    pending.push((pc + 4, state));
                } else {
                    pending.push((target, state));
                }
                continue;
            }
            if opcode == 0x63 {
                let Some(&target) = targets.get(&pc) else {
                    errors.insert(format!("{name}: unresolved branch"));
                    continue;
                };
                let mut taken = state.clone();
                let mut otherwise = state.clone();
                let compared = if rt == 0 {
                    Some(rs)
                } else if rs == 0 {
                    Some(rt)
                } else {
                    None
                };
                let known = state.registers[rs]
                    .constant
                    .zip(state.registers[rt].constant)
                    .and_then(|(left, right)| match funct {
                        0 => Some(left == right),
                        1 => Some(left != right),
                        4 => Some((left as i64) < (right as i64)),
                        5 => Some((left as i64) >= (right as i64)),
                        6 => Some(left < right),
                        7 => Some(left >= right),
                        _ => None,
                    })
                    .or_else(|| {
                        compared.filter(|&register| state.registers[register].nonzero).and(match funct {
                            0 => Some(false),
                            1 => Some(true),
                            _ => None,
                        })
                    });
                if let Some(register) = compared
                    && matches!(funct, 0 | 1)
                {
                    taken.refine(register, funct == 0);
                    otherwise.refine(register, funct != 0);
                }
                if known != Some(false) {
                    pending.push((target, taken));
                }
                if known != Some(true) {
                    pending.push((pc + 4, otherwise));
                }
                continue;
            }
            if word == 0x73 {
                // Cargo.lock pins ckb-vm 0.24.14: DefaultMachine::ecall
                // consumes a7=93 as EXIT. Other syscall memory effects are
                // deliberately not inferred by this register-only analysis.
                if state.registers[17].constant == Some(93) {
                    continue;
                }
                state.forget(pc);
                state.memory.clear();
                state.written &= !(0xff << 10);
                state.write(10, Value { status: Some(pc), ..Value::unknown() });
            } else if opcode == 0x37 || opcode == 0x17 {
                let constant = (word & 0xfffff000) as i32 as i64 as u64;
                state.write(rd, Value::constant(if opcode == 0x17 { pc.wrapping_add(constant) } else { constant }));
            } else if opcode == 0x13 || opcode == 0x1b {
                let input = state.registers[rs];
                let immediate = ((word as i32) >> 20) as i64;
                if opcode == 0x13 && funct == 0 && immediate == 0 && rd == rs {
                    pending.push((pc + 4, state));
                    continue; // A self move cannot establish a missing result.
                }
                let mut value = if funct == 0 && immediate == 0 {
                    input
                } else {
                    Value { available: input.available && input.pending.is_none(), ..Value::default() }
                };
                value.constant = input.constant.and_then(|input| match funct {
                    0 => Some(input.wrapping_add(immediate as u64)),
                    7 => Some(input & immediate as u64),
                    6 => Some(input | immediate as u64),
                    4 => Some(input ^ immediate as u64),
                    _ => None,
                });
                if funct == 7 && immediate == 0 {
                    value = Value::constant(0);
                }
                if opcode == 0x1b {
                    value.constant = value.constant.map(|value| value as u32 as i32 as i64 as u64);
                    value.stack = None;
                    value.status = None; // Narrowing zero does not prove a full-width status was zero.
                } else if funct == 0 {
                    value.stack = input.stack.and_then(|offset| offset.checked_add(immediate));
                }
                value.nonzero = value.constant.is_some_and(|value| value != 0);
                state.write(rd, value);
            } else if opcode == 3 {
                let width = 1usize << (funct & 3);
                let pointer = state.registers[rs];
                let offset = ((word as i32) >> 20) as i64;
                let mut value = pointer
                    .stack
                    .and_then(|base| base.checked_add(offset))
                    .map(|offset| state.load(offset, width, pointer))
                    .unwrap_or_else(|| Value { available: pointer.available && pointer.pending.is_none(), ..Value::default() });
                if width != 8 {
                    value.stack = None;
                    value.status = None;
                    value.constant = value.constant.map(|value| {
                        let shift = 64 - width * 8;
                        if funct < 4 {
                            ((value << shift) as i64 >> shift) as u64
                        } else {
                            value & (u64::MAX >> shift)
                        }
                    });
                    value.nonzero = value.constant.is_some_and(|value| value != 0);
                }
                state.write(rd, value);
            } else if opcode == 0x23 {
                let width = 1usize << (funct & 3);
                let pointer = state.registers[rs];
                let immediate = ((word >> 7 & 31) | (word >> 25 << 5)) as i32;
                let offset = i64::from(immediate << 20 >> 20);
                if let Some(offset) = pointer.stack.and_then(|base| base.checked_add(offset)) {
                    let mut value = state.registers[rt];
                    if width < 8 {
                        value.constant = value.constant.map(|value| value & (u64::MAX >> (64 - width * 8)));
                        value.nonzero = value.constant.is_some_and(|value| value != 0);
                        value.status = None;
                        value.stack = None;
                    }
                    state.store(offset, width, value);
                } else {
                    state.memory.clear();
                }
            } else if opcode == 0x33 || opcode == 0x3b {
                let left = state.registers[rs];
                let right = state.registers[rt];
                let mut value = if rs == rt && matches!((funct, word >> 25), (4, 0) | (0, 32) | (2, 0) | (3, 0)) {
                    Value::constant(0)
                } else if funct == 0 && word >> 25 == 0 && rt == 0 {
                    left
                } else if funct == 0 && word >> 25 == 0 && rs == 0 {
                    right
                } else {
                    Value {
                        available: left.available && right.available && left.pending.is_none() && right.pending.is_none(),
                        ..Value::default()
                    }
                };
                if opcode == 0x3b {
                    value.constant = value.constant.map(|value| value as u32 as i32 as i64 as u64);
                    value.nonzero = value.constant.is_some_and(|value| value != 0);
                    value.status = None;
                    value.stack = None;
                }
                state.write(rd, value);
            } else {
                errors.insert(format!("{name}: instruction effects are unmodeled at {pc:#x}: {word:#x}"));
                continue;
            }
            pending.push((pc + 4, state));
        }
    }
    errors.into_iter().collect()
}

#[test]
fn every_runtime_normal_return_obeys_its_declared_register_convention() {
    for diagnostics in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
        let (assembly, evidence) = runtime_inventory(diagnostics);
        let artifact = assembler::assemble_generated_elf(&assembly).unwrap();
        let elf = cellscript_artifact_checker::parse_elf(&artifact, 1_000_000).unwrap();
        let problems = audit(&elf, &evidence);
        let classes =
            evidence.symbols.keys().filter_map(|name| runtime_contract::return_abi(name)).map(Abi::name).collect::<BTreeSet<_>>();
        assert!(Abi::ALL.iter().all(|abi| classes.contains(abi.name())), "missing emitted return-register convention");
        assert!(problems.is_empty(), "{diagnostics}:\n{}", problems.join("\n"));
    }
}

#[test]
fn removing_zero_status_or_pair_results_cannot_pass_the_decoded_register_audit() {
    let (assembly, evidence) = runtime_inventory(crate::CompilerDiagnostics::without_optimizations());
    let artifact = assembler::assemble_generated_elf(&assembly).unwrap();
    let original = cellscript_artifact_checker::parse_elf(&artifact, 1_000_000).unwrap();
    assert!(audit(&original, &evidence).is_empty());
    for (name, register, opcode) in
        [("__ckb_cell_capacity", 11, 0x13), ("__ckb_pipe", 11, 3), ("__ckb_pipe", 12, 3), ("__ckb_spawn_with_fd1", 11, 3)]
    {
        let start = evidence.symbols[name];
        let end = evidence
            .globals
            .iter()
            .filter_map(|name| evidence.symbols.get(name))
            .copied()
            .filter(|address| *address > start)
            .min()
            .unwrap_or(evidence.text_end);
        let sites = original
            .instructions
            .iter()
            .filter(|instruction| {
                (start..end).contains(&instruction.address)
                    && instruction.word & 127 == opcode
                    && (instruction.word >> 7 & 31) == register
                    && (opcode != 0x13 || instruction.word >> 15 == 0)
            })
            .collect::<Vec<_>>();
        assert_eq!(sites.len(), 1, "{name}/x{register}: mutation must identify exactly one result definition");
        for replacement in [0x13, (register << 15) | (register << 7) | 0x13] {
            let mut bytes = artifact.clone();
            let offset = (original.text.offset + sites[0].address - original.text.address) as usize;
            bytes[offset..offset + 4].copy_from_slice(&replacement.to_le_bytes());
            let changed = cellscript_artifact_checker::parse_elf(&bytes, 1_000_000).unwrap();
            assert!(!audit(&changed, &evidence).is_empty(), "{name}/x{register}: omitted result or self-move escaped");
        }
    }
}

fn synthetic(functions: &[(&str, &str)]) -> (cellscript_artifact_checker::ParsedElf, MachineLayoutEvidence) {
    let ast = crate::frontend::parse_diagnostics("module register_audit", crate::CellScriptEdition::Edition2027).unwrap();
    let ir = crate::ir::generate(&ast).unwrap();
    let mut assembly = vec![".section .text".into(), ".global stack_inventory".into(), "stack_inventory:".into(), "ret".into()];
    for (name, body) in functions {
        assembly.extend([format!(".global {name}"), format!("{name}:")]);
        assembly.extend(body.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned));
    }
    let evidence = assembler::machine_layout_evidence(&assembly, &BTreeMap::new(), &ir).unwrap();
    let artifact = assembler::assemble_generated_elf(&assembly).unwrap();
    (cellscript_artifact_checker::parse_elf(&artifact, 1_000_000).unwrap(), evidence)
}

#[test]
fn failed_call_results_cannot_become_valid_by_overwriting_or_narrowing_the_status() {
    assert_eq!(runtime_contract::return_abi("__ckb_cell_data_size"), Some(Abi::ScalarZeroStatusOrAbort));
    assert_eq!(runtime_contract::return_abi("__ckb_cell_capacity"), Some(Abi::ScalarStatusA1));
    let callee = "beqz a0, .Lvalue\nli a1, 73\nret\n.Lvalue:\nli a0, 42\nli a1, 0\nret";
    let prefix = "addi sp, sp, -16\nsd ra, 8(sp)\ncall __ckb_cell_capacity\n";
    let suffix = "\nmv a0, a1\nli a7, 93\necall\n.Lok:\nli a1, 0\nld ra, 8(sp)\naddi sp, sp, 16\nret";
    for (guard, accepted) in
        [("beqz a1, .Lok", true), ("bnez a1, .Lok", false), ("slli a1, a1, 32\nsrli a1, a1, 32\nbeqz a1, .Lok", false)]
    {
        let caller = format!("{prefix}{guard}{suffix}");
        let (elf, evidence) = synthetic(&[("__ckb_cell_data_size", &caller), ("__ckb_cell_capacity", callee)]);
        assert_eq!(audit(&elf, &evidence).is_empty(), accepted, "{guard}");
    }
}

#[test]
fn byte_status_storage_truncates_before_failure_paths_can_ignore_a_missing_value() {
    assert_eq!(runtime_contract::return_abi("__ckb_cell_capacity"), Some(Abi::ScalarStatusA1));
    let caller="addi sp, sp, -16\nsd ra, 8(sp)\ncall __novaseal_bip340_require_signature_from_cell_dep\nli a1, 256\nsb a1, 0(sp)\nlbu a1, 0(sp)\nld ra, 8(sp)\naddi sp, sp, 16\nret";
    let (elf, evidence) = synthetic(&[("__ckb_cell_capacity", caller), ("__novaseal_bip340_require_signature_from_cell_dep", "ret")]);
    assert!(!audit(&elf, &evidence).is_empty());
    let valid = caller.replace("li a1, 256", "li a0, 42\nli a1, 256");
    let (elf, evidence) = synthetic(&[("__ckb_cell_capacity", &valid), ("__novaseal_bip340_require_signature_from_cell_dep", "ret")]);
    assert!(audit(&elf, &evidence).is_empty());
}

#[test]
fn nonreturning_helpers_cannot_acquire_a_normal_return() {
    for (body, accepted) in [("li a0, 73\nli a7, 93\necall", true), ("ret", false)] {
        let (elf, evidence) = synthetic(&[("__cellscript_abort", body)]);
        assert_eq!(audit(&elf, &evidence).is_empty(), accepted);
    }
}

#[test]
fn stack_spills_retain_unchecked_result_obligations_across_calls_and_partial_stores() {
    let callee = "li a0, 42\nli a1, 0\nret";
    let prefix = "addi sp, sp, -32\nsd ra, 24(sp)\ncall __ckb_cell_capacity\n";
    let suffix = "\nld a0, 0(sp)\nli a1, 0\nld ra, 24(sp)\naddi sp, sp, 32\nret";
    for middle in ["sd a0, 0(sp)\ncall __novaseal_bip340_require_signature_from_cell_dep", "sd a0, 0(sp)\nsb zero, 0(sp)"] {
        let caller = format!("{prefix}{middle}{suffix}");
        let (elf, evidence) = synthetic(&[
            ("__ckb_cell_data_size", &caller),
            ("__ckb_cell_capacity", callee),
            ("__novaseal_bip340_require_signature_from_cell_dep", "ret"),
        ]);
        assert!(!audit(&elf, &evidence).is_empty(), "unchecked spilled result escaped: {middle}");
        let caller = format!("{prefix}beqz a1, .Lchecked\nmv a0, a1\nli a7, 93\necall\n.Lchecked:\n{middle}{suffix}");
        let (elf, evidence) = synthetic(&[
            ("__ckb_cell_data_size", &caller),
            ("__ckb_cell_capacity", callee),
            ("__novaseal_bip340_require_signature_from_cell_dep", "ret"),
        ]);
        assert!(audit(&elf, &evidence).is_empty(), "checked spill rejected: {middle}");
    }
}
