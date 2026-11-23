include!("ickb_diff/measured_cases.rs");
#[path = "ickb_diff/measurement.rs"]
mod measurement;
use measurement::assert_execution as assert_matrix_execution_matches;

use camino::Utf8PathBuf;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[path = "support/ckb_script_runner.rs"]
mod ckb_script_runner;

#[path = "support/cost_measurement.rs"]
#[allow(dead_code)]
mod cost_measurement;

use ckb_script_runner::{
    build_dao_data_fixture, build_dao_fixture, build_simple_fixture, compile_cellscript_source_to_elf, execute_cellscript_script,
    load_original_ickb_binary, patch_ickb_logic_dao_hash, FixtureCell, VM_HARNESS_CELL_CAPACITY_ACTION,
    VM_HARNESS_CELL_CAPACITY_PROGRAM, VM_HARNESS_CELL_DATA_SIZE_ACTION, VM_HARNESS_CELL_DATA_SIZE_PROGRAM,
    VM_HARNESS_CELL_DEP_DATA_SIZE_ACTION, VM_HARNESS_CELL_DEP_DATA_SIZE_PROGRAM, VM_HARNESS_DAO_HAS_TYPE_NEG_ACTION,
    VM_HARNESS_DAO_HAS_TYPE_NEG_PROGRAM, VM_HARNESS_DAO_IS_DEPOSIT_ACTION, VM_HARNESS_DAO_IS_DEPOSIT_PROGRAM,
    VM_HARNESS_DAO_IS_WITHDRAWAL_ACTION, VM_HARNESS_DAO_IS_WITHDRAWAL_PROGRAM, VM_HARNESS_DAO_MISSING_HEADER_DEP_ACTION,
    VM_HARNESS_DAO_MISSING_HEADER_DEP_PROGRAM, VM_HARNESS_DAO_PASS_ACTION, VM_HARNESS_DAO_PASS_PROGRAM, VM_HARNESS_FAIL_ACTION,
    VM_HARNESS_FAIL_PROGRAM, VM_HARNESS_ICKB_DEPOSIT_ACTION, VM_HARNESS_ICKB_DEPOSIT_PROGRAM, VM_HARNESS_OCCUPIED_CAPACITY_ACTION,
    VM_HARNESS_OCCUPIED_CAPACITY_PROGRAM, VM_HARNESS_PASS_ACTION, VM_HARNESS_PASS_PROGRAM,
};
use ckb_testtool::ckb_types::core::{
    Capacity, DepType, EpochNumberWithFraction, HeaderBuilder, HeaderView, ScriptHashType, TransactionView,
};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

include!("ickb_diff/programs.rs");
include!("ickb_diff/evidence_matrix.rs");
include!("ickb_diff/vm_primitives.rs");
include!("ickb_diff/dao_withdrawal_execution.rs");
include!("ickb_diff/original_binary_checks.rs");
include!("ickb_diff/scenario_configuration.rs");
include!("ickb_diff/deposit_and_mint_scenarios.rs");
include!("ickb_diff/dao_withdrawal_scenarios.rs");
include!("ickb_diff/limit_order_scenarios.rs");
include!("ickb_diff/owned_owner_scenarios.rs");
include!("ickb_diff/deposit_and_mint_execution.rs");
include!("ickb_diff/owned_owner_execution.rs");
include!("ickb_diff/limit_order_execution.rs");
include!("ickb_diff/owned_owner_transactions.rs");
include!("ickb_diff/limit_order_transactions.rs");
include!("ickb_diff/deposit_and_mint_fixtures.rs");
include!("ickb_diff/dao_withdrawal_fixtures.rs");
include!("ickb_diff/mint_from_receipt_fixtures.rs");
include!("ickb_diff/owned_owner_fixtures.rs");
include!("ickb_diff/limit_order_fixtures.rs");
