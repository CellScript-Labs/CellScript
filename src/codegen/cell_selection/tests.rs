use super::*;
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, memory::Memory, registers::*, CoreMachine, DefaultCoreMachine,
    DefaultMachineBuilder, DefaultMachineRunner, Error, SparseMemory, SupportMachine, Syscalls, TraceMachine, WXorXMemory, ISA_B,
    ISA_IMC, ISA_MOP,
};

#[path = "../../../tests/support/cell_selection_reference.rs"]
#[allow(dead_code)]
mod reference;
use reference::{Access, Cell, Contract, Count, CurrentScript, Role, ScriptRole, Side};

mod nested;
mod transaction;

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;

fn derived(contract: &Contract) -> ExactSelection {
    ExactSelection {
        roles: contract
            .roles
            .iter()
            .map(|role| ExactRole {
                type_hash: role.type_hash,
                lock_hash: role.lock_hash,
                schema: role.schema,
                width: role.width,
                key_offset: role.key_offset,
                key_width: role.key_width,
                inputs: (role.inputs.min, role.inputs.max),
                outputs: (role.outputs.min, role.outputs.max),
                local_type: role.access == Access::LocalType,
                correspondence: match &role.correspondence {
                    reference::Correspondence::Preserve => Correspondence::Preserve,
                    reference::Correspondence::Create => Correspondence::Create,
                    reference::Correspondence::Burn => Correspondence::Burn,
                    reference::Correspondence::Merge { target_key } => Correspondence::Merge(target_key.clone()),
                },
                nested: None,
            })
            .collect(),
        input_scan: contract.input_scan,
        output_scan: contract.output_scan,
        selected_per_side: contract.selected_per_side,
        aggregate_bytes: contract.aggregate_bytes,
    }
}

fn contract() -> Contract {
    Contract {
        roles: (0..2)
            .map(|role| Role {
                type_hash: [1 + role; 32],
                lock_hash: [11 + role; 32],
                schema: [21 + role; 32],
                width: 16 + role as usize * 8,
                key_offset: role as usize * 8,
                key_width: 8,
                inputs: Count { min: 0, max: 32 },
                outputs: Count { min: 0, max: 32 },
                access: if role == 0 { Access::LocalType } else { Access::Observe },
                correspondence: reference::Correspondence::Preserve,
            })
            .collect(),
        input_scan: 64,
        output_scan: 64,
        selected_per_side: 32,
        aggregate_bytes: 32768,
    }
}

fn current() -> CurrentScript {
    CurrentScript { role: ScriptRole::Type, hash: [1; 32] }
}

fn cell(contract: &Contract, role: usize, key: u8) -> Cell {
    let role = &contract.roles[role];
    let mut data = vec![0; role.width];
    data[role.key_offset..role.key_offset + role.key_width].fill(key);
    Cell { type_hash: Some(role.type_hash), lock_hash: role.lock_hash, data }
}

fn fee() -> Cell {
    Cell { type_hash: None, lock_hash: [11; 32], data: vec![0; 7] }
}

fn status(error: reference::Error) -> i8 {
    match error {
        reference::Error::Authority => 47,
        reference::Error::LockIdentity => 41,
        reference::Error::Layout => 4,
        reference::Error::Correspondence | reference::Error::ProposalMismatch => 7,
        reference::Error::ScanBound
        | reference::Error::SelectedBound
        | reference::Error::ByteBound
        | reference::Error::DuplicateKey
        | reference::Error::Cardinality
        | reference::Error::DuplicateIndex
        | reference::Error::ProposalBound => 21,
        reference::Error::ProposalIndex => 44,
        reference::Error::Contract | reference::Error::AmbiguousRole => panic!("static admission failure"),
    }
}

fn program(contract: &ExactSelection, expected: Option<&reference::Selection>) -> Vec<u8> {
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    generator.emit(".section .text");
    generator.emit(".global entry");
    generator.emit_label("entry");
    generator.emit_large_addi("sp", "sp", -(contract.frame_bytes() as i64));
    generator.emit_exact_role_selection(contract).unwrap();
    if let Some(expected) = expected {
        for (side, count, table) in [(Side::Input, SELECTED_INPUTS, INPUT_TABLE), (Side::Output, SELECTED_OUTPUTS, OUTPUT_TABLE)] {
            let cells: Vec<_> = expected.cells.iter().filter(|cell| cell.side == side).collect();
            generator.emit_stack_load("t0", count);
            generator.emit(format!("li t1, {}", cells.len()));
            generator.emit("bne t0, t1, bad_result");
            for (ordinal, cell) in cells.iter().enumerate() {
                let pair = expected
                    .pairs
                    .iter()
                    .find(|pair| {
                        pair.role == cell.role
                            && match side {
                                Side::Input => pair.inputs.contains(&cell.index),
                                Side::Output => pair.outputs.contains(&cell.index),
                            }
                    })
                    .unwrap();
                let partner = match side {
                    Side::Input => pair.outputs.first().copied(),
                    Side::Output if pair.inputs.len() == 1 => pair.inputs.first().copied(),
                    Side::Output => None,
                };
                for (offset, value) in [
                    (0, cell.index as u64),
                    (8, cell.group_index.map_or(u64::MAX, |index| index as u64)),
                    (16, cell.role as u64),
                    (56, partner.map_or(u64::MAX, |index| index as u64)),
                ] {
                    generator.emit_stack_load("t0", table + ordinal * RECORD_BYTES + offset);
                    generator.emit(format!("li t1, {}", value as i64));
                    generator.emit("bne t0, t1, bad_result");
                }
                for (offset, byte) in cell.key.iter().enumerate() {
                    generator.emit_stack_load_byte("t0", table + ordinal * RECORD_BYTES + 24 + offset);
                    generator.emit(format!("li t1, {byte}"));
                    generator.emit("bne t0, t1, bad_result");
                }
            }
        }
    }
    generator.emit("li a0, 0");
    generator.emit("li a7, 93");
    generator.emit("ecall");
    generator.emit_label("bad_result");
    generator.emit("li a0, 127");
    generator.emit("li a7, 93");
    generator.emit("ecall");
    generator.emit_process_failure_helper();
    assemble_generated_elf(&generator.assembly).unwrap()
}

/// Independent syscall fixture only; real CKB transaction evidence is separate.
struct Transaction {
    current: CurrentScript,
    inputs: Vec<Cell>,
    outputs: Vec<Cell>,
    fault: Option<ReadFault>,
}

#[derive(Clone, Copy)]
struct ReadFault {
    syscall: u64,
    field: Option<u64>,
    status: u64,
    length: Option<u64>,
}

impl Transaction {
    fn cell(&self, source: u64, index: usize) -> Option<&Cell> {
        let cells = if source & 0xff == 1 { &self.inputs } else { &self.outputs };
        if source & CKB_SOURCE_GROUP_FLAG == 0 {
            return cells.get(index);
        }
        cells
            .iter()
            .filter(|cell| match self.current.role {
                ScriptRole::Type => cell.type_hash == Some(self.current.hash),
                ScriptRole::Lock => source & 0xff == 1 && cell.lock_hash == self.current.hash,
            })
            .nth(index)
    }
}

impl Syscalls<Core> for Transaction {
    fn initialize(&mut self, _: &mut Core) -> std::result::Result<(), Error> {
        Ok(())
    }
    fn ecall(&mut self, machine: &mut Core) -> std::result::Result<bool, Error> {
        let syscall = machine.registers()[A7];
        if ![ckb_abi::syscall::LOAD_SCRIPT_HASH, ckb_abi::syscall::LOAD_CELL_BY_FIELD, ckb_abi::syscall::LOAD_CELL_DATA]
            .contains(&syscall)
        {
            return Ok(false);
        }
        let fault = self.fault.filter(|fault| {
            syscall == fault.syscall
                && machine.registers()[A4] == CKB_SOURCE_INPUT
                && machine.registers()[A3] == 1
                && fault.field.is_none_or(|field| machine.registers()[A5] == field)
        });
        if let Some(fault) = fault
            && fault.status != 0
        {
            machine.set_register(A0, fault.status);
            return Ok(true);
        }
        let data = if syscall == ckb_abi::syscall::LOAD_SCRIPT_HASH {
            Some(self.current.hash.to_vec())
        } else {
            let source = machine.registers()[A4];
            assert!([CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_GROUP_INPUT, CKB_SOURCE_GROUP_OUTPUT].contains(&source));
            let Some(cell) = self.cell(source, machine.registers()[A3] as usize) else {
                machine.set_register(A0, 1);
                return Ok(true);
            };
            if syscall == ckb_abi::syscall::LOAD_CELL_DATA {
                Some(cell.data.clone())
            } else {
                match machine.registers()[A5] {
                    CKB_CELL_FIELD_CAPACITY => Some(100_000_000_000u64.to_le_bytes().to_vec()),
                    CKB_CELL_FIELD_TYPE_HASH => cell.type_hash.map(|hash| hash.to_vec()),
                    CKB_CELL_FIELD_LOCK_HASH => Some(cell.lock_hash.to_vec()),
                    field => panic!("unexpected field {field}"),
                }
            }
        };
        let Some(data) = data else {
            machine.set_register(A0, 2);
            return Ok(true);
        };
        assert_eq!(machine.registers()[A2], 0);
        let size_address = machine.registers()[A1];
        let buffer = machine.registers()[A0];
        let max = machine.memory_mut().load64(&size_address)? as usize;
        machine.memory_mut().store_bytes(buffer, &data[..max.min(data.len())])?;
        machine.memory_mut().store64(&size_address, &fault.and_then(|fault| fault.length).unwrap_or(data.len() as u64))?;
        for reg in [T0, T1, T2, T3, T4, T5, T6, A1, A2, A3, A4, A5, A6, A7] {
            machine.set_register(reg, 0xfeed);
        }
        machine.set_register(A0, 0);
        Ok(true)
    }
}

fn agree(contract: &Contract, current: CurrentScript, inputs: &[Cell], outputs: &[Cell]) -> (u64, usize) {
    let expected = reference::select(contract, current, inputs, outputs);
    let elf = program(&derived(contract), expected.as_ref().ok());
    let (actual, cycles, elf_len) =
        execute_vm(elf, Transaction { current, inputs: inputs.to_vec(), outputs: outputs.to_vec(), fault: None });
    assert_eq!(actual, expected.as_ref().err().map_or(0, |error| status(*error)), "oracle={expected:?}");
    (cycles, elf_len)
}

fn execute_vm(elf: Vec<u8>, transaction: Transaction) -> (i8, u64, usize) {
    let elf_len = elf.len();
    let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 10_000_000);
    let mut machine = TraceMachine::new(
        DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).syscall(Box::new(transaction)).build(),
    );
    machine.load_program(&ckb_vm::Bytes::from(elf), std::iter::empty::<std::result::Result<ckb_vm::Bytes, Error>>()).unwrap();
    let actual = machine.run().expect("bounded selection must terminate without a trap");
    (actual, machine.machine.cycles(), elf_len)
}

#[test]
fn exact_role_machine_non_end_syscall_errors_and_wrong_field_widths_are_fatal() {
    let contract = contract();
    let cells = vec![cell(&contract, 0, 1), cell(&contract, 1, 2)];
    let elf = program(&derived(&contract), None);
    for (syscall, field, width, malformed_code) in [
        (ckb_abi::syscall::LOAD_CELL_BY_FIELD, Some(CKB_CELL_FIELD_TYPE_HASH), 32, 44),
        (ckb_abi::syscall::LOAD_CELL_BY_FIELD, Some(CKB_CELL_FIELD_LOCK_HASH), 32, 44),
        (ckb_abi::syscall::LOAD_CELL_DATA, None, 24, 4),
    ] {
        for (status, length, expected) in
            [(1, None, 44), (3, None, 44), (255, None, 44), (0, Some(width - 1), malformed_code), (0, Some(width + 1), malformed_code)]
        {
            let (actual, _, _) = execute_vm(
                elf.clone(),
                Transaction {
                    current: current(),
                    inputs: cells.clone(),
                    outputs: cells.clone(),
                    fault: Some(ReadFault { syscall, field, status, length }),
                },
            );
            assert_eq!(actual, expected, "syscall={syscall} field={field:?} status={status} length={length:?}");
        }
    }
}

#[test]
fn exact_role_machine_selects_complete_mixed_sources_and_pairs_permutations() {
    fn permutations(cells: &[Cell]) -> Vec<Vec<Cell>> {
        if cells.is_empty() {
            return vec![vec![]];
        }
        let mut result = Vec::new();
        for index in 0..cells.len() {
            let mut rest = cells.to_vec();
            let first = rest.remove(index);
            for mut tail in permutations(&rest) {
                tail.insert(0, first.clone());
                result.push(tail);
            }
        }
        result
    }
    let contract = contract();
    let cells = vec![fee(), cell(&contract, 1, 9), cell(&contract, 0, 9), cell(&contract, 0, 3)];
    let permutations = permutations(&cells);
    assert_eq!(permutations.len(), 24);
    for inputs in &permutations {
        for outputs in &permutations {
            agree(&contract, current(), inputs, outputs);
        }
    }
}

#[test]
fn exact_role_machine_custody_layout_duplicates_and_budgets_are_terminal() {
    let contract = contract();
    let cells = vec![cell(&contract, 0, 1), fee(), cell(&contract, 1, 2)];
    for output in [false, true] {
        for mutation in 0..7 {
            let mut changed = cells.clone();
            match mutation {
                0 => changed[2].lock_hash[31] ^= 1,
                1 => {
                    changed[2].data.pop();
                }
                2 => changed[2].data.push(0),
                3 => changed.push(changed[2].clone()),
                4 => {
                    changed.remove(2);
                }
                5 => changed[2].data[8] ^= 1,
                _ => changed[2].type_hash.as_mut().unwrap()[31] ^= 1,
            }
            if output {
                agree(&contract, current(), &cells, &changed);
            } else {
                agree(&contract, current(), &changed, &cells);
            }
        }
    }
    for budget in 0..5 {
        let mut bounded = contract.clone();
        match budget {
            0 => bounded.input_scan = 2,
            1 => bounded.output_scan = 2,
            2 => bounded.selected_per_side = 1,
            3 => bounded.aggregate_bytes = 79,
            _ => bounded.roles[1].outputs.min = 2,
        }
        agree(&bounded, current(), &cells, &cells);
    }
    // Full-source budget failures precede protected data failures.
    let mut invalid = cells.clone();
    invalid[0].data.clear();
    let mut bounded = contract.clone();
    bounded.output_scan = 2;
    agree(&bounded, current(), &invalid, &cells);
}

#[test]
fn exact_role_machine_checks_explicit_create_burn_merge_and_foreign_authority() {
    let mut contract = contract();
    let local = cell(&contract, 0, 1);
    let a = cell(&contract, 1, 2);
    let b = cell(&contract, 1, 3);
    let target = cell(&contract, 1, 4);
    contract.roles[1].correspondence = reference::Correspondence::Create;
    agree(&contract, current(), std::slice::from_ref(&local), &[a.clone(), local.clone()]);
    agree(&contract, current(), &[a.clone(), local.clone()], &[a.clone(), local.clone()]);
    contract.roles[1].correspondence = reference::Correspondence::Burn;
    agree(&contract, current(), &[local.clone(), a.clone()], std::slice::from_ref(&local));
    agree(&contract, current(), std::slice::from_ref(&local), std::slice::from_ref(&local));
    contract.roles[1].correspondence = reference::Correspondence::Merge { target_key: vec![4; 8] };
    agree(&contract, current(), &[a.clone(), local.clone(), b.clone()], &[target.clone(), local.clone()]);
    agree(&contract, current(), &[a.clone(), local.clone(), b], &[a.clone(), local.clone()]);
    agree(&contract, current(), &[a.clone(), local.clone()], &[target, local.clone()]);
    contract.roles[1].access = Access::LocalType;
    agree(&contract, current(), &[local.clone(), a], std::slice::from_ref(&local));
}

#[test]
fn exact_role_machine_lock_observation_preserves_actual_group_indexes_and_empty_selection() {
    let mut contract = contract();
    for role in &mut contract.roles {
        role.access = Access::Observe;
    }
    let current = CurrentScript { role: ScriptRole::Lock, hash: [11; 32] };
    let cells = [fee(), cell(&contract, 1, 1), cell(&contract, 0, 2), fee(), cell(&contract, 0, 3)];
    agree(&contract, current, &cells, &cells);
    agree(&contract, current, &[fee()], &[fee()]);
    contract.roles[0].access = Access::LocalType;
    agree(&contract, current, &cells, &cells);
}

#[test]
fn exact_role_machine_creation_group_and_maximum_late_invalid_sets() {
    let mut creation = contract();
    creation.roles[0].correspondence = reference::Correspondence::Create;
    let output = cell(&creation, 0, 1);
    agree(&creation, current(), &[fee()], &[output]);
    let mut contract = contract();
    contract.roles = (0..8)
        .map(|index| Role {
            type_hash: [index + 1; 32],
            lock_hash: [index + 11; 32],
            schema: [index + 21; 32],
            width: 512,
            key_offset: 480,
            key_width: 32,
            inputs: Count { min: 4, max: 4 },
            outputs: Count { min: 4, max: 4 },
            access: if index == 0 { Access::LocalType } else { Access::Observe },
            correspondence: reference::Correspondence::Preserve,
        })
        .collect();
    for (index, role) in contract.roles.iter_mut().enumerate() {
        role.type_hash = [1; 32];
        role.type_hash[31] = index as u8 + 1;
        role.lock_hash = [11; 32];
        role.lock_hash[31] = index as u8 + 11;
    }
    let mut cells: Vec<_> = (0..32).flat_map(|index| [fee(), cell(&contract, index % 8, index as u8)]).collect();
    for (index, cell) in cells.iter_mut().enumerate().filter(|(index, _)| index % 2 == 1) {
        cell.data[480..511].fill(0);
        cell.data[511] = (index / 2) as u8;
    }
    let mut outputs = cells.clone();
    outputs.reverse();
    let valid = agree(&contract, current(), &cells, &outputs);
    cells[63].data[511] ^= 1;
    let invalid = agree(&contract, current(), &cells, &outputs);
    eprintln!("[exact-role primitive] roles=8 selected=64 data=32768 frame={SELECTION_FRAME_BYTES} valid_cycles={} late_invalid_cycles={} test_ELF={}", valid.0, invalid.0, valid.1);
}

#[test]
fn exact_role_machine_rejects_invalid_derived_contracts_before_emitting_code() {
    let baseline = derived(&contract());
    for mutation in 0..13 {
        let mut invalid = baseline.clone();
        match mutation {
            0 => invalid.roles.clear(),
            1 => invalid.roles = vec![invalid.roles[0].clone(); 9],
            2 => invalid.input_scan = 0,
            3 => invalid.output_scan = 65,
            4 => invalid.selected_per_side = 33,
            5 => invalid.aggregate_bytes = 32769,
            6 => invalid.roles[0].width = 513,
            7 => invalid.roles[0].key_width = 0,
            8 => invalid.roles[0].key_offset = usize::MAX,
            9 => invalid.roles[0].inputs = (3, 2),
            10 => invalid.roles[0].outputs = (0, 33),
            11 => invalid.roles[1].type_hash = invalid.roles[0].type_hash,
            _ => invalid.roles[0].correspondence = Correspondence::Merge(vec![1; 9]),
        }
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        assert!(generator.emit_exact_role_selection(&invalid).is_err());
        assert!(generator.assembly.is_empty());
    }
}
