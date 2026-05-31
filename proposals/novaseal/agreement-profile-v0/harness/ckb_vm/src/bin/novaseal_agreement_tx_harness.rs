#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::struct_excessive_bools, clippy::too_many_lines)]

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use blake2b_simd::Params as Blake2bParams;
use ckb_chain_spec::consensus::{Consensus, ConsensusBuilder};
use ckb_script::{TransactionScriptsVerifier, TxVerifyEnv};
use ckb_traits::{CellDataProvider, EpochProvider, ExtensionProvider, HeaderFields, HeaderFieldsProvider, HeaderProvider};
use ckb_types::{
    bytes::Bytes as CkbBytes,
    core::{
        BlockExt, BlockNumber, Capacity, DepType, EpochExt, EpochNumberWithFraction, HeaderBuilder, ScriptHashType, TransactionView,
        cell::{CellMetaBuilder, ResolvedTransaction},
        hardfork::{CKB2021, CKB2023, HardForks},
    },
    packed,
    prelude::*,
};
use ckb_verification::{ContextualTransactionVerifier, NonContextualTransactionVerifier};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

const DEFAULT_ORIGINATE_ELF: &str = "target/nova-agreement-originate-action.elf";
const DEFAULT_REPAY_ELF: &str = "target/nova-agreement-repay-action.elf";
const DEFAULT_CLAIM_ELF: &str = "target/nova-agreement-claim-action.elf";
const DEFAULT_LOCK_ELF: &str = "target/nova-agreement-always-success-lock.elf";
const DEFAULT_FIXTURES_DIR: &str = "fixtures";
const DEFAULT_OUTPUT: &str = "target/nova-agreement-ckb-tx-report.json";
const EXPECTED_TX_HARNESS_LIMIT_FIXTURES: &[&str] = &[];

const VERIFY_MAX_CYCLES: u64 = 800_000_000;
const VM2_ENABLED_EPOCH: u64 = 10;
const CKB_BLAKE2B_PERSONAL: &[u8; 16] = b"ckb-default-hash";

const CKB: u64 = 100_000_000;
const COLLATERAL_AMOUNT: u64 = 1_000 * CKB;
const PRINCIPAL_AMOUNT: u64 = 700 * CKB;
const FIXED_FEE_AMOUNT: u64 = 30 * CKB;
const AGREEMENT_OCCUPIED_CAPACITY: u64 = 40 * CKB;
const RECEIPT_OCCUPIED_CAPACITY: u64 = 20 * CKB;
const PAYOUT_OCCUPIED_CAPACITY: u64 = 40 * CKB;
const BUILDER_FEE_SHANNONS: u64 = 100_000;
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

const LOCK_WITNESS_MAGIC: &[u8; 8] = b"CSARGv1\0";
const ZERO_HASH: [u8; 32] = [0x00; 32];
const AGREEMENT_ID: [u8; 32] = [0xaa; 32];
const TERMS_HASH: [u8; 32] = [0xbb; 32];
const BORROWER_AUTHORITY: [u8; 32] = [0x11; 32];
const LENDER_AUTHORITY: [u8; 32] = [0x22; 32];
const STRANGER_AUTHORITY: [u8; 32] = [0x33; 32];
const OLD_RECEIPT_ROOT: [u8; 32] = [0x44; 32];
const NEW_RECEIPT_ROOT: [u8; 32] = [0x55; 32];
const OTHER_RECEIPT_ROOT: [u8; 32] = [0x66; 32];
const OTHER_TERMS_HASH: [u8; 32] = [0xcc; 32];

#[derive(Debug, Error)]
enum HarnessError {
    #[error(
        "usage: novaseal_agreement_tx_harness [--originate-elf PATH] [--repay-elf PATH] [--claim-elf PATH] [--lock-elf PATH] [--fixtures-dir PATH] [--output PATH] [--pretty]"
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
    lock_elf: PathBuf,
    fixtures_dir: PathBuf,
    output: PathBuf,
    pretty: bool,
}

#[derive(Clone, Debug, Default)]
struct HarnessDataLoader {
    cells: BTreeMap<packed::OutPoint, CkbBytes>,
    headers: BTreeMap<packed::Byte32, ckb_types::core::HeaderView>,
}

impl HarnessDataLoader {
    fn insert_cell(&mut self, out_point: packed::OutPoint, data: CkbBytes) {
        self.cells.insert(out_point, data);
    }

    fn insert_header(&mut self, header: ckb_types::core::HeaderView) -> packed::Byte32 {
        let hash = header.hash();
        self.headers.insert(hash.clone(), header);
        hash
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
    fn get_header(&self, hash: &packed::Byte32) -> Option<ckb_types::core::HeaderView> {
        self.headers.get(hash).cloned()
    }
}

impl HeaderFieldsProvider for HarnessDataLoader {
    fn get_header_fields(&self, hash: &packed::Byte32) -> Option<HeaderFields> {
        self.headers.get(hash).map(|header| HeaderFields {
            hash: hash.clone(),
            number: header.number(),
            epoch: header.epoch(),
            timestamp: header.timestamp(),
            parent_hash: header.parent_hash(),
        })
    }
}

impl EpochProvider for HarnessDataLoader {
    fn get_epoch_ext(&self, _block_header: &ckb_types::core::HeaderView) -> Option<EpochExt> {
        None
    }

    fn get_block_hash(&self, _number: BlockNumber) -> Option<packed::Byte32> {
        None
    }

    fn get_block_ext(&self, _block_hash: &packed::Byte32) -> Option<BlockExt> {
        None
    }

    fn get_block_header(&self, hash: &packed::Byte32) -> Option<ckb_types::core::HeaderView> {
        self.get_header(hash)
    }
}

impl ExtensionProvider for HarnessDataLoader {
    fn get_block_extension(&self, _hash: &packed::Byte32) -> Option<packed::Bytes> {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ActionKind {
    Originate,
    Repay,
    Claim,
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
    active_cell_data: Option<Vec<u8>>,
    agreement_output_data: Vec<u8>,
    receipt_output_data: Vec<u8>,
    payout_outputs: Vec<PayoutOutput>,
    under_capacity_agreement_output: bool,
}

#[derive(Clone, Debug)]
struct PayoutOutput {
    role: &'static str,
    capacity: u64,
    data: Vec<u8>,
}

#[derive(Debug)]
struct TransactionContext {
    transaction: packed::Transaction,
    transaction_view: TransactionView,
    resolved_transaction: ResolvedTransaction,
    data_loader: HarnessDataLoader,
    input_capacity: u64,
    output_capacity: u64,
    output_occupied_capacity: u64,
    fee_shannons: u64,
    script_group_count: usize,
    lock_group_present: bool,
    type_group_present: bool,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    classification: &'static str,
    action_elves: BTreeMap<&'static str, ElfReport>,
    lock_elf: ElfReport,
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
    resolved_transaction_harness_executed: bool,
    ckb_script_verifier_executed: bool,
    ckb_node_verification_stack_executed: bool,
    total_cases: usize,
    expected_accept: usize,
    expected_reject: usize,
    script_accepted: usize,
    script_rejected: usize,
    script_matched_expected: usize,
    script_mismatched: usize,
    node_accepted: usize,
    node_rejected: usize,
    node_matched_expected: usize,
    node_mismatched: usize,
    fixtures_covered: Vec<&'static str>,
    fixture_files_not_executed_by_tx_harness: Vec<String>,
    expected_fixture_files_not_executed_by_tx_harness: Vec<&'static str>,
    fixture_partition_matches_expected: bool,
    all_lock_and_type_groups_present: bool,
    non_contextual_checks_passed: bool,
    contextual_checks_match_expected: bool,
    builder_shape_checks_passed: bool,
    fee_shape_checks_passed: bool,
    max_script_cycles: u64,
    max_node_cycles: u64,
    max_tx_size_bytes: usize,
    max_output_occupied_capacity_shannons: u64,
}

#[derive(Debug, Serialize)]
struct CaseReport {
    fixture: &'static str,
    variant: &'static str,
    action: ActionKind,
    expected: &'static str,
    script_layer_expected: &'static str,
    expected_reason: &'static str,
    script_accepted: bool,
    script_error: Option<String>,
    script_cycles: Option<u64>,
    script_matched_expected: bool,
    node_verifier: NodeVerifierReport,
    transaction_hash: String,
    tx_size_bytes: usize,
    witness_size_bytes: usize,
    input_capacity_shannons: u64,
    output_capacity_shannons: u64,
    fee_shannons: u64,
    output_occupied_capacity_shannons: u64,
    output_capacity_covers_occupied_capacity: bool,
    payout_roles: Vec<&'static str>,
    script_group_count: usize,
    lock_group_present: bool,
    type_group_present: bool,
}

#[derive(Debug, Serialize)]
struct NodeVerifierReport {
    non_contextual_verified: bool,
    non_contextual_error: Option<String>,
    contextual_verified: bool,
    contextual_error: Option<String>,
    accepted: bool,
    matched_expected: bool,
    cycles: Option<u64>,
    fee_shannons: Option<u64>,
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
    let lock_elf = fs::read(&args.lock_elf)?;
    let fixture_expectations = load_fixture_expectations(&args.fixtures_dir)?;
    let cases = build_cases();
    ensure_fixture_coverage(&cases, &fixture_expectations)?;
    ensure_fixture_partition(&cases, &fixture_expectations)?;
    let reports = cases
        .iter()
        .map(|case| {
            let action_elf = match case.action {
                ActionKind::Originate => originate_elf.as_slice(),
                ActionKind::Repay => repay_elf.as_slice(),
                ActionKind::Claim => claim_elf.as_slice(),
            };
            run_case(action_elf, &lock_elf, case)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let report = build_report(&args, &originate_elf, &repay_elf, &claim_elf, &lock_elf, reports, &fixture_expectations);
    write_report(&args.output, &report, args.pretty)?;
    print_summary(&args.output, &report);
    if report.summary.script_mismatched == 0 && report.summary.node_mismatched == 0 {
        Ok(())
    } else {
        Err(HarnessError::Message(format!(
            "{} script verifier case(s) mismatched; {} node verifier case(s) mismatched",
            report.summary.script_mismatched, report.summary.node_mismatched
        )))
    }
}

fn parse_args() -> Result<Args, HarnessError> {
    let mut args = Args {
        originate_elf: PathBuf::from(DEFAULT_ORIGINATE_ELF),
        repay_elf: PathBuf::from(DEFAULT_REPAY_ELF),
        claim_elf: PathBuf::from(DEFAULT_CLAIM_ELF),
        lock_elf: PathBuf::from(DEFAULT_LOCK_ELF),
        fixtures_dir: PathBuf::from(DEFAULT_FIXTURES_DIR),
        output: PathBuf::from(DEFAULT_OUTPUT),
        pretty: false,
    };
    let mut raw = env::args().skip(1);
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--originate-elf" => args.originate_elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--repay-elf" => args.repay_elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--claim-elf" => args.claim_elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--lock-elf" => args.lock_elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--fixtures-dir" => args.fixtures_dir = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--output" => args.output = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--pretty" => args.pretty = true,
            "-h" | "--help" => return Err(HarnessError::Usage),
            _ => return Err(HarnessError::Message(format!("unknown argument: {arg}"))),
        }
    }
    Ok(args)
}

fn build_cases() -> Vec<AgreementCase> {
    let fields = AgreementFields::default();
    let terms = encode_terms(&fields);
    let active = encode_agreement_cell(&fields, STATUS_ACTIVE, OLD_RECEIPT_ROOT, 0);
    let originated = encode_agreement_cell(&fields, STATUS_ACTIVE, NEW_RECEIPT_ROOT, 0);
    let repaid = encode_agreement_cell(&fields, STATUS_REPAID, NEW_RECEIPT_ROOT, 1);
    let defaulted = encode_agreement_cell(&fields, STATUS_DEFAULTED, NEW_RECEIPT_ROOT, 1);
    let mut bad_terms = fields.clone();
    bad_terms.terms_hash = OTHER_TERMS_HASH;
    let bad_terms_bytes = encode_terms(&bad_terms);
    let originate_receipt =
        encode_receipt(&fields, PATH_ORIGINATE, STATUS_OFFERED, STATUS_ACTIVE, 0, ZERO_HASH, NEW_RECEIPT_ROOT, 0, 120);
    let originate_payout = encode_payout(
        &fields,
        PATH_ORIGINATE,
        PAYOUT_BORROWER_PRINCIPAL,
        BORROWER_AUTHORITY,
        ASSET_KIND_CKB,
        ZERO_HASH,
        PRINCIPAL_AMOUNT,
        0,
    );
    let repay_receipt = encode_receipt(
        &fields,
        PATH_REPAY_BEFORE_EXPIRY,
        STATUS_ACTIVE,
        STATUS_REPAID,
        PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
        OLD_RECEIPT_ROOT,
        NEW_RECEIPT_ROOT,
        1,
        180,
    );
    let lender_repayment = encode_payout(
        &fields,
        PATH_REPAY_BEFORE_EXPIRY,
        PAYOUT_LENDER_REPAYMENT,
        LENDER_AUTHORITY,
        ASSET_KIND_CKB,
        ZERO_HASH,
        PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
        1,
    );
    let borrower_collateral_return = encode_payout(
        &fields,
        PATH_REPAY_BEFORE_EXPIRY,
        PAYOUT_BORROWER_COLLATERAL_RETURN,
        BORROWER_AUTHORITY,
        ASSET_KIND_CKB,
        ZERO_HASH,
        COLLATERAL_AMOUNT,
        1,
    );
    let claim_receipt = encode_receipt(
        &fields,
        PATH_CLAIM_AFTER_EXPIRY,
        STATUS_ACTIVE,
        STATUS_DEFAULTED,
        COLLATERAL_AMOUNT,
        OLD_RECEIPT_ROOT,
        NEW_RECEIPT_ROOT,
        1,
        220,
    );
    let lender_default_claim = encode_payout(
        &fields,
        PATH_CLAIM_AFTER_EXPIRY,
        PAYOUT_LENDER_DEFAULT_CLAIM,
        LENDER_AUTHORITY,
        ASSET_KIND_CKB,
        ZERO_HASH,
        COLLATERAL_AMOUNT,
        1,
    );
    let mut bad_nonce = repaid.clone();
    overwrite_u64(&mut bad_nonce, 261, 2);
    let mut mutated_principal = repaid.clone();
    overwrite_u64(&mut mutated_principal, 204, PRINCIPAL_AMOUNT - CKB);
    let receipt_root_mismatch = encode_agreement_cell(&fields, STATUS_REPAID, OTHER_RECEIPT_ROOT, 1);
    let receipt_output_mismatch = encode_receipt(
        &fields,
        PATH_REPAY_BEFORE_EXPIRY,
        STATUS_ACTIVE,
        STATUS_REPAID,
        PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
        OLD_RECEIPT_ROOT,
        OTHER_RECEIPT_ROOT,
        1,
        180,
    );
    let mut wrong_repayment_amount = lender_repayment.clone();
    overwrite_u64(&mut wrong_repayment_amount, 99, PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT - CKB);

    vec![
        AgreementCase {
            fixture: "originate_valid",
            variant: "originate_valid",
            action: ActionKind::Originate,
            expected: "accepted",
            expected_reason: "valid origination transaction with agreement, receipt, and principal payout outputs",
            current_timepoint: 120,
            witness: build_originate_witness(&terms, &BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: None,
            agreement_output_data: originated.clone(),
            receipt_output_data: originate_receipt.clone(),
            payout_outputs: vec![PayoutOutput {
                role: "borrower_principal_payout",
                capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT,
                data: originate_payout.clone(),
            }],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "wrong_terms_hash_reject",
            variant: "originate_wrong_terms_hash",
            action: ActionKind::Originate,
            expected: "rejected",
            expected_reason: "created agreement, payout, and receipt outputs must bind the witness terms_hash",
            current_timepoint: 120,
            witness: build_originate_witness(&bad_terms_bytes, &BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: None,
            agreement_output_data: originated.clone(),
            receipt_output_data: originate_receipt.clone(),
            payout_outputs: vec![PayoutOutput {
                role: "borrower_principal_payout",
                capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT,
                data: originate_payout.clone(),
            }],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "wrong_originator_reject",
            variant: "originate_wrong_originator",
            action: ActionKind::Originate,
            expected: "rejected",
            expected_reason: "originator_authority_hash must equal borrower_authority_hash",
            current_timepoint: 120,
            witness: build_originate_witness(&terms, &STRANGER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: None,
            agreement_output_data: originated,
            receipt_output_data: originate_receipt,
            payout_outputs: vec![PayoutOutput {
                role: "borrower_principal_payout",
                capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT,
                data: originate_payout,
            }],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "repay_before_expiry_valid",
            variant: "repay_valid",
            action: ActionKind::Repay,
            expected: "accepted",
            expected_reason: "valid repayment transaction before expiry",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: repaid.clone(),
            receipt_output_data: repay_receipt.clone(),
            payout_outputs: vec![
                PayoutOutput {
                    role: "lender_repayment",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                    data: lender_repayment.clone(),
                },
                PayoutOutput {
                    role: "borrower_collateral_return",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                    data: borrower_collateral_return.clone(),
                },
            ],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "expired_repay_reject",
            variant: "repay_after_expiry",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "repayment after expiry must reject",
            current_timepoint: 220,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: repaid.clone(),
            receipt_output_data: repay_receipt.clone(),
            payout_outputs: vec![
                PayoutOutput {
                    role: "lender_repayment",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                    data: lender_repayment.clone(),
                },
                PayoutOutput {
                    role: "borrower_collateral_return",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                    data: borrower_collateral_return.clone(),
                },
            ],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "wrong_party_reject",
            variant: "repay_wrong_party",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "repayment actor must be borrower",
            current_timepoint: 180,
            witness: build_terminal_witness(&STRANGER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: repaid.clone(),
            receipt_output_data: repay_receipt.clone(),
            payout_outputs: vec![
                PayoutOutput {
                    role: "lender_repayment",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                    data: lender_repayment.clone(),
                },
                PayoutOutput {
                    role: "borrower_collateral_return",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                    data: borrower_collateral_return.clone(),
                },
            ],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "nonce_mismatch_reject",
            variant: "repay_nonce_mismatch",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "closed nonce must equal active nonce plus one",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: bad_nonce,
            receipt_output_data: repay_receipt.clone(),
            payout_outputs: vec![
                PayoutOutput {
                    role: "lender_repayment",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                    data: lender_repayment.clone(),
                },
                PayoutOutput {
                    role: "borrower_collateral_return",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                    data: borrower_collateral_return.clone(),
                },
            ],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "preserved_field_mutation_reject",
            variant: "repay_preserved_principal_mutation",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "preserved agreement fields cannot mutate on terminal transition",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: mutated_principal,
            receipt_output_data: repay_receipt.clone(),
            payout_outputs: vec![
                PayoutOutput {
                    role: "lender_repayment",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                    data: lender_repayment.clone(),
                },
                PayoutOutput {
                    role: "borrower_collateral_return",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                    data: borrower_collateral_return.clone(),
                },
            ],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "receipt_root_mismatch_reject",
            variant: "repay_receipt_root_mismatch",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "closed receipt_root must equal witness receipt_hash",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: receipt_root_mismatch,
            receipt_output_data: repay_receipt.clone(),
            payout_outputs: vec![
                PayoutOutput {
                    role: "lender_repayment",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                    data: lender_repayment.clone(),
                },
                PayoutOutput {
                    role: "borrower_collateral_return",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                    data: borrower_collateral_return.clone(),
                },
            ],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "receipt_hash_mismatch_reject",
            variant: "repay_receipt_output_mismatch",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "materialized receipt output must bind the witness receipt_hash",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: repaid.clone(),
            receipt_output_data: receipt_output_mismatch,
            payout_outputs: vec![
                PayoutOutput {
                    role: "lender_repayment",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                    data: lender_repayment.clone(),
                },
                PayoutOutput {
                    role: "borrower_collateral_return",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                    data: borrower_collateral_return.clone(),
                },
            ],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "wrong_settlement_amount_reject",
            variant: "repay_wrong_settlement_amount",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "typed lender repayment output must equal principal plus fixed fee",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: repaid.clone(),
            receipt_output_data: repay_receipt.clone(),
            payout_outputs: vec![
                PayoutOutput {
                    role: "lender_repayment",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                    data: wrong_repayment_amount,
                },
                PayoutOutput {
                    role: "borrower_collateral_return",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                    data: borrower_collateral_return.clone(),
                },
            ],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "claim_after_expiry_valid",
            variant: "claim_valid",
            action: ActionKind::Claim,
            expected: "accepted",
            expected_reason: "valid default claim after expiry",
            current_timepoint: 220,
            witness: build_terminal_witness(&LENDER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: defaulted.clone(),
            receipt_output_data: claim_receipt.clone(),
            payout_outputs: vec![PayoutOutput {
                role: "lender_default_claim",
                capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                data: lender_default_claim.clone(),
            }],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "early_claim_reject",
            variant: "claim_before_expiry",
            action: ActionKind::Claim,
            expected: "rejected",
            expected_reason: "claim before expiry must reject",
            current_timepoint: 180,
            witness: build_terminal_witness(&LENDER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: defaulted.clone(),
            receipt_output_data: claim_receipt.clone(),
            payout_outputs: vec![PayoutOutput {
                role: "lender_default_claim",
                capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                data: lender_default_claim.clone(),
            }],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "wrong_party_reject",
            variant: "claim_wrong_party",
            action: ActionKind::Claim,
            expected: "rejected",
            expected_reason: "claim actor must be lender",
            current_timepoint: 220,
            witness: build_terminal_witness(&STRANGER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active.clone()),
            agreement_output_data: defaulted.clone(),
            receipt_output_data: claim_receipt.clone(),
            payout_outputs: vec![PayoutOutput {
                role: "lender_default_claim",
                capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                data: lender_default_claim,
            }],
            under_capacity_agreement_output: false,
        },
        AgreementCase {
            fixture: "under_capacity_reject",
            variant: "repay_under_capacity",
            action: ActionKind::Repay,
            expected: "rejected",
            expected_reason: "agreement output below occupied capacity must be rejected by transaction validation",
            current_timepoint: 180,
            witness: build_terminal_witness(&BORROWER_AUTHORITY, &NEW_RECEIPT_ROOT),
            active_cell_data: Some(active),
            agreement_output_data: repaid,
            receipt_output_data: encode_receipt(
                &fields,
                PATH_REPAY_BEFORE_EXPIRY,
                STATUS_ACTIVE,
                STATUS_REPAID,
                PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                OLD_RECEIPT_ROOT,
                NEW_RECEIPT_ROOT,
                1,
                180,
            ),
            payout_outputs: vec![
                PayoutOutput {
                    role: "lender_repayment",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                    data: lender_repayment.clone(),
                },
                PayoutOutput {
                    role: "borrower_collateral_return",
                    capacity: PAYOUT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT,
                    data: borrower_collateral_return,
                },
            ],
            under_capacity_agreement_output: true,
        },
    ]
}

fn run_case(action_elf: &[u8], lock_elf: &[u8], case: &AgreementCase) -> Result<CaseReport, HarnessError> {
    let context = build_transaction_context(action_elf, lock_elf, case)?;
    let consensus = Arc::new(resolved_script_consensus());
    let header = HeaderBuilder::default()
        .epoch(EpochNumberWithFraction::new(case.current_timepoint.max(VM2_ENABLED_EPOCH), 0, 1).pack())
        .build();
    let tx_env = Arc::new(TxVerifyEnv::new_commit(&header));
    let verifier = TransactionScriptsVerifier::new(
        Arc::new(context.resolved_transaction.clone()),
        context.data_loader.clone(),
        Arc::clone(&consensus),
        Arc::clone(&tx_env),
    );
    let script_result = verifier.verify(VERIFY_MAX_CYCLES);
    let (script_accepted, script_cycles, script_error) = match script_result {
        Ok(cycles) => (true, Some(cycles), None),
        Err(error) => (false, None, Some(format!("{error}"))),
    };
    let script_layer_expected = script_layer_expected(case);
    let script_matched_expected = expected_matches(script_layer_expected, script_accepted)?;
    let node_verifier = run_node_verifier(&context, Arc::clone(&consensus), Arc::clone(&tx_env), case.expected);
    Ok(CaseReport {
        fixture: case.fixture,
        variant: case.variant,
        action: case.action,
        expected: case.expected,
        script_layer_expected,
        expected_reason: case.expected_reason,
        script_accepted,
        script_error,
        script_cycles,
        script_matched_expected,
        node_verifier,
        transaction_hash: hex0x(context.transaction_view.hash().as_slice()),
        tx_size_bytes: context.transaction.as_bytes().len(),
        witness_size_bytes: case.witness.len(),
        input_capacity_shannons: context.input_capacity,
        output_capacity_shannons: context.output_capacity,
        fee_shannons: context.fee_shannons,
        output_occupied_capacity_shannons: context.output_occupied_capacity,
        output_capacity_covers_occupied_capacity: context.output_capacity >= context.output_occupied_capacity,
        payout_roles: case.payout_outputs.iter().map(|payout| payout.role).collect(),
        script_group_count: context.script_group_count,
        lock_group_present: context.lock_group_present,
        type_group_present: context.type_group_present,
    })
}

fn run_node_verifier(
    context: &TransactionContext,
    consensus: Arc<Consensus>,
    tx_env: Arc<TxVerifyEnv>,
    expected: &str,
) -> NodeVerifierReport {
    let non_contextual = NonContextualTransactionVerifier::new(&context.transaction_view, consensus.as_ref()).verify();
    let (non_contextual_verified, non_contextual_error) = match non_contextual {
        Ok(()) => (true, None),
        Err(error) => (false, Some(format!("{error}"))),
    };
    let (contextual_verified, contextual_error, cycles, fee_shannons) = if non_contextual_verified {
        match ContextualTransactionVerifier::new(
            Arc::new(context.resolved_transaction.clone()),
            consensus,
            context.data_loader.clone(),
            tx_env,
        )
        .verify(VERIFY_MAX_CYCLES, false)
        {
            Ok(completed) => (true, None, Some(completed.cycles), Some(completed.fee.as_u64())),
            Err(error) => (false, Some(format!("{error}")), None, None),
        }
    } else {
        (false, Some("skipped contextual verification after non-contextual failure".to_string()), None, None)
    };
    let accepted = non_contextual_verified && contextual_verified;
    let matched_expected = expected_matches(expected, accepted).unwrap_or(false);
    NodeVerifierReport {
        non_contextual_verified,
        non_contextual_error,
        contextual_verified,
        contextual_error,
        accepted,
        matched_expected,
        cycles,
        fee_shannons,
    }
}

fn build_transaction_context(action_elf: &[u8], lock_elf: &[u8], case: &AgreementCase) -> Result<TransactionContext, HarnessError> {
    let action_code_hash = ckb_blake2b256(action_elf);
    let lock_code_hash = ckb_blake2b256(lock_elf);
    let type_script = build_data1_script(&action_code_hash);
    let lock_script = build_data1_script(&lock_code_hash);

    let action_dep_out_point = build_out_point(&action_code_hash, 0);
    let lock_dep_out_point = build_out_point(&lock_code_hash, 0);
    let mut cell_deps =
        vec![build_cell_dep_from_out_point(action_dep_out_point.clone()), build_cell_dep_from_out_point(lock_dep_out_point.clone())];

    let header = HeaderBuilder::default()
        .epoch(EpochNumberWithFraction::new(case.current_timepoint.max(VM2_ENABLED_EPOCH), 0, 1).pack())
        .build();
    let mut data_loader = HarnessDataLoader::default();
    let header_hash = data_loader.insert_header(header);

    let mut inputs = Vec::new();
    let mut resolved_inputs = Vec::new();
    let mut input_capacity = 0u64;

    if let Some(active_cell_data) = &case.active_cell_data {
        let active_out_point = build_out_point(&ckb_blake2b256(case.variant.as_bytes()), 0);
        let active_capacity = AGREEMENT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT;
        let active_output = packed::CellOutput::new_builder()
            .capacity(Capacity::shannons(active_capacity).pack())
            .lock(lock_script.clone())
            .type_(Some(type_script.clone()).pack())
            .build();
        let active_bytes = CkbBytes::copy_from_slice(active_cell_data);
        data_loader.insert_cell(active_out_point.clone(), active_bytes.clone());
        inputs.push(packed::CellInput::new_builder().previous_output(active_out_point.clone()).build());
        resolved_inputs.push(CellMetaBuilder::from_cell_output(active_output, active_bytes).out_point(active_out_point).build());
        input_capacity =
            input_capacity.checked_add(active_capacity).ok_or_else(|| HarnessError::Message("input capacity overflow".to_string()))?;
    }

    let agreement_output_without_capacity =
        packed::CellOutput::new_builder().lock(lock_script.clone()).type_(Some(type_script).pack()).build();
    let receipt_output_without_capacity = packed::CellOutput::new_builder().lock(lock_script.clone()).build();
    let agreement_occupied = agreement_output_without_capacity
        .occupied_capacity(capacity_bytes(case.agreement_output_data.len())?)
        .map_err(|error| HarnessError::Message(format!("failed to compute agreement output occupied capacity: {error}")))?
        .as_u64()
        .max(AGREEMENT_OCCUPIED_CAPACITY);
    let receipt_occupied = receipt_output_without_capacity
        .occupied_capacity(capacity_bytes(case.receipt_output_data.len())?)
        .map_err(|error| HarnessError::Message(format!("failed to compute receipt output occupied capacity: {error}")))?
        .as_u64()
        .max(RECEIPT_OCCUPIED_CAPACITY);
    let agreement_capacity = if case.under_capacity_agreement_output {
        agreement_occupied.saturating_sub(1)
    } else if case.action == ActionKind::Originate {
        agreement_occupied + COLLATERAL_AMOUNT
    } else {
        agreement_occupied
    };
    let receipt_capacity = receipt_occupied;

    let mut outputs =
        vec![agreement_output_without_capacity.as_builder().capacity(Capacity::shannons(agreement_capacity).pack()).build()];
    let mut outputs_data = vec![CkbBytes::from(case.agreement_output_data.clone()).pack()];
    let mut output_capacity = agreement_capacity + receipt_capacity;
    let mut output_occupied_capacity = agreement_occupied + receipt_occupied;

    for payout in &case.payout_outputs {
        let payout_output =
            packed::CellOutput::new_builder().capacity(Capacity::shannons(payout.capacity).pack()).lock(lock_script.clone()).build();
        output_capacity = output_capacity
            .checked_add(payout.capacity)
            .ok_or_else(|| HarnessError::Message("output capacity overflow".to_string()))?;
        output_occupied_capacity = output_occupied_capacity
            .checked_add(PAYOUT_OCCUPIED_CAPACITY)
            .ok_or_else(|| HarnessError::Message("occupied capacity overflow".to_string()))?;
        outputs.push(payout_output);
        outputs_data.push(CkbBytes::from(payout.data.clone()).pack());
    }
    outputs.push(receipt_output_without_capacity.as_builder().capacity(Capacity::shannons(receipt_capacity).pack()).build());
    outputs_data.push(CkbBytes::from(case.receipt_output_data.clone()).pack());

    let required_input_capacity = output_capacity
        .checked_add(BUILDER_FEE_SHANNONS)
        .ok_or_else(|| HarnessError::Message("required input capacity overflow".to_string()))?;
    if input_capacity < required_input_capacity {
        let funding_capacity = required_input_capacity - input_capacity;
        let funding_index = inputs.len() as u32;
        let funding_out_point = build_out_point(&ckb_blake2b256(format!("{}-funding-{funding_index}", case.variant).as_bytes()), 0);
        let funding_output =
            packed::CellOutput::new_builder().capacity(Capacity::shannons(funding_capacity).pack()).lock(lock_script.clone()).build();
        let funding_bytes = CkbBytes::new();
        data_loader.insert_cell(funding_out_point.clone(), funding_bytes.clone());
        inputs.push(packed::CellInput::new_builder().previous_output(funding_out_point.clone()).build());
        resolved_inputs.push(CellMetaBuilder::from_cell_output(funding_output, funding_bytes).out_point(funding_out_point).build());
        input_capacity = input_capacity
            .checked_add(funding_capacity)
            .ok_or_else(|| HarnessError::Message("input capacity overflow".to_string()))?;
    }
    if inputs.is_empty() {
        return Err(HarnessError::Message("transaction harness must construct at least one input".to_string()));
    }

    let witness_count = inputs.len();
    let mut witnesses = Vec::with_capacity(witness_count);
    witnesses.push(CkbBytes::from(case.witness.clone()).pack());
    for _ in 1..witness_count {
        witnesses.push(CkbBytes::new().pack());
    }

    let raw_transaction = packed::RawTransaction::new_builder()
        .version(0u32.pack())
        .cell_deps(std::mem::take(&mut cell_deps).pack())
        .header_deps(vec![header_hash].pack())
        .inputs(inputs.pack())
        .outputs(outputs.pack())
        .outputs_data(outputs_data.pack())
        .build();
    let transaction = packed::Transaction::new_builder().raw(raw_transaction).witnesses(witnesses.pack()).build();
    let transaction_view = transaction.clone().into_view();

    let action_bytes = CkbBytes::copy_from_slice(action_elf);
    let lock_bytes = CkbBytes::copy_from_slice(lock_elf);
    data_loader.insert_cell(action_dep_out_point.clone(), action_bytes.clone());
    data_loader.insert_cell(lock_dep_out_point.clone(), lock_bytes.clone());
    let action_dep_output = code_cell_output(action_elf)?;
    let lock_dep_output = code_cell_output(lock_elf)?;
    let resolved_transaction = ResolvedTransaction {
        transaction: transaction_view.clone(),
        resolved_cell_deps: vec![
            CellMetaBuilder::from_cell_output(action_dep_output, action_bytes).out_point(action_dep_out_point).build(),
            CellMetaBuilder::from_cell_output(lock_dep_output, lock_bytes).out_point(lock_dep_out_point).build(),
        ],
        resolved_inputs,
        resolved_dep_groups: Vec::new(),
    };
    let consensus = Arc::new(resolved_script_consensus());
    let tx_env = Arc::new(TxVerifyEnv::new_commit(
        &HeaderBuilder::default()
            .epoch(EpochNumberWithFraction::new(case.current_timepoint.max(VM2_ENABLED_EPOCH), 0, 1).pack())
            .build(),
    ));
    let verifier = TransactionScriptsVerifier::new(
        Arc::new(resolved_transaction.clone()),
        data_loader.clone(),
        Arc::clone(&consensus),
        Arc::clone(&tx_env),
    );
    let groups = verifier.groups_with_type().collect::<Vec<_>>();
    let lock_group_present = groups.iter().any(|(group_type, _, _)| *group_type == ckb_script::ScriptGroupType::Lock);
    let type_group_present = groups.iter().any(|(group_type, _, _)| *group_type == ckb_script::ScriptGroupType::Type);
    let fee_shannons =
        input_capacity.checked_sub(output_capacity).ok_or_else(|| HarnessError::Message("transaction fee underflow".to_string()))?;
    Ok(TransactionContext {
        transaction,
        transaction_view,
        resolved_transaction,
        data_loader,
        input_capacity,
        output_capacity,
        output_occupied_capacity,
        fee_shannons,
        script_group_count: groups.len(),
        lock_group_present,
        type_group_present,
    })
}

fn build_report(
    args: &Args,
    originate_elf: &[u8],
    repay_elf: &[u8],
    claim_elf: &[u8],
    lock_elf: &[u8],
    cases: Vec<CaseReport>,
    fixture_expectations: &BTreeMap<String, String>,
) -> Report {
    let total_cases = cases.len();
    let script_accepted = cases.iter().filter(|case| case.script_accepted).count();
    let script_matched_expected = cases.iter().filter(|case| case.script_matched_expected).count();
    let node_accepted = cases.iter().filter(|case| case.node_verifier.accepted).count();
    let node_matched_expected = cases.iter().filter(|case| case.node_verifier.matched_expected).count();
    let covered = cases.iter().map(|case| case.fixture).collect::<BTreeSet<_>>();
    let not_executed = fixture_expectations.keys().filter(|fixture| !covered.contains(fixture.as_str())).cloned().collect();
    let mut action_elves = BTreeMap::new();
    action_elves.insert("originate_agreement", elf_report(&args.originate_elf, originate_elf));
    action_elves.insert("repay_before_expiry", elf_report(&args.repay_elf, repay_elf));
    action_elves.insert("claim_after_expiry", elf_report(&args.claim_elf, claim_elf));
    Report {
        schema: "novaseal-agreement-ckb-tx-report-v0.1",
        classification: "agreement_profile_resolved_transaction_verification_stack_evidence",
        action_elves,
        lock_elf: elf_report(&args.lock_elf, lock_elf),
        summary: Summary {
            resolved_transaction_harness_executed: true,
            ckb_script_verifier_executed: true,
            ckb_node_verification_stack_executed: true,
            total_cases,
            expected_accept: cases.iter().filter(|case| case.expected == "accepted").count(),
            expected_reject: cases.iter().filter(|case| case.expected == "rejected").count(),
            script_accepted,
            script_rejected: total_cases - script_accepted,
            script_matched_expected,
            script_mismatched: total_cases - script_matched_expected,
            node_accepted,
            node_rejected: total_cases - node_accepted,
            node_matched_expected,
            node_mismatched: total_cases - node_matched_expected,
            fixtures_covered: covered.into_iter().collect(),
            fixture_files_not_executed_by_tx_harness: not_executed,
            expected_fixture_files_not_executed_by_tx_harness: EXPECTED_TX_HARNESS_LIMIT_FIXTURES.to_vec(),
            fixture_partition_matches_expected: true,
            all_lock_and_type_groups_present: cases.iter().all(|case| case.lock_group_present && case.type_group_present),
            non_contextual_checks_passed: cases.iter().all(|case| case.node_verifier.non_contextual_verified),
            contextual_checks_match_expected: cases.iter().all(|case| case.node_verifier.matched_expected),
            builder_shape_checks_passed: cases
                .iter()
                .all(|case| case.output_capacity_covers_occupied_capacity || case.fixture == "under_capacity_reject"),
            fee_shape_checks_passed: cases.iter().all(|case| case.fee_shannons >= BUILDER_FEE_SHANNONS),
            max_script_cycles: cases.iter().filter_map(|case| case.script_cycles).max().unwrap_or_default(),
            max_node_cycles: cases.iter().filter_map(|case| case.node_verifier.cycles).max().unwrap_or_default(),
            max_tx_size_bytes: cases.iter().map(|case| case.tx_size_bytes).max().unwrap_or_default(),
            max_output_occupied_capacity_shannons: cases
                .iter()
                .map(|case| case.output_occupied_capacity_shannons)
                .max()
                .unwrap_or_default(),
        },
        cases,
        limits: vec![
            "Runs ckb-script TransactionScriptsVerifier and ckb-verification NonContextualTransactionVerifier/ContextualTransactionVerifier over deterministic in-memory resolved transactions.",
            "Uses a local always-success lock only to let terminal transaction inputs reach the Agreement Profile type/action script. This lock is not part of the deployed protocol surface.",
            "Covers Agreement Profile action/type guards, typed payout output binding, receipt_hash output binding, terms_hash output binding, and CKB occupied-capacity rejection for the under-capacity fixture.",
            "Native CKB economic settlement is represented by typed payout outputs plus transaction capacity/value shape evidence; production wallet/builder integration still must preserve that mapping.",
            "Full Molecule/wallet signing preimage alignment is still not implemented.",
            "Cryptographic borrower/lender authority locks are still not implemented in this Agreement Profile slice.",
            "No live-chain RPC submission, mempool propagation, miner acceptance, or deployed CellDep liveness is proven.",
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

fn encode_agreement_cell(fields: &AgreementFields, status: u8, receipt_root: [u8; 32], nonce: u64) -> Vec<u8> {
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
    push_hash(&mut out, &receipt_root);
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
    old_receipt_root: [u8; 32],
    new_receipt_root: [u8; 32],
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
    push_hash(&mut out, &old_receipt_root);
    push_hash(&mut out, &new_receipt_root);
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

fn build_data1_script(code_hash: &[u8; 32]) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(packed_byte32(code_hash))
        .hash_type(ScriptHashType::Data1.into())
        .args(CkbBytes::new().pack())
        .build()
}

fn build_cell_dep_from_out_point(out_point: packed::OutPoint) -> packed::CellDep {
    packed::CellDep::new_builder().out_point(out_point).dep_type(DepType::Code.into()).build()
}

fn build_out_point(tx_hash: &[u8; 32], index: u32) -> packed::OutPoint {
    packed::OutPoint::new_builder().tx_hash(packed_byte32(tx_hash)).index(index.pack()).build()
}

fn code_cell_output(elf: &[u8]) -> Result<packed::CellOutput, HarnessError> {
    Ok(packed::CellOutput::new_builder().capacity(capacity_with_margin(elf.len())?.pack()).build())
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

fn expected_matches(expected: &str, accepted: bool) -> Result<bool, HarnessError> {
    match expected {
        "accepted" => Ok(accepted),
        "rejected" => Ok(!accepted),
        other => Err(HarnessError::Message(format!("unsupported expected value {other}"))),
    }
}

fn script_layer_expected(case: &AgreementCase) -> &'static str {
    if case.fixture == "under_capacity_reject" { "accepted" } else { case.expected }
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

fn ensure_fixture_partition(cases: &[AgreementCase], fixture_expectations: &BTreeMap<String, String>) -> Result<(), HarnessError> {
    let not_executed = tx_harness_not_executed_fixtures(cases, fixture_expectations);
    let expected = EXPECTED_TX_HARNESS_LIMIT_FIXTURES.iter().map(|fixture| (*fixture).to_string()).collect::<BTreeSet<_>>();
    if not_executed == expected {
        return Ok(());
    }
    Err(HarnessError::Message(format!(
        "transaction harness fixture partition changed: expected not-executed fixtures {expected:?}, got {not_executed:?}"
    )))
}

fn tx_harness_not_executed_fixtures(cases: &[AgreementCase], fixture_expectations: &BTreeMap<String, String>) -> BTreeSet<String> {
    let covered = cases.iter().map(|case| case.fixture).collect::<BTreeSet<_>>();
    fixture_expectations.keys().filter(|fixture| !covered.contains(fixture.as_str())).cloned().collect()
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
            .checked_add(AGREEMENT_OCCUPIED_CAPACITY)
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

fn packed_byte32(bytes: &[u8; 32]) -> packed::Byte32 {
    packed::Byte32::from_slice(bytes).expect("32-byte fixed hash")
}

fn ckb_blake2b256(data: &[u8]) -> [u8; 32] {
    let digest = Blake2bParams::new().hash_length(32).personal(CKB_BLAKE2B_PERSONAL).hash(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

fn elf_report(path: &Path, bytes: &[u8]) -> ElfReport {
    ElfReport { path: path.display().to_string(), size_bytes: bytes.len(), sha256: sha256_hex(bytes) }
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
        "summary: resolved_tx={} total={} script_accepted={} script_rejected={} script_matched={} script_mismatched={} node_matched={} node_mismatched={} max_script_cycles={} max_node_cycles={} max_tx_size_bytes={}",
        report.summary.resolved_transaction_harness_executed,
        report.summary.total_cases,
        report.summary.script_accepted,
        report.summary.script_rejected,
        report.summary.script_matched_expected,
        report.summary.script_mismatched,
        report.summary.node_matched_expected,
        report.summary.node_mismatched,
        report.summary.max_script_cycles,
        report.summary.max_node_cycles,
        report.summary.max_tx_size_bytes
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
