#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::struct_excessive_bools, clippy::too_many_lines)]

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

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

const DEFAULT_ORIGINATE_ELF: &str = "target/nova-agreement-originate-action.elf";
const DEFAULT_REPAY_ELF: &str = "target/nova-agreement-repay-action.elf";
const DEFAULT_CLAIM_ELF: &str = "target/nova-agreement-claim-action.elf";
const DEFAULT_FIXTURES_DIR: &str = "fixtures";
const DEFAULT_OUTPUT: &str = "target/nova-agreement-ckb-vm-action-report.json";
const MAX_CYCLES_DEFAULT: u64 = 200_000_000;

const CKB_SOURCE_INPUT: u64 = 0x01;
const CKB_SOURCE_OUTPUT: u64 = 0x02;
const CKB_SOURCE_HEADER_DEP: u64 = 0x04;
const CKB_SOURCE_GROUP_INPUT: u64 = 0x0100_0000_0000_0000 | CKB_SOURCE_INPUT;
const CKB_SOURCE_GROUP_OUTPUT: u64 = 0x0100_0000_0000_0000 | CKB_SOURCE_OUTPUT;

const CKB_LOAD_WITNESS_SYSCALL_NUMBER: u64 = 2074;
const CKB_LOAD_CELL_BY_FIELD_SYSCALL_NUMBER: u64 = 2081;
const CKB_LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER: u64 = 2082;
const CKB_LOAD_CELL_DATA_SYSCALL_NUMBER: u64 = 2092;
const CELL_FIELD_CAPACITY: u64 = 0;
const HEADER_FIELD_EPOCH_NUMBER: u64 = 0;
const LOCK_WITNESS_MAGIC: &[u8; 8] = b"CSARGv1\0";

const CKB: u64 = 100_000_000;
const COLLATERAL_AMOUNT: u64 = 1_000 * CKB;
const PRINCIPAL_AMOUNT: u64 = 700 * CKB;
const FIXED_FEE_AMOUNT: u64 = 30 * CKB;
const HARNESS_CELL_CAPACITY: u64 = 10_000 * CKB;
const START_TIMEPOINT: u64 = 100;
const EXPIRY_TIMEPOINT: u64 = 200;

const TERMS_LEN: usize = 237;
const AGREEMENT_CELL_LEN: usize = 269;
const AGREEMENT_RECEIPT_LEN: usize = 243;
const PAYOUT_LEN: usize = 147;

const AGREEMENT_VERSION: u16 = 0;
const ASSET_KIND_CKB: u8 = 0;
const EARLY_CLOSE_FIXED_FEE: u8 = 0;
const STATUS_OFFERED: u8 = 0;
const STATUS_ACTIVE: u8 = 1;
const STATUS_REPAID: u8 = 2;
const STATUS_DEFAULTED: u8 = 3;
const PATH_ORIGINATE: u8 = 0;
const PATH_REPAY_BEFORE_EXPIRY: u8 = 1;
const PATH_CLAIM_AFTER_EXPIRY: u8 = 2;
const PAYOUT_BORROWER_PRINCIPAL: u8 = 0;
const PAYOUT_LENDER_REPAYMENT: u8 = 1;
const PAYOUT_BORROWER_COLLATERAL_RETURN: u8 = 2;
const PAYOUT_LENDER_DEFAULT_CLAIM: u8 = 3;

const ZERO_HASH: [u8; 32] = [0x00; 32];
const AGREEMENT_ID: [u8; 32] = [0xaa; 32];
const TERMS_HASH: [u8; 32] = [0xbb; 32];
const BORROWER_AUTHORITY: [u8; 32] = [0x11; 32];
const LENDER_AUTHORITY: [u8; 32] = [0x22; 32];
const STRANGER_AUTHORITY: [u8; 32] = [0x33; 32];
const OLD_LATEST_RECEIPT_HASH: [u8; 32] = [0x44; 32];
const NEW_LATEST_RECEIPT_HASH: [u8; 32] = [0x55; 32];
const OTHER_LATEST_RECEIPT_HASH: [u8; 32] = [0x66; 32];
const OTHER_TERMS_HASH: [u8; 32] = [0xcc; 32];

type HarnessMachine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;

#[derive(Debug, Error)]
enum HarnessError {
    #[error(
        "usage: novaseal_agreement_ckb_vm_harness [--originate-elf PATH] [--repay-elf PATH] [--claim-elf PATH] [--fixtures-dir PATH] [--output PATH] [--max-cycles N] [--pretty]"
    )]
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
    originate_elf: PathBuf,
    repay_elf: PathBuf,
    claim_elf: PathBuf,
    fixtures_dir: PathBuf,
    output: PathBuf,
    max_cycles: u64,
    pretty: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ActionKind {
    Originate,
    Repay,
    Claim,
}

#[derive(Clone, Debug, Default, Serialize)]
struct AgreementTrace {
    load_witness_calls: usize,
    load_witness_failures: usize,
    load_cell_data_calls: usize,
    load_cell_data_failures: usize,
    load_cell_by_field_calls: usize,
    load_cell_by_field_failures: usize,
    load_header_by_field_calls: usize,
    load_header_by_field_failures: usize,
}

struct AgreementSyscalls {
    witness: Vec<u8>,
    input_cell_data: Vec<Vec<u8>>,
    output_cell_data: Vec<Vec<u8>>,
    current_timepoint: u64,
    trace: Arc<Mutex<AgreementTrace>>,
}

#[derive(Debug)]
struct AgreementCase {
    fixture: &'static str,
    variant: &'static str,
    action: ActionKind,
    expected: &'static str,
    expected_reason: &'static str,
    current_timepoint: u64,
    witness: Vec<u8>,
    input_cell_data: Vec<Vec<u8>>,
    output_cell_data: Vec<Vec<u8>>,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    classification: &'static str,
    action_elves: BTreeMap<&'static str, ElfReport>,
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
    action_ckb_vm_executed: bool,
    total_cases: usize,
    expected_accept: usize,
    expected_reject: usize,
    accepted: usize,
    rejected: usize,
    matched_expected: usize,
    mismatched: usize,
    fixtures_covered: Vec<&'static str>,
    fixture_files_not_executed_by_action_vm: Vec<String>,
    max_cycles: u64,
    load_witness_calls: usize,
    load_cell_data_calls: usize,
    load_header_by_field_calls: usize,
    originate_action_exercised: bool,
    repay_action_exercised: bool,
    claim_action_exercised: bool,
    time_guards_reject: bool,
    party_guards_reject: bool,
    nonce_guard_rejects: bool,
    latest_receipt_hash_guard_rejects: bool,
    preserve_field_guard_rejects: bool,
}

#[derive(Debug, Serialize)]
struct CaseReport {
    fixture: &'static str,
    variant: &'static str,
    action: ActionKind,
    expected: &'static str,
    expected_reason: &'static str,
    accepted: bool,
    exit_code: i8,
    matched_expected: bool,
    cycles: u64,
    current_timepoint: u64,
    witness_size_bytes: usize,
    input_cell_count: usize,
    output_cell_count: usize,
    input_cell_data_sizes: Vec<usize>,
    output_cell_data_sizes: Vec<usize>,
    syscall_trace: AgreementTrace,
}

#[derive(Clone, Debug)]
struct AgreementFields {
    agreement_id: [u8; 32],
    terms_hash: [u8; 32],
    borrower_authority_hash: [u8; 32],
    lender_authority_hash: [u8; 32],
    collateral_asset_kind: u8,
    collateral_asset_hash: [u8; 32],
    collateral_amount: u64,
    principal_asset_kind: u8,
    principal_asset_hash: [u8; 32],
    principal_amount: u64,
    fixed_fee_amount: u64,
    start_timepoint: u64,
    expiry_timepoint: u64,
    early_close_policy: u8,
}

impl Default for AgreementFields {
    fn default() -> Self {
        Self {
            agreement_id: AGREEMENT_ID,
            terms_hash: TERMS_HASH,
            borrower_authority_hash: BORROWER_AUTHORITY,
            lender_authority_hash: LENDER_AUTHORITY,
            collateral_asset_kind: ASSET_KIND_CKB,
            collateral_asset_hash: ZERO_HASH,
            collateral_amount: COLLATERAL_AMOUNT,
            principal_asset_kind: ASSET_KIND_CKB,
            principal_asset_hash: ZERO_HASH,
            principal_amount: PRINCIPAL_AMOUNT,
            fixed_fee_amount: FIXED_FEE_AMOUNT,
            start_timepoint: START_TIMEPOINT,
            expiry_timepoint: EXPIRY_TIMEPOINT,
            early_close_policy: EARLY_CLOSE_FIXED_FEE,
        }
    }
}

impl<Mac: SupportMachine<REG = u64>> Syscalls<Mac> for AgreementSyscalls {
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
            CKB_LOAD_CELL_BY_FIELD_SYSCALL_NUMBER => {
                self.load_cell_by_field(machine)?;
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

impl AgreementSyscalls {
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
            CKB_SOURCE_INPUT | CKB_SOURCE_GROUP_INPUT => self.input_cell_data.get(index).cloned(),
            CKB_SOURCE_OUTPUT | CKB_SOURCE_GROUP_OUTPUT => self.output_cell_data.get(index).cloned(),
            _ => None,
        };
        let Some(source_bytes) = source_bytes else {
            self.trace.lock().expect("trace mutex poisoned").load_cell_data_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        };
        Self::load_bytes(machine, &source_bytes, buffer, size_ptr, offset)
    }

    fn load_cell_by_field<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        self.trace.lock().expect("trace mutex poisoned").load_cell_by_field_calls += 1;
        let buffer = machine.registers()[A0];
        let size_ptr = machine.registers()[A1];
        let offset = machine.registers()[A2];
        let index = machine.registers()[A3];
        let source = machine.registers()[A4];
        let field = machine.registers()[A5];
        let Ok(index) = usize::try_from(index) else {
            self.trace.lock().expect("trace mutex poisoned").load_cell_by_field_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        };
        let source_exists = match source {
            CKB_SOURCE_INPUT | CKB_SOURCE_GROUP_INPUT => self.input_cell_data.get(index).is_some(),
            CKB_SOURCE_OUTPUT | CKB_SOURCE_GROUP_OUTPUT => self.output_cell_data.get(index).is_some(),
            _ => false,
        };
        if field != CELL_FIELD_CAPACITY || !source_exists {
            self.trace.lock().expect("trace mutex poisoned").load_cell_by_field_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }
        Self::load_bytes(machine, &HARNESS_CELL_CAPACITY.to_le_bytes(), buffer, size_ptr, offset)
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
            machine.memory_mut().store64(&size_ptr, &0)?;
            machine.set_register(A0, 1);
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
    let originate_elf = fs::read(&args.originate_elf)?;
    let repay_elf = fs::read(&args.repay_elf)?;
    let claim_elf = fs::read(&args.claim_elf)?;
    let fixture_expectations = load_fixture_expectations(&args.fixtures_dir)?;
    let cases = build_cases();
    ensure_fixture_coverage(&cases, &fixture_expectations)?;
    let reports = cases
        .iter()
        .map(|case| {
            let elf = match case.action {
                ActionKind::Originate => originate_elf.as_slice(),
                ActionKind::Repay => repay_elf.as_slice(),
                ActionKind::Claim => claim_elf.as_slice(),
            };
            run_case(&args, elf, case)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let report = build_report(&args, &originate_elf, &repay_elf, &claim_elf, reports, &fixture_expectations);
    write_report(&args.output, &report, args.pretty)?;
    print_summary(&args.output, &report);
    if report.summary.mismatched == 0 {
        Ok(())
    } else {
        Err(HarnessError::Message(format!("{} action VM case(s) mismatched", report.summary.mismatched)))
    }
}

fn parse_args() -> Result<Args, HarnessError> {
    let mut args = Args {
        originate_elf: PathBuf::from(DEFAULT_ORIGINATE_ELF),
        repay_elf: PathBuf::from(DEFAULT_REPAY_ELF),
        claim_elf: PathBuf::from(DEFAULT_CLAIM_ELF),
        fixtures_dir: PathBuf::from(DEFAULT_FIXTURES_DIR),
        output: PathBuf::from(DEFAULT_OUTPUT),
        max_cycles: MAX_CYCLES_DEFAULT,
        pretty: false,
    };
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--originate-elf" => args.originate_elf = PathBuf::from(iter.next().ok_or(HarnessError::Usage)?),
            "--repay-elf" => args.repay_elf = PathBuf::from(iter.next().ok_or(HarnessError::Usage)?),
            "--claim-elf" => args.claim_elf = PathBuf::from(iter.next().ok_or(HarnessError::Usage)?),
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

fn build_cases() -> Vec<AgreementCase> {
    let terms = AgreementFields::default();
    let terms_bytes = encode_terms(&terms);
    let active = encode_agreement_cell(&terms, STATUS_ACTIVE, OLD_LATEST_RECEIPT_HASH, 0);
    let originated = encode_agreement_cell(&terms, STATUS_ACTIVE, NEW_LATEST_RECEIPT_HASH, 0);
    let repaid = encode_agreement_cell(&terms, STATUS_REPAID, NEW_LATEST_RECEIPT_HASH, 1);
    let defaulted = encode_agreement_cell(&terms, STATUS_DEFAULTED, NEW_LATEST_RECEIPT_HASH, 1);
    let mut bad_terms = terms.clone();
    bad_terms.terms_hash = OTHER_TERMS_HASH;
    let bad_terms_bytes = encode_terms(&bad_terms);

    let originate_receipt =
        encode_receipt(&terms, PATH_ORIGINATE, STATUS_OFFERED, STATUS_ACTIVE, 0, ZERO_HASH, NEW_LATEST_RECEIPT_HASH, 0, 120);
    let originate_payout = encode_payout(
        &terms,
        PATH_ORIGINATE,
        PAYOUT_BORROWER_PRINCIPAL,
        BORROWER_AUTHORITY,
        ASSET_KIND_CKB,
        ZERO_HASH,
        PRINCIPAL_AMOUNT,
        0,
    );
    let repay_receipt = encode_receipt(
        &terms,
        PATH_REPAY_BEFORE_EXPIRY,
        STATUS_ACTIVE,
        STATUS_REPAID,
        PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
        OLD_LATEST_RECEIPT_HASH,
        NEW_LATEST_RECEIPT_HASH,
        1,
        180,
    );
    let lender_repayment = encode_payout(
        &terms,
        PATH_REPAY_BEFORE_EXPIRY,
        PAYOUT_LENDER_REPAYMENT,
        LENDER_AUTHORITY,
        ASSET_KIND_CKB,
        ZERO_HASH,
        PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
        1,
    );
    let borrower_collateral_return = encode_payout(
        &terms,
        PATH_REPAY_BEFORE_EXPIRY,
        PAYOUT_BORROWER_COLLATERAL_RETURN,
        BORROWER_AUTHORITY,
        ASSET_KIND_CKB,
        ZERO_HASH,
        COLLATERAL_AMOUNT,
        1,
    );
    let claim_receipt = encode_receipt(
        &terms,
        PATH_CLAIM_AFTER_EXPIRY,
        STATUS_ACTIVE,
        STATUS_DEFAULTED,
        COLLATERAL_AMOUNT,
        OLD_LATEST_RECEIPT_HASH,
        NEW_LATEST_RECEIPT_HASH,
        1,
        220,
    );
    let lender_default_claim = encode_payout(
        &terms,
        PATH_CLAIM_AFTER_EXPIRY,
        PAYOUT_LENDER_DEFAULT_CLAIM,
        LENDER_AUTHORITY,
        ASSET_KIND_CKB,
        ZERO_HASH,
        COLLATERAL_AMOUNT,
        1,
    );
    let mut wrong_repayment_amount = lender_repayment.clone();
    overwrite_u64(&mut wrong_repayment_amount, 99, PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT - CKB);
    let receipt_output_mismatch = encode_receipt(
        &terms,
        PATH_REPAY_BEFORE_EXPIRY,
        STATUS_ACTIVE,
        STATUS_REPAID,
        PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
        OLD_LATEST_RECEIPT_HASH,
        OTHER_LATEST_RECEIPT_HASH,
        1,
        180,
    );

    let mut bad_nonce = repaid.clone();
    overwrite_u64(&mut bad_nonce, 261, 2);
    let mut mutated_principal = repaid.clone();
    overwrite_u64(&mut mutated_principal, 204, PRINCIPAL_AMOUNT - CKB);
    let latest_receipt_hash_mismatch = encode_agreement_cell(&terms, STATUS_REPAID, OTHER_LATEST_RECEIPT_HASH, 1);

    vec![
        AgreementCase {
            fixture: "originate_valid",
            variant: "originate_valid",
            action: ActionKind::Originate,
            expected: "accepted",
            expected_reason: "valid terms, borrower originator, active agreement output, and receipt output",
            current_timepoint: 120,
            witness: build_originate_witness(&terms_bytes, &BORROWER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![],
            output_cell_data: vec![originated.clone(), originate_payout.clone(), originate_receipt.clone()],
        },
        AgreementCase {
            fixture: "wrong_terms_hash_reject",
            variant: "originate_wrong_terms_hash",
            action: ActionKind::Originate,
            expected: "rejected",
            expected_reason: "created agreement, payout, and receipt outputs must bind the witness terms_hash",
            current_timepoint: 120,
            witness: build_originate_witness(&bad_terms_bytes, &BORROWER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![],
            output_cell_data: vec![originated.clone(), originate_payout.clone(), originate_receipt.clone()],
        },
        AgreementCase {
            fixture: "wrong_originator_reject",
            variant: "originate_wrong_originator",
            action: ActionKind::Originate,
            expected: "rejected",
            expected_reason: "originator_authority_hash must equal terms.borrower_authority_hash",
            current_timepoint: 120,
            witness: build_originate_witness(&terms_bytes, &STRANGER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![],
            output_cell_data: vec![
                encode_agreement_cell(&terms, STATUS_ACTIVE, NEW_LATEST_RECEIPT_HASH, 0),
                originate_payout,
                encode_receipt(&terms, PATH_ORIGINATE, STATUS_OFFERED, STATUS_ACTIVE, 0, ZERO_HASH, NEW_LATEST_RECEIPT_HASH, 0, 120),
            ],
        },
        AgreementCase {
            fixture: "repay_before_expiry_valid",
            variant: "repay_valid",
            action: ActionKind::Repay,
            expected: "accepted",
            expected_reason: "borrower repays before expiry and terminal output preserves agreement fields",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![
                repaid.clone(),
                lender_repayment.clone(),
                borrower_collateral_return.clone(),
                repay_receipt.clone(),
            ],
        },
        AgreementCase {
            fixture: "expired_repay_reject",
            variant: "repay_after_expiry",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "repay_before_expiry requires now <= expiry_timepoint",
            current_timepoint: 220,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![
                repaid.clone(),
                lender_repayment.clone(),
                borrower_collateral_return.clone(),
                repay_receipt.clone(),
            ],
        },
        AgreementCase {
            fixture: "wrong_party_reject",
            variant: "repay_wrong_party",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "repay_before_expiry requires borrower actor hash",
            current_timepoint: 180,
            witness: build_terminal_witness(&STRANGER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![
                repaid.clone(),
                lender_repayment.clone(),
                borrower_collateral_return.clone(),
                repay_receipt.clone(),
            ],
        },
        AgreementCase {
            fixture: "nonce_mismatch_reject",
            variant: "repay_nonce_mismatch",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "closed.nonce must equal active.nonce + 1",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![bad_nonce, lender_repayment.clone(), borrower_collateral_return.clone(), repay_receipt.clone()],
        },
        AgreementCase {
            fixture: "preserved_field_mutation_reject",
            variant: "repay_preserved_principal_mutation",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "preserve closed from active rejects principal_amount mutation",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![
                mutated_principal,
                lender_repayment.clone(),
                borrower_collateral_return.clone(),
                repay_receipt.clone(),
            ],
        },
        AgreementCase {
            fixture: "latest_receipt_hash_mismatch_reject",
            variant: "repay_latest_receipt_hash_mismatch",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "closed.latest_receipt_hash must equal the witness receipt_hash",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![
                latest_receipt_hash_mismatch,
                lender_repayment.clone(),
                borrower_collateral_return.clone(),
                repay_receipt.clone(),
            ],
        },
        AgreementCase {
            fixture: "receipt_hash_mismatch_reject",
            variant: "repay_receipt_output_mismatch",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "materialized receipt output must bind the witness receipt_hash",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![
                repaid.clone(),
                lender_repayment.clone(),
                borrower_collateral_return.clone(),
                receipt_output_mismatch,
            ],
        },
        AgreementCase {
            fixture: "wrong_settlement_amount_reject",
            variant: "repay_wrong_settlement_amount",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "typed lender repayment output must equal principal plus fixed fee",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![repaid.clone(), wrong_repayment_amount, borrower_collateral_return.clone(), repay_receipt],
        },
        AgreementCase {
            fixture: "claim_after_expiry_valid",
            variant: "claim_valid",
            action: ActionKind::Claim,
            expected: "accepted",
            expected_reason: "lender claims locked collateral after expiry",
            current_timepoint: 220,
            witness: build_terminal_witness(&LENDER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![defaulted.clone(), lender_default_claim.clone(), claim_receipt.clone()],
        },
        AgreementCase {
            fixture: "early_claim_reject",
            variant: "claim_before_expiry",
            action: ActionKind::Claim,
            expected: "rejected",
            expected_reason: "claim_after_expiry requires now > expiry_timepoint",
            current_timepoint: 180,
            witness: build_terminal_witness(&LENDER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active.clone()],
            output_cell_data: vec![defaulted.clone(), lender_default_claim.clone(), claim_receipt.clone()],
        },
        AgreementCase {
            fixture: "wrong_party_reject",
            variant: "claim_wrong_party",
            action: ActionKind::Claim,
            expected: "rejected",
            expected_reason: "claim_after_expiry requires lender actor hash",
            current_timepoint: 220,
            witness: build_terminal_witness(&STRANGER_AUTHORITY, &NEW_LATEST_RECEIPT_HASH),
            input_cell_data: vec![active],
            output_cell_data: vec![defaulted, lender_default_claim, claim_receipt],
        },
    ]
}

fn run_case(args: &Args, action_elf: &[u8], case: &AgreementCase) -> Result<CaseReport, HarnessError> {
    let trace = Arc::new(Mutex::new(AgreementTrace::default()));
    let syscall = AgreementSyscalls {
        witness: case.witness.clone(),
        input_cell_data: case.input_cell_data.clone(),
        output_cell_data: case.output_cell_data.clone(),
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
        .map_err(|error| HarnessError::Message(format!("failed to load {:?} action ELF in ckb-vm: {error}", case.action)))?;
    let exit_code = machine
        .run()
        .map_err(|error| HarnessError::Message(format!("{:?} action ELF ckb-vm execution failed: {error}", case.action)))?;
    let accepted = exit_code == 0;
    let matched_expected = match case.expected {
        "accepted" => accepted,
        "rejected" => !accepted,
        other => return Err(HarnessError::Message(format!("{} has unsupported expected value: {other}", case.fixture))),
    };
    Ok(CaseReport {
        fixture: case.fixture,
        variant: case.variant,
        action: case.action,
        expected: case.expected,
        expected_reason: case.expected_reason,
        accepted,
        exit_code,
        matched_expected,
        cycles: machine.machine.cycles(),
        current_timepoint: case.current_timepoint,
        witness_size_bytes: case.witness.len(),
        input_cell_count: case.input_cell_data.len(),
        output_cell_count: case.output_cell_data.len(),
        input_cell_data_sizes: case.input_cell_data.iter().map(Vec::len).collect(),
        output_cell_data_sizes: case.output_cell_data.iter().map(Vec::len).collect(),
        syscall_trace: trace.lock().expect("trace mutex poisoned").clone(),
    })
}

fn build_report(
    args: &Args,
    originate_elf: &[u8],
    repay_elf: &[u8],
    claim_elf: &[u8],
    cases: Vec<CaseReport>,
    fixture_expectations: &BTreeMap<String, String>,
) -> Report {
    let total_cases = cases.len();
    let accepted = cases.iter().filter(|case| case.accepted).count();
    let matched_expected = cases.iter().filter(|case| case.matched_expected).count();
    let covered = cases.iter().map(|case| case.fixture).collect::<BTreeSet<_>>();
    let not_executed = fixture_expectations.keys().filter(|fixture| !covered.contains(fixture.as_str())).cloned().collect::<Vec<_>>();
    let mut action_elves = BTreeMap::new();
    action_elves.insert("originate_agreement", elf_report(&args.originate_elf, originate_elf));
    action_elves.insert("repay_before_expiry", elf_report(&args.repay_elf, repay_elf));
    action_elves.insert("claim_after_expiry", elf_report(&args.claim_elf, claim_elf));
    Report {
        schema: "novaseal-agreement-ckb-vm-action-report-v0.1",
        classification: "agreement_profile_action_ckb_vm_evidence",
        action_elves,
        summary: Summary {
            action_ckb_vm_executed: true,
            total_cases,
            expected_accept: cases.iter().filter(|case| case.expected == "accepted").count(),
            expected_reject: cases.iter().filter(|case| case.expected == "rejected").count(),
            accepted,
            rejected: total_cases - accepted,
            matched_expected,
            mismatched: total_cases - matched_expected,
            fixtures_covered: covered.into_iter().collect(),
            fixture_files_not_executed_by_action_vm: not_executed,
            max_cycles: cases.iter().map(|case| case.cycles).max().unwrap_or_default(),
            load_witness_calls: cases.iter().map(|case| case.syscall_trace.load_witness_calls).sum(),
            load_cell_data_calls: cases.iter().map(|case| case.syscall_trace.load_cell_data_calls).sum(),
            load_header_by_field_calls: cases.iter().map(|case| case.syscall_trace.load_header_by_field_calls).sum(),
            originate_action_exercised: cases.iter().any(|case| case.action == ActionKind::Originate),
            repay_action_exercised: cases.iter().any(|case| case.action == ActionKind::Repay),
            claim_action_exercised: cases.iter().any(|case| case.action == ActionKind::Claim),
            time_guards_reject: cases.iter().any(|case| {
                matches!(case.variant, "repay_after_expiry" | "claim_before_expiry") && !case.accepted && case.matched_expected
            }),
            party_guards_reject: cases.iter().any(|case| {
                matches!(case.variant, "repay_wrong_party" | "claim_wrong_party") && !case.accepted && case.matched_expected
            }),
            nonce_guard_rejects: cases
                .iter()
                .any(|case| case.variant == "repay_nonce_mismatch" && !case.accepted && case.matched_expected),
            latest_receipt_hash_guard_rejects: cases
                .iter()
                .any(|case| case.variant == "repay_latest_receipt_hash_mismatch" && !case.accepted && case.matched_expected),
            preserve_field_guard_rejects: cases
                .iter()
                .any(|case| case.variant == "repay_preserved_principal_mutation" && !case.accepted && case.matched_expected),
        },
        cases,
        limits: vec![
            "Executes the three compiled Agreement Profile action ELFs in ckb-vm with harnessed LOAD_WITNESS, LOAD_CELL_DATA, and LOAD_HEADER_BY_FIELD syscalls.",
            "This is action/type-script evidence only; it does not run ckb-verification or a full resolved transaction.",
            "Typed payout, terms_hash, and receipt_hash output bindings are executed at action/type scope; native CKB capacity remains transaction-layer evidence.",
            "Fixed-width wallet signing vectors are generated by scripts/novaseal_wallet_signing_vectors.py; this legacy action harness does not consume wallet UX artefacts.",
            "Cryptographic borrower/lender authority locks are still not implemented in this Agreement Profile slice.",
            "The transaction context is deterministic harness data, not live-chain RPC, mempool, miner, or deployment evidence.",
        ],
    }
}

fn encode_terms(fields: &AgreementFields) -> Vec<u8> {
    let mut out = Vec::with_capacity(TERMS_LEN);
    push_u16(&mut out, AGREEMENT_VERSION);
    push_hash(&mut out, &fields.agreement_id);
    push_hash(&mut out, &fields.terms_hash);
    push_hash(&mut out, &fields.borrower_authority_hash);
    push_hash(&mut out, &fields.lender_authority_hash);
    out.push(fields.collateral_asset_kind);
    push_hash(&mut out, &fields.collateral_asset_hash);
    push_u64(&mut out, fields.collateral_amount);
    out.push(fields.principal_asset_kind);
    push_hash(&mut out, &fields.principal_asset_hash);
    push_u64(&mut out, fields.principal_amount);
    push_u64(&mut out, fields.fixed_fee_amount);
    push_u64(&mut out, fields.start_timepoint);
    push_u64(&mut out, fields.expiry_timepoint);
    out.push(fields.early_close_policy);
    debug_assert_eq!(out.len(), TERMS_LEN);
    out
}

fn encode_agreement_cell(fields: &AgreementFields, status: u8, latest_receipt_hash: [u8; 32], nonce: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(AGREEMENT_CELL_LEN);
    push_u16(&mut out, AGREEMENT_VERSION);
    push_hash(&mut out, &fields.agreement_id);
    push_hash(&mut out, &fields.terms_hash);
    push_hash(&mut out, &fields.borrower_authority_hash);
    push_hash(&mut out, &fields.lender_authority_hash);
    out.push(fields.collateral_asset_kind);
    push_hash(&mut out, &fields.collateral_asset_hash);
    push_u64(&mut out, fields.collateral_amount);
    out.push(fields.principal_asset_kind);
    push_hash(&mut out, &fields.principal_asset_hash);
    push_u64(&mut out, fields.principal_amount);
    push_u64(&mut out, fields.fixed_fee_amount);
    push_u64(&mut out, fields.expiry_timepoint);
    out.push(status);
    push_hash(&mut out, &latest_receipt_hash);
    push_u64(&mut out, nonce);
    debug_assert_eq!(out.len(), AGREEMENT_CELL_LEN);
    out
}

#[allow(clippy::too_many_arguments)]
fn encode_receipt(
    fields: &AgreementFields,
    action: u8,
    old_status: u8,
    new_status: u8,
    terminal_amount: u64,
    old_latest_receipt_hash: [u8; 32],
    new_latest_receipt_hash: [u8; 32],
    nonce: u64,
    timepoint: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(AGREEMENT_RECEIPT_LEN);
    out.push(action);
    push_hash(&mut out, &fields.agreement_id);
    out.push(old_status);
    out.push(new_status);
    push_hash(&mut out, &fields.terms_hash);
    push_hash(&mut out, &fields.borrower_authority_hash);
    push_hash(&mut out, &fields.lender_authority_hash);
    push_u64(&mut out, fields.collateral_amount);
    push_u64(&mut out, fields.principal_amount);
    push_u64(&mut out, fields.fixed_fee_amount);
    push_u64(&mut out, terminal_amount);
    push_hash(&mut out, &old_latest_receipt_hash);
    push_hash(&mut out, &new_latest_receipt_hash);
    push_u64(&mut out, nonce);
    push_u64(&mut out, timepoint);
    debug_assert_eq!(out.len(), AGREEMENT_RECEIPT_LEN);
    out
}

fn encode_payout(
    fields: &AgreementFields,
    action: u8,
    role: u8,
    recipient: [u8; 32],
    asset_kind: u8,
    asset_hash: [u8; 32],
    amount: u64,
    nonce: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(PAYOUT_LEN);
    out.push(action);
    push_hash(&mut out, &fields.agreement_id);
    out.push(role);
    push_hash(&mut out, &recipient);
    out.push(asset_kind);
    push_hash(&mut out, &asset_hash);
    push_u64(&mut out, amount);
    push_hash(&mut out, &fields.terms_hash);
    push_u64(&mut out, nonce);
    debug_assert_eq!(out.len(), PAYOUT_LEN);
    out
}

fn build_originate_witness(terms: &[u8], originator: &[u8; 32], receipt_hash: &[u8; 32]) -> Vec<u8> {
    let mut witness = Vec::with_capacity(LOCK_WITNESS_MAGIC.len() + 4 + terms.len() + originator.len() + receipt_hash.len());
    witness.extend_from_slice(LOCK_WITNESS_MAGIC);
    witness.extend_from_slice(&(terms.len() as u32).to_le_bytes());
    witness.extend_from_slice(terms);
    witness.extend_from_slice(originator);
    witness.extend_from_slice(receipt_hash);
    witness
}

fn build_terminal_witness(actor: &[u8; 32], receipt_hash: &[u8; 32]) -> Vec<u8> {
    let mut witness = Vec::with_capacity(LOCK_WITNESS_MAGIC.len() + actor.len() + receipt_hash.len());
    witness.extend_from_slice(LOCK_WITNESS_MAGIC);
    witness.extend_from_slice(actor);
    witness.extend_from_slice(receipt_hash);
    witness
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_hash(out: &mut Vec<u8>, value: &[u8; 32]) {
    out.extend_from_slice(value);
}

fn overwrite_u64(out: &mut [u8], offset: usize, value: u64) {
    out[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn load_fixture_expectations(fixtures_dir: &Path) -> Result<BTreeMap<String, String>, HarnessError> {
    let mut expectations = BTreeMap::new();
    for entry in fs::read_dir(fixtures_dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let value: Value = serde_json::from_slice(&fs::read(entry.path())?)?;
        let fixture = value
            .get("fixture")
            .and_then(Value::as_str)
            .ok_or_else(|| HarnessError::Message(format!("{} missing fixture", entry.path().display())))?;
        let expected = value
            .get("expected")
            .and_then(Value::as_str)
            .ok_or_else(|| HarnessError::Message(format!("{fixture} missing expected")))?;
        if !matches!(expected, "accepted" | "rejected") {
            return Err(HarnessError::Message(format!("{fixture} has unsupported expected value {expected}")));
        }
        expectations.insert(fixture.to_string(), expected.to_string());
    }
    Ok(expectations)
}

fn ensure_fixture_coverage(cases: &[AgreementCase], fixture_expectations: &BTreeMap<String, String>) -> Result<(), HarnessError> {
    for case in cases {
        let Some(expected) = fixture_expectations.get(case.fixture) else {
            return Err(HarnessError::Message(format!("missing fixture JSON for {}", case.fixture)));
        };
        if expected != case.expected {
            return Err(HarnessError::Message(format!(
                "{} expects {expected} in fixture JSON but harness case '{}' expects {}",
                case.fixture, case.variant, case.expected
            )));
        }
    }
    Ok(())
}

fn elf_report(path: &Path, bytes: &[u8]) -> ElfReport {
    ElfReport { path: path.display().to_string(), size_bytes: bytes.len(), sha256: sha256_hex(bytes) }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("writing to string cannot fail");
    }
    out
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
        "summary: action_vm_executed={} total={} accepted={} rejected={} matched_expected={} mismatched={} max_cycles={}",
        report.summary.action_ckb_vm_executed,
        report.summary.total_cases,
        report.summary.accepted,
        report.summary.rejected,
        report.summary.matched_expected,
        report.summary.mismatched,
        report.summary.max_cycles
    );
}
