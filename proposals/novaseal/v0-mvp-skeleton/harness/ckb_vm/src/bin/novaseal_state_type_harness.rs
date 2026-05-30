#![allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::too_many_lines
)]

use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use blake2b_simd::Params as Blake2bParams;
use ckb_vm::{
    Bytes, DefaultCoreMachine, DefaultMachineBuilder, ISA_B, ISA_IMC, ISA_MOP, SparseMemory, SupportMachine, Syscalls, TraceMachine,
    WXorXMemory,
    cost_model::estimate_cycles,
    machine::VERSION2,
    memory::Memory,
    registers::{A0, A1, A2, A3, A4, A5, A7},
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

const DEFAULT_ACTION_ELF: &str = "target/novaseal-state-type-action.elf";
const DEFAULT_CANONICAL_VECTORS: &str = "target/novaseal-canonical-vectors.json";
const DEFAULT_FIXTURES_DIR: &str = "fixtures";
const DEFAULT_OUTPUT: &str = "target/novaseal-state-type-ckb-vm-report.json";
const MAX_CYCLES_DEFAULT: u64 = 200_000_000;

const CKB_SOURCE_INPUT: u64 = 0x01;
const CKB_SOURCE_OUTPUT: u64 = 0x02;
const CKB_SOURCE_HEADER_DEP: u64 = 0x04;
const CKB_SOURCE_GROUP_INPUT: u64 = 0x0100_0000_0000_0000 | CKB_SOURCE_INPUT;
const CKB_SOURCE_GROUP_OUTPUT: u64 = 0x0100_0000_0000_0000 | CKB_SOURCE_OUTPUT;

const CKB_LOAD_WITNESS_SYSCALL_NUMBER: u64 = 2074;
const CKB_LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER: u64 = 2082;
const CKB_LOAD_CELL_DATA_SYSCALL_NUMBER: u64 = 2092;

const HEADER_FIELD_EPOCH_NUMBER: u64 = 0;
const LOCK_WITNESS_MAGIC: &[u8; 8] = b"CSARGv1\0";

const NOVASEAL_CELL_LEN: usize = 146;
const INTENT_LEN: usize = 213;
const PROOF_RECEIPT_LEN: usize = 279;
const CELL_BTC_AUTHORITY_HASH_OFFSET: usize = 2;
const CELL_STATE_HASH_OFFSET: usize = 34;
const CELL_POLICY_HASH_OFFSET: usize = 66;
const CELL_RECEIPT_ROOT_OFFSET: usize = 98;
const CELL_NONCE_OFFSET: usize = 130;
const CELL_EXPIRY_OFFSET: usize = 138;
const INTENT_DOMAIN_OFFSET: usize = 0;
const INTENT_ACTION_OFFSET: usize = 32;
const INTENT_OLD_CELL_OFFSET: usize = 33;
const INTENT_OLD_STATE_HASH_OFFSET: usize = 69;
const INTENT_NEW_STATE_HASH_OFFSET: usize = 101;
const INTENT_POLICY_HASH_OFFSET: usize = 133;
const INTENT_NONCE_OFFSET: usize = 197;
const INTENT_EXPIRY_OFFSET: usize = 205;
const BYTE32_LEN: usize = 32;
const SIGNATURE_PAYLOAD_LEN: usize = 96;

type HarnessMachine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;

#[derive(Debug, Error)]
enum HarnessError {
    #[error(
        "usage: novaseal_state_type_harness [--action-elf PATH] [--canonical-vectors PATH] [--fixtures-dir PATH] [--output PATH] [--max-cycles N] [--pretty]"
    )]
    Usage,
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
}

#[derive(Debug)]
struct Args {
    action_elf: PathBuf,
    canonical_vectors: PathBuf,
    fixtures_dir: PathBuf,
    output: PathBuf,
    max_cycles: u64,
    pretty: bool,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    classification: &'static str,
    action_elf: ElfReport,
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
    state_type_action_ckb_vm_executed: bool,
    action_entry: &'static str,
    total_cases: usize,
    state_type_expected_accept: usize,
    state_type_expected_reject: usize,
    accepted: usize,
    rejected: usize,
    state_type_matched_expected: usize,
    state_type_mismatched: usize,
    source_fixture_matched_by_state_type_only: usize,
    source_fixture_requires_lock_or_external_context: usize,
    max_cycles: u64,
    load_witness_calls: usize,
    load_cell_data_calls: usize,
    load_header_by_field_calls: usize,
    wrong_signature_is_lock_scope: bool,
    schema_cell_intent_mismatch_detected: bool,
    schema_cell_intent_aligned: bool,
}

#[derive(Debug, Serialize)]
struct CaseReport {
    fixture: String,
    category: String,
    source_fixture_expected: String,
    state_type_expected: String,
    state_type_expected_reason: String,
    accepted: bool,
    exit_code: i8,
    matched_state_type_expected: bool,
    matched_source_fixture_expected: bool,
    cycles: u64,
    current_timepoint: u64,
    witness_size_bytes: usize,
    input_cell_data_size_bytes: usize,
    output_cell_data_size_bytes: usize,
    receipt_cell_data_size_bytes: usize,
    intent_size_bytes: usize,
    canonical_intent_size_bytes: usize,
    receipt_hash: String,
    state_hash_commitment: String,
    syscall_trace: StateTypeTrace,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StateTypeTrace {
    load_witness_calls: usize,
    load_witness_failures: usize,
    load_cell_data_calls: usize,
    load_cell_data_failures: usize,
    load_header_by_field_calls: usize,
    load_header_by_field_failures: usize,
}

struct StateTypeSyscalls {
    witness: Vec<u8>,
    input_cell_data: Vec<u8>,
    output_cell_data: Vec<Vec<u8>>,
    current_timepoint: u64,
    trace: Arc<Mutex<StateTypeTrace>>,
}

struct StateTypeCase {
    fixture: String,
    category: String,
    source_fixture_expected: String,
    state_type_expected: String,
    state_type_expected_reason: String,
    current_timepoint: u64,
    input_cell_data: Vec<u8>,
    output_cell_data: Vec<u8>,
    receipt_cell_data: Vec<u8>,
    intent: Vec<u8>,
    canonical_intent: Vec<u8>,
    receipt_hash: Vec<u8>,
    state_hash_commitment: Vec<u8>,
    witness: Vec<u8>,
}

impl<Mac: SupportMachine<REG = u64>> Syscalls<Mac> for StateTypeSyscalls {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, ckb_vm::Error> {
        match machine.registers()[A7] {
            CKB_LOAD_WITNESS_SYSCALL_NUMBER => {
                self.load_witness(machine)?;
                Ok(true)
            }
            CKB_LOAD_CELL_DATA_SYSCALL_NUMBER => {
                self.load_cell_data(machine)?;
                Ok(true)
            }
            CKB_LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER => {
                self.load_header_by_field(machine)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl StateTypeSyscalls {
    fn load_witness<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        self.trace.lock().expect("trace mutex poisoned").load_witness_calls += 1;
        let buffer = machine.registers()[A0];
        let size_ptr = machine.registers()[A1];
        let offset = machine.registers()[A2];
        let index = machine.registers()[A3];
        let source = machine.registers()[A4];
        if index != 0 || !matches!(source, CKB_SOURCE_GROUP_INPUT | CKB_SOURCE_GROUP_OUTPUT) {
            self.trace.lock().expect("trace mutex poisoned").load_witness_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }
        let witness = self.witness.clone();
        Self::load_bytes(machine, &witness, buffer, size_ptr, offset)
    }

    fn load_cell_data<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        self.trace.lock().expect("trace mutex poisoned").load_cell_data_calls += 1;
        let buffer = machine.registers()[A0];
        let size_ptr = machine.registers()[A1];
        let offset = machine.registers()[A2];
        let index = machine.registers()[A3];
        let source = machine.registers()[A4];
        let Ok(index) = usize::try_from(index) else {
            self.trace.lock().expect("trace mutex poisoned").load_cell_data_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        };
        let source_bytes = match source {
            CKB_SOURCE_INPUT | CKB_SOURCE_GROUP_INPUT if index == 0 => self.input_cell_data.clone(),
            CKB_SOURCE_OUTPUT | CKB_SOURCE_GROUP_OUTPUT => match self.output_cell_data.get(index) {
                Some(data) => data.clone(),
                None => {
                    self.trace.lock().expect("trace mutex poisoned").load_cell_data_failures += 1;
                    machine.set_register(A0, 1);
                    return Ok(());
                }
            },
            _ => {
                self.trace.lock().expect("trace mutex poisoned").load_cell_data_failures += 1;
                machine.set_register(A0, 1);
                return Ok(());
            }
        };
        Self::load_bytes(machine, &source_bytes, buffer, size_ptr, offset)
    }

    fn load_header_by_field<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        self.trace.lock().expect("trace mutex poisoned").load_header_by_field_calls += 1;
        let buffer = machine.registers()[A0];
        let size_ptr = machine.registers()[A1];
        let offset = machine.registers()[A2];
        let index = machine.registers()[A3];
        let source = machine.registers()[A4];
        let field = machine.registers()[A5];
        if index != 0 || source != CKB_SOURCE_HEADER_DEP || field != HEADER_FIELD_EPOCH_NUMBER {
            self.trace.lock().expect("trace mutex poisoned").load_header_by_field_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }
        Self::load_bytes(machine, &self.current_timepoint.to_le_bytes(), buffer, size_ptr, offset)
    }

    fn load_bytes<Mac: SupportMachine<REG = u64>>(
        machine: &mut Mac,
        source: &[u8],
        buffer: u64,
        size_ptr: u64,
        offset: u64,
    ) -> Result<(), ckb_vm::Error> {
        let capacity = machine.memory_mut().load64(&size_ptr)?;
        let Ok(offset) = usize::try_from(offset) else {
            machine.set_register(A0, 1);
            return Ok(());
        };
        if offset > source.len() {
            machine.set_register(A0, 1);
            machine.memory_mut().store64(&size_ptr, &0)?;
            return Ok(());
        }
        let remaining = &source[offset..];
        let write_len = remaining.len().min(capacity as usize);
        if write_len > 0 {
            machine.memory_mut().store_bytes(buffer, &Bytes::copy_from_slice(&remaining[..write_len]))?;
        }
        machine.memory_mut().store64(&size_ptr, &(remaining.len() as u64))?;
        machine.set_register(A0, 0);
        Ok(())
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
    let action_elf = fs::read(&args.action_elf)?;
    let vectors: Value = serde_json::from_slice(&fs::read(&args.canonical_vectors)?)?;
    let cases = build_cases(&vectors, &args.fixtures_dir)?;
    let reports: Vec<_> = cases.iter().map(|case| run_case(&args, &action_elf, case)).collect::<Result<_, _>>()?;
    let report = build_report(&args, &action_elf, reports);
    write_report(&args.output, &report, args.pretty)?;
    print_summary(&args.output, &report);
    if report.summary.state_type_mismatched == 0 {
        Ok(())
    } else {
        Err(HarnessError::Message(format!("{} state type case(s) mismatched", report.summary.state_type_mismatched)))
    }
}

fn parse_args() -> Result<Args, HarnessError> {
    let mut args = Args {
        action_elf: PathBuf::from(DEFAULT_ACTION_ELF),
        canonical_vectors: PathBuf::from(DEFAULT_CANONICAL_VECTORS),
        fixtures_dir: PathBuf::from(DEFAULT_FIXTURES_DIR),
        output: PathBuf::from(DEFAULT_OUTPUT),
        max_cycles: MAX_CYCLES_DEFAULT,
        pretty: false,
    };
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--action-elf" => args.action_elf = PathBuf::from(iter.next().ok_or(HarnessError::Usage)?),
            "--canonical-vectors" => args.canonical_vectors = PathBuf::from(iter.next().ok_or(HarnessError::Usage)?),
            "--fixtures-dir" => args.fixtures_dir = PathBuf::from(iter.next().ok_or(HarnessError::Usage)?),
            "--output" => args.output = PathBuf::from(iter.next().ok_or(HarnessError::Usage)?),
            "--max-cycles" => {
                args.max_cycles = iter.next().ok_or(HarnessError::Usage)?.parse().map_err(|_| HarnessError::Usage)?;
            }
            "--pretty" => args.pretty = true,
            _ => return Err(HarnessError::Usage),
        }
    }
    Ok(args)
}

fn build_cases(vectors: &Value, fixtures_dir: &Path) -> Result<Vec<StateTypeCase>, HarnessError> {
    let array = vectors
        .get("vectors")
        .and_then(Value::as_array)
        .ok_or_else(|| HarnessError::Message("canonical vectors missing vectors[]".to_string()))?;
    array.iter().map(|value| build_case(value, fixtures_dir)).collect()
}

fn build_case(value: &Value, fixtures_dir: &Path) -> Result<StateTypeCase, HarnessError> {
    let fixture = str_field(value, "fixture")?.to_string();
    let category = str_field(value, "category")?.to_string();
    let fixture_json: Value = serde_json::from_slice(&fs::read(fixtures_dir.join(&fixture))?)?;
    let current_timepoint = fixture_json.pointer("/inputs/current_timepoint").and_then(Value::as_u64).unwrap_or(200);
    let source_fixture_expected =
        value.pointer("/source_model_result/result").and_then(Value::as_str).unwrap_or("unknown").to_string();
    let wrong_signature_scope = fixture == "wrong_signature_reject.json";
    let state_type_expected = if wrong_signature_scope { "accepted" } else { source_fixture_expected.as_str() }.to_string();
    let state_type_expected_reason = if wrong_signature_scope {
        "BTC signature rejection is authority-lock scope; the state transition guards are otherwise valid.".to_string()
    } else {
        "State transition fixture expectation is enforced by key_auth_transition guards.".to_string()
    };

    let encoded = value.get("encoded").ok_or_else(|| HarnessError::Message(format!("{fixture}: missing encoded")))?.clone();
    let old_cell = hex_bytes(encoded.pointer("/old_cell/hex"), &fixture, "encoded.old_cell.hex")?;
    if old_cell.len() != NOVASEAL_CELL_LEN {
        return Err(HarnessError::Message(format!("{fixture}: old_cell has {} bytes, expected {NOVASEAL_CELL_LEN}", old_cell.len())));
    }
    let intent = if fixture == "receipt_hash_mismatch_reject.json" {
        hex_bytes(encoded.pointer("/intent/hex"), &fixture, "encoded.intent.hex")?
    } else {
        hex_bytes(encoded.pointer("/resolved/resolved_intent/hex"), &fixture, "encoded.resolved.resolved_intent.hex")?
    };
    if intent.len() != INTENT_LEN {
        return Err(HarnessError::Message(format!("{fixture}: intent has {} bytes, expected {INTENT_LEN}", intent.len())));
    }
    let receipt_hash = hex_string_field(value, "/hashes/resolved_receipt_hash", &fixture)?;
    let receipt_hash = decode_hex(&receipt_hash)?;
    if receipt_hash.len() != BYTE32_LEN {
        return Err(HarnessError::Message(format!("{fixture}: receipt_hash has {} bytes, expected {BYTE32_LEN}", receipt_hash.len())));
    }
    let state_hash_commitment =
        ckb_blake2b256(&intent[INTENT_NEW_STATE_HASH_OFFSET..INTENT_NEW_STATE_HASH_OFFSET + BYTE32_LEN]).to_vec();
    let output_cell_data = build_output_cell(&old_cell, &intent);
    let receipt_cell_data = build_receipt_cell(&old_cell, &intent)?;
    let witness = build_witness(&intent, &receipt_hash, &state_hash_commitment);
    Ok(StateTypeCase {
        fixture,
        category,
        source_fixture_expected,
        state_type_expected,
        state_type_expected_reason,
        current_timepoint,
        input_cell_data: old_cell,
        output_cell_data,
        receipt_cell_data,
        canonical_intent: intent.clone(),
        intent,
        receipt_hash,
        state_hash_commitment,
        witness,
    })
}

fn run_case(args: &Args, action_elf: &[u8], case: &StateTypeCase) -> Result<CaseReport, HarnessError> {
    let trace = Arc::new(Mutex::new(StateTypeTrace::default()));
    let syscall = StateTypeSyscalls {
        witness: case.witness.clone(),
        input_cell_data: case.input_cell_data.clone(),
        output_cell_data: vec![case.output_cell_data.clone(), case.receipt_cell_data.clone()],
        current_timepoint: case.current_timepoint,
        trace: Arc::clone(&trace),
    };
    let core_machine =
        DefaultCoreMachine::<u64, WXorXMemory<SparseMemory<u64>>>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, args.max_cycles);
    let builder =
        DefaultMachineBuilder::new(core_machine).instruction_cycle_func(Box::new(estimate_cycles)).syscall(Box::new(syscall));
    let mut machine = HarnessMachine::new(builder.build());
    machine
        .load_program(&Bytes::copy_from_slice(action_elf), &[])
        .map_err(|error| HarnessError::Message(format!("failed to load state type action ELF in ckb-vm: {error}")))?;
    let exit_code =
        machine.run().map_err(|error| HarnessError::Message(format!("state type action ELF ckb-vm execution failed: {error}")))?;
    let accepted = exit_code == 0;
    let matched_state_type_expected = match case.state_type_expected.as_str() {
        "accepted" => accepted,
        "rejected" => !accepted,
        other => return Err(HarnessError::Message(format!("{} has unsupported state type expected value: {other}", case.fixture))),
    };
    let matched_source_fixture_expected = match case.source_fixture_expected.as_str() {
        "accepted" => accepted,
        "rejected" => !accepted,
        _ => false,
    };
    Ok(CaseReport {
        fixture: case.fixture.clone(),
        category: case.category.clone(),
        source_fixture_expected: case.source_fixture_expected.clone(),
        state_type_expected: case.state_type_expected.clone(),
        state_type_expected_reason: case.state_type_expected_reason.clone(),
        accepted,
        exit_code,
        matched_state_type_expected,
        matched_source_fixture_expected,
        cycles: machine.machine.cycles(),
        current_timepoint: case.current_timepoint,
        witness_size_bytes: case.witness.len(),
        input_cell_data_size_bytes: case.input_cell_data.len(),
        output_cell_data_size_bytes: case.output_cell_data.len(),
        receipt_cell_data_size_bytes: case.receipt_cell_data.len(),
        intent_size_bytes: case.intent.len(),
        canonical_intent_size_bytes: case.canonical_intent.len(),
        receipt_hash: hex0x(&case.receipt_hash),
        state_hash_commitment: hex0x(&case.state_hash_commitment),
        syscall_trace: trace.lock().expect("trace mutex poisoned").clone(),
    })
}

fn build_output_cell(old_cell: &[u8], intent: &[u8]) -> Vec<u8> {
    let mut output = old_cell.to_vec();
    output[CELL_STATE_HASH_OFFSET..CELL_STATE_HASH_OFFSET + BYTE32_LEN]
        .copy_from_slice(&intent[INTENT_NEW_STATE_HASH_OFFSET..INTENT_NEW_STATE_HASH_OFFSET + BYTE32_LEN]);
    output[CELL_POLICY_HASH_OFFSET..CELL_POLICY_HASH_OFFSET + BYTE32_LEN]
        .copy_from_slice(&old_cell[CELL_POLICY_HASH_OFFSET..CELL_POLICY_HASH_OFFSET + BYTE32_LEN]);
    output[CELL_RECEIPT_ROOT_OFFSET..CELL_RECEIPT_ROOT_OFFSET + BYTE32_LEN]
        .copy_from_slice(&old_cell[CELL_RECEIPT_ROOT_OFFSET..CELL_RECEIPT_ROOT_OFFSET + BYTE32_LEN]);
    output[CELL_NONCE_OFFSET..CELL_NONCE_OFFSET + 8].copy_from_slice(&intent[INTENT_NONCE_OFFSET..INTENT_NONCE_OFFSET + 8]);
    output[CELL_EXPIRY_OFFSET..CELL_EXPIRY_OFFSET + 8].copy_from_slice(&intent[INTENT_EXPIRY_OFFSET..INTENT_EXPIRY_OFFSET + 8]);
    output
}

fn build_receipt_cell(old_cell: &[u8], intent: &[u8]) -> Result<Vec<u8>, HarnessError> {
    if old_cell.len() != NOVASEAL_CELL_LEN {
        return Err(HarnessError::Message(format!("old_cell has {} bytes, expected {NOVASEAL_CELL_LEN}", old_cell.len())));
    }
    if intent.len() != INTENT_LEN {
        return Err(HarnessError::Message(format!("intent has {} bytes, expected {INTENT_LEN}", intent.len())));
    }
    let mut receipt = Vec::with_capacity(PROOF_RECEIPT_LEN);
    receipt.extend_from_slice(&intent[INTENT_DOMAIN_OFFSET..INTENT_DOMAIN_OFFSET + BYTE32_LEN]);
    receipt.extend_from_slice(&old_cell[0..2]);
    receipt.push(intent[INTENT_ACTION_OFFSET]);
    receipt.extend_from_slice(&intent[INTENT_OLD_CELL_OFFSET..INTENT_OLD_CELL_OFFSET + 36]);
    receipt.extend_from_slice(&intent[INTENT_OLD_STATE_HASH_OFFSET..INTENT_OLD_STATE_HASH_OFFSET + BYTE32_LEN]);
    receipt.extend_from_slice(&intent[INTENT_NEW_STATE_HASH_OFFSET..INTENT_NEW_STATE_HASH_OFFSET + BYTE32_LEN]);
    receipt.extend_from_slice(&ckb_blake2b256(&intent[INTENT_DOMAIN_OFFSET..INTENT_DOMAIN_OFFSET + BYTE32_LEN]));
    receipt.extend_from_slice(&intent[INTENT_POLICY_HASH_OFFSET..INTENT_POLICY_HASH_OFFSET + BYTE32_LEN]);
    receipt.extend_from_slice(&old_cell[CELL_BTC_AUTHORITY_HASH_OFFSET..CELL_BTC_AUTHORITY_HASH_OFFSET + BYTE32_LEN]);
    receipt.extend_from_slice(&[0u8; BYTE32_LEN]);
    receipt.extend_from_slice(&intent[INTENT_NONCE_OFFSET..INTENT_NONCE_OFFSET + 8]);
    receipt.extend_from_slice(&intent[INTENT_EXPIRY_OFFSET..INTENT_EXPIRY_OFFSET + 8]);
    if receipt.len() != PROOF_RECEIPT_LEN {
        return Err(HarnessError::Message(format!("receipt has {} bytes, expected {PROOF_RECEIPT_LEN}", receipt.len())));
    }
    Ok(receipt)
}

fn build_witness(intent: &[u8], receipt_hash: &[u8], state_hash_commitment: &[u8]) -> Vec<u8> {
    let signature_payload = [0u8; SIGNATURE_PAYLOAD_LEN];
    let mut witness = Vec::with_capacity(
        LOCK_WITNESS_MAGIC.len() + 4 + intent.len() + receipt_hash.len() + state_hash_commitment.len() + 4 + signature_payload.len(),
    );
    witness.extend_from_slice(LOCK_WITNESS_MAGIC);
    witness.extend_from_slice(&(intent.len() as u32).to_le_bytes());
    witness.extend_from_slice(intent);
    witness.extend_from_slice(receipt_hash);
    witness.extend_from_slice(state_hash_commitment);
    witness.extend_from_slice(&(signature_payload.len() as u32).to_le_bytes());
    witness.extend_from_slice(&signature_payload);
    witness
}

fn build_report(args: &Args, action_elf: &[u8], cases: Vec<CaseReport>) -> Report {
    let total_cases = cases.len();
    let accepted = cases.iter().filter(|case| case.accepted).count();
    let state_type_matched_expected = cases.iter().filter(|case| case.matched_state_type_expected).count();
    let source_fixture_matched_by_state_type_only = cases.iter().filter(|case| case.matched_source_fixture_expected).count();
    let source_fixture_requires_lock_or_external_context = total_cases - source_fixture_matched_by_state_type_only;
    Report {
        schema: "novaseal-state-type-ckb-vm-report-v0.1",
        classification: "state_type_action_ckb_vm_fixture_evidence",
        action_elf: ElfReport {
            path: args.action_elf.display().to_string(),
            size_bytes: action_elf.len(),
            sha256: sha256_hex(action_elf),
        },
        summary: Summary {
            state_type_action_ckb_vm_executed: true,
            action_entry: "key_auth_transition",
            total_cases,
            state_type_expected_accept: cases.iter().filter(|case| case.state_type_expected == "accepted").count(),
            state_type_expected_reject: cases.iter().filter(|case| case.state_type_expected == "rejected").count(),
            accepted,
            rejected: total_cases - accepted,
            state_type_matched_expected,
            state_type_mismatched: total_cases - state_type_matched_expected,
            source_fixture_matched_by_state_type_only,
            source_fixture_requires_lock_or_external_context,
            max_cycles: cases.iter().map(|case| case.cycles).max().unwrap_or_default(),
            load_witness_calls: cases.iter().map(|case| case.syscall_trace.load_witness_calls).sum(),
            load_cell_data_calls: cases.iter().map(|case| case.syscall_trace.load_cell_data_calls).sum(),
            load_header_by_field_calls: cases.iter().map(|case| case.syscall_trace.load_header_by_field_calls).sum(),
            wrong_signature_is_lock_scope: cases.iter().any(|case| case.fixture == "wrong_signature_reject.json" && case.accepted),
            schema_cell_intent_mismatch_detected: false,
            schema_cell_intent_aligned: true,
        },
        cases,
        limits: vec![
            "Executes the compiled key_auth_transition action ELF in ckb-vm with harnessed LOAD_WITNESS, LOAD_CELL_DATA, and LOAD_HEADER_BY_FIELD syscalls.",
            "This is action/type-script evidence only; it does not run the btc_authority lock or the external BTC verifier.",
            "The wrong_signature_reject fixture is expected to pass at this layer because signature rejection is authority-lock scope.",
            "The .cell inline NovaSealIntentV0 now uses the same 213-byte old_cell: OutPoint shape as schemas/nova_intent_v0.schema; no intent-shortening adapter is used.",
            "The action entry parses the shared lock+type witness payload: intent, receipt_hash, state_hash_commitment, and SignaturePayload. The signature bytes are ignored at type scope so the same witness can be used by the authority lock.",
            "Receipt output data is materialised as Output#1, matching the combined transaction harness.",
            "The transaction context is harnessed directly, not produced by a production builder/full-node acceptance path.",
        ],
    }
}

fn str_field<'a>(value: &'a Value, key: &str) -> Result<&'a str, HarnessError> {
    value.get(key).and_then(Value::as_str).ok_or_else(|| HarnessError::Message(format!("missing string field: {key}")))
}

fn hex_bytes(value: Option<&Value>, fixture: &str, path: &str) -> Result<Vec<u8>, HarnessError> {
    let hex = value.and_then(Value::as_str).ok_or_else(|| HarnessError::Message(format!("{fixture}: missing {path}")))?;
    decode_hex(hex)
}

fn hex_string_field(value: &Value, pointer: &str, fixture: &str) -> Result<String, HarnessError> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| HarnessError::Message(format!("{fixture}: missing {pointer}")))
}

fn decode_hex(value: &str) -> Result<Vec<u8>, HarnessError> {
    Ok(hex::decode(value.strip_prefix("0x").unwrap_or(value))?)
}

fn ckb_blake2b256(data: &[u8]) -> [u8; 32] {
    let mut output = [0u8; 32];
    let hash = Blake2bParams::new().hash_length(32).personal(b"ckb-default-hash").to_state().update(data).finalize();
    output.copy_from_slice(hash.as_bytes());
    output
}

fn write_report(path: &Path, report: &Report, pretty: bool) -> Result<(), HarnessError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = if pretty { serde_json::to_string_pretty(report)? } else { serde_json::to_string(report)? };
    fs::write(path, json + "\n")?;
    Ok(())
}

fn print_summary(path: &Path, report: &Report) {
    println!("wrote {}", path.display());
    println!(
        "summary: state_type_vm_executed={} total={} accepted={} rejected={} state_type_matched_expected={} state_type_mismatched={} source_fixture_matched_by_state_type_only={} source_fixture_requires_lock_or_external_context={} max_cycles={}",
        report.summary.state_type_action_ckb_vm_executed,
        report.summary.total_cases,
        report.summary.accepted,
        report.summary.rejected,
        report.summary.state_type_matched_expected,
        report.summary.state_type_mismatched,
        report.summary.source_fixture_matched_by_state_type_only,
        report.summary.source_fixture_requires_lock_or_external_context,
        report.summary.max_cycles,
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
