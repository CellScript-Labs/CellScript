#![allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::struct_excessive_bools,
    clippy::too_many_lines
)]

use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
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
use k256::schnorr::SigningKey;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

const DEFAULT_PARENT_ELF: &str = "build/nova_state_type.elf";
const DEFAULT_TYPE_ELF: &str = "target/novaseal-state-type-action.elf";
const DEFAULT_CHILD_ELF: &str = "target/novaseal-btc-verifier-riscv-shell-release.elf";
const DEFAULT_CANONICAL_VECTORS: &str = "target/novaseal-canonical-vectors.json";
const DEFAULT_FIXTURES_DIR: &str = "fixtures";
const DEFAULT_OUTPUT: &str = "target/novaseal-combined-tx-report.json";
const VERIFY_MAX_CYCLES: u64 = 800_000_000;
const VM2_ENABLED_EPOCH: u64 = 10;
const REQUIRED_FIXTURE_COUNT: usize = 6;
const TRANSACTION_SHAPE_OUTPUT_MARGIN_SHANNONS: u64 = 10_000_000_000;
const BUILDER_FEE_SHANNONS: u64 = 100_000;
const MIN_BUILDER_FEE_SHANNONS: u64 = 100_000;

const NOVASEAL_CELL_LEN: usize = 146;
const NOVASEAL_INTENT_LEN: usize = 213;
const BYTE32_LEN: usize = 32;
const SIGNATURE_PAYLOAD_LEN: usize = 96;
const LOCK_WITNESS_MAGIC: &[u8; 8] = b"CSARGv1\0";
const CKB_BLAKE2B_PERSONAL: &[u8; 16] = b"ckb-default-hash";

const CELL_BTC_AUTHORITY_HASH_OFFSET: usize = 2;
const CELL_STATE_HASH_OFFSET: usize = 34;
const CELL_POLICY_HASH_OFFSET: usize = 66;
const CELL_RECEIPT_ROOT_OFFSET: usize = 98;
const CELL_NONCE_OFFSET: usize = 130;
const CELL_EXPIRY_OFFSET: usize = 138;
const INTENT_DOMAIN_OFFSET: usize = 0;
const INTENT_NEW_STATE_HASH_OFFSET: usize = 101;
const INTENT_NONCE_OFFSET: usize = 197;
const INTENT_EXPIRY_OFFSET: usize = 205;

const TEST_SECRET_KEY: [u8; 32] = [
    0x3e, 0x74, 0x90, 0x68, 0x06, 0x39, 0xa2, 0xf7, 0xbb, 0xe8, 0x36, 0x1d, 0xd3, 0xf3, 0x4e, 0xb6, 0x42, 0x9a, 0x9c, 0x92, 0x4d,
    0x8b, 0x34, 0x2c, 0x01, 0x5e, 0x55, 0x5e, 0x62, 0x8f, 0x94, 0xe5,
];
const TEST_AUX_RAND: [u8; 32] = [0x42; 32];

#[derive(Clone, Debug, Default)]
struct HarnessDataLoader {
    cells: HashMap<packed::OutPoint, CkbBytes>,
    headers: HashMap<packed::Byte32, ckb_types::core::HeaderView>,
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

impl ExtensionProvider for HarnessDataLoader {
    fn get_block_extension(&self, _hash: &packed::Byte32) -> Option<packed::Bytes> {
        None
    }
}

#[derive(Debug, Error)]
enum HarnessError {
    #[error(
        "usage: novaseal_combined_tx_harness [--parent-elf PATH] [--type-elf PATH] [--child-elf PATH] [--canonical-vectors PATH] [--fixtures-dir PATH] [--output PATH] [--pretty]"
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
    parent_elf: PathBuf,
    type_elf: PathBuf,
    child_elf: PathBuf,
    canonical_vectors: PathBuf,
    fixtures_dir: PathBuf,
    output: PathBuf,
    pretty: bool,
}

#[derive(Debug)]
struct CombinedCase {
    fixture: String,
    category: String,
    expected: String,
    expected_failure_mode: Option<String>,
    current_timepoint: u64,
    old_cell: Vec<u8>,
    output_cell: Vec<u8>,
    witness: Vec<u8>,
    signature_mutation: Option<&'static str>,
}

#[derive(Debug)]
struct TransactionContext {
    transaction: packed::Transaction,
    transaction_view: TransactionView,
    resolved_transaction: ResolvedTransaction,
    data_loader: HarnessDataLoader,
    child_code_hash: [u8; 32],
    parent_code_hash: [u8; 32],
    type_code_hash: [u8; 32],
    child_dep_out_point: packed::OutPoint,
    parent_dep_out_point: packed::OutPoint,
    type_dep_out_point: packed::OutPoint,
    output_occupied_capacity: u64,
    input_capacity: u64,
    output_capacity: u64,
    fee_shannons: u64,
    under_capacity: u64,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    classification: &'static str,
    parent_elf: ElfReport,
    type_elf: ElfReport,
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
    combined_full_transaction_executed: bool,
    total_cases: usize,
    expected_accept: usize,
    expected_reject: usize,
    accepted: usize,
    rejected: usize,
    matched_expected: usize,
    mismatched: usize,
    failure_scope_matched: usize,
    failure_scope_mismatched: usize,
    lock_and_type_script_groups_present: bool,
    child_spawn_target_cell_dep0_modelled: bool,
    shared_witness_abi_aligned: bool,
    shared_witness_size_bytes: usize,
    builder_shape_checks_passed: bool,
    fee_shape_checks_passed: bool,
    under_capacity_shape_rejects: bool,
    min_fee_shannons: u64,
    max_fee_shannons: u64,
    max_full_transaction_cycles: u64,
    max_consensus_tx_size_bytes: usize,
    max_output_occupied_capacity_shannons: u64,
    min_capacity_margin_shannons: u64,
}

#[derive(Debug, Serialize)]
struct CaseReport {
    fixture: String,
    category: String,
    expected: String,
    expected_failure_mode: Option<String>,
    expected_failure_scope: Option<&'static str>,
    accepted: bool,
    observed_failure_scope: Option<&'static str>,
    failure_scope_matched: bool,
    match_evidence: &'static str,
    matched_expected: bool,
    full_transaction_cycles: Option<u64>,
    full_transaction_error: Option<String>,
    transaction_hash: String,
    consensus_tx_size_bytes: usize,
    witness_size_bytes: usize,
    input_cell_data_size_bytes: usize,
    output_cell_data_size_bytes: usize,
    output_occupied_capacity_shannons: u64,
    capacity_margin_shannons: u64,
    builder_shape: BuilderShapeReport,
    script_group_count: usize,
    lock_group_present: bool,
    type_group_present: bool,
    child_cell_dep0: CellDepReport,
    parent_lock_dep: CellDepReport,
    state_type_dep: CellDepReport,
    shared_witness_payload_order: Vec<&'static str>,
    signature_mutation: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct BuilderShapeReport {
    classification: &'static str,
    input_capacity_shannons: u64,
    output_capacity_shannons: u64,
    output_occupied_capacity_shannons: u64,
    fee_shannons: u64,
    min_fee_shannons: u64,
    fee_covers_minimum: bool,
    output_capacity_covers_occupied_capacity: bool,
    under_capacity_output_capacity_shannons: u64,
    under_capacity_rejected_by_shape: bool,
    cell_dep0_is_spawn_target: bool,
    parent_lock_dep_present: bool,
    state_type_dep_present: bool,
    live_dep_resolution_model: &'static str,
}

#[derive(Debug, Serialize)]
struct CellDepReport {
    index: usize,
    role: &'static str,
    dep_type: &'static str,
    code_hash: String,
    out_point_tx_hash_placeholder: String,
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
    let type_elf = fs::read(&args.type_elf)?;
    let child_elf = fs::read(&args.child_elf)?;
    let vectors: Value = serde_json::from_slice(&fs::read(&args.canonical_vectors)?)?;
    let cases = build_cases(&vectors, &args.fixtures_dir)?;
    let reports = cases.iter().map(|case| run_case(&parent_elf, &type_elf, &child_elf, case)).collect::<Result<Vec<_>, _>>()?;
    let report = build_report(&args, &parent_elf, &type_elf, &child_elf, reports);
    write_report(&args.output, &report, args.pretty)?;
    print_summary(&args.output, &report);
    if report.summary.mismatched == 0 {
        Ok(())
    } else {
        Err(HarnessError::Message(format!("{} combined transaction case(s) mismatched", report.summary.mismatched)))
    }
}

fn parse_args() -> Result<Args, HarnessError> {
    let mut args = Args {
        parent_elf: PathBuf::from(DEFAULT_PARENT_ELF),
        type_elf: PathBuf::from(DEFAULT_TYPE_ELF),
        child_elf: PathBuf::from(DEFAULT_CHILD_ELF),
        canonical_vectors: PathBuf::from(DEFAULT_CANONICAL_VECTORS),
        fixtures_dir: PathBuf::from(DEFAULT_FIXTURES_DIR),
        output: PathBuf::from(DEFAULT_OUTPUT),
        pretty: false,
    };
    let mut raw = env::args().skip(1);
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--parent-elf" => args.parent_elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--type-elf" => args.type_elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--child-elf" => args.child_elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--canonical-vectors" => args.canonical_vectors = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--fixtures-dir" => args.fixtures_dir = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--output" => args.output = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--pretty" => args.pretty = true,
            "-h" | "--help" => return Err(HarnessError::Usage),
            _ => return Err(HarnessError::Message(format!("unknown argument: {arg}"))),
        }
    }
    Ok(args)
}

fn build_cases(vectors: &Value, fixtures_dir: &Path) -> Result<Vec<CombinedCase>, HarnessError> {
    let values = vectors
        .get("vectors")
        .and_then(Value::as_array)
        .ok_or_else(|| HarnessError::Message("canonical vectors missing vectors[]".to_string()))?;
    if values.len() != REQUIRED_FIXTURE_COUNT {
        return Err(HarnessError::Message(format!(
            "canonical vectors must contain exactly {REQUIRED_FIXTURE_COUNT} v0 fixtures, found {}",
            values.len()
        )));
    }
    values.iter().map(|value| build_case(value, fixtures_dir)).collect()
}

fn build_case(value: &Value, fixtures_dir: &Path) -> Result<CombinedCase, HarnessError> {
    let fixture = str_field(value, "fixture")?.to_string();
    let category = str_field(value, "category")?.to_string();
    let fixture_json: Value = serde_json::from_slice(&fs::read(fixtures_dir.join(&fixture))?)?;
    let expected = value.pointer("/source_model_result/result").and_then(Value::as_str).unwrap_or("unknown").to_string();
    let expected_failure_mode = value.pointer("/source_model_result/failure_mode").and_then(Value::as_str).map(ToString::to_string);
    validate_expected_result(&fixture, &expected, expected_failure_mode.as_deref())?;
    let current_timepoint = fixture_json.pointer("/inputs/current_timepoint").and_then(Value::as_u64).unwrap_or(200);
    if current_timepoint < VM2_ENABLED_EPOCH {
        return Err(HarnessError::Message(format!(
            "{fixture}: current_timepoint {current_timepoint} is below VM2-enabled epoch {VM2_ENABLED_EPOCH}; the harness refuses to rewrite fixture time"
        )));
    }
    let encoded = value.get("encoded").ok_or_else(|| HarnessError::Message(format!("{fixture}: missing encoded")))?;
    let old_cell = hex_bytes(encoded.pointer("/old_cell/hex"), &fixture, "encoded.old_cell.hex")?;
    if old_cell.len() != NOVASEAL_CELL_LEN {
        return Err(HarnessError::Message(format!("{fixture}: old_cell has {} bytes, expected {NOVASEAL_CELL_LEN}", old_cell.len())));
    }
    let intent = if fixture == "receipt_hash_mismatch_reject.json" {
        hex_bytes(encoded.pointer("/intent/hex"), &fixture, "encoded.intent.hex")?
    } else {
        hex_bytes(encoded.pointer("/resolved/resolved_intent/hex"), &fixture, "encoded.resolved.resolved_intent.hex")?
    };
    if intent.len() != NOVASEAL_INTENT_LEN {
        return Err(HarnessError::Message(format!("{fixture}: intent has {} bytes, expected {NOVASEAL_INTENT_LEN}", intent.len())));
    }
    let receipt_hash = decode_hex(&hex_string_field(value, "/hashes/resolved_receipt_hash", &fixture)?)?;
    if receipt_hash.len() != BYTE32_LEN {
        return Err(HarnessError::Message(format!("{fixture}: receipt_hash has {} bytes, expected {BYTE32_LEN}", receipt_hash.len())));
    }
    let state_hash_commitment =
        ckb_blake2b256(&intent[INTENT_NEW_STATE_HASH_OFFSET..INTENT_NEW_STATE_HASH_OFFSET + BYTE32_LEN]).to_vec();
    let mut signature_payload = sign_intent_domain(&intent)?;
    let signature_mutation = if fixture == "wrong_signature_reject.json" {
        let last =
            signature_payload.last_mut().ok_or_else(|| HarnessError::Message("signature payload unexpectedly empty".to_string()))?;
        *last ^= 0x01;
        Some("signature last byte flipped after signing lock-computed digest")
    } else {
        None
    };
    let output_cell = build_output_cell(&old_cell, &intent);
    let witness = build_witness(&intent, &receipt_hash, &state_hash_commitment, &signature_payload);
    Ok(CombinedCase {
        fixture,
        category,
        expected,
        expected_failure_mode,
        current_timepoint,
        old_cell,
        output_cell,
        witness,
        signature_mutation,
    })
}

fn run_case(parent_elf: &[u8], type_elf: &[u8], child_elf: &[u8], case: &CombinedCase) -> Result<CaseReport, HarnessError> {
    let context = build_transaction_context(parent_elf, type_elf, child_elf, case)?;
    let consensus = Arc::new(resolved_script_consensus());
    let header = HeaderBuilder::default().epoch(EpochNumberWithFraction::new(case.current_timepoint, 0, 1).pack()).build();
    let tx_env = Arc::new(TxVerifyEnv::new_commit(&header));
    let verifier = TransactionScriptsVerifier::new(
        Arc::new(context.resolved_transaction.clone()),
        context.data_loader.clone(),
        consensus,
        tx_env,
    );
    let groups: Vec<_> =
        verifier.groups_with_type().map(|(group_type, hash, group)| (group_type, hash.clone(), group.clone())).collect();
    let lock_group_present = groups.iter().any(|(group_type, _, _)| *group_type == ckb_script::ScriptGroupType::Lock);
    let type_group_present = groups.iter().any(|(group_type, _, _)| *group_type == ckb_script::ScriptGroupType::Type);
    let result = verifier.verify(VERIFY_MAX_CYCLES);
    let (accepted, full_transaction_cycles, full_transaction_error) = match result {
        Ok(cycles) => (true, Some(cycles), None),
        Err(error) => (false, None, Some(format!("{error}"))),
    };
    let outcome_matched = match case.expected.as_str() {
        "accepted" => accepted,
        "rejected" => !accepted,
        other => return Err(HarnessError::Message(format!("{} has unsupported expected result: {other}", case.fixture))),
    };
    let expected_failure_scope = expected_failure_scope(case.expected_failure_mode.as_deref());
    let observed_failure_scope = observed_failure_scope(full_transaction_error.as_deref());
    let failure_scope_matched = match case.expected.as_str() {
        "accepted" => expected_failure_scope.is_none() && observed_failure_scope.is_none(),
        "rejected" => expected_failure_scope.is_some() && expected_failure_scope == observed_failure_scope,
        _ => false,
    };
    let matched_expected = outcome_matched && failure_scope_matched;
    let match_evidence = match (accepted, outcome_matched, failure_scope_matched) {
        (true, true, true) => "accepted-outcome",
        (false, true, true) => "rejected-outcome-and-script-scope",
        (false, true, false) => "rejected-outcome-only",
        _ => "mismatch",
    };
    let capacity_margin = context.output_capacity.saturating_sub(context.output_occupied_capacity);
    let cell_dep0_is_spawn_target = transaction_cell_dep_at_matches(&context.transaction, 0, &context.child_dep_out_point)
        && resolved_cell_dep_at_matches(&context.resolved_transaction, 0, &context.child_dep_out_point);
    let parent_lock_dep_present = transaction_cell_dep_present(&context.transaction, &context.parent_dep_out_point)
        && resolved_cell_dep_present(&context.resolved_transaction, &context.parent_dep_out_point);
    let state_type_dep_present = transaction_cell_dep_present(&context.transaction, &context.type_dep_out_point)
        && resolved_cell_dep_present(&context.resolved_transaction, &context.type_dep_out_point);
    let builder_shape = BuilderShapeReport {
        classification: "production_builder_candidate_shape",
        input_capacity_shannons: context.input_capacity,
        output_capacity_shannons: context.output_capacity,
        output_occupied_capacity_shannons: context.output_occupied_capacity,
        fee_shannons: context.fee_shannons,
        min_fee_shannons: MIN_BUILDER_FEE_SHANNONS,
        fee_covers_minimum: context.fee_shannons >= MIN_BUILDER_FEE_SHANNONS,
        output_capacity_covers_occupied_capacity: context.output_capacity >= context.output_occupied_capacity,
        under_capacity_output_capacity_shannons: context.under_capacity,
        under_capacity_rejected_by_shape: context.under_capacity < context.output_occupied_capacity,
        cell_dep0_is_spawn_target,
        parent_lock_dep_present,
        state_type_dep_present,
        live_dep_resolution_model: "deterministic harness cells; not live chain deps",
    };
    Ok(CaseReport {
        fixture: case.fixture.clone(),
        category: case.category.clone(),
        expected: case.expected.clone(),
        expected_failure_mode: case.expected_failure_mode.clone(),
        expected_failure_scope,
        accepted,
        observed_failure_scope,
        failure_scope_matched,
        match_evidence,
        matched_expected,
        full_transaction_cycles,
        full_transaction_error,
        transaction_hash: hex0x(context.transaction_view.hash().as_slice()),
        consensus_tx_size_bytes: context.transaction.as_bytes().len(),
        witness_size_bytes: case.witness.len(),
        input_cell_data_size_bytes: case.old_cell.len(),
        output_cell_data_size_bytes: case.output_cell.len(),
        output_occupied_capacity_shannons: context.output_occupied_capacity,
        capacity_margin_shannons: capacity_margin,
        builder_shape,
        script_group_count: groups.len(),
        lock_group_present,
        type_group_present,
        child_cell_dep0: cell_dep_report(0, "spawn-target-child-verifier", &context.child_code_hash, &context.child_dep_out_point),
        parent_lock_dep: cell_dep_report(1, "parent-lock-code", &context.parent_code_hash, &context.parent_dep_out_point),
        state_type_dep: cell_dep_report(2, "state-type-action-code", &context.type_code_hash, &context.type_dep_out_point),
        shared_witness_payload_order: vec![
            "CSARGv1 magic",
            "u32 intent_len",
            "NovaSealIntentV0",
            "receipt_hash",
            "state_hash_commitment",
            "u32 signature_payload_len",
            "SignaturePayload",
        ],
        signature_mutation: case.signature_mutation,
    })
}

fn build_transaction_context(
    parent_elf: &[u8],
    type_elf: &[u8],
    child_elf: &[u8],
    case: &CombinedCase,
) -> Result<TransactionContext, HarnessError> {
    let parent_code_hash = code_type_hash("novaseal-parent-lock-code-type-v0", parent_elf);
    let type_code_hash = code_type_hash("novaseal-state-type-action-code-type-v0", type_elf);
    let child_code_hash = ckb_blake2b256(child_elf);
    let lock_args = cell_authority_hash(&case.old_cell)?;
    let lock_script = build_packed_script(&parent_code_hash, &lock_args);
    let type_script = build_packed_script_no_args(&type_code_hash);
    let output_without_capacity =
        packed::CellOutput::new_builder().lock(lock_script.clone()).type_(Some(type_script.clone()).pack()).build();
    let output_occupied_capacity = output_without_capacity
        .occupied_capacity(capacity_bytes(case.output_cell.len())?)
        .map_err(|error| HarnessError::Message(format!("failed to compute occupied capacity: {error}")))?
        .as_u64();
    let output_capacity = output_occupied_capacity
        .checked_add(TRANSACTION_SHAPE_OUTPUT_MARGIN_SHANNONS)
        .ok_or_else(|| HarnessError::Message("transaction output capacity overflow".to_string()))?;
    let input_capacity = output_capacity
        .checked_add(BUILDER_FEE_SHANNONS)
        .ok_or_else(|| HarnessError::Message("transaction input capacity overflow".to_string()))?;
    let fee_shannons =
        input_capacity.checked_sub(output_capacity).ok_or_else(|| HarnessError::Message("transaction fee underflow".to_string()))?;
    let under_capacity = output_occupied_capacity.saturating_sub(1);
    let output = output_without_capacity.as_builder().capacity(Capacity::shannons(output_capacity).pack()).build();

    let child_dep_out_point = build_out_point(&child_code_hash, 0);
    let parent_dep_out_point = build_out_point(&parent_code_hash, 0);
    let type_dep_out_point = build_out_point(&type_code_hash, 0);
    let input_out_point = build_out_point(&ckb_blake2b256(case.fixture.as_bytes()), 0);
    let child_dep = build_cell_dep_from_out_point(child_dep_out_point.clone());
    let parent_dep = build_cell_dep_from_out_point(parent_dep_out_point.clone());
    let type_dep = build_cell_dep_from_out_point(type_dep_out_point.clone());
    let input = packed::CellInput::new_builder().previous_output(input_out_point.clone()).build();

    let header = HeaderBuilder::default().epoch(EpochNumberWithFraction::new(case.current_timepoint, 0, 1).pack()).build();
    let mut data_loader = HarnessDataLoader::default();
    let header_hash = data_loader.insert_header(header);

    let raw_transaction = packed::RawTransaction::new_builder()
        .version(0u32.pack())
        .cell_deps(vec![child_dep, parent_dep, type_dep].pack())
        .header_deps(vec![header_hash].pack())
        .inputs(vec![input].pack())
        .outputs(vec![output.clone()].pack())
        .outputs_data(vec![CkbBytes::from(case.output_cell.clone()).pack()].pack())
        .build();
    let transaction = packed::Transaction::new_builder()
        .raw(raw_transaction)
        .witnesses(vec![CkbBytes::from(case.witness.clone()).pack()].pack())
        .build();
    let transaction_view = transaction.clone().into_view();

    let child_dep_output = packed::CellOutput::new_builder().capacity(capacity_with_margin(child_elf.len())?.pack()).build();
    let parent_dep_output = code_cell_output("novaseal-parent-lock-code-type-v0", parent_elf)?;
    let type_dep_output = code_cell_output("novaseal-state-type-action-code-type-v0", type_elf)?;
    let input_cell_output = packed::CellOutput::new_builder()
        .capacity(Capacity::shannons(input_capacity).pack())
        .lock(lock_script)
        .type_(Some(type_script).pack())
        .build();

    let child_bytes = CkbBytes::copy_from_slice(child_elf);
    let parent_bytes = CkbBytes::copy_from_slice(parent_elf);
    let type_bytes = CkbBytes::copy_from_slice(type_elf);
    let input_bytes = CkbBytes::copy_from_slice(&case.old_cell);
    data_loader.insert_cell(child_dep_out_point.clone(), child_bytes.clone());
    data_loader.insert_cell(parent_dep_out_point.clone(), parent_bytes.clone());
    data_loader.insert_cell(type_dep_out_point.clone(), type_bytes.clone());
    data_loader.insert_cell(input_out_point.clone(), input_bytes.clone());

    let resolved_transaction = ResolvedTransaction {
        transaction: transaction_view.clone(),
        resolved_cell_deps: vec![
            CellMetaBuilder::from_cell_output(child_dep_output, child_bytes).out_point(child_dep_out_point.clone()).build(),
            CellMetaBuilder::from_cell_output(parent_dep_output, parent_bytes).out_point(parent_dep_out_point.clone()).build(),
            CellMetaBuilder::from_cell_output(type_dep_output, type_bytes).out_point(type_dep_out_point.clone()).build(),
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
        child_code_hash,
        parent_code_hash,
        type_code_hash,
        child_dep_out_point,
        parent_dep_out_point,
        type_dep_out_point,
        output_occupied_capacity,
        input_capacity,
        output_capacity,
        fee_shannons,
        under_capacity,
    })
}

fn build_report(args: &Args, parent_elf: &[u8], type_elf: &[u8], child_elf: &[u8], cases: Vec<CaseReport>) -> Report {
    let total_cases = cases.len();
    let expected_accept = cases.iter().filter(|case| case.expected == "accepted").count();
    let expected_reject = cases.iter().filter(|case| case.expected == "rejected").count();
    let accepted = cases.iter().filter(|case| case.accepted).count();
    let matched_expected = cases.iter().filter(|case| case.matched_expected).count();
    let witness_sizes = cases.iter().map(|case| case.witness_size_bytes).collect::<Vec<_>>();
    let failure_scope_matched = cases.iter().filter(|case| case.failure_scope_matched).count();
    let builder_shape_checks_passed = cases.iter().all(|case| {
        case.builder_shape.output_capacity_covers_occupied_capacity
            && case.builder_shape.cell_dep0_is_spawn_target
            && case.builder_shape.parent_lock_dep_present
            && case.builder_shape.state_type_dep_present
    });
    let fee_shape_checks_passed = cases.iter().all(|case| case.builder_shape.fee_covers_minimum);
    let under_capacity_shape_rejects = cases.iter().all(|case| case.builder_shape.under_capacity_rejected_by_shape);
    Report {
        schema: "novaseal-combined-tx-report-v0.1",
        classification: "six_fixture_combined_lock_type_full_ckb_script_verifier_evidence",
        parent_elf: elf_report(&args.parent_elf, parent_elf),
        type_elf: elf_report(&args.type_elf, type_elf),
        child_elf: elf_report(&args.child_elf, child_elf),
        summary: Summary {
            combined_full_transaction_executed: true,
            total_cases,
            expected_accept,
            expected_reject,
            accepted,
            rejected: total_cases - accepted,
            matched_expected,
            mismatched: total_cases - matched_expected,
            failure_scope_matched,
            failure_scope_mismatched: total_cases - failure_scope_matched,
            lock_and_type_script_groups_present: cases.iter().all(|case| case.lock_group_present && case.type_group_present),
            child_spawn_target_cell_dep0_modelled: cases.iter().all(|case| case.child_cell_dep0.index == 0),
            shared_witness_abi_aligned: !witness_sizes.is_empty() && witness_sizes.iter().all(|size| *size == witness_sizes[0]),
            shared_witness_size_bytes: witness_sizes.first().copied().unwrap_or_default(),
            builder_shape_checks_passed,
            fee_shape_checks_passed,
            under_capacity_shape_rejects,
            min_fee_shannons: cases.iter().map(|case| case.builder_shape.fee_shannons).min().unwrap_or_default(),
            max_fee_shannons: cases.iter().map(|case| case.builder_shape.fee_shannons).max().unwrap_or_default(),
            max_full_transaction_cycles: cases.iter().filter_map(|case| case.full_transaction_cycles).max().unwrap_or_default(),
            max_consensus_tx_size_bytes: cases.iter().map(|case| case.consensus_tx_size_bytes).max().unwrap_or_default(),
            max_output_occupied_capacity_shannons: cases
                .iter()
                .map(|case| case.output_occupied_capacity_shannons)
                .max()
                .unwrap_or_default(),
            min_capacity_margin_shannons: cases.iter().map(|case| case.capacity_margin_shannons).min().unwrap_or_default(),
        },
        cases,
        limits: vec![
            "Runs ckb-script TransactionScriptsVerifier::verify over six constructed transactions containing the parent lock script, state type/action script, and staged child verifier cell_dep.",
            "Uses the shared CSARGv1 witness payload for both lock and type/action execution.",
            "The transactions are in-memory harness ResolvedTransaction values, not production builder or full-node submissions.",
            "Builder-shape checks derive fee, occupied-capacity floor, under-capacity negative shape, and expected code dep roles from the constructed transaction and resolved deps, but still do not prove live dep liveness or mempool acceptance.",
            "Resolved code deps and input cells are deterministic harness cells, not live chain cells.",
            "Negative fixture matching is outcome plus lock/type script-scope matching; it is not yet a full semantic error-code proof for every failure mode.",
            "Receipt output materialisation and Molecule/wallet signing alignment remain out of scope.",
        ],
    }
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

fn build_witness(intent: &[u8], receipt_hash: &[u8], state_hash_commitment: &[u8], signature_payload: &[u8]) -> Vec<u8> {
    let mut witness = Vec::with_capacity(
        LOCK_WITNESS_MAGIC.len() + 4 + intent.len() + receipt_hash.len() + state_hash_commitment.len() + 4 + signature_payload.len(),
    );
    witness.extend_from_slice(LOCK_WITNESS_MAGIC);
    witness.extend_from_slice(&(intent.len() as u32).to_le_bytes());
    witness.extend_from_slice(intent);
    witness.extend_from_slice(receipt_hash);
    witness.extend_from_slice(state_hash_commitment);
    witness.extend_from_slice(&(signature_payload.len() as u32).to_le_bytes());
    witness.extend_from_slice(signature_payload);
    witness
}

fn sign_intent_domain(intent: &[u8]) -> Result<Vec<u8>, HarnessError> {
    let digest = ckb_blake2b256(&intent[INTENT_DOMAIN_OFFSET..INTENT_DOMAIN_OFFSET + BYTE32_LEN]);
    let signing_key = SigningKey::from_bytes(&TEST_SECRET_KEY)
        .map_err(|error| HarnessError::Message(format!("failed to construct test BIP340 signing key: {error}")))?;
    let signature = signing_key
        .sign_prehash_with_aux_rand(&digest, &TEST_AUX_RAND)
        .map_err(|error| HarnessError::Message(format!("failed to sign fixture digest: {error}")))?;
    let mut payload = Vec::with_capacity(SIGNATURE_PAYLOAD_LEN);
    payload.extend_from_slice(signing_key.verifying_key().to_bytes().as_slice());
    payload.extend_from_slice(signature.to_bytes().as_slice());
    Ok(payload)
}

fn build_packed_script(code_hash: &[u8; 32], args: &[u8; 32]) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(packed_byte32(code_hash))
        .hash_type(ScriptHashType::Type.into())
        .args(CkbBytes::copy_from_slice(args).pack())
        .build()
}

fn build_packed_script_no_args(code_hash: &[u8; 32]) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(packed_byte32(code_hash))
        .hash_type(ScriptHashType::Type.into())
        .args(CkbBytes::new().pack())
        .build()
}

fn build_cell_dep_from_out_point(out_point: packed::OutPoint) -> packed::CellDep {
    packed::CellDep::new_builder().out_point(out_point).dep_type(DepType::Code.into()).build()
}

fn code_cell_output(role: &str, elf: &[u8]) -> Result<packed::CellOutput, HarnessError> {
    Ok(packed::CellOutput::new_builder()
        .capacity(capacity_with_margin(elf.len())?.pack())
        .type_(Some(build_code_type_script(role, elf)).pack())
        .build())
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

fn code_type_hash(role: &str, elf: &[u8]) -> [u8; 32] {
    byte32_to_array(&build_code_type_script(role, elf).calc_script_hash())
}

fn build_out_point(tx_hash: &[u8; 32], index: u32) -> packed::OutPoint {
    packed::OutPoint::new_builder().tx_hash(packed_byte32(tx_hash)).index(index.pack()).build()
}

fn cell_authority_hash(cell: &[u8]) -> Result<[u8; 32], HarnessError> {
    if cell.len() < CELL_BTC_AUTHORITY_HASH_OFFSET + BYTE32_LEN {
        return Err(HarnessError::Message("cell is too short to contain btc_authority_hash".to_string()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&cell[CELL_BTC_AUTHORITY_HASH_OFFSET..CELL_BTC_AUTHORITY_HASH_OFFSET + BYTE32_LEN]);
    Ok(out)
}

fn cell_dep_report(index: usize, role: &'static str, code_hash: &[u8; 32], out_point: &packed::OutPoint) -> CellDepReport {
    CellDepReport {
        index,
        role,
        dep_type: "code",
        code_hash: hex0x(code_hash),
        out_point_tx_hash_placeholder: hex0x(out_point.tx_hash().as_slice()),
    }
}

fn validate_expected_result(fixture: &str, expected: &str, failure_mode: Option<&str>) -> Result<(), HarnessError> {
    match (expected, failure_mode) {
        ("accepted", None) | ("rejected", Some(_)) => Ok(()),
        ("accepted", Some(mode)) => {
            Err(HarnessError::Message(format!("{fixture}: accepted fixture must not declare failure_mode={mode}")))
        }
        ("rejected", None) => Err(HarnessError::Message(format!("{fixture}: rejected fixture must declare failure_mode"))),
        (other, _) => Err(HarnessError::Message(format!("{fixture}: unsupported expected result: {other}"))),
    }
}

fn expected_failure_scope(failure_mode: Option<&str>) -> Option<&'static str> {
    match failure_mode {
        Some("btc_signature_verification_failed" | "policy_hash_mismatch") => Some("lock"),
        Some("intent_expired" | "nonce_must_increment" | "receipt_hash_mismatch") => Some("type"),
        Some(_) => Some("unknown"),
        None => None,
    }
}

fn observed_failure_scope(error: Option<&str>) -> Option<&'static str> {
    let error = error?;
    if error.contains("Inputs[0].Lock") {
        Some("lock")
    } else if error.contains("Inputs[0].Type") {
        Some("type")
    } else {
        Some("unknown")
    }
}

fn transaction_cell_dep_at_matches(transaction: &packed::Transaction, index: usize, out_point: &packed::OutPoint) -> bool {
    transaction.raw().cell_deps().get(index).is_some_and(|dep| out_point_matches(&dep.out_point(), out_point))
}

fn transaction_cell_dep_present(transaction: &packed::Transaction, out_point: &packed::OutPoint) -> bool {
    transaction.raw().cell_deps().into_iter().any(|dep| out_point_matches(&dep.out_point(), out_point))
}

fn resolved_cell_dep_at_matches(resolved_transaction: &ResolvedTransaction, index: usize, out_point: &packed::OutPoint) -> bool {
    resolved_transaction.resolved_cell_deps.get(index).is_some_and(|cell_meta| out_point_matches(&cell_meta.out_point, out_point))
}

fn resolved_cell_dep_present(resolved_transaction: &ResolvedTransaction, out_point: &packed::OutPoint) -> bool {
    resolved_transaction.resolved_cell_deps.iter().any(|cell_meta| out_point_matches(&cell_meta.out_point, out_point))
}

fn out_point_matches(left: &packed::OutPoint, right: &packed::OutPoint) -> bool {
    left.as_slice() == right.as_slice()
}

fn elf_report(path: &Path, bytes: &[u8]) -> ElfReport {
    ElfReport { path: path.display().to_string(), size_bytes: bytes.len(), sha256: sha256_hex(bytes) }
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
    let digest = blake2b_simd::Params::new().hash_length(32).personal(CKB_BLAKE2B_PERSONAL).hash(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

fn packed_byte32(bytes: &[u8; 32]) -> packed::Byte32 {
    packed::Byte32::from_slice(bytes).expect("32-byte fixed hash")
}

fn byte32_to_array(byte32: &packed::Byte32) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(byte32.as_slice());
    bytes
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
        "summary: combined_full_tx_executed={} total={} accepted={} rejected={} matched_expected={} mismatched={} failure_scope_matched={} failure_scope_mismatched={} lock_and_type_groups_present={} shared_witness_abi_aligned={} builder_shape_checks_passed={} fee_shape_checks_passed={} max_cycles={} max_tx_size_bytes={} max_occupied_capacity_shannons={}",
        report.summary.combined_full_transaction_executed,
        report.summary.total_cases,
        report.summary.accepted,
        report.summary.rejected,
        report.summary.matched_expected,
        report.summary.mismatched,
        report.summary.failure_scope_matched,
        report.summary.failure_scope_mismatched,
        report.summary.lock_and_type_script_groups_present,
        report.summary.shared_witness_abi_aligned,
        report.summary.builder_shape_checks_passed,
        report.summary.fee_shape_checks_passed,
        report.summary.max_full_transaction_cycles,
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
