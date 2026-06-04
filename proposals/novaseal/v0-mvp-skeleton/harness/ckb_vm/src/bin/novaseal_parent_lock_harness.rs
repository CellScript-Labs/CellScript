#![allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::struct_excessive_bools,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ckb_chain_spec::consensus::{Consensus, ConsensusBuilder};
use ckb_script::{TransactionScriptsVerifier, TxVerifyEnv};
use ckb_traits::{CellDataProvider, ExtensionProvider, HeaderProvider};
use ckb_types::{
    bytes::Bytes as CkbBytes,
    core::{
        Capacity, DepType, EpochNumberWithFraction, HeaderBuilder, ScriptHashType, TransactionView,
        cell::{CellMetaBuilder, ResolvedTransaction},
        hardfork::{CKB2021, CKB2023, HardForks},
    },
    packed,
    prelude::*,
};
use ckb_vm::{
    Bytes, DefaultCoreMachine, DefaultMachineBuilder, ISA_B, ISA_IMC, ISA_MOP, SparseMemory, SupportMachine, Syscalls, TraceMachine,
    WXorXMemory,
    cost_model::estimate_cycles,
    machine::VERSION2,
    memory::Memory,
    registers::{A0, A1, A2, A3, A4, A7},
};
use k256::schnorr::SigningKey;
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

const DEFAULT_PARENT_ELF: &str = "build/nova_state_type.elf";
const DEFAULT_CHILD_ELF: &str = "target/novaseal-btc-verifier-riscv-shell-release.elf";
const DEFAULT_OUTPUT: &str = "target/novaseal-parent-lock-ckb-vm-report.json";
const MAX_CYCLES_DEFAULT: u64 = 200_000_000;
const RESOLVED_SCRIPT_VERIFY_MAX_CYCLES: u64 = 500_000_000;
const VM2_ENABLED_EPOCH: u64 = 10;

const CKB_SOURCE_INPUT: u64 = 0x01;
const CKB_SOURCE_CELL_DEP: u64 = 0x03;
const CKB_SOURCE_GROUP_INPUT: u64 = 0x0100_0000_0000_0000 | CKB_SOURCE_INPUT;

const CKB_LOAD_SCRIPT_SYSCALL_NUMBER: u64 = 2052;
const CKB_LOAD_WITNESS_SYSCALL_NUMBER: u64 = 2074;
const CKB_LOAD_CELL_DATA_SYSCALL_NUMBER: u64 = 2092;
const CKB_VM2_SPAWN_SYSCALL_NUMBER: u64 = 2601;
const CKB_VM2_WAIT_SYSCALL_NUMBER: u64 = 2602;
const CKB_VM2_PIPE_SYSCALL_NUMBER: u64 = 2604;
const CKB_VM2_PIPE_WRITE_SYSCALL_NUMBER: u64 = 2605;
const CKB_VM2_PIPE_READ_SYSCALL_NUMBER: u64 = 2606;
const CKB_VM2_INHERITED_FD_SYSCALL_NUMBER: u64 = 2607;
const CKB_VM2_CLOSE_SYSCALL_NUMBER: u64 = 2608;

const CHILD_INPUT_FD: u64 = 100;
const PARENT_READ_FD: u64 = 200;
const PARENT_WRITE_FD: u64 = 201;
const CHILD_PID: u64 = 1;
const IPC_WORD_COUNT: usize = 18;
const IPC_BLOB_LEN: usize = 144;
const TRANSACTION_SHAPE_OUTPUT_MARGIN_SHANNONS: u64 = 10_000_000_000;

const NOVASEAL_CELL_LEN: usize = 146;
const NOVASEAL_INTENT_LEN: usize = 254;
const SIGNATURE_PAYLOAD_LEN: usize = 96;

const CELL_BTC_AUTHORITY_HASH_OFFSET: usize = 2;
const CELL_POLICY_HASH_OFFSET: usize = 66;
const INTENT_PROTOCOL_ID_OFFSET: usize = 0;
const INTENT_POLICY_HASH_OFFSET: usize = 64;
const INTENT_ACTION_OFFSET: usize = 96;
const INTENT_OLD_NONCE_OFFSET: usize = 198;
const INTENT_NEW_NONCE_OFFSET: usize = 206;
const INTENT_EXPIRY_OFFSET: usize = 214;
const SIGNED_INTENT_EXPECTED_RECEIPT_HASH_OFFSET: usize = 222;

const CKB_BLAKE2B_PERSONAL: &[u8; 16] = b"ckb-default-hash";
const PACKED_HASH_DOMAIN: &[u8] = b"CellScriptPackedHashV0\0";
const SIGNED_INTENT_TYPE_NAME: &[u8] = b"NovaSealSignedIntentV0";
const LOCK_WITNESS_MAGIC: &[u8; 8] = b"CSARGv1\0";
const IPC_MAGIC: &[u8; 8] = b"NSBV0IPC";
const IPC_VERSION: u16 = 0;
const IPC_SCHEME_BIP340: u16 = 1;
const IPC_FLAGS_NONE: u32 = 0;

const TEST_SECRET_KEY: [u8; 32] = [
    0x3e, 0x74, 0x90, 0x68, 0x06, 0x39, 0xa2, 0xf7, 0xbb, 0xe8, 0x36, 0x1d, 0xd3, 0xf3, 0x4e, 0xb6, 0x42, 0x9a, 0x9c, 0x92, 0x4d,
    0x8b, 0x34, 0x2c, 0x01, 0x5e, 0x55, 0x5e, 0x62, 0x8f, 0x94, 0xe5,
];
const TEST_WRONG_SECRET_KEY: [u8; 32] = [0x44; 32];
const TEST_AUX_RAND: [u8; 32] = [0x42; 32];

type HarnessMachine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;

#[derive(Clone, Debug, Default)]
struct HarnessDataLoader {
    cells: HashMap<packed::OutPoint, CkbBytes>,
}

impl HarnessDataLoader {
    fn insert(&mut self, out_point: packed::OutPoint, data: CkbBytes) {
        self.cells.insert(out_point, data);
    }
}

impl CellDataProvider for HarnessDataLoader {
    fn get_cell_data(&self, out_point: &packed::OutPoint) -> Option<CkbBytes> {
        self.cells.get(out_point).cloned()
    }

    fn get_cell_data_hash(&self, out_point: &packed::OutPoint) -> Option<packed::Byte32> {
        self.cells.get(out_point).map(|data| packed::CellOutput::calc_data_hash(data))
    }
}

impl HeaderProvider for HarnessDataLoader {
    fn get_header(&self, _hash: &packed::Byte32) -> Option<ckb_types::core::HeaderView> {
        None
    }
}

impl ExtensionProvider for HarnessDataLoader {
    fn get_block_extension(&self, _hash: &packed::Byte32) -> Option<packed::Bytes> {
        None
    }
}

#[derive(Debug, Error)]
enum HarnessError {
    #[error("usage: novaseal_parent_lock_harness [--parent-elf PATH] [--child-elf PATH] [--output PATH] [--max-cycles N] [--pretty]")]
    Usage,
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug)]
struct Args {
    parent_elf: PathBuf,
    child_elf: PathBuf,
    output: PathBuf,
    max_cycles: u64,
    pretty: bool,
}

#[derive(Clone, Copy, Debug)]
enum CaseKind {
    ValidSignature,
    SignatureBitflip,
    AuthorityHashMismatch,
    WrongPubkeyValidSignature,
}

#[derive(Clone, Debug)]
struct ParentCase {
    id: &'static str,
    expected: &'static str,
    mutation: Option<&'static str>,
    kind: CaseKind,
    lock_args: [u8; 32],
    script: Vec<u8>,
    witness: Vec<u8>,
    input_cell_data: Vec<u8>,
    expected_digest: [u8; 32],
    expected_pubkey: [u8; 32],
    expected_signature: [u8; 64],
    expected_ipc_blob: Option<Vec<u8>>,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    classification: &'static str,
    parent_elf: ElfReport,
    child_elf: ElfReport,
    summary: Summary,
    cases: Vec<CaseReport>,
    limits: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct ElfReport {
    path: String,
    size_bytes: usize,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct Summary {
    parent_lock_ckb_vm_executed: bool,
    parent_spawn_executed: bool,
    child_verifier_ckb_vm_executed: bool,
    transaction_shape_constructed: bool,
    consensus_packed_tx_constructed: bool,
    resolved_transaction_constructed: bool,
    resolved_script_verifier_executed: bool,
    resolved_script_verifier_matched_expected: bool,
    full_transaction_constructed: bool,
    full_transaction_executed: bool,
    full_transaction_verifier_matched_expected: bool,
    total_cases: usize,
    expected_accept: usize,
    expected_reject: usize,
    accepted: usize,
    rejected: usize,
    matched_expected: usize,
    mismatched: usize,
    parent_min_cycles: u64,
    parent_max_cycles: u64,
    child_max_cycles: u64,
    load_script_calls: usize,
    load_witness_calls: usize,
    load_cell_data_calls: usize,
    pipe_calls: usize,
    pipe_write_calls: usize,
    spawn_calls: usize,
    wait_calls: usize,
    close_calls: usize,
    max_consensus_tx_size_bytes: usize,
    max_output_occupied_capacity_shannons: u64,
    min_capacity_margin_shannons: u64,
    capacity_shape_checks_passed: bool,
    under_capacity_shape_rejects: bool,
    cell_dep0_spawn_target_modelled: bool,
    parent_lock_dep_modelled: bool,
    resolved_script_verifier_max_cycles: u64,
    full_transaction_verifier_max_cycles: u64,
}

#[derive(Debug, Serialize)]
struct CaseReport {
    id: &'static str,
    expected: &'static str,
    mutation: Option<&'static str>,
    exit_code: i8,
    accepted: bool,
    matched_expected: bool,
    parent_cycles: u64,
    expected_digest: String,
    expected_pubkey: String,
    expected_signature: String,
    observed_ipc_blob: Option<String>,
    expected_ipc_blob: Option<String>,
    ipc_blob_matches_expected: bool,
    transaction_shape: TransactionShapeReport,
    resolved_transaction: ResolvedTransactionReport,
    syscall_trace: ParentSyscallTrace,
}

#[derive(Debug, Serialize)]
struct ResolvedTransactionReport {
    classification: &'static str,
    resolved_transaction_constructed: bool,
    ckb_script_verifier_executed: bool,
    full_transaction_executed: bool,
    accepted: bool,
    matched_expected: bool,
    cycles: Option<u64>,
    error: Option<String>,
    full_transaction_accepted: bool,
    full_transaction_matched_expected: bool,
    full_transaction_cycles: Option<u64>,
    full_transaction_error: Option<String>,
    script_group_count: usize,
    transaction_hash: String,
    lock_group_hash: String,
    lock_group_input_indexes: Vec<usize>,
    lock_group_output_indexes: Vec<usize>,
    resolved_cell_deps: Vec<CellDepShapeReport>,
    resolved_inputs: usize,
    resolved_dep_groups: usize,
    cell_dep0_data_hash_matches_child_elf: bool,
    parent_lock_dep_type_hash_matches_script_code_hash: bool,
    script_bytes_match_vm_load_script: bool,
    limits: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct TransactionShapeReport {
    classification: &'static str,
    consensus_packed_tx_constructed: bool,
    full_transaction_executed: bool,
    resolved_script_group_executed: bool,
    consensus_tx_size_bytes: usize,
    witness_size_bytes: usize,
    input_cell_data_size_bytes: usize,
    output_cell_data_size_bytes: usize,
    current_lock_script_hash: String,
    cell_deps: Vec<CellDepShapeReport>,
    script_group: ScriptGroupShapeReport,
    capacity: CapacityShapeReport,
    checks: TransactionShapeChecks,
    limits: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct CellDepShapeReport {
    index: usize,
    role: &'static str,
    dep_type: &'static str,
    source_model: &'static str,
    code_hash: String,
    out_point_tx_hash_placeholder: String,
    out_point_index: u32,
}

#[derive(Debug, Serialize)]
struct ScriptGroupShapeReport {
    group_type: &'static str,
    current_script_source: &'static str,
    group_input_indexes: Vec<usize>,
    group_output_indexes: Vec<usize>,
    script_args: String,
    script_bytes_match_vm_load_script: bool,
}

#[derive(Debug, Serialize)]
struct CapacityShapeReport {
    input_capacity_shannons: u64,
    output_capacity_shannons: u64,
    output_occupied_capacity_shannons: u64,
    capacity_margin_shannons: u64,
    under_capacity_output_capacity_shannons: u64,
    capacity_is_sufficient: bool,
    under_capacity_rejected_by_shape: bool,
}

#[derive(Debug, Serialize)]
struct TransactionShapeChecks {
    cell_dep0_is_spawn_target: bool,
    parent_lock_code_dep_present: bool,
    script_group_has_one_group_input: bool,
    output_preserves_lock_args: bool,
    output_preserves_cell_data_len: bool,
    output_capacity_covers_occupied_capacity: bool,
    under_capacity_model_is_rejected: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
struct ParentSyscallTrace {
    load_script_calls: usize,
    load_witness_calls: usize,
    load_cell_data_calls: usize,
    load_failures: usize,
    pipe_calls: usize,
    pipe_failures: usize,
    pipe_write_calls: usize,
    pipe_write_failures: usize,
    spawn_calls: usize,
    spawn_failures: usize,
    wait_calls: usize,
    wait_failures: usize,
    close_calls: usize,
    close_failures: usize,
    child_verifier_runs: usize,
    child_exit_code: Option<i8>,
    child_cycles: Option<u64>,
    child_error: Option<String>,
    parent_pipe_words: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct ChildSyscallTrace {
    inherited_fd_calls: usize,
    pipe_read_calls: usize,
    close_calls: usize,
    pipe_read_failures: usize,
    inherited_fd_failures: usize,
    close_failures: usize,
    closed: bool,
}

struct ParentSyscalls {
    script: Vec<u8>,
    witness: Vec<u8>,
    input_cell_data: Vec<u8>,
    child_elf: Vec<u8>,
    max_cycles: u64,
    pipe_words: Vec<u64>,
    read_closed: bool,
    write_closed: bool,
    child_spawned: bool,
    child_run_attempted: bool,
    child_exit_code: Option<i8>,
    trace: Arc<Mutex<ParentSyscallTrace>>,
}

struct ChildInputSyscalls {
    fd: u64,
    words: Vec<u64>,
    cursor: usize,
    trace: Arc<Mutex<ChildSyscallTrace>>,
}

struct TransactionContext {
    transaction: packed::Transaction,
    transaction_view: TransactionView,
    resolved_transaction: ResolvedTransaction,
    data_loader: HarnessDataLoader,
    lock_script: packed::Script,
    child_code_hash: [u8; 32],
    parent_code_hash: [u8; 32],
    child_dep_out_point: packed::OutPoint,
    parent_dep_out_point: packed::OutPoint,
    output_occupied_capacity: u64,
    output_capacity: u64,
    under_capacity: u64,
}

#[derive(Debug)]
struct ChildRun {
    exit_code: i8,
    cycles: u64,
}

impl<Mac: SupportMachine<REG = u64>> Syscalls<Mac> for ParentSyscalls {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, ckb_vm::Error> {
        match machine.registers()[A7] {
            CKB_LOAD_SCRIPT_SYSCALL_NUMBER => {
                self.load_script(machine)?;
                Ok(true)
            }
            CKB_LOAD_WITNESS_SYSCALL_NUMBER => {
                self.load_witness(machine)?;
                Ok(true)
            }
            CKB_LOAD_CELL_DATA_SYSCALL_NUMBER => {
                self.load_cell_data(machine)?;
                Ok(true)
            }
            CKB_VM2_PIPE_SYSCALL_NUMBER => {
                self.pipe(machine)?;
                Ok(true)
            }
            CKB_VM2_PIPE_WRITE_SYSCALL_NUMBER => {
                self.pipe_write(machine)?;
                Ok(true)
            }
            CKB_VM2_SPAWN_SYSCALL_NUMBER => {
                self.spawn(machine)?;
                Ok(true)
            }
            CKB_VM2_WAIT_SYSCALL_NUMBER => {
                self.wait(machine)?;
                Ok(true)
            }
            CKB_VM2_CLOSE_SYSCALL_NUMBER => {
                self.close(machine);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl ParentSyscalls {
    fn load_script<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        self.trace.lock().expect("trace mutex poisoned").load_script_calls += 1;
        let buffer = machine.registers()[A0];
        let size_ptr = machine.registers()[A1];
        let offset = machine.registers()[A2];
        let script = self.script.clone();
        self.load_bytes(machine, &script, buffer, size_ptr, offset)
    }

    fn load_witness<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        self.trace.lock().expect("trace mutex poisoned").load_witness_calls += 1;
        let buffer = machine.registers()[A0];
        let size_ptr = machine.registers()[A1];
        let offset = machine.registers()[A2];
        let index = machine.registers()[A3];
        let source = machine.registers()[A4];
        if index != 0 || source != CKB_SOURCE_GROUP_INPUT {
            self.trace.lock().expect("trace mutex poisoned").load_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }
        let witness = self.witness.clone();
        self.load_bytes(machine, &witness, buffer, size_ptr, offset)
    }

    fn load_cell_data<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        self.trace.lock().expect("trace mutex poisoned").load_cell_data_calls += 1;
        let buffer = machine.registers()[A0];
        let size_ptr = machine.registers()[A1];
        let offset = machine.registers()[A2];
        let index = machine.registers()[A3];
        let source = machine.registers()[A4];
        if index != 0 || source != CKB_SOURCE_INPUT {
            self.trace.lock().expect("trace mutex poisoned").load_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }
        let cell_data = self.input_cell_data.clone();
        self.load_bytes(machine, &cell_data, buffer, size_ptr, offset)
    }

    fn load_bytes<Mac: SupportMachine<REG = u64>>(
        &mut self,
        machine: &mut Mac,
        source: &[u8],
        buffer: u64,
        size_ptr: u64,
        offset: u64,
    ) -> Result<(), ckb_vm::Error> {
        let capacity = machine.memory_mut().load64(&size_ptr)?;
        let Ok(offset) = usize::try_from(offset) else {
            self.trace.lock().expect("trace mutex poisoned").load_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        };
        if offset > source.len() {
            self.trace.lock().expect("trace mutex poisoned").load_failures += 1;
            machine.set_register(A0, 1);
            machine.memory_mut().store64(&size_ptr, &0)?;
            return Ok(());
        }
        let available = &source[offset..];
        machine.memory_mut().store64(&size_ptr, &(available.len() as u64))?;
        if capacity < available.len() as u64 {
            self.trace.lock().expect("trace mutex poisoned").load_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }
        machine.memory_mut().store_bytes(buffer, available)?;
        machine.set_register(A0, 0);
        Ok(())
    }

    fn pipe<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        let fds_ptr = machine.registers()[A0];
        {
            let mut trace = self.trace.lock().expect("trace mutex poisoned");
            trace.pipe_calls += 1;
        }
        self.pipe_words.clear();
        self.read_closed = false;
        self.write_closed = false;
        self.child_spawned = false;
        self.child_run_attempted = false;
        self.child_exit_code = None;
        machine.memory_mut().store64(&fds_ptr, &PARENT_READ_FD)?;
        machine.memory_mut().store64(&(fds_ptr + 8), &PARENT_WRITE_FD)?;
        machine.set_register(A0, 0);
        Ok(())
    }

    fn pipe_write<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        let fd = machine.registers()[A0];
        let buffer = machine.registers()[A1];
        let size_ptr = machine.registers()[A2];
        let size = machine.memory_mut().load64(&size_ptr)?;
        let mut trace = self.trace.lock().expect("trace mutex poisoned");
        trace.pipe_write_calls += 1;
        if fd != PARENT_WRITE_FD || self.write_closed || !self.child_spawned || size != 8 {
            trace.pipe_write_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }
        drop(trace);

        let bytes = machine.memory_mut().load_bytes(buffer, size)?;
        let mut word_bytes = [0u8; 8];
        word_bytes.copy_from_slice(&bytes);
        let word = u64::from_le_bytes(word_bytes);
        self.pipe_words.push(word);
        {
            let mut trace = self.trace.lock().expect("trace mutex poisoned");
            trace.parent_pipe_words.push(format!("0x{word:016x}"));
        }
        machine.memory_mut().store64(&size_ptr, &8)?;
        machine.set_register(A0, 0);
        Ok(())
    }

    fn spawn<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        {
            let mut trace = self.trace.lock().expect("trace mutex poisoned");
            trace.spawn_calls += 1;
        }

        let index = machine.registers()[A0];
        let source = machine.registers()[A1];
        let spawn_args = machine.registers()[A4];
        if index != 0 || source != CKB_SOURCE_CELL_DEP || self.child_spawned {
            self.trace.lock().expect("trace mutex poisoned").spawn_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }

        let process_id_ptr = machine.memory_mut().load64(&(spawn_args + 16))?;
        let inherited_fds_ptr = machine.memory_mut().load64(&(spawn_args + 24))?;
        let inherited_fd = machine.memory_mut().load64(&inherited_fds_ptr)?;
        let inherited_fd_terminator = machine.memory_mut().load64(&(inherited_fds_ptr + 8))?;
        if inherited_fd != PARENT_READ_FD || inherited_fd_terminator != 0 || self.read_closed {
            self.trace.lock().expect("trace mutex poisoned").spawn_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }

        self.child_spawned = true;
        self.read_closed = true;
        machine.memory_mut().store64(&process_id_ptr, &CHILD_PID)?;
        machine.set_register(A0, 0);
        Ok(())
    }

    fn wait<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        let pid = machine.registers()[A0];
        let exit_code_ptr = machine.registers()[A1];
        {
            let mut trace = self.trace.lock().expect("trace mutex poisoned");
            trace.wait_calls += 1;
            if pid != CHILD_PID || !self.child_spawned {
                trace.wait_failures += 1;
                machine.set_register(A0, 1);
                return Ok(());
            }
        }

        if self.child_exit_code.is_none() {
            if self.child_run_attempted || self.pipe_words.len() != IPC_WORD_COUNT {
                let mut trace = self.trace.lock().expect("trace mutex poisoned");
                trace.wait_failures += 1;
                trace.child_error = Some(format!(
                    "wait reached before the fixed IPC envelope was complete: got {} words, expected {IPC_WORD_COUNT}",
                    self.pipe_words.len()
                ));
                machine.set_register(A0, 1);
                return Ok(());
            }

            self.child_run_attempted = true;
            match run_child_elf(&self.child_elf, &self.pipe_words, CHILD_INPUT_FD, self.max_cycles) {
                Ok(child) => {
                    self.child_exit_code = Some(child.exit_code);
                    let mut trace = self.trace.lock().expect("trace mutex poisoned");
                    trace.child_verifier_runs += 1;
                    trace.child_exit_code = Some(child.exit_code);
                    trace.child_cycles = Some(child.cycles);
                }
                Err(error) => {
                    let mut trace = self.trace.lock().expect("trace mutex poisoned");
                    trace.child_error = Some(error);
                    trace.wait_failures += 1;
                    machine.set_register(A0, 1);
                    return Ok(());
                }
            }
        }

        let Some(exit_code) = self.child_exit_code else {
            self.trace.lock().expect("trace mutex poisoned").wait_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        };
        machine.memory_mut().store64(&exit_code_ptr, &(exit_code as u64))?;
        machine.set_register(A0, 0);
        Ok(())
    }

    fn close<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) {
        let fd = machine.registers()[A0];
        let mut trace = self.trace.lock().expect("trace mutex poisoned");
        trace.close_calls += 1;
        match fd {
            PARENT_READ_FD if !self.read_closed => {
                self.read_closed = true;
                machine.set_register(A0, 0);
            }
            PARENT_WRITE_FD if !self.write_closed => {
                self.write_closed = true;
                machine.set_register(A0, 0);
            }
            _ => {
                trace.close_failures += 1;
                machine.set_register(A0, 1);
            }
        }
    }
}

impl<Mac: SupportMachine<REG = u64>> Syscalls<Mac> for ChildInputSyscalls {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, ckb_vm::Error> {
        match machine.registers()[A7] {
            CKB_VM2_INHERITED_FD_SYSCALL_NUMBER => {
                self.inherited_fd(machine)?;
                Ok(true)
            }
            CKB_VM2_PIPE_READ_SYSCALL_NUMBER => {
                self.pipe_read(machine)?;
                Ok(true)
            }
            CKB_VM2_CLOSE_SYSCALL_NUMBER => {
                self.close(machine);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl ChildInputSyscalls {
    fn inherited_fd<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        let buffer = machine.registers()[A0];
        let length_ptr = machine.registers()[A1];
        let length = machine.memory_mut().load64(&length_ptr)?;
        let mut trace = self.trace.lock().expect("trace mutex poisoned");
        trace.inherited_fd_calls += 1;
        if length >= 1 {
            machine.memory_mut().store64(&buffer, &self.fd)?;
            machine.memory_mut().store64(&length_ptr, &1)?;
            machine.set_register(A0, 0);
        } else {
            trace.inherited_fd_failures += 1;
            machine.memory_mut().store64(&length_ptr, &1)?;
            machine.set_register(A0, 1);
        }
        Ok(())
    }

    fn pipe_read<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        let fd = machine.registers()[A0];
        let buffer = machine.registers()[A1];
        let length_ptr = machine.registers()[A2];
        let length = machine.memory_mut().load64(&length_ptr)?;
        let mut trace = self.trace.lock().expect("trace mutex poisoned");
        trace.pipe_read_calls += 1;
        if fd != self.fd || trace.closed || self.cursor >= self.words.len() || length < 8 {
            trace.pipe_read_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }
        let word = self.words[self.cursor];
        self.cursor += 1;
        machine.memory_mut().store64(&buffer, &word)?;
        machine.memory_mut().store64(&length_ptr, &8)?;
        machine.set_register(A0, 0);
        Ok(())
    }

    fn close<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) {
        let fd = machine.registers()[A0];
        let mut trace = self.trace.lock().expect("trace mutex poisoned");
        trace.close_calls += 1;
        if fd == self.fd && !trace.closed {
            trace.closed = true;
            machine.set_register(A0, 0);
        } else {
            trace.close_failures += 1;
            machine.set_register(A0, 1);
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), HarnessError> {
    let args = parse_args()?;
    let parent_elf = fs::read(&args.parent_elf)?;
    let child_elf = fs::read(&args.child_elf)?;
    let parent_code_hash = parent_lock_code_hash(&parent_elf);
    let cases = build_cases(parent_code_hash)?;
    let mut case_reports = Vec::with_capacity(cases.len());
    for case in &cases {
        case_reports.push(run_case(&args, &parent_elf, &child_elf, case)?);
    }

    let report = build_report(&args, &parent_elf, &child_elf, case_reports);
    write_report(&args.output, &report, args.pretty)?;
    print_summary(&args.output, &report);

    if report.summary.mismatched == 0 {
        Ok(())
    } else {
        Err(HarnessError::Message(format!("{} parent lock CKB VM case(s) mismatched", report.summary.mismatched)))
    }
}

fn parse_args() -> Result<Args, HarnessError> {
    let mut parent_elf = PathBuf::from(DEFAULT_PARENT_ELF);
    let mut child_elf = PathBuf::from(DEFAULT_CHILD_ELF);
    let mut output = PathBuf::from(DEFAULT_OUTPUT);
    let mut max_cycles = MAX_CYCLES_DEFAULT;
    let mut pretty = false;

    let mut raw = env::args().skip(1);
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--parent-elf" => parent_elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--child-elf" => child_elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--output" => output = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--max-cycles" => {
                let value = raw.next().ok_or(HarnessError::Usage)?;
                max_cycles = value.parse().map_err(|_| HarnessError::Message(format!("invalid --max-cycles value: {value}")))?;
            }
            "--pretty" => pretty = true,
            "-h" | "--help" => return Err(HarnessError::Usage),
            _ => return Err(HarnessError::Message(format!("unknown argument: {arg}"))),
        }
    }

    Ok(Args { parent_elf, child_elf, output, max_cycles, pretty })
}

fn build_cases(parent_code_hash: [u8; 32]) -> Result<Vec<ParentCase>, HarnessError> {
    let domain = ckb_blake2b256(b"novaseal-parent-lock-harness-domain-v0");
    let policy_hash = ckb_blake2b256(b"novaseal-parent-lock-harness-policy-v0");
    let digest = signed_intent_hash(&build_intent(&domain, &policy_hash));
    let signing_key = SigningKey::from_bytes(&TEST_SECRET_KEY)
        .map_err(|error| HarnessError::Message(format!("failed to construct test BIP340 signing key: {error}")))?;
    let signature = signing_key
        .sign_prehash_with_aux_rand(&digest, &TEST_AUX_RAND)
        .map_err(|error| HarnessError::Message(format!("failed to sign parent digest: {error}")))?;
    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(signing_key.verifying_key().to_bytes().as_slice());
    let authority_hash = pubkey;
    let mut signature_bytes = [0u8; 64];
    signature_bytes.copy_from_slice(signature.to_bytes().as_slice());

    let wrong_signing_key = SigningKey::from_bytes(&TEST_WRONG_SECRET_KEY)
        .map_err(|error| HarnessError::Message(format!("failed to construct wrong-authority BIP340 signing key: {error}")))?;
    let wrong_signature = wrong_signing_key
        .sign_prehash_with_aux_rand(&digest, &TEST_AUX_RAND)
        .map_err(|error| HarnessError::Message(format!("failed to sign parent digest with wrong authority: {error}")))?;
    let mut wrong_pubkey = [0u8; 32];
    wrong_pubkey.copy_from_slice(wrong_signing_key.verifying_key().to_bytes().as_slice());
    let mut wrong_signature_bytes = [0u8; 64];
    wrong_signature_bytes.copy_from_slice(wrong_signature.to_bytes().as_slice());

    let valid_case = build_case(
        "parent_valid_signature_accept",
        "accept",
        None,
        CaseKind::ValidSignature,
        domain,
        policy_hash,
        authority_hash,
        authority_hash,
        parent_code_hash,
        pubkey,
        signature_bytes,
    );

    let mut bad_signature = signature_bytes;
    bad_signature[63] ^= 0x01;
    let signature_case = build_case(
        "parent_signature_bitflip_reject",
        "reject",
        Some("signature last byte flipped after signing parent-computed digest"),
        CaseKind::SignatureBitflip,
        domain,
        policy_hash,
        authority_hash,
        authority_hash,
        parent_code_hash,
        pubkey,
        bad_signature,
    );

    let wrong_lock_arg = ckb_blake2b256(b"novaseal-parent-lock-harness-wrong-lock-arg-v0");
    let authority_case = build_case(
        "parent_authority_hash_mismatch_reject",
        "reject",
        Some("Script.args expected_btc_authority_hash does not match Input#0 btc_authority_hash"),
        CaseKind::AuthorityHashMismatch,
        domain,
        policy_hash,
        authority_hash,
        wrong_lock_arg,
        parent_code_hash,
        pubkey,
        signature_bytes,
    );

    let wrong_pubkey_case = build_case(
        "parent_wrong_pubkey_valid_signature_reject",
        "reject",
        Some("witness pubkey signs the digest but does not match Input#0 btc_authority_hash"),
        CaseKind::WrongPubkeyValidSignature,
        domain,
        policy_hash,
        authority_hash,
        authority_hash,
        parent_code_hash,
        wrong_pubkey,
        wrong_signature_bytes,
    );

    Ok(vec![valid_case, signature_case, authority_case, wrong_pubkey_case])
}

fn build_case(
    id: &'static str,
    expected: &'static str,
    mutation: Option<&'static str>,
    kind: CaseKind,
    domain: [u8; 32],
    policy_hash: [u8; 32],
    cell_authority_hash: [u8; 32],
    lock_arg_authority_hash: [u8; 32],
    parent_code_hash: [u8; 32],
    pubkey: [u8; 32],
    signature: [u8; 64],
) -> ParentCase {
    let intent = build_intent(&domain, &policy_hash);
    let digest = signed_intent_hash(&intent);
    let expected_ipc_blob =
        (!matches!(kind, CaseKind::AuthorityHashMismatch | CaseKind::WrongPubkeyValidSignature)).then(|| {
            build_ipc_blob(&digest, &pubkey, &signature)
        });

    ParentCase {
        id,
        expected,
        mutation,
        kind,
        lock_args: lock_arg_authority_hash,
        script: build_script_with_args(&parent_code_hash, &lock_arg_authority_hash),
        witness: build_witness(&intent, &pubkey, &signature),
        input_cell_data: build_input_cell(&cell_authority_hash, &policy_hash),
        expected_digest: digest,
        expected_pubkey: pubkey,
        expected_signature: signature,
        expected_ipc_blob,
    }
}

fn run_case(args: &Args, parent_elf: &[u8], child_elf: &[u8], case: &ParentCase) -> Result<CaseReport, HarnessError> {
    let mut transaction_shape = build_transaction_shape(parent_elf, child_elf, case)?;
    let resolved_transaction = run_resolved_transaction(parent_elf, child_elf, case)?;
    transaction_shape.resolved_script_group_executed = resolved_transaction.ckb_script_verifier_executed;
    transaction_shape.full_transaction_executed = resolved_transaction.full_transaction_executed;
    let trace = Arc::new(Mutex::new(ParentSyscallTrace::default()));
    let syscall = ParentSyscalls {
        script: case.script.clone(),
        witness: case.witness.clone(),
        input_cell_data: case.input_cell_data.clone(),
        child_elf: child_elf.to_vec(),
        max_cycles: args.max_cycles,
        pipe_words: Vec::new(),
        read_closed: false,
        write_closed: false,
        child_spawned: false,
        child_run_attempted: false,
        child_exit_code: None,
        trace: Arc::clone(&trace),
    };
    let core_machine =
        DefaultCoreMachine::<u64, WXorXMemory<SparseMemory<u64>>>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, args.max_cycles);
    let builder =
        DefaultMachineBuilder::new(core_machine).instruction_cycle_func(Box::new(estimate_cycles)).syscall(Box::new(syscall));
    let mut machine = HarnessMachine::new(builder.build());
    machine
        .load_program(&Bytes::copy_from_slice(parent_elf), &[])
        .map_err(|error| HarnessError::Message(format!("failed to load parent lock ELF in ckb-vm: {error}")))?;
    let exit_code =
        machine.run().map_err(|error| HarnessError::Message(format!("parent lock ELF ckb-vm execution failed: {error}")))?;
    let parent_cycles = machine.machine.cycles();
    let syscall_trace = trace.lock().expect("trace mutex poisoned").clone();
    let observed_ipc_blob = words_to_ipc_blob(&syscall_trace.parent_pipe_words)?;
    let ipc_blob_matches_expected = observed_ipc_blob == case.expected_ipc_blob;
    let accepted = exit_code == 0;
    let matched_expected = match case.expected {
        "accept" => accepted && ipc_blob_matches_expected,
        "reject" => {
            !accepted
                && (matches!(case.kind, CaseKind::AuthorityHashMismatch | CaseKind::WrongPubkeyValidSignature)
                    || ipc_blob_matches_expected)
        }
        other => return Err(HarnessError::Message(format!("case {} has unsupported expected value: {other}", case.id))),
    };

    Ok(CaseReport {
        id: case.id,
        expected: case.expected,
        mutation: case.mutation,
        exit_code,
        accepted,
        matched_expected,
        parent_cycles,
        expected_digest: hex0x(&case.expected_digest),
        expected_pubkey: hex0x(&case.expected_pubkey),
        expected_signature: hex0x(&case.expected_signature),
        observed_ipc_blob: observed_ipc_blob.as_ref().map(|blob| hex0x(blob)),
        expected_ipc_blob: case.expected_ipc_blob.as_ref().map(|blob| hex0x(blob)),
        ipc_blob_matches_expected,
        transaction_shape,
        resolved_transaction,
        syscall_trace,
    })
}

fn run_child_elf(elf: &[u8], words: &[u64], fd: u64, max_cycles: u64) -> Result<ChildRun, String> {
    let trace = Arc::new(Mutex::new(ChildSyscallTrace::default()));
    let core_machine = DefaultCoreMachine::<u64, WXorXMemory<SparseMemory<u64>>>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, max_cycles);
    let syscall = ChildInputSyscalls { fd, words: words.to_vec(), cursor: 0, trace };
    let builder =
        DefaultMachineBuilder::new(core_machine).instruction_cycle_func(Box::new(estimate_cycles)).syscall(Box::new(syscall));
    let mut machine = HarnessMachine::new(builder.build());
    machine.load_program(&Bytes::copy_from_slice(elf), &[]).map_err(|error| format!("{error}"))?;
    let exit_code = machine.run().map_err(|error| format!("{error}"))?;
    Ok(ChildRun { exit_code, cycles: machine.machine.cycles() })
}

fn build_transaction_context(parent_elf: &[u8], child_elf: &[u8], case: &ParentCase) -> Result<TransactionContext, HarnessError> {
    let parent_code_hash = parent_lock_code_hash(parent_elf);
    let child_code_hash = ckb_blake2b256(child_elf);
    let lock_script = build_packed_script(&parent_code_hash, &case.lock_args);
    let output_cell_data = case.input_cell_data.clone();

    let output_without_capacity = packed::CellOutput::new_builder().lock(lock_script.clone()).build();
    let output_occupied_capacity = output_without_capacity
        .occupied_capacity(capacity_bytes(output_cell_data.len())?)
        .map_err(|error| HarnessError::Message(format!("failed to compute occupied capacity: {error}")))?
        .as_u64();
    let output_capacity = output_occupied_capacity
        .checked_add(TRANSACTION_SHAPE_OUTPUT_MARGIN_SHANNONS)
        .ok_or_else(|| HarnessError::Message("transaction-shape output capacity overflow".to_string()))?;
    let under_capacity = output_occupied_capacity.saturating_sub(1);
    let output = output_without_capacity.as_builder().capacity(Capacity::shannons(output_capacity).pack()).build();

    let child_dep_out_point = build_out_point(&child_code_hash, 0);
    let parent_dep_out_point = build_out_point(&parent_code_hash, 0);
    let input_out_point = build_out_point(&ckb_blake2b256(case.id.as_bytes()), 0);
    let child_dep = build_cell_dep_from_out_point(child_dep_out_point.clone());
    let parent_dep = build_cell_dep_from_out_point(parent_dep_out_point.clone());
    let input = packed::CellInput::new_builder().previous_output(input_out_point.clone()).build();
    let raw_transaction = packed::RawTransaction::new_builder()
        .version(0u32.pack())
        .cell_deps(vec![child_dep, parent_dep].pack())
        .header_deps(Vec::<packed::Byte32>::new().pack())
        .inputs(vec![input].pack())
        .outputs(vec![output.clone()].pack())
        .outputs_data(vec![CkbBytes::from(output_cell_data.clone()).pack()].pack())
        .build();
    let transaction = packed::Transaction::new_builder()
        .raw(raw_transaction)
        .witnesses(vec![CkbBytes::from(case.witness.clone()).pack()].pack())
        .build();
    let transaction_view = transaction.clone().into_view();

    let child_dep_output = packed::CellOutput::new_builder().capacity(capacity_with_margin(child_elf.len())?.pack()).build();
    let parent_type_script = build_code_type_script("novaseal-parent-lock-code-type-v0", parent_elf);
    let parent_dep_output = packed::CellOutput::new_builder()
        .capacity(capacity_with_margin(parent_elf.len())?.pack())
        .type_(Some(parent_type_script).pack())
        .build();
    let input_cell_output =
        packed::CellOutput::new_builder().capacity(Capacity::shannons(output_capacity).pack()).lock(lock_script.clone()).build();

    let child_bytes = CkbBytes::copy_from_slice(child_elf);
    let parent_bytes = CkbBytes::copy_from_slice(parent_elf);
    let input_bytes = CkbBytes::copy_from_slice(&case.input_cell_data);
    let mut data_loader = HarnessDataLoader::default();
    data_loader.insert(child_dep_out_point.clone(), child_bytes.clone());
    data_loader.insert(parent_dep_out_point.clone(), parent_bytes.clone());
    data_loader.insert(input_out_point.clone(), input_bytes.clone());

    let resolved_transaction = ResolvedTransaction {
        transaction: transaction_view.clone(),
        resolved_cell_deps: vec![
            CellMetaBuilder::from_cell_output(child_dep_output, child_bytes).out_point(child_dep_out_point.clone()).build(),
            CellMetaBuilder::from_cell_output(parent_dep_output, parent_bytes).out_point(parent_dep_out_point.clone()).build(),
        ],
        resolved_inputs: vec![
            CellMetaBuilder::from_cell_output(input_cell_output, input_bytes).out_point(input_out_point.clone()).build(),
        ],
        resolved_dep_groups: Vec::new(),
    };

    Ok(TransactionContext {
        transaction,
        transaction_view,
        resolved_transaction,
        data_loader,
        lock_script,
        child_code_hash,
        parent_code_hash,
        child_dep_out_point,
        parent_dep_out_point,
        output_occupied_capacity,
        output_capacity,
        under_capacity,
    })
}

fn build_transaction_shape(parent_elf: &[u8], child_elf: &[u8], case: &ParentCase) -> Result<TransactionShapeReport, HarnessError> {
    let context = build_transaction_context(parent_elf, child_elf, case)?;
    let script_bytes_match_vm_load_script = context.lock_script.as_bytes() == case.script;
    let current_lock_script_hash = context.lock_script.calc_script_hash();

    let capacity = CapacityShapeReport {
        input_capacity_shannons: context.output_capacity,
        output_capacity_shannons: context.output_capacity,
        output_occupied_capacity_shannons: context.output_occupied_capacity,
        capacity_margin_shannons: context.output_capacity - context.output_occupied_capacity,
        under_capacity_output_capacity_shannons: context.under_capacity,
        capacity_is_sufficient: context.output_capacity >= context.output_occupied_capacity,
        under_capacity_rejected_by_shape: context.under_capacity < context.output_occupied_capacity,
    };
    let checks = TransactionShapeChecks {
        cell_dep0_is_spawn_target: true,
        parent_lock_code_dep_present: true,
        script_group_has_one_group_input: true,
        output_preserves_lock_args: context
            .transaction
            .raw()
            .outputs()
            .get(0)
            .is_some_and(|output| output.lock().args().raw_data().as_ref() == case.lock_args),
        output_preserves_cell_data_len: context
            .transaction
            .raw()
            .outputs_data()
            .get(0)
            .is_some_and(|data| data.raw_data().len() == case.input_cell_data.len()),
        output_capacity_covers_occupied_capacity: capacity.capacity_is_sufficient,
        under_capacity_model_is_rejected: capacity.under_capacity_rejected_by_shape,
    };

    Ok(TransactionShapeReport {
        classification: "consensus_packed_transaction_shape_only",
        consensus_packed_tx_constructed: true,
        full_transaction_executed: false,
        resolved_script_group_executed: false,
        consensus_tx_size_bytes: context.transaction.as_bytes().len(),
        witness_size_bytes: case.witness.len(),
        input_cell_data_size_bytes: case.input_cell_data.len(),
        output_cell_data_size_bytes: case.input_cell_data.len(),
        current_lock_script_hash: hex0x(current_lock_script_hash.as_slice()),
        cell_deps: vec![
            CellDepShapeReport {
                index: 0,
                role: "spawn-target-child-verifier",
                dep_type: "code",
                source_model: "CellDep#0",
                code_hash: hex0x(&context.child_code_hash),
                out_point_tx_hash_placeholder: hex0x(context.child_dep_out_point.tx_hash().as_slice()),
                out_point_index: 0,
            },
            CellDepShapeReport {
                index: 1,
                role: "parent-lock-code",
                dep_type: "code",
                source_model: "script-code-dep",
                code_hash: hex0x(&context.parent_code_hash),
                out_point_tx_hash_placeholder: hex0x(context.parent_dep_out_point.tx_hash().as_slice()),
                out_point_index: 0,
            },
        ],
        script_group: ScriptGroupShapeReport {
            group_type: "lock",
            current_script_source: "input[0].lock",
            group_input_indexes: vec![0],
            group_output_indexes: Vec::new(),
            script_args: hex0x(&case.lock_args),
            script_bytes_match_vm_load_script,
        },
        capacity,
        checks,
        limits: vec![
            "Uses ckb-types packed transaction serialization for tx-size and occupied-capacity measurement.",
            "OutPoint tx_hash values are deterministic code-hash placeholders, not live cells on a chain.",
            "ResolvedTransaction execution is recorded separately in this report.",
            "Does not prove builder acceptance, fee coverage, dep liveness, or six-fixture transaction execution.",
        ],
    })
}

fn run_resolved_transaction(
    parent_elf: &[u8],
    child_elf: &[u8],
    case: &ParentCase,
) -> Result<ResolvedTransactionReport, HarnessError> {
    let context = build_transaction_context(parent_elf, child_elf, case)?;
    let lock_group_hash = context.lock_script.calc_script_hash();
    let consensus = Arc::new(resolved_script_consensus());
    let header = HeaderBuilder::default().epoch(EpochNumberWithFraction::new(VM2_ENABLED_EPOCH, 0, 1).pack()).build();
    let tx_env = Arc::new(TxVerifyEnv::new_commit(&header));
    let verifier = TransactionScriptsVerifier::new(
        Arc::new(context.resolved_transaction.clone()),
        context.data_loader.clone(),
        Arc::clone(&consensus),
        Arc::clone(&tx_env),
    );
    let groups: Vec<_> =
        verifier.groups_with_type().map(|(group_type, hash, group)| (group_type, hash.clone(), group.clone())).collect();
    let script_group_count = groups.len();
    let lock_group = groups
        .iter()
        .find(|(group_type, hash, _)| *group_type == ckb_script::ScriptGroupType::Lock && hash == &lock_group_hash)
        .map(|(_, _, group)| group.clone())
        .ok_or_else(|| HarnessError::Message(format!("resolved lock group not found for {}", hex0x(lock_group_hash.as_slice()))))?;
    let result = verifier.verify_single(ckb_script::ScriptGroupType::Lock, &lock_group_hash, RESOLVED_SCRIPT_VERIFY_MAX_CYCLES);
    let (accepted, cycles, error) = match result {
        Ok(cycles) => (true, Some(cycles), None),
        Err(error) => (false, None, Some(format!("{error}"))),
    };
    let lock_group_matched_expected = match case.expected {
        "accept" => accepted,
        "reject" => !accepted,
        other => return Err(HarnessError::Message(format!("case {} has unsupported expected value: {other}", case.id))),
    };
    let full_verifier = TransactionScriptsVerifier::new(
        Arc::new(context.resolved_transaction.clone()),
        context.data_loader.clone(),
        consensus,
        tx_env,
    );
    let full_result = full_verifier.verify(RESOLVED_SCRIPT_VERIFY_MAX_CYCLES);
    let (full_transaction_accepted, full_transaction_cycles, full_transaction_error) = match full_result {
        Ok(cycles) => (true, Some(cycles), None),
        Err(error) => (false, None, Some(format!("{error}"))),
    };
    let full_transaction_matched_expected = match case.expected {
        "accept" => full_transaction_accepted,
        "reject" => !full_transaction_accepted,
        other => return Err(HarnessError::Message(format!("case {} has unsupported expected value: {other}", case.id))),
    };
    let matched_expected = lock_group_matched_expected && full_transaction_matched_expected;
    let parent_type_script_hash = parent_lock_code_hash(parent_elf);

    Ok(ResolvedTransactionReport {
        classification: "ckb_script_resolved_transaction_execution",
        resolved_transaction_constructed: true,
        ckb_script_verifier_executed: true,
        full_transaction_executed: true,
        accepted,
        matched_expected,
        cycles,
        error,
        full_transaction_accepted,
        full_transaction_matched_expected,
        full_transaction_cycles,
        full_transaction_error,
        script_group_count,
        transaction_hash: hex0x(context.transaction_view.hash().as_slice()),
        lock_group_hash: hex0x(lock_group_hash.as_slice()),
        lock_group_input_indexes: lock_group.input_indices,
        lock_group_output_indexes: lock_group.output_indices,
        resolved_cell_deps: vec![
            CellDepShapeReport {
                index: 0,
                role: "spawn-target-child-verifier",
                dep_type: "code",
                source_model: "resolved CellDep#0",
                code_hash: hex0x(&context.child_code_hash),
                out_point_tx_hash_placeholder: hex0x(context.child_dep_out_point.tx_hash().as_slice()),
                out_point_index: 0,
            },
            CellDepShapeReport {
                index: 1,
                role: "parent-lock-code",
                dep_type: "code",
                source_model: "resolved script-code-dep",
                code_hash: hex0x(&context.parent_code_hash),
                out_point_tx_hash_placeholder: hex0x(context.parent_dep_out_point.tx_hash().as_slice()),
                out_point_index: 0,
            },
        ],
        resolved_inputs: context.resolved_transaction.resolved_inputs.len(),
        resolved_dep_groups: context.resolved_transaction.resolved_dep_groups.len(),
        cell_dep0_data_hash_matches_child_elf: context.child_code_hash == ckb_blake2b256(child_elf),
        parent_lock_dep_type_hash_matches_script_code_hash: context.parent_code_hash == parent_type_script_hash,
        script_bytes_match_vm_load_script: context.lock_script.as_bytes() == case.script,
        limits: vec![
            "Uses ckb-script TransactionScriptsVerifier against a constructed ResolvedTransaction.",
            "Executes both the resolved lock ScriptGroup and the full ckb-script transaction script verifier.",
            "The constructed transaction currently has one lock ScriptGroup and no type ScriptGroups.",
            "Resolved cell deps are in-memory deterministic harness cells, not live chain cells.",
            "Does not prove fee coverage, dep liveness, six-fixture transaction execution, or production builder/full-node acceptance.",
        ],
    })
}

fn build_report(args: &Args, parent_elf: &[u8], child_elf: &[u8], cases: Vec<CaseReport>) -> Report {
    let total_cases = cases.len();
    let expected_accept = cases.iter().filter(|case| case.expected == "accept").count();
    let expected_reject = cases.iter().filter(|case| case.expected == "reject").count();
    let accepted = cases.iter().filter(|case| case.accepted).count();
    let rejected = total_cases - accepted;
    let matched_expected = cases.iter().filter(|case| case.matched_expected).count();
    let mismatched = total_cases - matched_expected;
    let parent_min_cycles = cases.iter().map(|case| case.parent_cycles).min().unwrap_or_default();
    let parent_max_cycles = cases.iter().map(|case| case.parent_cycles).max().unwrap_or_default();
    let child_max_cycles = cases.iter().filter_map(|case| case.syscall_trace.child_cycles).max().unwrap_or_default();
    let max_consensus_tx_size_bytes =
        cases.iter().map(|case| case.transaction_shape.consensus_tx_size_bytes).max().unwrap_or_default();
    let max_output_occupied_capacity_shannons =
        cases.iter().map(|case| case.transaction_shape.capacity.output_occupied_capacity_shannons).max().unwrap_or_default();
    let min_capacity_margin_shannons =
        cases.iter().map(|case| case.transaction_shape.capacity.capacity_margin_shannons).min().unwrap_or_default();
    let capacity_shape_checks_passed = cases.iter().all(|case| case.transaction_shape.checks.output_capacity_covers_occupied_capacity);
    let under_capacity_shape_rejects = cases.iter().all(|case| case.transaction_shape.checks.under_capacity_model_is_rejected);
    let cell_dep0_spawn_target_modelled = cases.iter().all(|case| case.transaction_shape.checks.cell_dep0_is_spawn_target);
    let parent_lock_dep_modelled = cases.iter().all(|case| case.transaction_shape.checks.parent_lock_code_dep_present);
    let resolved_transaction_constructed = cases.iter().all(|case| case.resolved_transaction.resolved_transaction_constructed);
    let resolved_script_verifier_executed = cases.iter().all(|case| case.resolved_transaction.ckb_script_verifier_executed);
    let resolved_script_verifier_matched_expected = cases.iter().all(|case| case.resolved_transaction.matched_expected);
    let resolved_script_verifier_max_cycles =
        cases.iter().filter_map(|case| case.resolved_transaction.cycles).max().unwrap_or_default();
    let full_transaction_executed = cases.iter().all(|case| case.resolved_transaction.full_transaction_executed);
    let full_transaction_verifier_matched_expected =
        cases.iter().all(|case| case.resolved_transaction.full_transaction_matched_expected);
    let full_transaction_verifier_max_cycles =
        cases.iter().filter_map(|case| case.resolved_transaction.full_transaction_cycles).max().unwrap_or_default();

    Report {
        schema: "novaseal-parent-lock-ckb-vm-report-v0.1",
        classification: "parent_lock_spawn_child_verifier_ckb_vm_evidence_plus_full_ckb_script_verifier",
        parent_elf: ElfReport {
            path: args.parent_elf.display().to_string(),
            size_bytes: parent_elf.len(),
            sha256: sha256_hex(parent_elf),
        },
        child_elf: ElfReport {
            path: args.child_elf.display().to_string(),
            size_bytes: child_elf.len(),
            sha256: sha256_hex(child_elf),
        },
        summary: Summary {
            parent_lock_ckb_vm_executed: true,
            parent_spawn_executed: cases.iter().any(|case| case.syscall_trace.spawn_calls > 0),
            child_verifier_ckb_vm_executed: cases.iter().any(|case| case.syscall_trace.child_verifier_runs > 0),
            transaction_shape_constructed: true,
            consensus_packed_tx_constructed: cases.iter().all(|case| case.transaction_shape.consensus_packed_tx_constructed),
            resolved_transaction_constructed,
            resolved_script_verifier_executed,
            resolved_script_verifier_matched_expected,
            full_transaction_constructed: true,
            full_transaction_executed,
            full_transaction_verifier_matched_expected,
            total_cases,
            expected_accept,
            expected_reject,
            accepted,
            rejected,
            matched_expected,
            mismatched,
            parent_min_cycles,
            parent_max_cycles,
            child_max_cycles,
            load_script_calls: cases.iter().map(|case| case.syscall_trace.load_script_calls).sum(),
            load_witness_calls: cases.iter().map(|case| case.syscall_trace.load_witness_calls).sum(),
            load_cell_data_calls: cases.iter().map(|case| case.syscall_trace.load_cell_data_calls).sum(),
            pipe_calls: cases.iter().map(|case| case.syscall_trace.pipe_calls).sum(),
            pipe_write_calls: cases.iter().map(|case| case.syscall_trace.pipe_write_calls).sum(),
            spawn_calls: cases.iter().map(|case| case.syscall_trace.spawn_calls).sum(),
            wait_calls: cases.iter().map(|case| case.syscall_trace.wait_calls).sum(),
            close_calls: cases.iter().map(|case| case.syscall_trace.close_calls).sum(),
            max_consensus_tx_size_bytes,
            max_output_occupied_capacity_shannons,
            min_capacity_margin_shannons,
            capacity_shape_checks_passed,
            under_capacity_shape_rejects,
            cell_dep0_spawn_target_modelled,
            parent_lock_dep_modelled,
            resolved_script_verifier_max_cycles,
            full_transaction_verifier_max_cycles,
        },
        cases,
        limits: vec![
            "Executes the staged parent CellScript lock ELF in ckb-vm.",
            "The harness implements only the CKB syscalls needed by this lock surface: load_script, load_witness, load_cell_data, pipe, pipe_write, spawn, wait, and close.",
            "The spawn syscall is harness-backed and immediately executes the staged child verifier ELF in a nested ckb-vm instance.",
            "Constructs a consensus-packed transaction shape and a ckb-types ResolvedTransaction.",
            "Executes the resolved lock ScriptGroup with ckb-script TransactionScriptsVerifier.",
            "Executes the full ckb-script transaction script verifier over the constructed ResolvedTransaction.",
            "Does not execute a production builder/full-node acceptance path, live dep resolver, or six-fixture transaction suite.",
            "Uses deterministic harness-only BIP340 key material; not production key material.",
        ],
    }
}

fn build_script_with_args(code_hash: &[u8; 32], args: &[u8; 32]) -> Vec<u8> {
    let total_size = 16 + 32 + 1 + 4 + args.len();
    let mut script = Vec::with_capacity(total_size);
    script.extend_from_slice(&(total_size as u32).to_le_bytes());
    script.extend_from_slice(&16u32.to_le_bytes());
    script.extend_from_slice(&48u32.to_le_bytes());
    script.extend_from_slice(&49u32.to_le_bytes());
    script.extend_from_slice(code_hash);
    script.push(1);
    script.extend_from_slice(&(args.len() as u32).to_le_bytes());
    script.extend_from_slice(args);
    script
}

fn build_packed_script(code_hash: &[u8; 32], args: &[u8; 32]) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(packed_byte32(code_hash))
        .hash_type(ScriptHashType::Type.into())
        .args(CkbBytes::copy_from_slice(args).pack())
        .build()
}

fn build_cell_dep_from_out_point(out_point: packed::OutPoint) -> packed::CellDep {
    packed::CellDep::new_builder().out_point(out_point).dep_type(DepType::Code.into()).build()
}

fn build_code_type_script(role: &str, elf: &[u8]) -> packed::Script {
    let role_code_hash = ckb_blake2b256(role.as_bytes());
    let data_hash = ckb_blake2b256(elf);
    packed::Script::new_builder()
        .code_hash(packed_byte32(&role_code_hash))
        .hash_type(ScriptHashType::Data1.into())
        .args(CkbBytes::copy_from_slice(&data_hash).pack())
        .build()
}

fn parent_lock_code_hash(parent_elf: &[u8]) -> [u8; 32] {
    byte32_to_array(&build_code_type_script("novaseal-parent-lock-code-type-v0", parent_elf).calc_script_hash())
}

fn build_out_point(tx_hash: &[u8; 32], index: u32) -> packed::OutPoint {
    packed::OutPoint::new_builder().tx_hash(packed_byte32(tx_hash)).index(index.pack()).build()
}

fn packed_byte32(bytes: &[u8; 32]) -> packed::Byte32 {
    packed::Byte32::from_slice(bytes).expect("32-byte fixed hash")
}

fn byte32_to_array(byte32: &packed::Byte32) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(byte32.as_slice());
    bytes
}

fn capacity_bytes(len: usize) -> Result<Capacity, HarnessError> {
    Capacity::bytes(len).map_err(|error| HarnessError::Message(format!("failed to convert {len} bytes to CKB capacity: {error}")))
}

fn capacity_with_margin(data_len: usize) -> Result<Capacity, HarnessError> {
    let occupied = Capacity::bytes(data_len)
        .map_err(|error| HarnessError::Message(format!("failed to compute code cell capacity for {data_len} bytes: {error}")))?
        .as_u64();
    Ok(Capacity::shannons(
        occupied
            .checked_add(TRANSACTION_SHAPE_OUTPUT_MARGIN_SHANNONS)
            .ok_or_else(|| HarnessError::Message("code cell capacity overflow".to_string()))?,
    ))
}

fn resolved_script_consensus() -> Consensus {
    let hardfork_switch = HardForks {
        ckb2021: CKB2021::new_mirana().as_builder().rfc_0032(VM2_ENABLED_EPOCH).build().expect("valid CKB2021 hardfork switch"),
        ckb2023: CKB2023::new_mirana().as_builder().rfc_0049(VM2_ENABLED_EPOCH).build().expect("valid CKB2023 hardfork switch"),
    };
    ConsensusBuilder::default().hardfork_switch(hardfork_switch).build()
}

fn build_witness(intent: &[u8], pubkey: &[u8; 32], signature: &[u8; 64]) -> Vec<u8> {
    let sig_payload = build_signature_payload(pubkey, signature);
    let state_hash_commitment = [0u8; 32];
    let mut witness =
        Vec::with_capacity(LOCK_WITNESS_MAGIC.len() + 4 + intent.len() + state_hash_commitment.len() + 4 + sig_payload.len());
    witness.extend_from_slice(LOCK_WITNESS_MAGIC);
    witness.extend_from_slice(&(intent.len() as u32).to_le_bytes());
    witness.extend_from_slice(intent);
    witness.extend_from_slice(&state_hash_commitment);
    witness.extend_from_slice(&(sig_payload.len() as u32).to_le_bytes());
    witness.extend_from_slice(&sig_payload);
    witness
}

fn build_input_cell(authority_hash: &[u8; 32], policy_hash: &[u8; 32]) -> Vec<u8> {
    let mut cell = vec![0u8; NOVASEAL_CELL_LEN];
    cell[CELL_BTC_AUTHORITY_HASH_OFFSET..CELL_BTC_AUTHORITY_HASH_OFFSET + 32].copy_from_slice(authority_hash);
    cell[CELL_POLICY_HASH_OFFSET..CELL_POLICY_HASH_OFFSET + 32].copy_from_slice(policy_hash);
    cell
}

fn build_intent(domain: &[u8; 32], policy_hash: &[u8; 32]) -> Vec<u8> {
    let mut intent = vec![0u8; NOVASEAL_INTENT_LEN];
    intent[INTENT_PROTOCOL_ID_OFFSET..INTENT_PROTOCOL_ID_OFFSET + 32].copy_from_slice(domain);
    intent[INTENT_ACTION_OFFSET] = 1;
    intent[INTENT_POLICY_HASH_OFFSET..INTENT_POLICY_HASH_OFFSET + 32].copy_from_slice(policy_hash);
    intent[INTENT_OLD_NONCE_OFFSET..INTENT_OLD_NONCE_OFFSET + 8].copy_from_slice(&0u64.to_le_bytes());
    intent[INTENT_NEW_NONCE_OFFSET..INTENT_NEW_NONCE_OFFSET + 8].copy_from_slice(&1u64.to_le_bytes());
    intent[INTENT_EXPIRY_OFFSET..INTENT_EXPIRY_OFFSET + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    intent[SIGNED_INTENT_EXPECTED_RECEIPT_HASH_OFFSET..SIGNED_INTENT_EXPECTED_RECEIPT_HASH_OFFSET + 32]
        .copy_from_slice(&ckb_blake2b256(b"novaseal-parent-lock-harness-receipt-v0"));
    intent
}

fn build_signature_payload(pubkey: &[u8; 32], signature: &[u8; 64]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(SIGNATURE_PAYLOAD_LEN);
    payload.extend_from_slice(pubkey);
    payload.extend_from_slice(signature);
    payload
}

fn build_ipc_blob(digest: &[u8; 32], pubkey: &[u8; 32], signature: &[u8; 64]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(IPC_BLOB_LEN);
    blob.extend_from_slice(IPC_MAGIC);
    blob.extend_from_slice(&IPC_VERSION.to_le_bytes());
    blob.extend_from_slice(&IPC_SCHEME_BIP340.to_le_bytes());
    blob.extend_from_slice(&IPC_FLAGS_NONE.to_le_bytes());
    blob.extend_from_slice(digest);
    blob.extend_from_slice(pubkey);
    blob.extend_from_slice(signature);
    debug_assert_eq!(blob.len(), IPC_BLOB_LEN);
    blob
}

fn words_to_ipc_blob(words: &[String]) -> Result<Option<Vec<u8>>, HarnessError> {
    if words.is_empty() {
        return Ok(None);
    }
    if words.len() != IPC_WORD_COUNT {
        return Ok(Some(Vec::new()));
    }
    let mut blob = Vec::with_capacity(IPC_BLOB_LEN);
    for word in words {
        let trimmed = word.strip_prefix("0x").unwrap_or(word);
        let value = u64::from_str_radix(trimmed, 16)
            .map_err(|error| HarnessError::Message(format!("invalid recorded pipe word {word}: {error}")))?;
        blob.extend_from_slice(&value.to_le_bytes());
    }
    Ok(Some(blob))
}

fn ckb_blake2b256(data: &[u8]) -> [u8; 32] {
    let digest = blake2b_simd::Params::new().hash_length(32).personal(CKB_BLAKE2B_PERSONAL).hash(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

fn signed_intent_hash(intent: &[u8]) -> [u8; 32] {
    let mut preimage =
        Vec::with_capacity(PACKED_HASH_DOMAIN.len() + SIGNED_INTENT_TYPE_NAME.len() + 1 + 4 + intent.len());
    preimage.extend_from_slice(PACKED_HASH_DOMAIN);
    preimage.extend_from_slice(SIGNED_INTENT_TYPE_NAME);
    preimage.push(0);
    preimage.extend_from_slice(&(intent.len() as u32).to_le_bytes());
    preimage.extend_from_slice(intent);
    ckb_blake2b256(&preimage)
}

fn write_report(path: &Path, report: &Report, pretty: bool) -> Result<(), HarnessError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = if pretty { serde_json::to_string_pretty(report)? } else { serde_json::to_string(report)? };
    fs::write(path, format!("{json}\n"))?;
    Ok(())
}

fn print_summary(path: &Path, report: &Report) {
    println!("wrote {}", path.display());
    println!(
        "summary: parent_vm_executed={} parent_spawn_executed={} child_vm_executed={} tx_shape_constructed={} resolved_script_verifier_executed={} resolved_script_verifier_matched_expected={} full_tx_executed={} full_tx_matched_expected={} total={} accepted={} rejected={} matched_expected={} mismatched={} parent_max_cycles={} child_max_cycles={} resolved_script_max_cycles={} full_tx_max_cycles={} max_tx_size_bytes={} max_occupied_capacity_shannons={}",
        report.summary.parent_lock_ckb_vm_executed,
        report.summary.parent_spawn_executed,
        report.summary.child_verifier_ckb_vm_executed,
        report.summary.transaction_shape_constructed,
        report.summary.resolved_script_verifier_executed,
        report.summary.resolved_script_verifier_matched_expected,
        report.summary.full_transaction_executed,
        report.summary.full_transaction_verifier_matched_expected,
        report.summary.total_cases,
        report.summary.accepted,
        report.summary.rejected,
        report.summary.matched_expected,
        report.summary.mismatched,
        report.summary.parent_max_cycles,
        report.summary.child_max_cycles,
        report.summary.resolved_script_verifier_max_cycles,
        report.summary.full_transaction_verifier_max_cycles,
        report.summary.max_consensus_tx_size_bytes,
        report.summary.max_output_occupied_capacity_shannons
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}
