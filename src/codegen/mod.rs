use crate::aggregate_lowering::{
    action_has_group_amount_conservation_evidence, body_contains_runtime_helper, xudt_group_amount_conservation_type,
    XUDT_GROUP_AMOUNT_CONSERVED_CODEGEN_HELPER,
};
use crate::ast::{BinaryOp, ParamSource, UnaryOp};
use crate::ckb_abi;
use crate::error::{CompileError, Result};
use crate::flow::FLOW_STATE_FIELD_NAME;
use crate::ir::*;
use crate::runtime_errors::CellScriptRuntimeError;
use crate::{ArtifactFormat, TargetProfile, ENTRY_WITNESS_ABI_MAGIC};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CKB_LOAD_HEADER_SYSCALL_NUMBER: u64 = ckb_abi::syscall::LOAD_HEADER;
const CKB_LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER: u64 = ckb_abi::syscall::LOAD_HEADER_BY_FIELD;
const CKB_LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER: u64 = ckb_abi::syscall::LOAD_INPUT_BY_FIELD;
const CKB_LOAD_WITNESS_SYSCALL_NUMBER: u64 = ckb_abi::syscall::LOAD_WITNESS;
const CKB_LOAD_SCRIPT_SYSCALL_NUMBER: u64 = ckb_abi::syscall::LOAD_SCRIPT;
const CKB_LOAD_CELL_BY_FIELD_SYSCALL_NUMBER: u64 = ckb_abi::syscall::LOAD_CELL_BY_FIELD;
const CKB_LOAD_CELL_DATA_SYSCALL_NUMBER: u64 = ckb_abi::syscall::LOAD_CELL_DATA;
const CKB_LOAD_SCRIPT_HASH_SYSCALL_NUMBER: u64 = ckb_abi::syscall::LOAD_SCRIPT_HASH;
const CLAIM_SECP256K1_VERIFY_SYSCALL_NUMBER: u64 = 3002;
const CLAIM_LOAD_ECDSA_SIGNATURE_HASH_SYSCALL_NUMBER: u64 = 3004;
const CKB_HEADER_FIELD_EPOCH_NUMBER: u64 = ckb_abi::header_field::EPOCH_NUMBER;
const CKB_HEADER_FIELD_EPOCH_START_BLOCK_NUMBER: u64 = ckb_abi::header_field::EPOCH_START_BLOCK_NUMBER;
const CKB_HEADER_FIELD_EPOCH_LENGTH: u64 = ckb_abi::header_field::EPOCH_LENGTH;
const CKB_DAO_HEADER_FIELD_ABSOLUTE_OFFSET: u64 = 160;
const CKB_DAO_HEADER_ACCUMULATED_RATE_ABSOLUTE_OFFSET: u64 = 160 + 8;
const CKB_DAO_TYPE_HASH_WORDS_LE: [i64; 4] = [-8442554211429484596, 7297449809414763189, -7890662964692133976, 6381290010727626424];
const CKB_INPUT_FIELD_OUT_POINT: u64 = ckb_abi::input_field::OUT_POINT;
const CKB_INPUT_FIELD_SINCE: u64 = ckb_abi::input_field::SINCE;
const CKB_SINCE_METRIC_TYPE_FLAG_MASK: u64 = ckb_abi::since::METRIC_TYPE_FLAG_MASK;
const CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG: u64 = ckb_abi::since::EPOCH_NUMBER_WITH_FRACTION_FLAG;
const CKB_SINCE_REMAIN_FLAGS_BITS: u64 = ckb_abi::since::REMAIN_FLAGS_BITS;
const CKB_SINCE_VALUE_MASK: u64 = ckb_abi::since::VALUE_MASK;
const CKB_EPOCH_NUMBER_BOUND: u64 = ckb_abi::since::EPOCH_NUMBER_BOUND;
const CKB_EPOCH_FRACTION_BOUND: u64 = ckb_abi::since::EPOCH_FRACTION_BOUND;
const CKB_EPOCH_NUMBER_MASK: u64 = CKB_EPOCH_NUMBER_BOUND - 1;
const CKB_EPOCH_FRACTION_MASK: u64 = CKB_EPOCH_FRACTION_BOUND - 1;
const CKB_SOURCE_INPUT: u64 = ckb_abi::source::INPUT;
const CKB_SOURCE_OUTPUT: u64 = ckb_abi::source::OUTPUT;
const CKB_SOURCE_CELL_DEP: u64 = ckb_abi::source::CELL_DEP;
const CKB_SOURCE_HEADER_DEP: u64 = ckb_abi::source::HEADER_DEP;
const CKB_SOURCE_GROUP_FLAG: u64 = ckb_abi::source::GROUP_FLAG;
const CKB_SOURCE_GROUP_INPUT: u64 = ckb_abi::source::GROUP_INPUT;
const CKB_SOURCE_GROUP_OUTPUT: u64 = ckb_abi::source::GROUP_OUTPUT;
const CKB_SOURCE_VIEW_INPUT: u64 = ckb_abi::source_view::INPUT;
const CKB_SOURCE_VIEW_OUTPUT: u64 = ckb_abi::source_view::OUTPUT;
const CKB_SOURCE_VIEW_CELL_DEP: u64 = ckb_abi::source_view::CELL_DEP;
const CKB_SOURCE_VIEW_HEADER_DEP: u64 = ckb_abi::source_view::HEADER_DEP;
const CKB_SOURCE_VIEW_GROUP_INPUT: u64 = ckb_abi::source_view::GROUP_INPUT;
const CKB_SOURCE_VIEW_GROUP_OUTPUT: u64 = ckb_abi::source_view::GROUP_OUTPUT;
const CKB_SOURCE_VIEW_SHIFT: u64 = ckb_abi::source_view::SHIFT;
const CKB_ROLE_UNKNOWN: u64 = 0;
const CKB_CELL_FIELD_CAPACITY: u64 = ckb_abi::cell_field::CAPACITY;
const CKB_CELL_FIELD_LOCK: u64 = ckb_abi::cell_field::LOCK;
const CKB_CELL_FIELD_TYPE: u64 = ckb_abi::cell_field::TYPE;
const CKB_CELL_FIELD_LOCK_HASH: u64 = ckb_abi::cell_field::LOCK_HASH;
const CKB_CELL_FIELD_TYPE_HASH: u64 = ckb_abi::cell_field::TYPE_HASH;
const CKB_CELL_FIELD_OCCUPIED_CAPACITY: u64 = ckb_abi::cell_field::OCCUPIED_CAPACITY;
const CKB_INDEX_OUT_OF_BOUND: u64 = ckb_abi::syscall_error::INDEX_OUT_OF_BOUND;
const CKB_ITEM_MISSING: u64 = ckb_abi::syscall_error::ITEM_MISSING;
const CKB_LENGTH_NOT_ENOUGH: u64 = ckb_abi::syscall_error::LENGTH_NOT_ENOUGH;
#[allow(dead_code)]
const CKB_SIG_HASH_ALL: u64 = 1;
const RUNTIME_SCRATCH_BUFFER_SIZE: usize = 512;
const RUNTIME_SCRATCH_SLOT_SIZE: usize = 8 + RUNTIME_SCRATCH_BUFFER_SIZE;
const RUNTIME_SCRATCH_SIZE: usize = RUNTIME_SCRATCH_SLOT_SIZE * 2;
const RUNTIME_EXPR_TEMP_SLOTS: usize = 16;
const RUNTIME_EXPR_TEMP_SIZE: usize = RUNTIME_EXPR_TEMP_SLOTS * 8;
const RUNTIME_CELL_BUFFER_SIZE: usize = 512;
const RUNTIME_CELL_SLOT_SIZE: usize = 8 + RUNTIME_CELL_BUFFER_SIZE;
const RUNTIME_COLLECTION_BUFFER_SIZE: usize = 256;
const ENTRY_WITNESS_LABEL: &str = "_cellscript_entry";
const ENTRY_WITNESS_MAGIC: &[u8; 8] = ENTRY_WITNESS_ABI_MAGIC;
const ENTRY_WITNESS_HEADER_SIZE: usize = 8;
const ENTRY_WITNESS_BUFFER_SIZE: usize = 4096;
const ENTRY_SCRIPT_SIZE_OFFSET: usize = ENTRY_WITNESS_BUFFER_OFFSET + ENTRY_WITNESS_BUFFER_SIZE;
const ENTRY_SCRIPT_ARGS_START_OFFSET: usize = ENTRY_SCRIPT_SIZE_OFFSET + 8;
const ENTRY_SCRIPT_ARGS_LEN_OFFSET: usize = ENTRY_SCRIPT_ARGS_START_OFFSET + 8;
const ENTRY_SCRIPT_ARGS_CURSOR_OFFSET: usize = ENTRY_SCRIPT_ARGS_LEN_OFFSET + 8;
const ENTRY_SCRIPT_BUFFER_OFFSET: usize = ENTRY_SCRIPT_ARGS_CURSOR_OFFSET + 8;
const ENTRY_SCRIPT_BUFFER_SIZE: usize = 1024;
const ENTRY_WITNESS_FRAME_SIZE: usize = 5376;
const ENTRY_WITNESS_SIZE_OFFSET: usize = 0;
const ENTRY_WITNESS_BUFFER_OFFSET: usize = 8;
const ENTRY_WITNESS_RA_OFFSET: usize = ENTRY_WITNESS_FRAME_SIZE - 8;

#[derive(Debug, Clone, Copy)]
struct RuntimeSyscallAbi {
    load_header: u64,
    load_header_by_field: u64,
    load_input_by_field: u64,
    load_witness: u64,
    load_script: u64,
    load_cell_by_field: u64,
    load_cell_data: u64,
    load_script_hash: u64,
    #[allow(dead_code)]
    secp256k1_verify: u64,
    #[allow(dead_code)]
    load_ecdsa_signature_hash: u64,
    source_group_input: u64,
    source_header_dep: u64,
}

const CKB_RUNTIME_SYSCALL_ABI: RuntimeSyscallAbi = RuntimeSyscallAbi {
    load_header: CKB_LOAD_HEADER_SYSCALL_NUMBER,
    load_header_by_field: CKB_LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER,
    load_input_by_field: CKB_LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER,
    load_witness: CKB_LOAD_WITNESS_SYSCALL_NUMBER,
    load_script: CKB_LOAD_SCRIPT_SYSCALL_NUMBER,
    load_cell_by_field: CKB_LOAD_CELL_BY_FIELD_SYSCALL_NUMBER,
    load_cell_data: CKB_LOAD_CELL_DATA_SYSCALL_NUMBER,
    load_script_hash: CKB_LOAD_SCRIPT_HASH_SYSCALL_NUMBER,
    // Claim helper syscalls are rejected by CKB profile policy before codegen.
    secp256k1_verify: CLAIM_SECP256K1_VERIFY_SYSCALL_NUMBER,
    load_ecdsa_signature_hash: CLAIM_LOAD_ECDSA_SIGNATURE_HASH_SYSCALL_NUMBER,
    source_group_input: CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT,
    source_header_dep: CKB_SOURCE_HEADER_DEP,
};

fn runtime_syscall_abi(profile: TargetProfile) -> RuntimeSyscallAbi {
    match profile {
        TargetProfile::Ckb => CKB_RUNTIME_SYSCALL_ABI,
    }
}

fn referenced_v014_runtime_helpers(ir: &IrModule) -> BTreeSet<String> {
    let mut helpers = BTreeSet::new();
    for item in &ir.items {
        let body = match item {
            IrItem::Action(action) => Some(&action.body),
            IrItem::PureFn(function) => Some(&function.body),
            IrItem::Lock(lock) => Some(&lock.body),
            IrItem::TypeDef(_) | IrItem::Invariant(_) => None,
        };
        let Some(body) = body else {
            continue;
        };
        for block in &body.blocks {
            for instruction in &block.instructions {
                let IrInstruction::Call { func, .. } = instruction else {
                    continue;
                };
                if is_v014_runtime_helper(func) {
                    helpers.insert(func.clone());
                }
            }
        }
    }
    helpers.extend(auto_lowered_aggregate_runtime_helpers_by_action(ir).into_values().flatten());
    if helpers.contains("__ckb_cell_unoccupied_capacity") {
        helpers.insert("__ckb_cell_capacity".to_string());
        helpers.insert("__ckb_cell_occupied_capacity".to_string());
    }
    if helpers.contains("__ckb_require_lock_type_metapoint_pairs")
        || helpers.contains("__ckb_require_type_lock_metapoint_pairs")
        || helpers.contains("__ckb_require_lock_type_metapoint_pairs_from_i32_data")
        || helpers.contains("__ckb_require_type_lock_metapoint_pairs_from_i32_data")
        || helpers.contains("__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered")
        || helpers.contains("__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered")
        || helpers.contains("__ckb_require_lock_match_master_out_point_pairs_from_data")
    {
        helpers.insert("__ckb_require_metapoint_relative".to_string());
    }
    if helpers.contains("__xudt_require_owner_mode_type_args_current_script") {
        helpers.insert("__xudt_require_owner_mode_type_args".to_string());
    }
    if helpers.contains("__novaseal_bip340_require_signature") {
        helpers.insert("__ckb_pipe".to_string());
        helpers.insert("__ckb_pipe_write".to_string());
        helpers.insert("__ckb_close".to_string());
        helpers.insert("__ckb_spawn_with_fd1".to_string());
        helpers.insert("__ckb_wait".to_string());
    }
    helpers
}

fn auto_lowered_aggregate_runtime_helpers_by_action(ir: &IrModule) -> HashMap<String, BTreeSet<String>> {
    let invariants = ir
        .items
        .iter()
        .filter_map(|item| match item {
            IrItem::Invariant(invariant) => Some(invariant),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut by_action = HashMap::new();
    for item in &ir.items {
        let IrItem::Action(action) = item else {
            continue;
        };
        let helpers = invariants
            .iter()
            .flat_map(|invariant| {
                invariant
                    .aggregates
                    .iter()
                    .filter_map(|aggregate| auto_lowered_aggregate_runtime_helper_for_action(invariant, aggregate, action))
            })
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        if !helpers.is_empty() {
            by_action.insert(action.name.clone(), helpers);
        }
    }
    by_action
}

fn auto_lowered_aggregate_runtime_helper_for_action(
    invariant: &IrInvariant,
    aggregate: &IrAggregateInvariant,
    action: &IrAction,
) -> Option<&'static str> {
    let type_name = xudt_group_amount_conservation_type(invariant, aggregate)?;
    if body_contains_runtime_helper(&action.body, XUDT_GROUP_AMOUNT_CONSERVED_CODEGEN_HELPER) {
        return None;
    }
    action_has_group_amount_conservation_evidence(action, type_name).then_some(XUDT_GROUP_AMOUNT_CONSERVED_CODEGEN_HELPER)
}

fn is_v014_runtime_helper(func: &str) -> bool {
    matches!(
        func,
        "__ckb_spawn"
            | "__ckb_wait"
            | "__ckb_process_id"
            | "__ckb_pipe"
            | "__ckb_pipe_write"
            | "__ckb_pipe_read"
            | "__ckb_inherited_fd"
            | "__ckb_close"
            | "__ckb_spawn_with_fd1"
            | "__ckb_source_input"
            | "__ckb_source_output"
            | "__ckb_source_cell_dep"
            | "__ckb_source_header_dep"
            | "__ckb_source_group_input"
            | "__ckb_source_group_output"
            | "__ckb_since_epoch_absolute"
            | "__ckb_since_epoch_relative"
            | "__ckb_current_role"
            | "__ckb_current_script_hash"
            | "__ckb_cell_capacity"
            | "__ckb_cell_occupied_capacity"
            | "__ckb_cell_unoccupied_capacity"
            | "__ckb_cell_output_index"
            | "__ckb_input_out_point_index"
            | "__ckb_input_out_point_tx_hash_low"
            | "__ckb_input_out_point_tx_hash"
            | "__ckb_require_input_out_point_tx_hash"
            | "__ckb_require_input_out_point"
            | "__ckb_require_metapoint_relative"
            | "__ckb_require_lock_type_metapoint_pairs"
            | "__ckb_require_type_lock_metapoint_pairs"
            | "__ckb_require_lock_type_metapoint_pairs_from_i32_data"
            | "__ckb_require_type_lock_metapoint_pairs_from_i32_data"
            | "__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered"
            | "__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered"
            | "__ckb_require_lock_match_master_out_point_pairs_from_data"
            | "__ckb_cell_lock_hash_low"
            | "__ckb_cell_type_hash_low"
            | "__ckb_cell_lock_hash"
            | "__ckb_cell_type_hash"
            | "__ckb_cell_data_hash"
            | "__ckb_cell_data_hash_at"
            | "__ckb_cell_lock_code_hash"
            | "__ckb_cell_type_code_hash"
            | "__ckb_cell_lock_hash_type"
            | "__ckb_cell_type_hash_type"
            | "__ckb_cell_lock_args_empty"
            | "__ckb_cell_type_args_empty"
            | "__ckb_cell_lock_args_hash"
            | "__ckb_cell_type_args_hash"
            | "__ckb_require_cell_lock_hash"
            | "__ckb_require_cell_type_hash"
            | "__ckb_require_current_script_args_empty"
            | "__ckb_require_cell_lock_args_empty"
            | "__ckb_require_cell_type_args_empty"
            | "__ckb_require_cell_lock_args_hash"
            | "__ckb_require_cell_type_args_hash"
            | "__ckb_require_cell_lock_args_exact"
            | "__ckb_require_cell_type_args_exact"
            | "__ckb_require_cell_lock_args_prefix_hash"
            | "__ckb_require_cell_type_args_prefix_hash"
            | "__ckb_require_cell_lock_args_suffix_hash"
            | "__ckb_require_cell_type_args_suffix_hash"
            | "__ckb_require_cell_lock_script_hash_type"
            | "__ckb_require_cell_type_script_hash_type"
            | "__c256_require_u128_product_lte"
            | "__c256_require_u128_product_eq"
            | "__c256_require_u128_sum2_products_lte"
            | "__c256_require_u128_sum2_products_eq"
            | "__ckb_cell_data_size"
            | "__ckb_cell_data_u32_le"
            | "__ckb_cell_data_u64_le"
            | "__dao_accumulated_rate"
            | "__dao_input_accumulated_rate"
            | "__dao_has_dao_type"
            | "__dao_is_deposit_data"
            | "__dao_is_withdrawal_request_data"
            | "__dao_require_header_dep_for_input"
            | "__dao_require_input_since_at_least"
            | "__dao_require_input_relative_epoch_since_at_least"
            | "__xudt_amount_low"
            | "__xudt_amount_high"
            | "__xudt_owner_mode_input_type_hash"
            | "__xudt_require_owner_mode_input_type"
            | "__xudt_require_owner_mode_type_args"
            | "__xudt_require_owner_mode_type_args_current_script"
            | "__xudt_require_group_amount_conserved"
            | "__xudt_require_group_amount_minted"
            | "__xudt_require_group_amount_burned"
            | "__ckb_witness_raw"
            | "__ckb_witness_lock"
            | "__ckb_witness_input_type"
            | "__ckb_witness_output_type"
            | "__ckb_witness_size"
            | "__ckb_require_witness_size_at_least"
            | "__ckb_sighash_all"
            | "__ckb_require_maturity"
            | "__ckb_require_time"
            | "__ckb_require_epoch_after"
            | "__ckb_require_epoch_relative"
            | "__ckb_occupied_capacity"
            | "__ckb_hash_chain"
            | "__ckb_hash_pair"
            | "__ckb_hash_blake2b"
            | "__ckb_hash_blake2b_var"
            | "__ckb_hash_blake2b_packed"
            | "__ckb_hash_data_packed"
            | "__novaseal_bip340_require_signature"
    )
}

fn is_ckb_fixed_hash_helper(func: &str) -> bool {
    matches!(
        func,
        "__ckb_hash_chain"
            | "__ckb_hash_pair"
            | "__ckb_hash_blake2b"
            | "__ckb_hash_blake2b_var"
            | "__ckb_hash_blake2b_packed"
            | "__ckb_hash_data_packed"
    )
}

#[derive(Debug, Clone)]
struct SchemaFieldLayout {
    index: usize,
    offset: usize,
    ty: IrType,
    fixed_size: Option<usize>,
    fixed_enum_size: Option<usize>,
}

#[derive(Debug, Clone)]
struct SchemaFieldValueSource {
    obj_var_id: usize,
    type_name: String,
    field: String,
    layout: SchemaFieldLayout,
}

#[derive(Debug, Clone)]
struct AggregatePointerSource {
    ty: IrType,
}

#[derive(Debug, Clone)]
enum ExpectedFixedByteSource {
    SchemaField(SchemaFieldValueSource),
    Const(Vec<u8>),
    StackSlot { var_id: usize, width: usize },
    PointerBytes { var_id: usize, width: usize },
    ParamBytes { var_id: usize, size_offset: usize, width: usize },
    LoadedBytes { var_id: usize, size_offset: usize, width: usize },
}

#[derive(Debug, Clone, Copy)]
enum ScriptHashFieldRead {
    CodeHash,
    Args32,
}

#[derive(Debug, Clone, Copy)]
enum ScriptScalarFieldRead {
    HashType,
    ArgsEmpty,
}

#[derive(Debug, Clone, Copy)]
enum ScriptArgsHashRequirementMode {
    Exact32,
    Prefix32,
    Suffix32,
}

#[derive(Debug, Clone, Copy)]
enum SourcePointer {
    LoadedStackPointer { var_id: usize, offset: usize },
    StackAddress { offset: usize },
}

fn fixed_scalar_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    match (ty, fixed_size) {
        (IrType::Bool | IrType::U8, Some(1)) => Some(1),
        (IrType::U16, Some(2)) => Some(2),
        (IrType::U32, Some(4)) => Some(4),
        (IrType::I32, Some(4)) => Some(4),
        (IrType::U64, Some(8)) => Some(8),
        _ => None,
    }
}

fn is_fixed_scalar_ir_type(ty: &IrType) -> bool {
    matches!(ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64)
}

fn identity_policy_label(identity: &IrIdentityPolicy) -> String {
    match identity {
        IrIdentityPolicy::None => "none".to_string(),
        IrIdentityPolicy::CkbTypeId => "ckb_type_id".to_string(),
        IrIdentityPolicy::Field(path) => format!("field({})", path),
        IrIdentityPolicy::ScriptArgs => "script_args".to_string(),
        IrIdentityPolicy::SingletonType => "singleton_type".to_string(),
    }
}

/// Fixed-width types that fit in a single RISC-V 64-bit register (≤8 bytes).
/// Used by transition formula verification which needs scalar add/sub.
fn fixed_register_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    let w = fixed_scalar_width(ty, fixed_size)?;
    (w <= 8).then_some(w)
}

fn fixed_byte_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    if let Some(width) = fixed_scalar_width(ty, fixed_size) {
        return Some(width);
    }
    match (ty, fixed_size) {
        (IrType::Address | IrType::Hash, Some(32)) => Some(32),
        (IrType::U128, Some(16)) => Some(16),
        (IrType::Array(inner, len), Some(size)) if matches!(inner.as_ref(), IrType::U8) && *len == size => Some(size),
        (IrType::Ref(inner) | IrType::MutRef(inner), _) => fixed_byte_width(inner, type_static_length(inner)),
        _ => None,
    }
}

fn molecule_vector_element_fixed_width(
    ty: &IrType,
    type_fixed_sizes: &HashMap<String, usize>,
    enum_fixed_sizes: &HashMap<String, usize>,
) -> Option<usize> {
    let IrType::Named(name) = ty else {
        return None;
    };
    if name == "String" {
        return Some(1);
    }
    let inner = name.strip_prefix("Vec<")?.strip_suffix('>')?;
    molecule_inline_type_fixed_width(inner, type_fixed_sizes, enum_fixed_sizes)
}

fn molecule_inline_type_fixed_width(
    ty: &str,
    type_fixed_sizes: &HashMap<String, usize>,
    enum_fixed_sizes: &HashMap<String, usize>,
) -> Option<usize> {
    match ty.trim() {
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" => Some(4),
        "i32" => Some(4),
        "u64" => Some(8),
        "u128" => Some(16),
        "Address" | "Hash" => Some(32),
        other => type_fixed_sizes.get(other).copied().or_else(|| enum_fixed_sizes.get(other).copied()),
    }
}

fn layout_fixed_scalar_width(layout: &SchemaFieldLayout) -> Option<usize> {
    fixed_scalar_width(&layout.ty, layout.fixed_size).or(layout.fixed_enum_size)
}

fn layout_fixed_byte_width(layout: &SchemaFieldLayout) -> Option<usize> {
    fixed_byte_width(&layout.ty, layout.fixed_size).or(layout.fixed_enum_size)
}

fn type_static_length(ty: &IrType) -> Option<usize> {
    match ty {
        IrType::Bool | IrType::U8 => Some(1),
        IrType::U16 => Some(2),
        IrType::U32 => Some(4),
        IrType::I32 => Some(4),
        IrType::U64 => Some(8),
        IrType::U128 => Some(16),
        IrType::Address | IrType::Hash => Some(32),
        IrType::Array(inner, len) => type_static_length(inner).map(|inner_len| inner_len * len),
        IrType::Tuple(items) => items.iter().try_fold(0usize, |acc, item| type_static_length(item).map(|len| acc + len)),
        IrType::Unit => Some(0),
        IrType::Ref(inner) | IrType::MutRef(inner) => type_static_length(inner),
        IrType::Named(_) => None,
    }
}

fn operand_fixed_byte_width(operand: &IrOperand) -> Option<usize> {
    let ty = match operand {
        IrOperand::Const(IrConst::Address(_)) | IrOperand::Const(IrConst::Hash(_)) => return Some(32),
        IrOperand::Const(IrConst::Array(values)) => return Some(values.len()),
        IrOperand::Const(IrConst::U128(_)) => return Some(16),
        IrOperand::Var(var) => &var.ty,
        _ => return None,
    };
    match ty {
        IrType::Address | IrType::Hash => Some(32),
        IrType::U128 => Some(16),
        IrType::Array(inner, len) if matches!(inner.as_ref(), IrType::U8) => Some(*len),
        _ => None,
    }
}

fn constructed_byte_vector_part_width(operand: &IrOperand) -> Option<usize> {
    operand_fixed_byte_width(operand).or_else(|| match operand {
        IrOperand::Var(var) => fixed_scalar_width(&var.ty, type_static_length(&var.ty)),
        IrOperand::Const(IrConst::Bool(_)) | IrOperand::Const(IrConst::U8(_)) => Some(1),
        IrOperand::Const(IrConst::U16(_)) => Some(2),
        IrOperand::Const(IrConst::U32(_)) => Some(4),
        IrOperand::Const(IrConst::U64(_)) => Some(8),
        _ => None,
    })
}

fn fixed_scalar_operand_width(operand: &IrOperand) -> Option<usize> {
    match operand {
        IrOperand::Var(var) => fixed_scalar_width(&var.ty, type_static_length(&var.ty)),
        IrOperand::Const(IrConst::Bool(_)) | IrOperand::Const(IrConst::U8(_)) => Some(1),
        IrOperand::Const(IrConst::U16(_)) => Some(2),
        IrOperand::Const(IrConst::U32(_)) => Some(4),
        IrOperand::Const(IrConst::U64(_)) => Some(8),
        _ => None,
    }
}

fn operand_is_signed_i32(operand: &IrOperand) -> bool {
    matches!(operand, IrOperand::Var(var) if var.ty == IrType::I32)
}

fn binary_operands_signed_i32(left: &IrOperand, right: &IrOperand) -> bool {
    operand_is_signed_i32(left) || operand_is_signed_i32(right)
}

fn collect_pure_const_returns(ir: &IrModule) -> HashMap<String, IrConst> {
    ir.items
        .iter()
        .filter_map(|item| {
            let IrItem::PureFn(function) = item else {
                return None;
            };
            pure_const_return(&function.body).map(|value| (function.name.clone(), value))
        })
        .collect()
}

fn pure_const_return(body: &IrBody) -> Option<IrConst> {
    let [block] = body.blocks.as_slice() else {
        return None;
    };
    match (&block.instructions[..], &block.terminator) {
        ([], IrTerminator::Return(Some(IrOperand::Const(value)))) => Some(value.clone()),
        ([IrInstruction::LoadConst { dest, value }], IrTerminator::Return(Some(IrOperand::Var(var)))) if dest.id == var.id => {
            Some(value.clone())
        }
        _ => None,
    }
}

fn fixed_byte_pointer_param_width(ty: &IrType) -> Option<usize> {
    fixed_byte_width(ty, type_static_length(ty)).filter(|width| *width > 8)
}

fn fixed_aggregate_pointer_param_width(ty: &IrType) -> Option<usize> {
    match ty {
        IrType::Array(_, _) | IrType::Tuple(_) => type_static_length(ty).filter(|width| *width > 8),
        _ => None,
    }
}

fn fixed_byte_const_bytes(value: &IrConst) -> Option<Vec<u8>> {
    match value {
        IrConst::Address(bytes) | IrConst::Hash(bytes) => Some(bytes.to_vec()),
        IrConst::U128(value) => Some(value.to_le_bytes().to_vec()),
        IrConst::Array(values) => values
            .iter()
            .map(|value| match value {
                IrConst::U8(byte) => Some(*byte),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

fn fixed_scalar_const_value(value: &IrConst) -> Option<u64> {
    match value {
        IrConst::Bool(value) => Some(u64::from(*value)),
        IrConst::U8(value) => Some((*value).into()),
        IrConst::U16(value) => Some((*value).into()),
        IrConst::U32(value) => Some((*value).into()),
        IrConst::U64(value) => Some(*value),
        _ => None,
    }
}

fn const_usize_operand(operand: &IrOperand) -> Option<usize> {
    match operand {
        IrOperand::Const(IrConst::U8(value)) => Some((*value).into()),
        IrOperand::Const(IrConst::U16(value)) => Some((*value).into()),
        IrOperand::Const(IrConst::U32(value)) => Some(*value as usize),
        IrOperand::Const(IrConst::U64(value)) => usize::try_from(*value).ok(),
        _ => None,
    }
}

fn aggregate_type_label(ty: &IrType) -> String {
    match ty {
        IrType::Tuple(_) => "tuple".to_string(),
        IrType::Array(_, len) => format!("array{}", len),
        IrType::Address => "Address".to_string(),
        IrType::Hash => "Hash".to_string(),
        other => format!("{:?}", other),
    }
}

fn aggregate_field_layout(ty: &IrType, field: &str) -> Option<SchemaFieldLayout> {
    match ty {
        IrType::Tuple(items) => {
            let index = field.parse::<usize>().ok()?;
            let field_ty = items.get(index)?.clone();
            let offset = items.iter().take(index).try_fold(0usize, |acc, item| type_static_length(item).map(|size| acc + size))?;
            let fixed_size = type_static_length(&field_ty);
            Some(SchemaFieldLayout { index, offset, ty: field_ty, fixed_size, fixed_enum_size: None })
        }
        IrType::Address | IrType::Hash if field == "0" => Some(SchemaFieldLayout {
            index: 0,
            offset: 0,
            ty: IrType::Array(Box::new(IrType::U8), 32),
            fixed_size: Some(32),
            fixed_enum_size: None,
        }),
        _ => None,
    }
}

fn tuple_return_field_type(ty: &IrType, field: &str) -> Option<IrType> {
    let IrType::Tuple(items) = ty else {
        return None;
    };
    let index = field.parse::<usize>().ok()?;
    (index < 8).then(|| items.get(index).cloned()).flatten()
}

fn abi_arg_label(index: usize) -> String {
    if index < 8 {
        format!("a{}", index)
    } else {
        format!("stack+{}", (index - 8) * 8)
    }
}

fn call_abi_arg_count(abi: Option<&CallableAbi>, args: &[IrOperand]) -> usize {
    let mut count = 0usize;
    for (arg_index, _) in args.iter().enumerate() {
        if let Some(abi) = abi {
            if let Some(param) = abi.params.get(arg_index) {
                count += call_param_abi_arg_count(param, abi.type_hash_param_indices.contains(&arg_index));
                continue;
            }
        }
        count += 1;
    }
    count
}

fn entry_abi_arg_count(params: &[IrParam], abi: Option<&CallableAbi>) -> usize {
    let type_hash_param_indices = abi.map(|abi| &abi.type_hash_param_indices);
    params
        .iter()
        .enumerate()
        .map(|(index, param)| call_param_abi_arg_count(param, type_hash_param_indices.is_some_and(|indices| indices.contains(&index))))
        .sum()
}

fn align_stack_arg_bytes(bytes: usize) -> usize {
    if bytes == 0 {
        0
    } else {
        bytes.next_multiple_of(16)
    }
}

fn call_param_abi_arg_count(param: &IrParam, needs_type_hash: bool) -> usize {
    if named_type_name(&param.ty).is_some() {
        return 2 + usize::from(needs_type_hash) * 2;
    }
    if fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty)).is_some() {
        return 2;
    }
    1
}

#[derive(Debug, Clone)]
enum PreludeU64OperandSource {
    Const(u64),
    ParamVar(usize),
    StackVar(usize),
    Field(SchemaFieldValueSource),
    Expr(Box<PreludeU64ValueSource>),
}

#[derive(Debug, Clone)]
enum PreludeU64ValueSource {
    Const(u64),
    ParamVar(usize),
    StackVar(usize),
    Field(SchemaFieldValueSource),
    Binary { op: BinaryOp, left: Box<PreludeU64ValueSource>, right: PreludeU64OperandSource },
    Min { left: Box<PreludeU64ValueSource>, right: PreludeU64OperandSource },
}

#[derive(Debug, Clone)]
struct CallableAbi {
    params: Vec<IrParam>,
    type_hash_param_indices: BTreeSet<usize>,
    runtime_bound_param_indices: BTreeSet<usize>,
}

#[derive(Debug, Clone, Copy)]
enum CallLengthKind {
    Schema,
    FixedBytes,
}

#[derive(Debug, Clone, Copy)]
struct EntryWitnessPayloadArg {
    width: usize,
    schema_dynamic: bool,
    unsupported: bool,
}

#[derive(Debug, Clone)]
pub struct CodegenOptions {
    pub opt_level: u8,
    pub debug: bool,
    /// Artifact target profile. CKB selects the CKB syscall/source ABI.
    pub target_profile: TargetProfile,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self { opt_level: 0, debug: false, target_profile: TargetProfile::Ckb }
    }
}

pub struct CodeGenerator {
    options: CodegenOptions,
    assembly: Vec<String>,
    current_function: Option<String>,
    frame_size: usize,
    next_virtual_output: usize,
    /// Stack-frame start offset for runtime collection buffers.
    collection_region_start: usize,
    /// Runtime collection buffer allocator for the current function.
    next_collection_slot: usize,
    /// Named schema field layouts, keyed by type name then field name.
    type_layouts: HashMap<String, HashMap<String, SchemaFieldLayout>>,
    /// Fieldless enum storage widths, keyed by enum name.
    enum_fixed_sizes: HashMap<String, usize>,
    /// Fixed encoded size of named schemas when all fields have fixed-width layouts.
    type_fixed_sizes: HashMap<String, usize>,
    /// Named types declared as receipts.
    receipt_type_names: BTreeSet<String>,
    /// Named types that are transaction cell-backed values.
    cell_type_names: BTreeSet<String>,
    /// State names for schemas that declared flow policy.
    flow_states: HashMap<String, Vec<String>>,
    /// Flow field name keyed by schema type.
    flow_state_fields: HashMap<String, String>,
    /// Declared flow/flow transition graph keyed by schema type.
    flow_rules: HashMap<String, Vec<IrFlowRule>>,
    /// Action-specific state edges for the function currently being emitted.
    current_state_transition_edges: Vec<IrStateTransitionEdge>,
    /// Runtime helpers emitted in action preludes by compiler-lowered aggregate invariants.
    auto_aggregate_runtime_helpers_by_action: HashMap<String, BTreeSet<String>>,
    /// ABI summaries for locally emitted actions/functions/locks.
    callable_abis: HashMap<String, CallableAbi>,
    /// Function parameters whose slot contains a pointer to encoded schema bytes.
    schema_pointer_vars: BTreeSet<usize>,
    /// Function parameter slots available before the prelude summaries run.
    param_vars: BTreeSet<usize>,
    /// Schema pointer slots backed by a VM-loaded cell buffer size word.
    schema_pointer_size_offsets: HashMap<usize, usize>,
    /// Fixed-byte parameter pointer slots backed by a separate ABI length word.
    fixed_byte_param_size_offsets: HashMap<usize, usize>,
    /// Fixed-width aggregate pointer slots backed by ABI bytes, keyed by IR variable id.
    aggregate_pointer_sources: HashMap<usize, AggregatePointerSource>,
    /// Tuple-valued call results that can be projected from RISC-V return registers.
    tuple_call_return_vars: HashMap<usize, IrType>,
    /// Stack slots populated from tuple call return registers, keyed by `(tuple_var_id, field)`.
    tuple_call_return_field_slots: HashMap<(usize, String), usize>,
    /// Tuple aggregate fields produced in the current function body, keyed by tuple var id.
    tuple_aggregate_fields: HashMap<usize, Vec<IrOperand>>,
    /// Fixed scalar temporaries that are aliases for schema-backed field loads.
    schema_field_value_sources: HashMap<usize, SchemaFieldValueSource>,
    /// U64 temporaries that can be recomputed in the CKB-runtime prelude.
    prelude_u64_value_sources: HashMap<usize, PreludeU64ValueSource>,
    /// Fixed scalar temporaries that can be recomputed as immediates in the CKB-runtime prelude.
    prelude_scalar_immediates: HashMap<usize, u64>,
    /// Fixed-byte constant temporaries that can be recomputed byte-by-byte in the CKB-runtime prelude.
    prelude_fixed_byte_constants: HashMap<usize, Vec<u8>>,
    /// Function-local 16-byte storage for materialized u128 values.
    u128_value_offsets: HashMap<usize, usize>,
    /// Function-local fixed-byte storage for wide scalar temporaries such as u128.
    fixed_byte_local_offsets: HashMap<usize, usize>,
    /// Named IR variable slots used by StoreVar/LoadVar instructions.
    named_var_offsets: HashMap<String, usize>,
    /// Deduplicated immutable byte constants emitted into .rodata.
    const_data_labels: HashMap<Vec<u8>, String>,
    const_data_entries: Vec<(String, Vec<u8>)>,
    /// Local pure functions proven to return one constant on every path.
    pure_const_returns: HashMap<String, IrConst>,
    /// Per-CKB-runtime cell data buffers keyed by IR variable id.
    cell_buffer_offsets: HashMap<usize, usize>,
    /// Per-CKB-runtime cell size words keyed by IR variable id.
    cell_buffer_size_offsets: HashMap<usize, usize>,
    /// Byte-size slots for dynamic Molecule values projected from schema table fields.
    dynamic_value_size_offsets: HashMap<usize, usize>,
    /// Empty collection temporaries that can be verified as empty Molecule vectors.
    empty_molecule_vector_vars: BTreeSet<usize>,
    /// Stack-backed local collection variables whose length word and buffer are emitted in this frame.
    stack_collection_vars: BTreeSet<usize>,
    /// Locally constructed `Vec<u8>` bytes keyed by collection variable id.
    constructed_byte_vectors: HashMap<usize, Vec<IrOperand>>,
    /// Root `CollectionNew` variable for aliases of locally constructed vectors.
    constructed_byte_vector_roots: HashMap<usize, usize>,
    /// Collection variable ids whose full construction is covered by create-output vector verification.
    verified_collection_construction_vectors: BTreeSet<usize>,
    /// `type_hash()` temporaries that can be loaded from a created Output cell's TypeHash field.
    output_type_hash_sources: HashMap<usize, usize>,
    /// Schema parameter TypeHash pointer slots, keyed by source parameter variable id.
    param_type_hash_pointer_offsets: HashMap<usize, usize>,
    /// Schema parameter TypeHash length slots, keyed by source parameter variable id.
    param_type_hash_size_offsets: HashMap<usize, usize>,
    /// `type_hash()` temporaries backed by trusted parameter TypeHash ABI bytes.
    param_type_hash_sources: HashMap<usize, usize>,
    /// Consumed IR operand variable ids in source lowering order.
    consume_order: Vec<usize>,
    /// Consumed Input index keyed by IR operand variable id.
    consume_indices: HashMap<usize, usize>,
    /// Consumed named schema type keyed by IR operand variable id.
    consume_type_names: HashMap<usize, String>,
    /// Consumed IR operand variable id keyed by source binding name.
    consume_binding_ids: HashMap<String, usize>,
    /// Read-ref IR destination variable ids in source lowering order.
    read_ref_order: Vec<usize>,
    /// Read-ref CellDep index keyed by IR destination variable id.
    read_ref_indices: HashMap<usize, usize>,
    /// Read-only schema parameter variable ids keyed by source binding name.
    read_ref_param_ids: HashMap<String, usize>,
    /// CKB Input index for read-only schema parameters keyed by IR variable id.
    read_ref_param_input_indices: HashMap<usize, usize>,
    /// CKB CellDep index for read_ref schema parameters keyed by IR variable id.
    read_ref_param_dep_indices: HashMap<usize, usize>,
    /// Proposed transaction Output parameter variable ids keyed by source binding name.
    output_param_ids: HashMap<String, usize>,
    /// Whether the current entry function should bind read-only schema params from Inputs.
    bind_readonly_schema_params: bool,
    /// Whether the current function is a CKB lock predicate entry.
    current_lock_entry: bool,
    /// Mutable schema parameter variable ids keyed by source binding name.
    mutate_param_ids: HashMap<String, usize>,
    /// Output index for source-level operations that materialize transaction Outputs.
    operation_output_indices: HashMap<usize, usize>,
    /// Operation destination ids whose transaction Output relation is fully verifier-covered.
    verified_operation_outputs: BTreeSet<usize>,
    /// Collection push value ids whose effect is covered by a mutate append verifier.
    verified_collection_push_values: BTreeSet<usize>,
    /// Function-local cold fail handlers keyed by returned verifier error code.
    fail_handler_codes: BTreeSet<CellScriptRuntimeError>,
    /// Unique label counter for runtime checks.
    next_runtime_label: usize,
}

impl CodeGenerator {
    fn fixed_named_type_width(&self, ty: &IrType) -> Option<usize> {
        match ty {
            IrType::Named(name) => self.type_fixed_sizes.get(name).copied().or_else(|| self.enum_fixed_sizes.get(name).copied()),
            IrType::Ref(inner) | IrType::MutRef(inner) => self.fixed_named_type_width(inner),
            _ => None,
        }
    }

    fn fixed_byte_like_width(&self, ty: &IrType) -> Option<usize> {
        fixed_byte_width(ty, type_static_length(ty)).or_else(|| self.fixed_named_type_width(ty))
    }

    fn const_data_label_for_bytes(&mut self, bytes: Vec<u8>) -> String {
        if let Some(label) = self.const_data_labels.get(&bytes) {
            return label.clone();
        }
        let label = format!("__cellscript_const_data_{}", self.const_data_entries.len());
        self.const_data_labels.insert(bytes.clone(), label.clone());
        self.const_data_entries.push((label.clone(), bytes));
        label
    }

    fn emit_const_data_pool(&mut self) {
        if self.const_data_entries.is_empty() {
            return;
        }
        self.emit_section(".rodata");
        for (label, bytes) in self.const_data_entries.clone() {
            self.emit_label(&label);
            for byte in bytes {
                self.emit(format!(".byte {}", byte));
            }
            self.emit(".align 3");
        }
    }

    fn constructed_byte_vector_part_width(&self, operand: &IrOperand) -> Option<usize> {
        constructed_byte_vector_part_width(operand).or_else(|| match operand {
            IrOperand::Var(var) => self.fixed_named_type_width(&var.ty),
            _ => None,
        })
    }

    fn param_is_runtime_bound(&self, param: &IrParam) -> bool {
        param.source == ParamSource::LockArgs
            || param.is_ref
            || named_type_name(&param.ty).is_some_and(|name| self.cell_type_names.contains(name))
    }

    pub fn new(options: CodegenOptions) -> Self {
        Self {
            options,
            assembly: Vec::new(),
            current_function: None,
            frame_size: 16,
            next_virtual_output: 0,
            collection_region_start: 0,
            next_collection_slot: 0,
            type_layouts: HashMap::new(),
            enum_fixed_sizes: HashMap::new(),
            type_fixed_sizes: HashMap::new(),
            receipt_type_names: BTreeSet::new(),
            cell_type_names: BTreeSet::new(),
            flow_states: HashMap::new(),
            flow_state_fields: HashMap::new(),
            flow_rules: HashMap::new(),
            current_state_transition_edges: Vec::new(),
            auto_aggregate_runtime_helpers_by_action: HashMap::new(),
            callable_abis: HashMap::new(),
            schema_pointer_vars: BTreeSet::new(),
            param_vars: BTreeSet::new(),
            schema_pointer_size_offsets: HashMap::new(),
            fixed_byte_param_size_offsets: HashMap::new(),
            aggregate_pointer_sources: HashMap::new(),
            tuple_call_return_vars: HashMap::new(),
            tuple_call_return_field_slots: HashMap::new(),
            tuple_aggregate_fields: HashMap::new(),
            schema_field_value_sources: HashMap::new(),
            prelude_u64_value_sources: HashMap::new(),
            prelude_scalar_immediates: HashMap::new(),
            prelude_fixed_byte_constants: HashMap::new(),
            u128_value_offsets: HashMap::new(),
            fixed_byte_local_offsets: HashMap::new(),
            named_var_offsets: HashMap::new(),
            const_data_labels: HashMap::new(),
            const_data_entries: Vec::new(),
            pure_const_returns: HashMap::new(),
            cell_buffer_offsets: HashMap::new(),
            cell_buffer_size_offsets: HashMap::new(),
            dynamic_value_size_offsets: HashMap::new(),
            empty_molecule_vector_vars: BTreeSet::new(),
            stack_collection_vars: BTreeSet::new(),
            constructed_byte_vectors: HashMap::new(),
            constructed_byte_vector_roots: HashMap::new(),
            verified_collection_construction_vectors: BTreeSet::new(),
            output_type_hash_sources: HashMap::new(),
            param_type_hash_pointer_offsets: HashMap::new(),
            param_type_hash_size_offsets: HashMap::new(),
            param_type_hash_sources: HashMap::new(),
            consume_order: Vec::new(),
            consume_indices: HashMap::new(),
            consume_type_names: HashMap::new(),
            consume_binding_ids: HashMap::new(),
            read_ref_order: Vec::new(),
            read_ref_indices: HashMap::new(),
            read_ref_param_ids: HashMap::new(),
            read_ref_param_input_indices: HashMap::new(),
            read_ref_param_dep_indices: HashMap::new(),
            output_param_ids: HashMap::new(),
            bind_readonly_schema_params: false,
            current_lock_entry: false,
            mutate_param_ids: HashMap::new(),
            operation_output_indices: HashMap::new(),
            verified_operation_outputs: BTreeSet::new(),
            verified_collection_push_values: BTreeSet::new(),
            fail_handler_codes: BTreeSet::new(),
            next_runtime_label: 0,
        }
    }

    fn runtime_abi(&self) -> RuntimeSyscallAbi {
        runtime_syscall_abi(self.options.target_profile)
    }

    pub fn generate(mut self, ir: &IrModule, format: ArtifactFormat) -> Result<Vec<u8>> {
        let has_entrypoint = ir.items.iter().any(|item| matches!(item, IrItem::Action(_) | IrItem::Lock(_)));
        self.enum_fixed_sizes = ir.enum_fixed_sizes.clone();
        self.pure_const_returns = collect_pure_const_returns(ir);
        self.auto_aggregate_runtime_helpers_by_action = auto_lowered_aggregate_runtime_helpers_by_action(ir);
        for item in &ir.items {
            if let IrItem::TypeDef(type_def) = item {
                self.register_type_def(type_def);
            }
        }
        for type_def in &ir.external_type_defs {
            self.register_type_def(type_def);
        }
        self.register_callable_abis(ir);

        self.emit_header();

        for item in &ir.items {
            if let IrItem::TypeDef(type_def) = item {
                self.generate_type_def(type_def)?;
            }
        }

        self.emit_section(".text");
        if let Some((entry_name, entry_params)) = first_entrypoint(ir) {
            if entry_params.is_empty() {
                self.emit_entry_direct_wrapper(entry_name);
            } else {
                self.emit_entry_witness_wrapper(entry_name, entry_params)?;
            }
        }

        for item in &ir.items {
            if let IrItem::Action(action) = item {
                self.generate_action(action)?;
            }
        }
        for item in &ir.items {
            if let IrItem::Lock(lock) = item {
                self.generate_lock(lock)?;
            }
        }
        if has_entrypoint {
            for item in &ir.items {
                if let IrItem::PureFn(function) = item {
                    self.generate_pure_fn(function)?;
                }
            }
        }

        self.generate_runtime_support(ir);
        self.emit_const_data_pool();

        self.assemble(format)
    }

    fn emit_header(&mut self) {
        self.assembly.push("# CellScript Generated Assembly".to_string());
        self.assembly.push(format!("# opt_level={}, debug={}", self.options.opt_level, self.options.debug));
        self.assembly.push(".option arch, +rv64imac".to_string());
        self.assembly.push("".to_string());
    }

    fn emit_section(&mut self, section: &str) {
        self.assembly.push(format!(".section {}", section));
    }

    fn emit_global(&mut self, name: &str) {
        self.assembly.push(format!(".global {}", name));
        self.assembly.push(format!(".type {}, @function", name));
    }

    fn emit_label(&mut self, name: &str) {
        self.assembly.push(format!("{}:", name));
    }

    fn block_label(&self, block_id: BlockId) -> String {
        format!(".L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), block_id.0)
    }

    fn emit_jump_to_block(&mut self, block_id: BlockId, fallthrough: Option<BlockId>) {
        if Some(block_id) != fallthrough {
            self.emit(format!("j {}", self.block_label(block_id)));
        }
    }

    fn emit(&mut self, instruction: impl Into<String>) {
        let instruction = instruction.into();
        if self.emit_large_immediate_access_if_needed(&instruction) {
            return;
        }
        self.assembly.push(format!("    {}", instruction));
    }

    fn emit_large_immediate_access_if_needed(&mut self, instruction: &str) -> bool {
        let Some(clean) = strip_comment(instruction) else {
            return false;
        };
        if clean.is_empty() || clean.starts_with('.') || clean.ends_with(':') {
            return false;
        }

        let mut parts = clean.splitn(2, char::is_whitespace);
        let opcode = parts.next().unwrap_or_default();
        let args = parts.next().unwrap_or("").trim();
        let args = if args.is_empty() { Vec::new() } else { args.split(',').map(str::trim).collect::<Vec<_>>() };

        match opcode {
            "ld" | "lbu" if args.len() == 2 => {
                let Some((offset, base)) = memory_operand_offset_and_base(args[1]) else {
                    return false;
                };
                if parse_register(args[0]).is_err() || parse_register(base).is_err() {
                    return false;
                }
                if small_signed_immediate(offset) {
                    return false;
                }
                let scratch = scratch_register_avoiding(&[args[0], base]);
                self.assembly.push(format!("    li {}, {}", scratch, offset));
                self.assembly.push(format!("    add {}, {}, {}", scratch, base, scratch));
                self.assembly.push(format!("    {} {}, 0({})", opcode, args[0], scratch));
                true
            }
            "sb" | "sh" | "sw" | "sd" if args.len() == 2 => {
                let Some((offset, base)) = memory_operand_offset_and_base(args[1]) else {
                    return false;
                };
                if parse_register(args[0]).is_err() || parse_register(base).is_err() {
                    return false;
                }
                if small_signed_immediate(offset) {
                    return false;
                }
                let scratch = scratch_register_avoiding(&[args[0], base]);
                self.assembly.push(format!("    li {}, {}", scratch, offset));
                self.assembly.push(format!("    add {}, {}, {}", scratch, base, scratch));
                self.assembly.push(format!("    {} {}, 0({})", opcode, args[0], scratch));
                true
            }
            "addi" if args.len() == 3 => {
                let Ok(offset) = parse_immediate(args[2]) else {
                    return false;
                };
                if parse_register(args[0]).is_err() || parse_register(args[1]).is_err() {
                    return false;
                }
                if small_signed_immediate(offset) {
                    return false;
                }
                let scratch = scratch_register_avoiding(&[args[0], args[1]]);
                self.assembly.push(format!("    li {}, {}", scratch, offset));
                self.assembly.push(format!("    add {}, {}, {}", args[0], args[1], scratch));
                true
            }
            _ => false,
        }
    }

    fn emit_entry_abi_marker(&mut self, name: &str) {
        self.assembly.push(format!("# cellscript entry abi: {} requires-explicit-parameter-abi", name));
    }

    fn emit_entry_direct_wrapper(&mut self, target: &str) {
        self.emit_global(ENTRY_WITNESS_LABEL);
        self.emit_label(ENTRY_WITNESS_LABEL);
        self.emit(format!("# cellscript entry abi: {} tail-calls no-arg {}", ENTRY_WITNESS_LABEL, target));
        self.emit(format!("j {}", target));
    }

    fn emit_entry_witness_wrapper(&mut self, target: &str, params: &[IrParam]) -> Result<()> {
        let callable_abi = self.callable_abis.get(target).cloned();
        let type_hash_param_indices = callable_abi.as_ref().map(|abi| abi.type_hash_param_indices.clone()).unwrap_or_default();
        let runtime_bound_param_indices = callable_abi.as_ref().map(|abi| abi.runtime_bound_param_indices.clone()).unwrap_or_default();
        let outgoing_stack_arg_bytes = align_stack_arg_bytes(entry_abi_arg_count(params, callable_abi.as_ref()).saturating_sub(8) * 8);
        let payload = entry_witness_payload_layout(params, &runtime_bound_param_indices);
        let payload_len = payload.iter().map(|arg| arg.width).sum::<usize>();
        let has_witness_payload = payload.iter().any(|arg| arg.width > 0 || arg.unsupported);
        let has_lock_args = params.iter().any(|param| param.source == ParamSource::LockArgs);
        let has_dynamic_payload = payload.iter().any(|arg| arg.schema_dynamic);
        let min_witness_len = ENTRY_WITNESS_HEADER_SIZE + payload_len;
        let loaded_label = self.fresh_label("entry_witness_loaded");
        let try_group_input_label = self.fresh_label("entry_witness_try_group_input");
        let try_group_output_label = self.fresh_label("entry_witness_try_group_output");
        let buffer_ok_label = self.fresh_label("entry_witness_buffer_ok");
        let size_ok_label = self.fresh_label("entry_witness_size_ok");
        let fail_label = self.fresh_label("entry_witness_fail");
        let done_label = self.fresh_label("entry_witness_done");

        self.emit_global(ENTRY_WITNESS_LABEL);
        self.emit_label(ENTRY_WITNESS_LABEL);
        self.emit(format!(
            "# cellscript entry abi: {} loads Input#0 witness args for {} and falls back to GroupInput#0/GroupOutput#0",
            ENTRY_WITNESS_LABEL, target
        ));
        self.emit("# cellscript entry abi: witness magic CSARGv1 followed by positional fixed/scalar payload");
        self.emit_large_addi("sp", "sp", -(ENTRY_WITNESS_FRAME_SIZE as i64));
        self.emit_stack_store("ra", ENTRY_WITNESS_RA_OFFSET);
        if has_lock_args {
            self.emit_entry_load_script_args(&fail_label);
        }
        if has_witness_payload {
            self.emit_load_witness_syscall_to_offsets(
                "entry_args",
                CKB_SOURCE_INPUT,
                0,
                ENTRY_WITNESS_SIZE_OFFSET,
                ENTRY_WITNESS_BUFFER_OFFSET,
                ENTRY_WITNESS_BUFFER_SIZE,
            );
            self.emit(format!("beqz a0, {}", loaded_label));
            self.emit(format!("j {}", try_group_input_label));
            self.emit_label(&try_group_input_label);
            self.emit_load_witness_syscall_to_offsets(
                "entry_args_fallback_group_input",
                self.runtime_abi().source_group_input,
                0,
                ENTRY_WITNESS_SIZE_OFFSET,
                ENTRY_WITNESS_BUFFER_OFFSET,
                ENTRY_WITNESS_BUFFER_SIZE,
            );
            self.emit(format!("beqz a0, {}", loaded_label));
            self.emit(format!("j {}", try_group_output_label));
            self.emit_label(&try_group_output_label);
            self.emit_load_witness_syscall_to_offsets(
                "entry_args_fallback_group_output",
                CKB_SOURCE_GROUP_OUTPUT,
                0,
                ENTRY_WITNESS_SIZE_OFFSET,
                ENTRY_WITNESS_BUFFER_OFFSET,
                ENTRY_WITNESS_BUFFER_SIZE,
            );
            self.emit(format!("beqz a0, {}", loaded_label));
            self.emit(format!("j {}", fail_label));
            self.emit_label(&loaded_label);

            self.emit_stack_load("t0", ENTRY_WITNESS_SIZE_OFFSET);
            self.emit("# cellscript entry abi: reject witnesses larger than the local entry buffer");
            self.emit(format!("li t1, {}", ENTRY_WITNESS_BUFFER_SIZE + 1));
            self.emit("sltu t2, t0, t1");
            self.emit(format!("bnez t2, {}", buffer_ok_label));
            self.emit(format!("j {}", fail_label));
            self.emit_label(&buffer_ok_label);
            self.emit(format!("li t1, {}", min_witness_len));
            self.emit("sltu t2, t0, t1");
            self.emit(format!("beqz t2, {}", size_ok_label));
            self.emit(format!("j {}", fail_label));
            self.emit_label(&size_ok_label);

            for (index, byte) in ENTRY_WITNESS_MAGIC.iter().enumerate() {
                self.emit_stack_load_byte("t0", ENTRY_WITNESS_BUFFER_OFFSET + index);
                self.emit(format!("li t1, {}", byte));
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", fail_label));
            }

            if !has_dynamic_payload {
                let exact_size_label = self.fresh_label("entry_witness_exact_size_ok");
                self.emit("# cellscript entry abi: reject trailing witness payload bytes");
                self.emit_stack_load("t0", ENTRY_WITNESS_SIZE_OFFSET);
                self.emit(format!("li t1, {}", min_witness_len));
                self.emit("sub t2, t0, t1");
                self.emit(format!("beqz t2, {}", exact_size_label));
                self.emit(format!("j {}", fail_label));
                self.emit_label(&exact_size_label);
            }
        }

        if payload.iter().any(|arg| arg.unsupported) {
            self.emit("# cellscript entry abi: unsupported witness parameter shape; fail closed");
            self.emit(format!("j {}", fail_label));
        } else if has_dynamic_payload {
            let mut abi_index = 0usize;
            self.emit("# cellscript entry abi: witness payload contains schema-backed dynamic segments");
            self.emit_stack_load("t5", ENTRY_WITNESS_SIZE_OFFSET);
            self.emit(format!("li t6, {}", ENTRY_WITNESS_HEADER_SIZE));
            for (param_index, param) in params.iter().enumerate() {
                let param_is_runtime_bound =
                    runtime_bound_param_indices.contains(&param_index) || matches!(param.ty, IrType::Ref(_) | IrType::MutRef(_));
                if param.source == ParamSource::LockArgs {
                    self.emit_entry_lock_args_param(&mut abi_index, param, outgoing_stack_arg_bytes, &fail_label);
                } else if param_is_runtime_bound {
                    self.emit(format!("# cellscript entry abi: runtime-bound param {} is loaded from transaction cells", param.name));
                    self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                    self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                    abi_index += 2;
                    if type_hash_param_indices.contains(&param_index) {
                        self.emit(format!(
                            "# cellscript entry abi: runtime-bound param {} TypeHash witness bytes unavailable; pass null ABI bytes",
                            param.name
                        ));
                        self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                        self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                        abi_index += 2;
                    }
                } else if entry_witness_dynamic_schema_param(&param.ty) {
                    let len_ok_label = self.fresh_label("entry_witness_schema_len_ok");
                    let bytes_ok_label = self.fresh_label("entry_witness_schema_bytes_ok");
                    self.emit(format!(
                        "# cellscript entry abi: schema param {} -> {}={} {}={} (length-prefixed witness bytes)",
                        param.name,
                        abi_arg_label(abi_index),
                        "ptr",
                        abi_arg_label(abi_index + 1),
                        "len"
                    ));
                    self.emit("addi t1, t6, 4");
                    self.emit("sltu t2, t5, t1");
                    self.emit(format!("beqz t2, {}", len_ok_label));
                    self.emit(format!("j {}", fail_label));
                    self.emit_label(&len_ok_label);
                    self.emit("add t0, sp, t6");
                    self.emit(format!("addi t0, t0, {}", ENTRY_WITNESS_BUFFER_OFFSET));
                    self.emit("li t4, 0");
                    for byte_index in 0..4 {
                        self.emit(format!("lbu t1, {}(t0)", byte_index));
                        if byte_index != 0 {
                            self.emit(format!("slli t1, t1, {}", byte_index * 8));
                        }
                        self.emit("or t4, t4, t1");
                    }
                    self.emit("addi t1, t6, 4");
                    self.emit("add t1, t1, t4");
                    self.emit("sltu t2, t5, t1");
                    self.emit(format!("beqz t2, {}", bytes_ok_label));
                    self.emit(format!("j {}", fail_label));
                    self.emit_label(&bytes_ok_label);
                    self.emit_entry_abi_pointer_from_dynamic_offset(abi_index, "t6", 4, "t0", outgoing_stack_arg_bytes);
                    self.emit_entry_abi_reg_arg(abi_index + 1, "t4", outgoing_stack_arg_bytes);
                    abi_index += 2;
                    self.emit("addi t6, t6, 4");
                    self.emit("add t6, t6, t4");
                    if type_hash_param_indices.contains(&param_index) {
                        self.emit(format!(
                            "# cellscript entry abi: schema param {} TypeHash witness bytes unavailable; pass null ABI bytes",
                            param.name
                        ));
                        self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                        self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                        abi_index += 2;
                    }
                } else if let Some(width) =
                    fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty))
                {
                    let bytes_ok_label = self.fresh_label("entry_witness_fixed_bytes_ok");
                    self.emit(format!(
                        "# cellscript entry abi: fixed-byte param {} pointer={} length={} size={}",
                        param.name,
                        abi_arg_label(abi_index),
                        abi_arg_label(abi_index + 1),
                        width
                    ));
                    self.emit(format!("addi t1, t6, {}", width));
                    self.emit("sltu t2, t5, t1");
                    self.emit(format!("beqz t2, {}", bytes_ok_label));
                    self.emit(format!("j {}", fail_label));
                    self.emit_label(&bytes_ok_label);
                    self.emit_entry_abi_pointer_from_dynamic_offset(abi_index, "t6", 0, "t0", outgoing_stack_arg_bytes);
                    self.emit_entry_abi_immediate_arg(abi_index + 1, width as u64, outgoing_stack_arg_bytes);
                    self.emit(format!("addi t6, t6, {}", width));
                    abi_index += 2;
                } else if let Some(width) = entry_witness_register_param_width(&param.ty) {
                    let bytes_ok_label = self.fresh_label("entry_witness_scalar_bytes_ok");
                    self.emit(format!(
                        "# cellscript entry abi: scalar param {} -> {} size={}",
                        param.name,
                        abi_arg_label(abi_index),
                        width
                    ));
                    self.emit(format!("addi t1, t6, {}", width));
                    self.emit("sltu t2, t5, t1");
                    self.emit(format!("beqz t2, {}", bytes_ok_label));
                    self.emit(format!("j {}", fail_label));
                    self.emit_label(&bytes_ok_label);
                    self.emit("add t0, sp, t6");
                    self.emit(format!("addi t0, t0, {}", ENTRY_WITNESS_BUFFER_OFFSET));
                    if abi_index < 8 {
                        self.emit_entry_witness_scalar_load_from_reg(&format!("a{}", abi_index), "t0", width, param.ty == IrType::I32);
                    } else {
                        let caller_stack_offset = (abi_index - 8) * 8;
                        self.emit_entry_witness_scalar_load_from_reg("t3", "t0", width, param.ty == IrType::I32);
                        self.emit(format!(
                            "# cellscript entry abi: scalar param {} stored to caller stack +{}",
                            param.name, caller_stack_offset
                        ));
                        self.emit_entry_abi_reg_arg(abi_index, "t3", outgoing_stack_arg_bytes);
                    }
                    self.emit(format!("addi t6, t6, {}", width));
                    abi_index += 1;
                } else {
                    self.emit(format!("# cellscript entry abi: unsupported param {} shape; fail closed", param.name));
                    self.emit(format!("j {}", fail_label));
                }
            }
            let exact_size_label = self.fresh_label("entry_witness_exact_size_ok");
            self.emit("# cellscript entry abi: reject trailing witness payload bytes");
            self.emit_stack_load("t5", ENTRY_WITNESS_SIZE_OFFSET);
            self.emit("sub t2, t5, t6");
            self.emit(format!("beqz t2, {}", exact_size_label));
            self.emit(format!("j {}", fail_label));
            self.emit_label(&exact_size_label);
            if has_lock_args {
                self.emit_entry_lock_args_exact_size_check(&fail_label);
            }
            self.emit_entry_call_target(target, outgoing_stack_arg_bytes);
            self.emit(format!("j {}", done_label));
        } else {
            let mut abi_index = 0usize;
            let mut payload_cursor = 0usize;
            for (param_index, param) in params.iter().enumerate() {
                let param_is_runtime_bound =
                    runtime_bound_param_indices.contains(&param_index) || matches!(param.ty, IrType::Ref(_) | IrType::MutRef(_));
                if param.source == ParamSource::LockArgs {
                    self.emit_entry_lock_args_param(&mut abi_index, param, outgoing_stack_arg_bytes, &fail_label);
                } else if param_is_runtime_bound {
                    self.emit(format!("# cellscript entry abi: runtime-bound param {} is loaded from transaction cells", param.name));
                    self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                    self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                    abi_index += 2;
                    if type_hash_param_indices.contains(&param_index) {
                        self.emit(format!(
                            "# cellscript entry abi: runtime-bound param {} TypeHash witness bytes unavailable; pass null ABI bytes",
                            param.name
                        ));
                        self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                        self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                        abi_index += 2;
                    }
                } else if entry_witness_dynamic_schema_param(&param.ty) {
                    self.emit(format!("# cellscript entry abi: schema param {} is runtime-loaded; pass null ABI bytes", param.name));
                    self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                    self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                    abi_index += 2;
                    if type_hash_param_indices.contains(&param_index) {
                        self.emit(format!(
                            "# cellscript entry abi: schema param {} TypeHash witness bytes unavailable; pass null ABI bytes",
                            param.name
                        ));
                        self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                        self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                        abi_index += 2;
                    }
                } else if let Some(width) =
                    fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty))
                {
                    self.emit(format!(
                        "# cellscript entry abi: fixed-byte param {} pointer={} length={} size={}",
                        param.name,
                        abi_arg_label(abi_index),
                        abi_arg_label(abi_index + 1),
                        width
                    ));
                    self.emit_entry_abi_pointer_arg(
                        abi_index,
                        ENTRY_WITNESS_BUFFER_OFFSET + ENTRY_WITNESS_HEADER_SIZE + payload_cursor,
                        outgoing_stack_arg_bytes,
                    );
                    self.emit_entry_abi_immediate_arg(abi_index + 1, width as u64, outgoing_stack_arg_bytes);
                    payload_cursor += width;
                    abi_index += 2;
                } else if let Some(width) = entry_witness_register_param_width(&param.ty) {
                    self.emit(format!(
                        "# cellscript entry abi: scalar param {} -> {} size={}",
                        param.name,
                        abi_arg_label(abi_index),
                        width
                    ));
                    let stack_offset = ENTRY_WITNESS_BUFFER_OFFSET + ENTRY_WITNESS_HEADER_SIZE + payload_cursor;
                    if abi_index < 8 {
                        self.emit_entry_witness_scalar_load(&format!("a{}", abi_index), stack_offset, width, param.ty == IrType::I32);
                    } else {
                        let caller_stack_offset = (abi_index - 8) * 8;
                        self.emit_entry_witness_scalar_load("t3", stack_offset, width, param.ty == IrType::I32);
                        self.emit(format!(
                            "# cellscript entry abi: scalar param {} stored to caller stack +{}",
                            param.name, caller_stack_offset
                        ));
                        self.emit_entry_abi_reg_arg(abi_index, "t3", outgoing_stack_arg_bytes);
                    }
                    payload_cursor += width;
                    abi_index += 1;
                } else {
                    self.emit(format!("# cellscript entry abi: unsupported param {} shape; fail closed", param.name));
                    self.emit(format!("j {}", fail_label));
                }
            }
            if has_lock_args {
                self.emit_entry_lock_args_exact_size_check(&fail_label);
            }
            self.emit_entry_call_target(target, outgoing_stack_arg_bytes);
            self.emit(format!("j {}", done_label));
        }

        self.emit_label(&fail_label);
        self.emit_runtime_error_comment(CellScriptRuntimeError::EntryWitnessAbiInvalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::EntryWitnessAbiInvalid.code()));
        self.emit_label(&done_label);
        self.emit_stack_load("ra", ENTRY_WITNESS_RA_OFFSET);
        self.emit_large_addi("sp", "sp", ENTRY_WITNESS_FRAME_SIZE as i64);
        self.emit("ret");
        Ok(())
    }

    fn emit_entry_call_target(&mut self, target: &str, outgoing_stack_arg_bytes: usize) {
        if outgoing_stack_arg_bytes > 0 {
            self.emit(format!("# cellscript entry abi: reserve {} bytes for outgoing stack call arguments", outgoing_stack_arg_bytes));
            self.emit_large_addi("sp", "sp", -(outgoing_stack_arg_bytes as i64));
        }
        self.emit(format!("call {}", target));
        if outgoing_stack_arg_bytes > 0 {
            self.emit_large_addi("sp", "sp", outgoing_stack_arg_bytes as i64);
        }
    }

    fn emit_entry_abi_zero_arg(&mut self, abi_index: usize, outgoing_stack_arg_bytes: usize) {
        self.emit_entry_abi_immediate_arg(abi_index, 0, outgoing_stack_arg_bytes);
    }

    fn emit_entry_abi_reg_arg(&mut self, abi_index: usize, source_reg: &str, outgoing_stack_arg_bytes: usize) {
        if abi_index < 8 {
            self.emit(format!("addi a{}, {}, 0", abi_index, source_reg));
        } else {
            self.emit_entry_outgoing_stack_arg_store(source_reg, abi_index, outgoing_stack_arg_bytes);
        }
    }

    fn emit_entry_abi_immediate_arg(&mut self, abi_index: usize, value: u64, outgoing_stack_arg_bytes: usize) {
        if abi_index < 8 {
            self.emit(format!("li a{}, {}", abi_index, value));
        } else {
            self.emit(format!("# cellscript entry abi: stack arg{} <- {}", abi_index, value));
            self.emit(format!("li t0, {}", value));
            self.emit_entry_outgoing_stack_arg_store("t0", abi_index, outgoing_stack_arg_bytes);
        }
    }

    fn emit_entry_abi_pointer_arg(&mut self, abi_index: usize, stack_offset: usize, outgoing_stack_arg_bytes: usize) {
        if abi_index < 8 {
            self.emit_sp_addi(&format!("a{}", abi_index), stack_offset);
        } else {
            self.emit(format!("# cellscript entry abi: stack arg{} <- sp+{}", abi_index, stack_offset));
            self.emit_sp_addi("t0", stack_offset);
            self.emit_entry_outgoing_stack_arg_store("t0", abi_index, outgoing_stack_arg_bytes);
        }
    }

    fn emit_entry_abi_pointer_from_dynamic_offset(
        &mut self,
        abi_index: usize,
        offset_reg: &str,
        extra_offset: usize,
        temp_reg: &str,
        outgoing_stack_arg_bytes: usize,
    ) {
        self.emit(format!("add {}, sp, {}", temp_reg, offset_reg));
        if ENTRY_WITNESS_BUFFER_OFFSET + extra_offset != 0 {
            self.emit(format!("addi {}, {}, {}", temp_reg, temp_reg, ENTRY_WITNESS_BUFFER_OFFSET + extra_offset));
        }
        self.emit_entry_abi_reg_arg(abi_index, temp_reg, outgoing_stack_arg_bytes);
    }

    fn emit_entry_outgoing_stack_arg_store(&mut self, register: &str, abi_index: usize, outgoing_stack_arg_bytes: usize) {
        let stack_slot_offset = (abi_index - 8) * 8;
        let offset = i64::try_from(stack_slot_offset).expect("entry call stack slot should fit in i64")
            - i64::try_from(outgoing_stack_arg_bytes).expect("entry call stack argument area should fit in i64");
        self.emit(format!(
            "# cellscript entry abi: stage stack arg{} at pre-call sp{}{}",
            abi_index,
            if offset < 0 { "" } else { "+" },
            offset
        ));
        self.emit_sp_store_signed(register, offset);
    }

    fn emit_entry_witness_scalar_load(&mut self, dest_reg: &str, stack_offset: usize, width: usize, signed_i32: bool) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit_stack_load_byte("t0", stack_offset + byte_index);
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit(format!("or {}, {}, t0", dest_reg, dest_reg));
        }
        if signed_i32 {
            self.emit_sign_extend_i32(dest_reg);
        }
    }

    fn emit_entry_witness_scalar_load_from_reg(&mut self, dest_reg: &str, base_reg: &str, width: usize, signed_i32: bool) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit(format!("lbu t0, {}({})", byte_index, base_reg));
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit(format!("or {}, {}, t0", dest_reg, dest_reg));
        }
        if signed_i32 {
            self.emit_sign_extend_i32(dest_reg);
        }
    }

    fn emit_entry_load_u32_from_stack(&mut self, dest_reg: &str, stack_offset: usize) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..4 {
            self.emit_stack_load_byte("t0", stack_offset + byte_index);
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit(format!("or {}, {}, t0", dest_reg, dest_reg));
        }
    }

    fn emit_entry_load_u32_from_reg(&mut self, dest_reg: &str, base_reg: &str) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..4 {
            self.emit(format!("lbu t0, {}({})", byte_index, base_reg));
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit(format!("or {}, {}, t0", dest_reg, dest_reg));
        }
    }

    fn emit_entry_load_script_args(&mut self, fail_label: &str) {
        let loaded_label = self.fresh_label("entry_script_loaded");
        let buffer_ok_label = self.fresh_label("entry_script_buffer_ok");
        let total_ok_label = self.fresh_label("entry_script_total_ok");
        let table_header_ok_label = self.fresh_label("entry_script_table_header_ok");
        let args_offset_min_ok_label = self.fresh_label("entry_script_args_offset_min_ok");
        let args_offset_ok_label = self.fresh_label("entry_script_args_offset_ok");
        let args_span_ok_label = self.fresh_label("entry_script_args_span_ok");

        self.emit("# cellscript entry abi: lock_args parameters are decoded from the executing Script.args bytes");
        self.emit_load_script_syscall_to_offsets(
            "entry_lock_args",
            ENTRY_SCRIPT_SIZE_OFFSET,
            ENTRY_SCRIPT_BUFFER_OFFSET,
            ENTRY_SCRIPT_BUFFER_SIZE,
        );
        self.emit(format!("beqz a0, {}", loaded_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&loaded_label);

        self.emit_stack_load("t0", ENTRY_SCRIPT_SIZE_OFFSET);
        self.emit(format!("li t1, {}", ENTRY_SCRIPT_BUFFER_SIZE + 1));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", buffer_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&buffer_ok_label);

        self.emit_entry_load_u32_from_stack("t3", ENTRY_SCRIPT_BUFFER_OFFSET);
        self.emit_stack_load("t0", ENTRY_SCRIPT_SIZE_OFFSET);
        self.emit("sub t2, t0, t3");
        self.emit(format!("beqz t2, {}", total_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&total_ok_label);

        self.emit("li t1, 16");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("beqz t2, {}", table_header_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&table_header_ok_label);

        self.emit_entry_load_u32_from_stack("t4", ENTRY_SCRIPT_BUFFER_OFFSET + 12);
        self.emit("li t1, 16");
        self.emit("sltu t2, t4, t1");
        self.emit(format!("beqz t2, {}", args_offset_min_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&args_offset_min_ok_label);
        self.emit("addi t1, t4, 4");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("beqz t2, {}", args_offset_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&args_offset_ok_label);

        self.emit_sp_addi("t0", ENTRY_SCRIPT_BUFFER_OFFSET);
        self.emit("add t0, t0, t4");
        self.emit_entry_load_u32_from_reg("t5", "t0");
        self.emit("addi t6, t4, 4");
        self.emit("add t1, t6, t5");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("beqz t2, {}", args_span_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&args_span_ok_label);
        self.emit_stack_store("t6", ENTRY_SCRIPT_ARGS_START_OFFSET);
        self.emit_stack_store("t5", ENTRY_SCRIPT_ARGS_LEN_OFFSET);
        self.emit("li t0, 0");
        self.emit_stack_store("t0", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
    }

    fn emit_entry_lock_args_param(
        &mut self,
        abi_index: &mut usize,
        param: &IrParam,
        outgoing_stack_arg_bytes: usize,
        fail_label: &str,
    ) {
        let fixed_byte_width = fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty));
        let scalar_width = entry_witness_register_param_width(&param.ty);
        let Some(width) = fixed_byte_width.or(scalar_width) else {
            self.emit(format!("# cellscript entry abi: unsupported lock_args param {} shape; fail closed", param.name));
            self.emit(format!("j {}", fail_label));
            return;
        };
        let bytes_ok_label = self.fresh_label("entry_lock_args_bytes_ok");
        self.emit(format!("# cellscript entry abi: lock_args param {} consumes {} script arg byte(s)", param.name, width));
        self.emit_stack_load("t6", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
        self.emit_stack_load("t5", ENTRY_SCRIPT_ARGS_LEN_OFFSET);
        self.emit(format!("addi t1, t6, {}", width));
        self.emit("sltu t2, t5, t1");
        self.emit(format!("beqz t2, {}", bytes_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&bytes_ok_label);
        self.emit_stack_load("t3", ENTRY_SCRIPT_ARGS_START_OFFSET);
        self.emit("add t3, t3, t6");
        self.emit_sp_addi("t0", ENTRY_SCRIPT_BUFFER_OFFSET);
        self.emit("add t0, t0, t3");

        if fixed_byte_width.is_some() {
            self.emit_entry_abi_reg_arg(*abi_index, "t0", outgoing_stack_arg_bytes);
            self.emit_entry_abi_immediate_arg(*abi_index + 1, width as u64, outgoing_stack_arg_bytes);
            *abi_index += 2;
        } else if *abi_index < 8 {
            self.emit_entry_witness_scalar_load_from_reg(&format!("a{}", *abi_index), "t0", width, param.ty == IrType::I32);
            *abi_index += 1;
        } else {
            self.emit_entry_witness_scalar_load_from_reg("t4", "t0", width, param.ty == IrType::I32);
            self.emit_entry_abi_reg_arg(*abi_index, "t4", outgoing_stack_arg_bytes);
            *abi_index += 1;
        }

        self.emit_stack_load("t6", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
        self.emit(format!("addi t6, t6, {}", width));
        self.emit_stack_store("t6", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
    }

    fn emit_entry_lock_args_exact_size_check(&mut self, fail_label: &str) {
        let exact_label = self.fresh_label("entry_lock_args_exact_size_ok");
        self.emit("# cellscript entry abi: reject trailing Script.args bytes after typed lock_args");
        self.emit_stack_load("t0", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
        self.emit_stack_load("t1", ENTRY_SCRIPT_ARGS_LEN_OFFSET);
        self.emit("sub t2, t1, t0");
        self.emit(format!("beqz t2, {}", exact_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&exact_label);
    }

    fn generate_type_def(&mut self, type_def: &IrTypeDef) -> Result<()> {
        self.emit_section(".rodata");
        self.emit_label(&format!("__type_desc_{}", type_def.name));

        self.emit(format!(".word {}", type_def.fields.len()));

        for field in &type_def.fields {
            self.emit(format!(".byte {}", field.name.len()));
            self.emit(format!(".ascii \"{}\"", field.name));
            self.emit(".align 3");
            self.emit(format!(".word {}", self.type_id(&field.ty)));
        }

        Ok(())
    }

    fn register_type_def(&mut self, type_def: &IrTypeDef) {
        if let Some(fixed_size) = type_def.fields.iter().try_fold(0usize, |acc, field| field.fixed_size.map(|size| acc + size)) {
            self.type_fixed_sizes.insert(type_def.name.clone(), fixed_size);
        }
        if let Some(states) = &type_def.flow_states {
            self.flow_states.insert(type_def.name.clone(), states.clone());
        }
        if let Some(field) = &type_def.flow_state_field {
            self.flow_state_fields.insert(type_def.name.clone(), field.clone());
        }
        if !type_def.flow_rules.is_empty() {
            self.flow_rules.insert(type_def.name.clone(), type_def.flow_rules.clone());
        }
        if matches!(type_def.kind, IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt) {
            self.cell_type_names.insert(type_def.name.clone());
            if type_def.kind == IrTypeKind::Receipt {
                self.receipt_type_names.insert(type_def.name.clone());
            }
        }
        let fields = type_def
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let fixed_enum_size = match &field.ty {
                    IrType::Named(name) => self.enum_fixed_sizes.get(name).copied(),
                    _ => None,
                };
                (
                    field.name.clone(),
                    SchemaFieldLayout {
                        index,
                        offset: field.offset,
                        ty: field.ty.clone(),
                        fixed_size: field.fixed_size,
                        fixed_enum_size,
                    },
                )
            })
            .collect();
        self.type_layouts.insert(type_def.name.clone(), fields);
    }

    fn register_callable_abis(&mut self, ir: &IrModule) {
        self.callable_abis.clear();
        for item in &ir.items {
            let (name, params, body) = match item {
                IrItem::Action(action) => (&action.name, &action.params, &action.body),
                IrItem::PureFn(function) => (&function.name, &function.params, &function.body),
                IrItem::Lock(lock) => (&lock.name, &lock.params, &lock.body),
                IrItem::TypeDef(_) | IrItem::Invariant(_) => continue,
            };
            let param_indices = params.iter().enumerate().map(|(index, param)| (param.binding.id, index)).collect::<HashMap<_, _>>();
            let mut type_hash_param_indices = BTreeSet::new();
            let mut runtime_bound_param_indices = params
                .iter()
                .enumerate()
                .filter_map(|(index, param)| self.param_is_runtime_bound(param).then_some(index))
                .collect::<BTreeSet<_>>();
            for pattern in body.consume_set.iter().chain(body.read_refs.iter()) {
                if let Some(param) = params.iter().position(|param| param.name == pattern.binding) {
                    runtime_bound_param_indices.insert(param);
                }
            }
            for pattern in &body.mutate_set {
                if let Some(param) = params.iter().position(|param| param.name == pattern.binding) {
                    runtime_bound_param_indices.insert(param);
                }
            }
            for block in &body.blocks {
                for instruction in &block.instructions {
                    if let IrInstruction::TypeHash { operand: IrOperand::Var(var), .. } = instruction {
                        if let Some(index) = param_indices.get(&var.id).copied() {
                            type_hash_param_indices.insert(index);
                        }
                    }
                }
            }
            self.callable_abis
                .insert(name.clone(), CallableAbi { params: params.clone(), type_hash_param_indices, runtime_bound_param_indices });
        }
        for external in &ir.external_callable_abis {
            if self.callable_abis.contains_key(&external.name) {
                continue;
            }
            let runtime_bound_param_indices = external
                .params
                .iter()
                .enumerate()
                .filter_map(|(index, param)| self.param_is_runtime_bound(param).then_some(index))
                .collect();
            self.callable_abis.insert(
                external.name.clone(),
                CallableAbi {
                    params: external.params.clone(),
                    type_hash_param_indices: external.type_hash_param_indices.clone(),
                    runtime_bound_param_indices,
                },
            );
        }
    }

    fn type_id(&self, ty: &IrType) -> u32 {
        match ty {
            IrType::U8 => 1,
            IrType::U16 => 2,
            IrType::U32 => 3,
            IrType::U64 => 4,
            IrType::U128 => 5,
            IrType::Bool => 6,
            IrType::Address => 7,
            IrType::Hash => 8,
            IrType::Array(_, _) => 9,
            IrType::Tuple(_) => 10,
            IrType::Named(_) => 11,
            IrType::Ref(_) => 12,
            IrType::MutRef(_) => 13,
            IrType::Unit => 14,
            IrType::I32 => 15,
        }
    }

    fn generate_action(&mut self, action: &IrAction) -> Result<()> {
        self.current_function = Some(action.name.clone());
        self.current_state_transition_edges = action.state_transition_edges.clone();
        self.bind_readonly_schema_params = true;
        self.fail_handler_codes.clear();
        self.prepare_function_layout(&action.body, &action.params);
        self.next_virtual_output = 0;
        self.set_schema_pointer_params(&action.params);
        self.set_consumed_schema_pointers(&action.body);
        self.set_read_ref_schema_pointers(&action.body);
        self.set_pointer_aliases(&action.body);
        self.set_schema_field_value_sources(&action.body);
        self.set_verified_operation_outputs(&action.body);
        self.set_constructed_byte_vectors(&action.body);
        self.set_verified_collection_push_values(&action.body);

        if !action.params.is_empty() {
            self.emit_entry_abi_marker(&action.name);
        }
        self.emit_global(&action.name);
        self.emit_label(&action.name);

        self.emit_prologue();
        self.emit_param_spills(&action.params)?;
        self.emit_auto_aggregate_invariant_checks(&action.name);

        self.generate_body(&action.body)?;
        self.emit_shared_epilogue();

        self.current_function = None;
        self.current_state_transition_edges.clear();
        self.bind_readonly_schema_params = false;
        self.schema_pointer_vars.clear();
        self.schema_pointer_size_offsets.clear();
        self.fixed_byte_param_size_offsets.clear();
        self.schema_field_value_sources.clear();
        self.aggregate_pointer_sources.clear();
        self.tuple_call_return_vars.clear();
        self.tuple_call_return_field_slots.clear();
        self.tuple_aggregate_fields.clear();
        self.output_type_hash_sources.clear();
        self.param_type_hash_pointer_offsets.clear();
        self.param_type_hash_size_offsets.clear();
        self.param_type_hash_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.prelude_fixed_byte_constants.clear();
        self.u128_value_offsets.clear();
        self.operation_output_indices.clear();
        self.verified_operation_outputs.clear();
        self.output_param_ids.clear();
        self.verified_collection_push_values.clear();
        self.stack_collection_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        self.param_vars.clear();
        Ok(())
    }

    fn emit_auto_aggregate_invariant_checks(&mut self, action_name: &str) {
        let helpers = self
            .auto_aggregate_runtime_helpers_by_action
            .get(action_name)
            .into_iter()
            .flat_map(|helpers| helpers.iter().cloned())
            .collect::<Vec<_>>();
        for helper in helpers {
            if helper == "__xudt_require_group_amount_conserved" {
                self.emit("# cellscript aggregate invariant: auto-lowered xUDT group amount conservation");
                self.emit("call __xudt_require_group_amount_conserved");
                let ok_label = self.fresh_label("auto_aggregate_xudt_conserved_ok");
                self.emit(format!("beqz a0, {}", ok_label));
                self.emit_epilogue();
                self.emit_label(&ok_label);
            }
        }
    }

    fn generate_pure_fn(&mut self, function: &IrPureFn) -> Result<()> {
        self.current_function = Some(function.name.clone());
        self.bind_readonly_schema_params = false;
        self.fail_handler_codes.clear();
        self.prepare_function_layout(&function.body, &function.params);
        self.next_virtual_output = 0;
        self.set_schema_pointer_params(&function.params);
        self.set_consumed_schema_pointers(&function.body);
        self.set_read_ref_schema_pointers(&function.body);
        self.set_pointer_aliases(&function.body);
        self.set_schema_field_value_sources(&function.body);
        self.set_verified_operation_outputs(&function.body);
        self.set_constructed_byte_vectors(&function.body);
        self.set_verified_collection_push_values(&function.body);

        self.emit_global(&function.name);
        self.emit_label(&function.name);

        self.emit_prologue();
        self.emit_param_spills(&function.params)?;
        self.generate_body(&function.body)?;
        self.emit_shared_epilogue();

        self.current_function = None;
        self.schema_pointer_vars.clear();
        self.schema_pointer_size_offsets.clear();
        self.fixed_byte_param_size_offsets.clear();
        self.schema_field_value_sources.clear();
        self.aggregate_pointer_sources.clear();
        self.tuple_call_return_vars.clear();
        self.tuple_call_return_field_slots.clear();
        self.tuple_aggregate_fields.clear();
        self.output_type_hash_sources.clear();
        self.param_type_hash_pointer_offsets.clear();
        self.param_type_hash_size_offsets.clear();
        self.param_type_hash_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.prelude_fixed_byte_constants.clear();
        self.u128_value_offsets.clear();
        self.operation_output_indices.clear();
        self.verified_operation_outputs.clear();
        self.output_param_ids.clear();
        self.verified_collection_push_values.clear();
        self.stack_collection_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        self.param_vars.clear();
        Ok(())
    }

    fn generate_lock(&mut self, lock: &IrLock) -> Result<()> {
        self.current_function = Some(lock.name.clone());
        self.bind_readonly_schema_params = true;
        self.current_lock_entry = true;
        self.fail_handler_codes.clear();
        self.prepare_function_layout(&lock.body, &lock.params);
        self.next_virtual_output = 0;
        self.set_schema_pointer_params(&lock.params);
        self.set_consumed_schema_pointers(&lock.body);
        self.set_read_ref_schema_pointers(&lock.body);
        self.set_pointer_aliases(&lock.body);
        self.set_schema_field_value_sources(&lock.body);
        self.set_verified_operation_outputs(&lock.body);
        self.set_constructed_byte_vectors(&lock.body);
        self.set_verified_collection_push_values(&lock.body);

        if !lock.params.is_empty() {
            self.emit_entry_abi_marker(&lock.name);
        }
        self.emit_global(&lock.name);
        self.emit_label(&lock.name);

        self.emit_prologue();
        self.emit_param_spills(&lock.params)?;

        self.generate_body(&lock.body)?;
        self.emit_shared_epilogue();

        self.current_function = None;
        self.bind_readonly_schema_params = false;
        self.current_lock_entry = false;
        self.schema_pointer_vars.clear();
        self.schema_pointer_size_offsets.clear();
        self.fixed_byte_param_size_offsets.clear();
        self.schema_field_value_sources.clear();
        self.aggregate_pointer_sources.clear();
        self.tuple_call_return_vars.clear();
        self.tuple_call_return_field_slots.clear();
        self.tuple_aggregate_fields.clear();
        self.output_type_hash_sources.clear();
        self.param_type_hash_pointer_offsets.clear();
        self.param_type_hash_size_offsets.clear();
        self.param_type_hash_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.prelude_fixed_byte_constants.clear();
        self.u128_value_offsets.clear();
        self.operation_output_indices.clear();
        self.verified_operation_outputs.clear();
        self.output_param_ids.clear();
        self.verified_collection_push_values.clear();
        self.stack_collection_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        self.param_vars.clear();
        Ok(())
    }

    fn set_schema_pointer_params(&mut self, params: &[IrParam]) {
        self.schema_pointer_vars.clear();
        self.param_vars.clear();
        self.aggregate_pointer_sources.clear();
        for param in params {
            self.param_vars.insert(param.binding.id);
            if named_type_name(&param.ty).is_some() {
                self.schema_pointer_vars.insert(param.binding.id);
            } else if fixed_byte_pointer_param_width(&param.ty).is_some() || fixed_aggregate_pointer_param_width(&param.ty).is_some() {
                self.aggregate_pointer_sources.insert(param.binding.id, AggregatePointerSource { ty: param.ty.clone() });
            }
        }
    }

    fn set_read_ref_schema_pointers(&mut self, body: &IrBody) {
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::ReadRef { dest, .. } = instruction {
                    self.schema_pointer_vars.insert(dest.id);
                    if let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() {
                        self.schema_pointer_size_offsets.insert(dest.id, size_offset);
                    }
                }
            }
        }
    }

    fn set_consumed_schema_pointers(&mut self, body: &IrBody) {
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let Some(var) = consumed_operand_var(instruction) {
                    self.schema_pointer_vars.insert(var.id);
                    if let Some(size_offset) = self.cell_buffer_size_offsets.get(&var.id).copied() {
                        self.schema_pointer_size_offsets.insert(var.id, size_offset);
                    }
                }
            }
        }
    }

    fn set_pointer_aliases(&mut self, body: &IrBody) {
        let mut changed = true;
        while changed {
            changed = false;
            for block in &body.blocks {
                for instruction in &block.instructions {
                    let alias = match instruction {
                        IrInstruction::Unary { dest, op: UnaryOp::Ref | UnaryOp::Deref, operand: IrOperand::Var(src) }
                        | IrInstruction::Move { dest, src: IrOperand::Var(src) } => Some((dest, src)),
                        _ => None,
                    };
                    let Some((dest, src)) = alias else {
                        continue;
                    };
                    if self.schema_pointer_vars.contains(&src.id) && self.schema_pointer_vars.insert(dest.id) {
                        changed = true;
                    }
                    if let Some(size_offset) = self.schema_pointer_size_offsets.get(&src.id).copied() {
                        if self.schema_pointer_size_offsets.insert(dest.id, size_offset) != Some(size_offset) {
                            changed = true;
                        }
                    }
                    if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&src.id).copied() {
                        if self.fixed_byte_param_size_offsets.insert(dest.id, size_offset) != Some(size_offset) {
                            changed = true;
                        }
                    }
                    if let Some(size_offset) = self.dynamic_value_size_offsets.get(&src.id).copied() {
                        if self.dynamic_value_size_offsets.insert(dest.id, size_offset) != Some(size_offset) {
                            changed = true;
                        }
                    }
                    if let Some(size_offset) = self.cell_buffer_size_offsets.get(&src.id).copied() {
                        if self.cell_buffer_size_offsets.insert(dest.id, size_offset) != Some(size_offset) {
                            changed = true;
                        }
                    }
                    if let Some(buffer_offset) = self.cell_buffer_offsets.get(&src.id).copied() {
                        if self.cell_buffer_offsets.insert(dest.id, buffer_offset) != Some(buffer_offset) {
                            changed = true;
                        }
                    }
                    if self.empty_molecule_vector_vars.contains(&src.id) && self.empty_molecule_vector_vars.insert(dest.id) {
                        changed = true;
                    }
                    if let Some(source) = self.aggregate_pointer_sources.get(&src.id).cloned() {
                        if self.aggregate_pointer_sources.insert(dest.id, source).is_none() {
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    fn set_schema_field_value_sources(&mut self, body: &IrBody) {
        self.schema_field_value_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.prelude_fixed_byte_constants.clear();
        self.tuple_call_return_vars.clear();
        self.tuple_call_return_field_slots.clear();
        self.tuple_aggregate_fields.clear();
        let mut named_stack_collections = HashMap::<String, usize>::new();
        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    IrInstruction::StoreVar { name, src: IrOperand::Var(src) } => {
                        if self.stack_collection_vars.contains(&src.id) {
                            named_stack_collections.insert(name.clone(), src.id);
                        }
                    }
                    IrInstruction::LoadVar { dest, name } => {
                        if named_stack_collections.contains_key(name) {
                            self.stack_collection_vars.insert(dest.id);
                        }
                    }
                    IrInstruction::Tuple { dest, fields } => {
                        self.tuple_aggregate_fields.insert(dest.id, fields.clone());
                    }
                    IrInstruction::Call { dest: Some(dest), .. } if matches!(dest.ty, IrType::Tuple(_)) => {
                        self.tuple_call_return_vars.insert(dest.id, dest.ty.clone());
                    }
                    IrInstruction::Call { dest: Some(dest), func, .. } if self.pure_const_returns.contains_key(func) => {
                        let value = self.pure_const_returns.get(func).cloned().expect("guarded pure const return");
                        if let Some(value) = fixed_scalar_const_value(&value) {
                            self.prelude_scalar_immediates.insert(dest.id, value);
                            if dest.ty == IrType::U64 {
                                self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Const(value));
                            }
                        }
                        if let Some(bytes) = fixed_byte_const_bytes(&value) {
                            self.prelude_fixed_byte_constants.insert(dest.id, bytes);
                        }
                    }
                    IrInstruction::LoadConst { dest, value } => {
                        if let Some(value) = fixed_scalar_const_value(value) {
                            self.prelude_scalar_immediates.insert(dest.id, value);
                            if dest.ty == IrType::U64 {
                                self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Const(value));
                            }
                        }
                        if let Some(bytes) = fixed_byte_const_bytes(value) {
                            self.prelude_fixed_byte_constants.insert(dest.id, bytes);
                        }
                    }
                    IrInstruction::FieldAccess { dest, obj: IrOperand::Var(obj), field } => {
                        if self
                            .tuple_call_return_vars
                            .get(&obj.id)
                            .and_then(|ty| tuple_return_field_type(ty, field))
                            .is_some_and(|field_ty| field_ty == dest.ty)
                        {
                            self.tuple_call_return_field_slots.insert((obj.id, field.clone()), dest.id);
                            continue;
                        }
                        let source = if self.schema_pointer_vars.contains(&obj.id) {
                            let Some(type_name) = named_type_name(&obj.ty) else {
                                continue;
                            };
                            let Some(layout) = self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
                                continue;
                            };
                            Some(SchemaFieldValueSource {
                                obj_var_id: obj.id,
                                type_name: type_name.to_string(),
                                field: field.clone(),
                                layout,
                            })
                        } else if let Some(parent) = self.schema_field_value_sources.get(&obj.id) {
                            aggregate_field_layout(&obj.ty, field).map(|nested| {
                                let mut layout = nested;
                                layout.offset += parent.layout.offset;
                                SchemaFieldValueSource {
                                    obj_var_id: parent.obj_var_id,
                                    type_name: parent.type_name.clone(),
                                    field: format!("{}.{}", parent.field, field),
                                    layout,
                                }
                            })
                        } else {
                            self.aggregate_pointer_sources.get(&obj.id).and_then(|source| {
                                aggregate_field_layout(&source.ty, field).map(|layout| SchemaFieldValueSource {
                                    obj_var_id: obj.id,
                                    type_name: aggregate_type_label(&source.ty),
                                    field: field.clone(),
                                    layout,
                                })
                            })
                        };
                        let Some(source) = source else {
                            continue;
                        };
                        let layout = source.layout.clone();
                        let scalar_width = layout_fixed_scalar_width(&layout);
                        let field_width = layout_fixed_byte_width(&layout).or_else(|| self.fixed_named_type_width(&layout.ty));
                        if field_width.is_some()
                            && (layout.ty == dest.ty
                                || (scalar_width.is_some() && is_fixed_scalar_ir_type(&dest.ty))
                                || field_width == self.fixed_byte_like_width(&dest.ty))
                        {
                            self.schema_field_value_sources.insert(dest.id, source.clone());
                            if scalar_width.is_some() {
                                self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Field(source));
                            }
                        }
                    }
                    IrInstruction::Index { dest, arr: IrOperand::Var(arr), idx } => {
                        if self.aggregate_pointer_sources.contains_key(&arr.id) {
                            if let (IrType::Array(inner, len), Some(index)) = (&arr.ty, const_usize_operand(idx)) {
                                let element_ty = inner.as_ref();
                                if index < *len && type_static_length(element_ty).is_some() {
                                    if fixed_scalar_width(element_ty, type_static_length(element_ty)).is_some()
                                        && element_ty == &dest.ty
                                    {
                                        if dest.ty == IrType::U64 {
                                            self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                                        }
                                    } else {
                                        self.aggregate_pointer_sources
                                            .insert(dest.id, AggregatePointerSource { ty: element_ty.clone() });
                                    }
                                }
                            }
                        } else if self.stack_collection_vars.contains(&arr.id)
                            && molecule_vector_element_fixed_width(&arr.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
                                .is_some_and(|element_width| {
                                    self.fixed_byte_like_width(&dest.ty)
                                        .is_some_and(|dest_width| dest_width == element_width && dest_width > 8)
                                })
                        {
                            self.aggregate_pointer_sources.insert(dest.id, AggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                    IrInstruction::Binary { dest, op, left, right }
                        if dest.ty == IrType::U64 && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div) =>
                    {
                        let Some(left) = self.prelude_u64_value_source(left) else {
                            continue;
                        };
                        let Some(right) = self.prelude_u64_operand_source(right) else {
                            continue;
                        };
                        self.prelude_u64_value_sources
                            .insert(dest.id, PreludeU64ValueSource::Binary { op: *op, left: Box::new(left), right });
                    }
                    IrInstruction::Call { dest: Some(dest), func, args }
                        if dest.ty == IrType::U64 && is_min_call(func) && args.len() == 2 =>
                    {
                        let Some(left) = self.prelude_u64_value_source(&args[0]) else {
                            continue;
                        };
                        let Some(right) = self.prelude_u64_operand_source(&args[1]) else {
                            continue;
                        };
                        self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Min { left: Box::new(left), right });
                    }
                    IrInstruction::Call { dest: Some(dest), func, args }
                        if dest.ty == IrType::U64 && is_runtime_header_u64_call(func) && args.is_empty() =>
                    {
                        self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                    }
                    IrInstruction::Length { dest, operand }
                        if dest.ty == IrType::U64
                            && (self.static_length(operand).is_some()
                                || self.dynamic_length_from_size_offset(operand).is_some()
                                || matches!(
                                    operand,
                                    IrOperand::Var(var)
                                        if self.dynamic_value_size_offsets.contains_key(&var.id)
                                            || self.schema_pointer_size_offsets.contains_key(&var.id)
                                )) =>
                    {
                        self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                    }
                    IrInstruction::CollectionCapacity { dest, collection: IrOperand::Var(collection) }
                        if dest.ty == IrType::U64
                            && self.stack_collection_vars.contains(&collection.id)
                            && molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
                                .is_some_and(|width| width != 0) =>
                    {
                        self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                    }
                    IrInstruction::CollectionRemove { dest, collection: IrOperand::Var(collection), .. }
                    | IrInstruction::CollectionPop { dest, collection: IrOperand::Var(collection) }
                        if self.stack_collection_vars.contains(&collection.id)
                            && molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
                                .is_some_and(|element_width| {
                                    self.fixed_byte_like_width(&dest.ty)
                                        .is_some_and(|dest_width| dest_width == element_width && dest_width > 8)
                                }) =>
                    {
                        self.aggregate_pointer_sources.insert(dest.id, AggregatePointerSource { ty: dest.ty.clone() });
                    }
                    IrInstruction::Move { dest, src } if dest.ty == IrType::U64 => {
                        if self.prelude_u64_value_source(src).is_some() {
                            self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                        }
                    }
                    IrInstruction::Move { dest, src }
                        if matches!(dest.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32) =>
                    {
                        if let Some(value) = self.prelude_scalar_immediate(src) {
                            self.prelude_scalar_immediates.insert(dest.id, value);
                        }
                    }
                    IrInstruction::Move { dest, src } if fixed_byte_width(&dest.ty, type_static_length(&dest.ty)).is_some() => {
                        if let Some(bytes) = self.prelude_fixed_byte_constant(src) {
                            self.prelude_fixed_byte_constants.insert(dest.id, bytes);
                        }
                    }
                    IrInstruction::Move { dest, src: IrOperand::Var(src) }
                    | IrInstruction::Unary { dest, op: UnaryOp::Ref | UnaryOp::Deref, operand: IrOperand::Var(src) } => {
                        if self.stack_collection_vars.contains(&src.id) && dest.ty == src.ty {
                            self.stack_collection_vars.insert(dest.id);
                        }
                        if let Some(source) = self.schema_field_value_sources.get(&src.id).cloned() {
                            self.schema_field_value_sources.insert(dest.id, source);
                        }
                    }
                    IrInstruction::CollectionNew { dest, .. } => {
                        self.stack_collection_vars.insert(dest.id);
                    }
                    _ => {}
                }
            }
        }
        let max_provenance_iterations = body.blocks.iter().map(|block| block.instructions.len()).sum::<usize>() + 1;
        for _ in 0..max_provenance_iterations {
            let mut changed = false;
            for block in &body.blocks {
                for instruction in &block.instructions {
                    match instruction {
                        IrInstruction::Move { dest, src: IrOperand::Var(src) }
                        | IrInstruction::Unary { dest, op: UnaryOp::Ref | UnaryOp::Deref, operand: IrOperand::Var(src) } => {
                            if !self.schema_field_value_sources.contains_key(&dest.id) {
                                if let Some(source) = self.schema_field_value_sources.get(&src.id).cloned() {
                                    self.schema_field_value_sources.insert(dest.id, source);
                                    changed = true;
                                }
                            }
                        }
                        _ => {}
                    }
                    let IrInstruction::FieldAccess { dest, obj: IrOperand::Var(obj), field } = instruction else {
                        continue;
                    };
                    if self.schema_field_value_sources.contains_key(&dest.id) {
                        continue;
                    }
                    let Some(parent) = self.schema_field_value_sources.get(&obj.id).cloned() else {
                        continue;
                    };
                    let Some(nested) = aggregate_field_layout(&obj.ty, field) else {
                        continue;
                    };
                    let mut layout = nested;
                    layout.offset += parent.layout.offset;
                    let scalar_width = layout_fixed_scalar_width(&layout);
                    let field_width = layout_fixed_byte_width(&layout).or_else(|| self.fixed_named_type_width(&layout.ty));
                    if field_width.is_none()
                        || !(layout.ty == dest.ty
                            || (scalar_width.is_some() && is_fixed_scalar_ir_type(&dest.ty))
                            || field_width == self.fixed_byte_like_width(&dest.ty))
                    {
                        continue;
                    }
                    let source = SchemaFieldValueSource {
                        obj_var_id: parent.obj_var_id,
                        type_name: parent.type_name,
                        field: format!("{}.{}", parent.field, field),
                        layout,
                    };
                    self.schema_field_value_sources.insert(dest.id, source.clone());
                    if scalar_width.is_some() {
                        self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Field(source));
                    }
                    changed = true;
                }
            }
            if !changed {
                return;
            }
        }
    }

    fn set_verified_operation_outputs(&mut self, body: &IrBody) {
        self.operation_output_indices.clear();
        self.verified_operation_outputs.clear();

        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    IrInstruction::Create { dest, pattern }
                    | IrInstruction::CreateUnique { dest, pattern, .. }
                    | IrInstruction::ReplaceUnique { dest, pattern, .. } => {
                        if pattern.operation != "create" {
                            if let Some(output_index) =
                                Self::create_output_index(body, &pattern.operation, &pattern.binding, &pattern.ty)
                            {
                                self.operation_output_indices.insert(dest.id, output_index);
                            }
                        }
                    }
                    IrInstruction::Transfer { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "transfer", dest) {
                            self.record_verified_operation_output(body, output_index, dest, "transfer");
                        }
                    }
                    IrInstruction::Claim { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "claim", dest) {
                            self.record_verified_operation_output(body, output_index, dest, "claim");
                        }
                    }
                    IrInstruction::Settle { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "settle", dest) {
                            self.record_verified_operation_output(body, output_index, dest, "settle");
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn create_output_index(body: &IrBody, operation: &str, binding: &str, ty: &str) -> Option<usize> {
        body.create_set.iter().position(|pattern| pattern.operation == operation && pattern.binding == binding && pattern.ty == ty)
    }

    fn create_output_index_for_dest(body: &IrBody, operation: &str, dest: &IrVar) -> Option<usize> {
        let ty = named_type_name(&dest.ty)?;
        Self::create_output_index(body, operation, &dest.name, ty)
    }

    fn record_verified_operation_output(&mut self, body: &IrBody, output_index: usize, dest: &IrVar, operation: &str) {
        self.operation_output_indices.insert(dest.id, output_index);
        if body
            .create_set
            .get(output_index)
            .is_some_and(|pattern| self.operation_output_pattern_is_verified(pattern, operation, &dest.ty))
        {
            self.verified_operation_outputs.insert(dest.id);
        }
    }

    fn operation_output_pattern_is_verified(&self, pattern: &CreatePattern, operation: &str, dest_ty: &IrType) -> bool {
        pattern.operation == operation
            && named_type_name(dest_ty).is_some_and(|type_name| type_name == pattern.ty.as_str())
            && self.can_verify_create_output_fields(pattern)
            && self.can_verify_output_lock(pattern)
    }

    fn set_verified_collection_push_values(&mut self, body: &IrBody) {
        self.verified_collection_push_values.clear();
        for pattern in &body.mutate_set {
            for transition in &pattern.transitions {
                if transition.op != MutateTransitionOp::Append {
                    continue;
                }
                let IrOperand::Var(var) = &transition.operand else {
                    continue;
                };
                let Some(layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field)) else {
                    continue;
                };
                let Some(element_width) =
                    molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
                else {
                    continue;
                };
                if self.fixed_append_fields(&transition.operand, element_width).is_some() {
                    self.verified_collection_push_values.insert(var.id);
                }
            }
        }
    }

    fn set_constructed_byte_vectors(&mut self, body: &IrBody) {
        self.stack_collection_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        let mut named_vectors = HashMap::<String, usize>::new();
        let mut named_stack_collections = HashMap::<String, usize>::new();
        let mut loaded_vector_names = HashMap::<usize, String>::new();
        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    IrInstruction::StoreVar { name, src: IrOperand::Var(src) } => {
                        if self.stack_collection_vars.contains(&src.id) {
                            named_stack_collections.insert(name.clone(), src.id);
                        }
                        if self.constructed_byte_vectors.contains_key(&src.id) {
                            named_vectors.insert(name.clone(), src.id);
                        }
                    }
                    IrInstruction::LoadVar { dest, name } => {
                        if let Some(source_id) = named_stack_collections.get(name).copied() {
                            self.stack_collection_vars.insert(dest.id);
                            named_stack_collections.insert(name.clone(), dest.id);
                            if let Some(bytes) = self.constructed_byte_vectors.get(&source_id).cloned() {
                                self.constructed_byte_vectors.insert(dest.id, bytes);
                                if let Some(root_id) = self.constructed_byte_vector_roots.get(&source_id).copied() {
                                    self.constructed_byte_vector_roots.insert(dest.id, root_id);
                                }
                                loaded_vector_names.insert(dest.id, name.clone());
                            }
                            continue;
                        }
                        if let Some(source_id) = named_vectors.get(name).copied() {
                            if let Some(bytes) = self.constructed_byte_vectors.get(&source_id).cloned() {
                                self.constructed_byte_vectors.insert(dest.id, bytes);
                                if let Some(root_id) = self.constructed_byte_vector_roots.get(&source_id).copied() {
                                    self.constructed_byte_vector_roots.insert(dest.id, root_id);
                                }
                                loaded_vector_names.insert(dest.id, name.clone());
                            }
                        }
                    }
                    IrInstruction::CollectionNew { dest, .. } => {
                        self.stack_collection_vars.insert(dest.id);
                        self.constructed_byte_vectors.insert(dest.id, Vec::new());
                        self.constructed_byte_vector_roots.insert(dest.id, dest.id);
                    }
                    IrInstruction::CollectionPush { collection: IrOperand::Var(collection), value } => {
                        let width = self.constructed_byte_vector_part_width(value);
                        let source_available = width.is_some_and(|width| self.expected_fixed_byte_source(value, width).is_some());
                        if let Some(bytes) = self.constructed_byte_vectors.get_mut(&collection.id) {
                            if source_available {
                                bytes.push(value.clone());
                                if let Some(name) = loaded_vector_names.get(&collection.id).cloned() {
                                    named_vectors.insert(name, collection.id);
                                }
                            } else {
                                self.constructed_byte_vectors.remove(&collection.id);
                            }
                        }
                    }
                    IrInstruction::CollectionExtend { collection: IrOperand::Var(collection), slice } => {
                        let Some(width) = operand_fixed_byte_width(slice) else {
                            self.constructed_byte_vectors.remove(&collection.id);
                            continue;
                        };
                        let source_available = self.expected_fixed_byte_source(slice, width).is_some();
                        if let Some(bytes) = self.constructed_byte_vectors.get_mut(&collection.id) {
                            if source_available {
                                bytes.push(slice.clone());
                                if let Some(name) = loaded_vector_names.get(&collection.id).cloned() {
                                    named_vectors.insert(name, collection.id);
                                }
                            } else {
                                self.constructed_byte_vectors.remove(&collection.id);
                            }
                        }
                    }
                    IrInstruction::CollectionClear { collection: IrOperand::Var(collection) } => {
                        if let Some(bytes) = self.constructed_byte_vectors.get_mut(&collection.id) {
                            bytes.clear();
                            if let Some(name) = loaded_vector_names.get(&collection.id).cloned() {
                                named_vectors.insert(name, collection.id);
                            }
                        }
                    }
                    IrInstruction::CollectionReverse { collection: IrOperand::Var(collection) } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionTruncate { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionSwap { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionInsert { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionSet { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionPop { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::Move { dest, src: IrOperand::Var(src) }
                    | IrInstruction::Unary { dest, op: UnaryOp::Ref | UnaryOp::Deref, operand: IrOperand::Var(src) } => {
                        if self.stack_collection_vars.contains(&src.id) {
                            self.stack_collection_vars.insert(dest.id);
                        }
                        if let Some(bytes) = self.constructed_byte_vectors.get(&src.id).cloned() {
                            self.constructed_byte_vectors.insert(dest.id, bytes);
                            if let Some(root_id) = self.constructed_byte_vector_roots.get(&src.id).copied() {
                                self.constructed_byte_vector_roots.insert(dest.id, root_id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut verified_roots = BTreeSet::new();
        for pattern in &body.create_set {
            let Some(layouts) = self.type_layouts.get(&pattern.ty) else {
                continue;
            };
            for (field, value) in &pattern.fields {
                let Some(layout) = layouts.get(field) else {
                    continue;
                };
                if molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).is_none() {
                    continue;
                }
                let IrOperand::Var(var) = value else {
                    continue;
                };
                if self.constructed_byte_vectors.contains_key(&var.id) {
                    verified_roots.insert(self.constructed_byte_vector_roots.get(&var.id).copied().unwrap_or(var.id));
                }
            }
        }
        for (var_id, root_id) in &self.constructed_byte_vector_roots {
            if verified_roots.contains(root_id) {
                self.verified_collection_construction_vectors.insert(*var_id);
            }
        }
    }

    fn prelude_scalar_immediate(&self, operand: &IrOperand) -> Option<u64> {
        match operand {
            IrOperand::Const(value) => fixed_scalar_const_value(value),
            IrOperand::Var(var) => self.prelude_scalar_immediates.get(&var.id).copied(),
        }
    }

    fn prelude_fixed_byte_constant(&self, operand: &IrOperand) -> Option<Vec<u8>> {
        match operand {
            IrOperand::Const(value) => fixed_byte_const_bytes(value),
            IrOperand::Var(var) => self.prelude_fixed_byte_constants.get(&var.id).cloned(),
        }
    }

    fn prelude_u64_value_source(&self, operand: &IrOperand) -> Option<PreludeU64ValueSource> {
        match operand {
            IrOperand::Const(IrConst::U64(n)) => Some(PreludeU64ValueSource::Const(*n)),
            IrOperand::Var(var) if var.ty == IrType::U64 && self.param_vars.contains(&var.id) => {
                Some(PreludeU64ValueSource::ParamVar(var.id))
            }
            IrOperand::Var(var) => self.prelude_u64_value_sources.get(&var.id).cloned(),
            _ => None,
        }
    }

    fn prelude_u64_operand_source(&self, operand: &IrOperand) -> Option<PreludeU64OperandSource> {
        match operand {
            IrOperand::Const(IrConst::U64(n)) => Some(PreludeU64OperandSource::Const(*n)),
            IrOperand::Var(var) if var.ty == IrType::U64 && self.param_vars.contains(&var.id) => {
                Some(PreludeU64OperandSource::ParamVar(var.id))
            }
            IrOperand::Var(var) => match self.prelude_u64_value_sources.get(&var.id)? {
                PreludeU64ValueSource::Const(n) => Some(PreludeU64OperandSource::Const(*n)),
                PreludeU64ValueSource::ParamVar(var_id) => Some(PreludeU64OperandSource::ParamVar(*var_id)),
                PreludeU64ValueSource::StackVar(var_id) => Some(PreludeU64OperandSource::StackVar(*var_id)),
                PreludeU64ValueSource::Field(source) => Some(PreludeU64OperandSource::Field(source.clone())),
                PreludeU64ValueSource::Binary { .. } | PreludeU64ValueSource::Min { .. } => {
                    Some(PreludeU64OperandSource::Expr(Box::new(self.prelude_u64_value_sources.get(&var.id)?.clone())))
                }
            },
            _ => None,
        }
    }

    fn generate_body(&mut self, body: &IrBody) -> Result<()> {
        self.emit_read_ref_parameter_bindings();

        for (index, pattern) in body.consume_set.iter().enumerate() {
            self.generate_consume(pattern, index)?;
        }

        let mut read_ref_index = 0usize;
        for pattern in &body.read_refs {
            if self.read_ref_param_ids.contains_key(&pattern.binding) {
                continue;
            }
            let index = read_ref_index;
            read_ref_index += 1;
            self.generate_read_ref(pattern, index)?;
        }

        // Signature-bound outputs are loaded in the entry prelude so
        // verification constraints can read them. Explicit `create name = ...` field
        // checks must stay in body order because their expected expressions may
        // depend on earlier `let`/index computations.
        let explicit_output_create_bindings = body
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .filter_map(|instruction| match instruction {
                IrInstruction::Create { pattern, .. }
                | IrInstruction::CreateUnique { pattern, .. }
                | IrInstruction::ReplaceUnique { pattern, .. } => Some(pattern.binding.as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        for (index, pattern) in body.create_set.iter().enumerate() {
            if !matches!(pattern.operation.as_str(), "create" | "create_unique" | "replace_unique") {
                let explicit_output_create = explicit_output_create_bindings.contains(pattern.binding.as_str());
                self.generate_create(pattern, index, !explicit_output_create, explicit_output_create)?;
            }
        }

        for pattern in &body.mutate_set {
            self.generate_mutate_replacement(pattern)?;
        }

        for (index, block) in body.blocks.iter().enumerate() {
            let fallthrough = body.blocks.get(index + 1).map(|next| next.id);
            self.generate_block(block, fallthrough)?;
        }

        Ok(())
    }

    fn emit_read_ref_parameter_bindings(&mut self) {
        let mut input_bindings = self
            .read_ref_param_ids
            .iter()
            .filter_map(|(binding, var_id)| {
                self.read_ref_param_input_indices.get(var_id).copied().map(|input_index| (input_index, binding.clone(), *var_id))
            })
            .collect::<Vec<_>>();
        input_bindings.sort_by_key(|(input_index, _, _)| *input_index);
        for (input_index, binding, var_id) in input_bindings {
            let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
                continue;
            };
            let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
                continue;
            };
            self.emit(format!("# cellscript abi: bind read-only param {} to Input#{} cell data", binding, input_index));
            self.emit_load_cell_data_syscall_to_offsets(
                "read_ref_param_input",
                CKB_SOURCE_INPUT,
                input_index,
                size_offset,
                buffer_offset,
                RUNTIME_CELL_BUFFER_SIZE,
            );
            self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
            self.emit_sp_addi("t0", buffer_offset);
            self.emit_stack_store("t0", var_id * 8);
        }

        let mut dep_bindings = self
            .read_ref_param_ids
            .iter()
            .filter_map(|(binding, var_id)| {
                self.read_ref_param_dep_indices.get(var_id).copied().map(|dep_index| (dep_index, binding.clone(), *var_id))
            })
            .collect::<Vec<_>>();
        dep_bindings.sort_by_key(|(dep_index, _, _)| *dep_index);
        for (dep_index, binding, var_id) in dep_bindings {
            let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
                continue;
            };
            let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
                continue;
            };
            self.emit(format!("# cellscript abi: bind read-only param {} to CellDep#{} cell data", binding, dep_index));
            self.emit_load_cell_data_syscall_to_offsets(
                "read_ref_param_dep",
                CKB_SOURCE_CELL_DEP,
                dep_index,
                size_offset,
                buffer_offset,
                RUNTIME_CELL_BUFFER_SIZE,
            );
            self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
            self.emit_sp_addi("t0", buffer_offset);
            self.emit_stack_store("t0", var_id * 8);
        }
    }

    fn generate_consume(&mut self, pattern: &CellPattern, index: usize) -> Result<()> {
        self.emit(format!("# {} input {}", pattern.operation, pattern.binding));
        if let Some(var_id) =
            self.consume_binding_ids.get(&pattern.binding).copied().or_else(|| self.consume_order.get(index).copied())
        {
            if let (Some(size_offset), Some(buffer_offset)) =
                (self.cell_buffer_size_offsets.get(&var_id).copied(), self.cell_buffer_offsets.get(&var_id).copied())
            {
                let input_index = self.consume_indices.get(&var_id).copied().unwrap_or(index);
                self.emit_load_cell_data_syscall_to_offsets(
                    &pattern.operation,
                    CKB_SOURCE_INPUT,
                    input_index,
                    size_offset,
                    buffer_offset,
                    RUNTIME_CELL_BUFFER_SIZE,
                );
                self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
                self.emit_sp_addi("t0", buffer_offset);
                self.emit_stack_store("t0", var_id * 8);
                if pattern.operation == "destroy" {
                    self.emit_destroy_group_output_absence_scan(pattern, input_index);
                }
                return Ok(());
            }
        }

        self.emit_load_cell_data_syscall(&pattern.operation, CKB_SOURCE_INPUT, index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if pattern.operation == "destroy" {
            self.emit_destroy_group_output_absence_scan(pattern, index);
        }
        Ok(())
    }

    fn generate_read_ref(&mut self, pattern: &CellPattern, index: usize) -> Result<()> {
        self.emit(format!("# read_ref {}", pattern.binding));
        if let Some(var_id) = self.read_ref_order.get(index).copied() {
            if let (Some(size_offset), Some(buffer_offset)) =
                (self.cell_buffer_size_offsets.get(&var_id).copied(), self.cell_buffer_offsets.get(&var_id).copied())
            {
                let dep_index = self.read_ref_indices.get(&var_id).copied().unwrap_or(index);
                self.emit_load_cell_data_syscall_to_offsets(
                    "read_ref",
                    CKB_SOURCE_CELL_DEP,
                    dep_index,
                    size_offset,
                    buffer_offset,
                    RUNTIME_CELL_BUFFER_SIZE,
                );
                self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
                self.emit_sp_addi("t0", buffer_offset);
                self.emit_stack_store("t0", var_id * 8);
                return Ok(());
            }
        }

        self.emit_load_cell_data_syscall("read_ref", CKB_SOURCE_CELL_DEP, index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        Ok(())
    }

    fn generate_create(
        &mut self,
        pattern: &CreatePattern,
        index: usize,
        defer_unverifiable_output_fields: bool,
        defer_all_output_fields: bool,
    ) -> Result<()> {
        // The verifier cannot create cells inside CKB-VM; it can only verify the
        // transaction output selected by the lowering metadata.
        self.emit(format!("# {} output {}", pattern.operation, pattern.ty));
        if pattern.operation == "output" {
            if let Some(var_id) = self.output_param_ids.get(&pattern.binding).copied() {
                let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                };
                let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                };
                self.emit_load_cell_data_syscall_to_offsets(
                    "output_param",
                    CKB_SOURCE_OUTPUT,
                    index,
                    size_offset,
                    buffer_offset,
                    RUNTIME_CELL_BUFFER_SIZE,
                );
                self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
                self.emit_sp_addi("t0", buffer_offset);
                self.emit_stack_store("t0", var_id * 8);
                self.operation_output_indices.insert(var_id, index);
                if defer_all_output_fields {
                    self.emit("# cellscript abi: output field verification deferred to ordered create constraint");
                } else if pattern.fields.is_empty() {
                    self.emit_state_transition_check(pattern, size_offset, buffer_offset);
                } else if self.can_verify_create_output_fields(pattern) {
                    self.emit_create_output_checks_at(pattern, size_offset, buffer_offset);
                } else if defer_unverifiable_output_fields && self.create_output_fields_cover_type(pattern) {
                    self.emit("# cellscript abi: output field verification deferred to explicit verification constraints");
                } else {
                    self.emit("# cellscript abi: output field verification incomplete for this named output");
                    self.emit("# cellscript abi: fail closed because the output state is not fully verified");
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                }
                if let Some(lock) = &pattern.lock {
                    if defer_all_output_fields {
                        self.emit("# cellscript abi: output lock verification deferred to ordered create constraint");
                        self.next_virtual_output = self.next_virtual_output.max(index + 1);
                        return Ok(());
                    }
                    if !(self.can_verify_output_lock(pattern) && self.emit_output_lock_hash_check(index, lock)) {
                        self.emit("# cellscript abi: output lock verification incomplete for this named output");
                        self.emit("# cellscript abi: fail closed because the output lock is not fully verified");
                        self.emit_fail(CellScriptRuntimeError::EntryWitnessMagicMismatch);
                        return Ok(());
                    }
                }
                self.next_virtual_output = self.next_virtual_output.max(index + 1);
                return Ok(());
            }
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return Ok(());
        }
        self.emit_load_cell_data_syscall(&pattern.operation, CKB_SOURCE_OUTPUT, index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);

        if pattern.lock.is_some() {
            self.emit("# set lock script");
        }

        if self.can_verify_create_output_fields(pattern) {
            self.emit_create_output_checks(pattern);
        } else {
            self.emit("# cellscript abi: output field verification incomplete for this create pattern");
            self.emit("# cellscript abi: fail closed because the output state is not fully verified");
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return Ok(());
        }

        if let Some(lock) = &pattern.lock {
            if self.can_verify_output_lock(pattern) && self.emit_output_lock_hash_check(index, lock) {
                return Ok(());
            }
            self.emit("# cellscript abi: output lock verification incomplete for this create pattern");
            self.emit("# cellscript abi: fail closed because the output lock is not fully verified");
            self.emit_fail(CellScriptRuntimeError::EntryWitnessMagicMismatch);
        }

        Ok(())
    }

    fn generate_mutate_replacement(&mut self, pattern: &MutatePattern) -> Result<()> {
        self.emit(format!(
            "# mutate output {} {} Input#{} -> Output#{}",
            pattern.binding, pattern.ty, pattern.input_index, pattern.output_index
        ));
        self.emit_mutate_parameter_binding(pattern);
        if pattern.preserve_type_hash {
            self.emit_mutate_replacement_field_hash_check(
                pattern,
                CKB_CELL_FIELD_TYPE_HASH,
                "type_hash",
                CellScriptRuntimeError::TypeHashPreservationMismatch,
            );
        }
        if pattern.preserve_lock_hash {
            self.emit_mutate_replacement_field_hash_check(
                pattern,
                CKB_CELL_FIELD_LOCK_HASH,
                "lock_hash",
                CellScriptRuntimeError::LockHashPreservationMismatch,
            );
        }
        self.emit_mutate_replacement_preserved_field_checks(pattern);
        self.emit_mutate_replacement_transition_checks(pattern);
        self.emit_mutate_replacement_set_transition_checks(pattern);
        self.emit_mutate_replacement_u128_transition_checks(pattern);
        Ok(())
    }

    fn emit_mutate_parameter_binding(&mut self, pattern: &MutatePattern) {
        let Some(var_id) = self.mutate_param_ids.get(&pattern.binding).copied() else {
            return;
        };
        let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
            return;
        };
        let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
            return;
        };
        self.emit(format!("# cellscript abi: bind mutable param {} to Input#{} cell data", pattern.binding, pattern.input_index));
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_param_input",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            size_offset,
            buffer_offset,
            RUNTIME_CELL_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", var_id * 8);
    }

    fn generate_block(&mut self, block: &IrBlock, fallthrough: Option<BlockId>) -> Result<()> {
        self.emit_label(&self.block_label(block.id));

        for instruction in &block.instructions {
            self.generate_instruction(instruction)?;
        }

        self.generate_terminator(&block.terminator, fallthrough)?;

        Ok(())
    }

    fn generate_instruction(&mut self, instruction: &IrInstruction) -> Result<()> {
        match instruction {
            IrInstruction::LoadConst { dest, value } => {
                self.emit_load_const(dest, value)?;
            }
            IrInstruction::LoadVar { dest, name } => {
                self.emit_load_var(dest, name)?;
            }
            IrInstruction::StoreVar { name, src } => {
                self.emit_store_var(name, src)?;
            }
            IrInstruction::Binary { dest, op, left, right } => {
                self.emit_binary(dest, *op, left, right)?;
            }
            IrInstruction::Unary { dest, op, operand } => {
                self.emit_unary(dest, *op, operand)?;
            }
            IrInstruction::FieldAccess { dest, obj, field } => {
                self.emit_field_access(dest, obj, field)?;
            }
            IrInstruction::Index { dest, arr, idx } => {
                self.emit_index(dest, arr, idx)?;
            }
            IrInstruction::Length { dest, operand } => {
                self.emit_length(dest, operand)?;
            }
            IrInstruction::TypeHash { dest, operand } => {
                self.emit_type_hash(dest, operand)?;
            }
            IrInstruction::CollectionNew { dest, ty, capacity } => {
                self.emit_collection_new(dest, ty, capacity.as_ref())?;
            }
            IrInstruction::CollectionCapacity { dest, collection } => {
                self.emit_collection_capacity(dest, collection)?;
            }
            IrInstruction::CollectionPush { collection, value } => {
                self.emit_collection_push(collection, value)?;
            }
            IrInstruction::CollectionExtend { collection, slice } => {
                self.emit_collection_extend(collection, slice)?;
            }
            IrInstruction::CollectionClear { collection } => {
                self.emit_collection_clear(collection)?;
            }
            IrInstruction::CollectionReverse { collection } => {
                self.emit_collection_reverse(collection)?;
            }
            IrInstruction::CollectionTruncate { collection, len } => {
                self.emit_collection_truncate(collection, len)?;
            }
            IrInstruction::CollectionSwap { collection, left, right } => {
                self.emit_collection_swap(collection, left, right)?;
            }
            IrInstruction::CollectionContains { dest, collection, value } => {
                self.emit_collection_contains(dest, collection, value)?;
            }
            IrInstruction::CollectionRemove { dest, collection, index } => {
                self.emit_collection_remove(dest, collection, index)?;
            }
            IrInstruction::CollectionInsert { collection, index, value } => {
                self.emit_collection_insert(collection, index, value)?;
            }
            IrInstruction::CollectionSet { collection, index, value } => {
                self.emit_collection_set(collection, index, value)?;
            }
            IrInstruction::CollectionPop { dest, collection } => {
                self.emit_collection_pop(dest, collection)?;
            }
            IrInstruction::Call { dest, func, args } => {
                self.emit_call(dest.as_ref(), func, args)?;
            }
            IrInstruction::ReadRef { dest, ty } => {
                self.emit_read_ref(dest, ty)?;
            }
            IrInstruction::Move { dest, src } => {
                self.emit_move(dest, src)?;
            }
            IrInstruction::Tuple { dest, fields } => {
                self.emit_tuple(dest, fields)?;
            }
            IrInstruction::Consume { operand } => {
                self.emit_consume(operand)?;
            }
            IrInstruction::Create { dest, pattern } => {
                self.emit_create(dest, pattern)?;
            }
            IrInstruction::Transfer { dest, operand, to } => {
                self.emit_transfer(dest, operand, to)?;
            }
            IrInstruction::Destroy { operand, policy: _ } => {
                self.emit_destroy(operand)?;
            }
            IrInstruction::Claim { dest, receipt } => {
                self.emit_claim(dest, receipt)?;
            }
            IrInstruction::Settle { dest, operand } => {
                self.emit_settle(dest, operand)?;
            }
            IrInstruction::CellMetadataEquality { left, right, field } => {
                self.emit_cell_metadata_equality(left, right, *field)?;
            }
            IrInstruction::CreateUnique { dest, pattern, identity } => {
                self.emit_create_unique(dest, pattern, identity)?;
            }
            IrInstruction::ReplaceUnique { dest, operand, pattern, identity } => {
                self.emit_replace_unique(dest, operand, pattern, identity)?;
            }
        }
        Ok(())
    }

    fn generate_terminator(&mut self, terminator: &IrTerminator, fallthrough: Option<BlockId>) -> Result<()> {
        match terminator {
            IrTerminator::Return(None) => {
                self.emit("li a0, 0");
                self.emit_epilogue();
            }
            IrTerminator::Return(Some(operand)) => {
                if !self.current_lock_entry && self.operand_is_u128_like(operand) {
                    self.emit("# cellscript abi: return u128 via a0(low)/a1(high)");
                    if self.emit_u128_operand_limbs("a0", "a1", "t6", "t4", operand, "u128 return") {
                        self.emit_epilogue();
                    }
                    return Ok(());
                }
                if let IrOperand::Var(v) = operand {
                    if let Some(fields) = self.tuple_aggregate_fields.get(&v.id).cloned() {
                        self.emit(format!("# cellscript abi: return tuple aggregate var{} fields={}", v.id, fields.len()));
                        if fields.is_empty() {
                            self.emit("li a0, 0");
                        }
                        for (index, field) in fields.iter().take(8).enumerate() {
                            self.emit(format!("# cellscript abi: return tuple field .{} via a{}", index, index));
                            self.emit_operand_to_register(&format!("a{}", index), field);
                        }
                        self.emit_epilogue();
                        return Ok(());
                    }
                }
                self.emit_operand_to_register("a0", operand);
                if self.current_lock_entry {
                    let ok_label = self.fresh_label("lock_predicate_true");
                    self.emit(format!("bnez a0, {}", ok_label));
                    self.emit_runtime_error_comment(CellScriptRuntimeError::AssertionFailed);
                    self.emit(format!("li a0, {}", CellScriptRuntimeError::AssertionFailed.code()));
                    self.emit_epilogue();
                    self.emit_label(&ok_label);
                    self.emit("li a0, 0");
                    self.emit_epilogue();
                    return Ok(());
                }
                self.emit_epilogue();
            }
            IrTerminator::Jump(block_id) => {
                self.emit_jump_to_block(*block_id, fallthrough);
            }
            IrTerminator::Branch { cond, then_block, else_block } => match cond {
                IrOperand::Const(IrConst::Bool(b)) => {
                    self.emit_jump_to_block(if *b { *then_block } else { *else_block }, fallthrough);
                }
                IrOperand::Const(IrConst::U64(n)) => {
                    self.emit_jump_to_block(if *n != 0 { *then_block } else { *else_block }, fallthrough);
                }
                IrOperand::Var(_) if then_block == else_block => {
                    self.emit_jump_to_block(*then_block, fallthrough);
                }
                IrOperand::Var(v) => {
                    self.emit_stack_load("t0", v.id * 8);
                    if Some(*then_block) == fallthrough {
                        self.emit(format!("beqz t0, {}", self.block_label(*else_block)));
                    } else if Some(*else_block) == fallthrough {
                        self.emit(format!("bnez t0, {}", self.block_label(*then_block)));
                    } else {
                        self.emit(format!("beqz t0, {}", self.block_label(*else_block)));
                        self.emit_jump_to_block(*then_block, fallthrough);
                    }
                }
                _ => {
                    self.emit_jump_to_block(*else_block, fallthrough);
                }
            },
        }
        Ok(())
    }

    fn emit_prologue(&mut self) {
        self.emit_large_addi("sp", "sp", -(self.frame_size as i64));
        self.emit_stack_store("ra", self.frame_size - 8);
        self.emit_stack_store("fp", self.frame_size - 16);
        self.emit_sp_addi("fp", self.frame_size);
    }

    fn emit_epilogue(&mut self) {
        if let Some(function) = &self.current_function {
            self.emit(format!("j .L{}_epilogue", function));
            return;
        }
        self.emit_epilogue_body();
    }

    fn emit_fail(&mut self, error: CellScriptRuntimeError) {
        if let Some(function) = self.current_function.clone() {
            self.fail_handler_codes.insert(error);
            self.emit(format!("j .L{}_fail_{}", function, error.code()));
            return;
        }
        self.emit_runtime_error_comment(error);
        self.emit(format!("li a0, {}", error.code()));
        self.emit_epilogue_body();
    }

    fn emit_shared_epilogue(&mut self) {
        let Some(function) = self.current_function.clone() else {
            return;
        };
        let fail_codes = self.fail_handler_codes.iter().copied().collect::<Vec<_>>();
        for error in fail_codes {
            self.emit_label(&format!(".L{}_fail_{}", function, error.code()));
            self.emit_runtime_error_comment(error);
            self.emit(format!("li a0, {}", error.code()));
            self.emit(format!("j .L{}_epilogue", function));
        }
        self.emit_label(&format!(".L{}_epilogue", function));
        self.emit_epilogue_body();
    }

    fn emit_runtime_error_comment(&mut self, error: CellScriptRuntimeError) {
        self.emit(format!("# cellscript runtime error {} {}", error.code(), error.name()));
    }

    fn emit_epilogue_body(&mut self) {
        self.emit_stack_load("ra", self.frame_size - 8);
        self.emit_stack_load("fp", self.frame_size - 16);
        self.emit_large_addi("sp", "sp", self.frame_size as i64);
        self.emit("ret");
    }

    /// Emit `addi rd, rs1, imm` handling immediates that don't fit in 12 bits.
    fn emit_large_addi(&mut self, rd: &str, rs1: &str, imm: i64) {
        if (-2048..=2047).contains(&imm) {
            self.emit(format!("addi {}, {}, {}", rd, rs1, imm));
        } else {
            let scratch = scratch_register_avoiding(&[rs1]);
            self.emit(format!("li {}, {}", scratch, imm));
            self.emit(format!("add {}, {}, {}", rd, rs1, scratch));
        }
    }

    fn emit_memory_load_with_avoid(&mut self, opcode: &str, dst: &str, base: &str, offset: usize, avoid: &[&str]) {
        let offset = i64::try_from(offset).expect("memory offset should fit in i64");
        if small_signed_immediate(offset) {
            self.emit(format!("{} {}, {}({})", opcode, dst, offset, base));
        } else {
            let mut registers = Vec::with_capacity(2 + avoid.len());
            registers.push(dst);
            registers.push(base);
            registers.extend_from_slice(avoid);
            let scratch = scratch_register_avoiding(&registers);
            self.emit(format!("li {}, {}", scratch, offset));
            self.emit(format!("add {}, {}, {}", scratch, base, scratch));
            self.emit(format!("{} {}, 0({})", opcode, dst, scratch));
        }
    }

    /// Emit `ld rd, offset(sp)` through the centralized stack-offset gate.
    fn emit_stack_load(&mut self, rd: &str, offset: usize) {
        self.emit_stack_access("ld", rd, offset);
    }

    /// Emit `lbu rd, offset(sp)` through the centralized stack-offset gate.
    fn emit_stack_load_byte(&mut self, rd: &str, offset: usize) {
        self.emit_stack_access("lbu", rd, offset);
    }

    /// Emit `sd rs2, offset(sp)` through the centralized stack-offset gate.
    fn emit_stack_store(&mut self, rs2: &str, offset: usize) {
        self.emit_stack_access("sd", rs2, offset);
    }

    /// Emit `sb rs2, offset(sp)` through the centralized stack-offset gate.
    fn emit_stack_store_byte(&mut self, rs2: &str, offset: usize) {
        self.emit_stack_access("sb", rs2, offset);
    }

    fn emit_stack_access(&mut self, opcode: &str, register: &str, offset: usize) {
        let offset = i64::try_from(offset).expect("stack offset should fit in i64");
        if small_signed_immediate(offset) {
            self.emit(format!("{} {}, {}(sp)", opcode, register, offset));
        } else {
            let scratch = scratch_register_avoiding(&[register]);
            self.emit(format!("li {}, {}", scratch, offset));
            self.emit(format!("add {}, sp, {}", scratch, scratch));
            self.emit(format!("{} {}, 0({})", opcode, register, scratch));
        }
    }

    /// Emit `addi rd, sp, offset` handling offsets that don't fit in 12 bits.
    fn emit_sp_addi(&mut self, rd: &str, offset: usize) {
        if offset <= 2047 {
            self.emit(format!("addi {}, sp, {}", rd, offset));
        } else if rd == "sp" {
            self.emit_large_addi("sp", "sp", offset as i64);
        } else {
            self.emit(format!("li {}, {}", rd, offset));
            self.emit(format!("add {}, sp, {}", rd, rd));
        }
    }

    fn prepare_function_layout(&mut self, body: &IrBody, params: &[IrParam]) {
        let mut max_var_id = None;
        let mut fixed_byte_locals = HashMap::<usize, usize>::new();
        let mut named_vars = BTreeSet::<String>::new();
        for param in params {
            self.record_var(&param.binding, &mut max_var_id);
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                self.record_instruction_var(instruction, &mut max_var_id);
                self.record_instruction_fixed_byte_local(instruction, &mut fixed_byte_locals);
                if let IrInstruction::StoreVar { name, .. } = instruction {
                    named_vars.insert(name.clone());
                }
            }
            self.record_terminator_var(&block.terminator, &mut max_var_id);
        }

        let locals_size = max_var_id.map(|id| (id + 1) * 8).unwrap_or(0);
        self.fixed_byte_local_offsets.clear();
        self.named_var_offsets.clear();
        self.cell_buffer_offsets.clear();
        self.cell_buffer_size_offsets.clear();
        self.dynamic_value_size_offsets.clear();
        self.empty_molecule_vector_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        self.output_type_hash_sources.clear();
        self.consume_order.clear();
        self.consume_indices.clear();
        self.consume_type_names.clear();
        self.consume_binding_ids.clear();
        self.read_ref_order.clear();
        self.read_ref_indices.clear();
        self.read_ref_param_ids.clear();
        self.read_ref_param_input_indices.clear();
        self.read_ref_param_dep_indices.clear();
        self.output_param_ids.clear();
        self.mutate_param_ids.clear();
        self.schema_pointer_size_offsets.clear();
        self.fixed_byte_param_size_offsets.clear();
        self.param_type_hash_pointer_offsets.clear();
        self.param_type_hash_size_offsets.clear();
        self.param_type_hash_sources.clear();
        self.u128_value_offsets.clear();
        self.collection_region_start = 0;
        self.next_collection_slot = 0;

        let schema_param_ids =
            params.iter().filter(|param| named_type_name(&param.ty).is_some()).map(|param| param.binding.id).collect::<BTreeSet<_>>();
        let mut param_type_hash_ids = BTreeSet::new();
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::TypeHash { dest, operand: IrOperand::Var(var) } = instruction {
                    if schema_param_ids.contains(&var.id) {
                        param_type_hash_ids.insert(var.id);
                        self.param_type_hash_sources.insert(dest.id, var.id);
                    }
                }
            }
        }

        let mut next_cell_slot = locals_size;
        let mut fixed_byte_locals = fixed_byte_locals.into_iter().collect::<Vec<_>>();
        fixed_byte_locals.sort_unstable_by_key(|(var_id, _)| *var_id);
        for (var_id, width) in fixed_byte_locals {
            next_cell_slot = align_up(next_cell_slot, 8);
            self.fixed_byte_local_offsets.insert(var_id, next_cell_slot);
            next_cell_slot += align_up(width, 8);
        }
        for name in named_vars {
            next_cell_slot = align_up(next_cell_slot, 8);
            self.named_var_offsets.insert(name, next_cell_slot);
            next_cell_slot += 8;
        }
        for param in params {
            if param.source == ParamSource::Output {
                self.output_param_ids.insert(param.name.clone(), param.binding.id);
                self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
                self.cell_buffer_size_offsets.insert(param.binding.id, next_cell_slot);
                self.cell_buffer_offsets.insert(param.binding.id, next_cell_slot + 8);
                next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                continue;
            }
            if named_type_name(&param.ty).is_some() {
                self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
                next_cell_slot += 8;
            } else if fixed_byte_pointer_param_width(&param.ty).is_some() || fixed_aggregate_pointer_param_width(&param.ty).is_some() {
                self.fixed_byte_param_size_offsets.insert(param.binding.id, next_cell_slot);
                next_cell_slot += 8;
            }
        }
        for param in params {
            if param_type_hash_ids.contains(&param.binding.id) {
                self.param_type_hash_pointer_offsets.insert(param.binding.id, next_cell_slot);
                next_cell_slot += 8;
                self.param_type_hash_size_offsets.insert(param.binding.id, next_cell_slot);
                next_cell_slot += 8;
            }
        }

        if self.bind_readonly_schema_params {
            let consumed_param_names = body.consume_set.iter().map(|pattern| pattern.binding.as_str()).collect::<BTreeSet<_>>();
            let mutate_param_names = body.mutate_set.iter().map(|pattern| pattern.binding.as_str()).collect::<BTreeSet<_>>();
            let read_ref_indices_by_binding =
                body.read_refs.iter().enumerate().map(|(index, pattern)| (pattern.binding.as_str(), index)).collect::<HashMap<_, _>>();
            let mut read_ref_param_index = 0usize;
            for param in params {
                if param.source == ParamSource::Output {
                    continue;
                }
                if !self.param_is_runtime_bound(param) {
                    continue;
                }
                if mutate_param_names.contains(param.name.as_str()) || consumed_param_names.contains(param.name.as_str()) {
                    continue;
                }
                self.read_ref_param_ids.insert(param.name.clone(), param.binding.id);
                if let Some(dep_index) = read_ref_indices_by_binding.get(param.name.as_str()).copied() {
                    self.read_ref_param_dep_indices.insert(param.binding.id, dep_index);
                } else {
                    let input_index = body.consume_set.len() + body.mutate_set.len() + read_ref_param_index;
                    self.read_ref_param_input_indices.insert(param.binding.id, input_index);
                    read_ref_param_index += 1;
                }
                self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
                self.cell_buffer_size_offsets.insert(param.binding.id, next_cell_slot);
                self.cell_buffer_offsets.insert(param.binding.id, next_cell_slot + 8);
                next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
            }
        }

        for pattern in &body.mutate_set {
            let Some(param) = params.iter().find(|param| param.name == pattern.binding) else {
                continue;
            };
            self.mutate_param_ids.insert(pattern.binding.clone(), param.binding.id);
            self.consume_type_names.insert(param.binding.id, pattern.ty.clone());
            self.consume_binding_ids.insert(pattern.binding.clone(), param.binding.id);
            self.consume_indices.insert(param.binding.id, pattern.input_index);
            self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
            self.cell_buffer_size_offsets.insert(param.binding.id, next_cell_slot);
            self.cell_buffer_offsets.insert(param.binding.id, next_cell_slot + 8);
            next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
        }

        let consume_pattern_indices =
            body.consume_set.iter().enumerate().map(|(index, pattern)| (pattern.binding.as_str(), index)).collect::<HashMap<_, _>>();
        for pattern in &body.consume_set {
            let Some(param) = params.iter().find(|param| param.name == pattern.binding) else {
                continue;
            };
            if self.consume_binding_ids.contains_key(&pattern.binding) {
                continue;
            }
            if let Some(type_name) = named_type_name(&param.ty) {
                self.consume_type_names.insert(param.binding.id, type_name.to_string());
            }
            self.consume_binding_ids.insert(pattern.binding.clone(), param.binding.id);
            self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
            self.cell_buffer_size_offsets.insert(param.binding.id, next_cell_slot);
            self.cell_buffer_offsets.insert(param.binding.id, next_cell_slot + 8);
            self.consume_order.push(param.binding.id);
            self.consume_indices.insert(param.binding.id, consume_pattern_indices.get(pattern.binding.as_str()).copied().unwrap_or(0));
            next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let Some(var) = consumed_operand_var(instruction) {
                    if self.consume_binding_ids.contains_key(&var.name) {
                        continue;
                    }
                    if let Some(type_name) = named_type_name(&var.ty) {
                        self.consume_type_names.insert(var.id, type_name.to_string());
                    }
                    self.consume_binding_ids.insert(var.name.clone(), var.id);
                    self.schema_pointer_size_offsets.insert(var.id, next_cell_slot);
                    self.cell_buffer_size_offsets.insert(var.id, next_cell_slot);
                    self.cell_buffer_offsets.insert(var.id, next_cell_slot + 8);
                    self.consume_order.push(var.id);
                    self.consume_indices.insert(
                        var.id,
                        consume_pattern_indices.get(var.name.as_str()).copied().unwrap_or(self.consume_order.len() - 1),
                    );
                    next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                }
            }
        }

        let mut read_ref_index = 0usize;
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::ReadRef { dest, .. } = instruction {
                    self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                    self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                    self.read_ref_order.push(dest.id);
                    self.read_ref_indices.insert(dest.id, read_ref_index);
                    next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                    read_ref_index += 1;
                }
            }
        }

        let mut create_dest_outputs = HashMap::new();
        let mut next_create_output_index =
            body.create_set.iter().position(|pattern| pattern.operation == "create").unwrap_or(body.create_set.len());
        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    IrInstruction::FieldAccess { dest, obj: IrOperand::Var(obj), field } => {
                        if named_type_name(&dest.ty).is_some()
                            && named_type_name(&obj.ty)
                                .and_then(|type_name| self.type_layouts.get(type_name))
                                .and_then(|fields| fields.get(field))
                                .is_some_and(|layout| {
                                    layout_fixed_byte_width(layout).is_none()
                                        && molecule_vector_element_fixed_width(
                                            &layout.ty,
                                            &self.type_fixed_sizes,
                                            &self.enum_fixed_sizes,
                                        )
                                        .is_some()
                                })
                        {
                            self.dynamic_value_size_offsets.insert(dest.id, next_cell_slot);
                            next_cell_slot += 8;
                        }
                    }
                    IrInstruction::Create { dest, pattern } => {
                        let output_index = if pattern.operation == "create" {
                            let output_index = next_create_output_index;
                            next_create_output_index += 1;
                            Some(output_index)
                        } else {
                            Self::create_output_index(body, &pattern.operation, &pattern.binding, &pattern.ty)
                        };
                        if let Some(output_index) = output_index {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::CreateUnique { dest, pattern, .. } | IrInstruction::ReplaceUnique { dest, pattern, .. } => {
                        if let Some(output_index) = Self::create_output_index(body, &pattern.operation, &pattern.binding, &pattern.ty)
                        {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::Transfer { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "transfer", dest) {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::Claim { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "claim", dest) {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::Settle { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "settle", dest) {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::TypeHash { dest, operand: IrOperand::Var(var) } => {
                        if let Some(output_index) = create_dest_outputs.get(&var.id).copied() {
                            self.output_type_hash_sources.insert(dest.id, output_index);
                            self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                            self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                            next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                        } else if self.consume_indices.contains_key(&var.id)
                            || self.read_ref_indices.contains_key(&var.id)
                            || self.read_ref_param_input_indices.contains_key(&var.id)
                        {
                            self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                            self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                            next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                        }
                    }
                    IrInstruction::Call { dest: Some(dest), func, args }
                        if func == "__ckb_current_script_hash" && args.is_empty() && dest.ty == IrType::Hash =>
                    {
                        self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                        self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                        next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                    }
                    IrInstruction::Call { dest: Some(dest), func, args }
                        if matches!(
                            func.as_str(),
                            "__ckb_input_out_point_tx_hash"
                                | "__ckb_cell_lock_hash"
                                | "__ckb_cell_type_hash"
                                | "__ckb_cell_data_hash"
                                | "__ckb_cell_data_hash_at"
                                | "__ckb_cell_lock_code_hash"
                                | "__ckb_cell_type_code_hash"
                                | "__ckb_cell_lock_args_hash"
                                | "__ckb_cell_type_args_hash"
                        ) && (args.len() == 1 || (func == "__ckb_cell_data_hash_at" && args.len() == 2))
                            && dest.ty == IrType::Hash =>
                    {
                        self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                        self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                        next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                    }
                    IrInstruction::Call { dest: Some(dest), func, args }
                        if matches!(
                            func.as_str(),
                            "__ckb_witness_raw" | "__ckb_witness_lock" | "__ckb_witness_input_type" | "__ckb_witness_output_type"
                        ) && args.len() == 1
                            && dest.ty == IrType::Hash =>
                    {
                        self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                        self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                        next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                    }
                    _ => {}
                }
            }
        }

        let mut u128_value_ids = BTreeSet::new();
        for param in params {
            if param.ty == IrType::U128 {
                u128_value_ids.insert(param.binding.id);
            }
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                self.collect_u128_instruction_vars(instruction, &mut u128_value_ids);
            }
            self.collect_u128_terminator_vars(&block.terminator, &mut u128_value_ids);
        }
        for var_id in u128_value_ids {
            self.u128_value_offsets.insert(var_id, next_cell_slot);
            next_cell_slot += 16;
        }

        let collection_slot_size = 8 + RUNTIME_COLLECTION_BUFFER_SIZE;
        let collection_count = body
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .filter(|instruction| matches!(instruction, IrInstruction::CollectionNew { .. }))
            .count();
        self.collection_region_start = next_cell_slot;
        next_cell_slot += collection_count * collection_slot_size;

        self.frame_size = align_frame(next_cell_slot + RUNTIME_EXPR_TEMP_SIZE + RUNTIME_SCRATCH_SIZE + 16);
    }

    fn runtime_expr_temp_offset(&self, depth: usize) -> Option<usize> {
        (depth < RUNTIME_EXPR_TEMP_SLOTS).then(|| self.runtime_scratch_size_offset() - RUNTIME_EXPR_TEMP_SIZE + depth * 8)
    }

    fn runtime_scratch_size_offset(&self) -> usize {
        self.frame_size - 16 - RUNTIME_SCRATCH_SIZE
    }

    fn runtime_scratch_buffer_offset(&self) -> usize {
        self.runtime_scratch_size_offset() + 8
    }

    fn runtime_scratch2_size_offset(&self) -> usize {
        self.runtime_scratch_size_offset() + RUNTIME_SCRATCH_SLOT_SIZE
    }

    fn runtime_scratch2_buffer_offset(&self) -> usize {
        self.runtime_scratch2_size_offset() + 8
    }

    fn emit_store_data_args_at(&mut self, max_bytes: usize, size_offset: usize, buffer_offset: usize) {
        self.emit(format!("li t0, {}", max_bytes));
        self.emit_stack_store("t0", size_offset);
        self.emit_sp_addi("a0", buffer_offset);
        self.emit_sp_addi("a1", size_offset);
        self.emit("li a2, 0");
    }

    fn emit_load_cell_data_syscall(&mut self, reason: &str, source: u64, index: usize) {
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(reason, source, index, size_offset, buffer_offset, RUNTIME_SCRATCH_BUFFER_SIZE);
    }

    fn emit_load_cell_data_syscall_to_offsets(
        &mut self,
        reason: &str,
        source: u64,
        index: usize,
        size_offset: usize,
        buffer_offset: usize,
        max_bytes: usize,
    ) {
        self.emit(format!("# cellscript abi: LOAD_CELL_DATA reason={} source={} index={}", reason, ckb_source_name(source), index));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("li a3, {}", index));
        self.emit(format!("li a4, {}", source));
        self.emit(format!("li a7, {}", self.runtime_abi().load_cell_data));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_load_witness_syscall_to_offsets(
        &mut self,
        reason: &str,
        source: u64,
        index: usize,
        size_offset: usize,
        buffer_offset: usize,
        max_bytes: usize,
    ) {
        self.emit(format!("# cellscript abi: LOAD_WITNESS reason={} source={} index={}", reason, ckb_source_name(source), index));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("li a3, {}", index));
        self.emit(format!("li a4, {}", source));
        self.emit(format!("li a7, {}", self.runtime_abi().load_witness));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_load_script_syscall_to_offsets(&mut self, reason: &str, size_offset: usize, buffer_offset: usize, max_bytes: usize) {
        self.emit(format!("# cellscript abi: LOAD_SCRIPT reason={}", reason));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("li a7, {}", self.runtime_abi().load_script));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_load_cell_by_field_syscall_to_offsets(
        &mut self,
        reason: &str,
        source: u64,
        index: usize,
        field: u64,
        size_offset: usize,
        buffer_offset: usize,
        max_bytes: usize,
    ) {
        self.emit(format!(
            "# cellscript abi: LOAD_CELL_BY_FIELD reason={} source={} index={} field={}",
            reason,
            ckb_source_name(source),
            index,
            field
        ));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("li a3, {}", index));
        self.emit(format!("li a4, {}", source));
        self.emit(format!("li a5, {}", field));
        self.emit(format!("li a7, {}", self.runtime_abi().load_cell_by_field));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_load_cell_by_field_syscall_to_offsets_dynamic_index(
        &mut self,
        reason: &str,
        source: u64,
        index_reg: &str,
        field: u64,
        size_offset: usize,
        buffer_offset: usize,
        max_bytes: usize,
    ) {
        self.emit(format!(
            "# cellscript abi: LOAD_CELL_BY_FIELD reason={} source={} index={} field={}",
            reason,
            ckb_source_name(source),
            index_reg,
            field
        ));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("addi a3, {}, 0", index_reg));
        self.emit(format!("li a4, {}", source));
        self.emit(format!("li a5, {}", field));
        self.emit(format!("li a7, {}", self.runtime_abi().load_cell_by_field));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_return_on_syscall_error(&mut self, error: CellScriptRuntimeError) {
        let ok_label = self.fresh_label("ckb_syscall_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_fail(error);
        self.emit_label(&ok_label);
    }

    fn emit_loaded_schema_bounds_check(&mut self, size_offset: usize, required_size: usize, context: &str) {
        self.emit(format!("# cellscript abi: bounds check {} required={}", context, required_size));
        let ok_label = self.fresh_label("schema_bounds_ok");
        self.emit_stack_load("a0", size_offset);
        self.emit(format!("li a1, {}", required_size));
        self.emit("call __cellscript_require_min_size");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&ok_label);
    }

    fn emit_loaded_schema_exact_size_check(&mut self, size_offset: usize, expected_size: usize, context: &str) {
        self.emit(format!("# cellscript abi: exact size check {} expected={}", context, expected_size));
        let ok_label = self.fresh_label("schema_size_ok");
        self.emit_stack_load("a0", size_offset);
        self.emit(format!("li a1, {}", expected_size));
        self.emit("call __cellscript_require_exact_size");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::ExactSizeMismatch);
        self.emit_label(&ok_label);
    }

    fn emit_molecule_table_field_bounds_to_t5(
        &mut self,
        base_reg: &str,
        size_offset: usize,
        field_index: usize,
        field_width: usize,
        context: &str,
    ) {
        self.emit(format!("# cellscript abi: molecule table field {} index={} min_width={}", context, field_index, field_width));
        let field_count = field_index + 1;
        let header_size = 4 + 4 * field_count;
        self.emit_loaded_schema_bounds_check(size_offset, header_size, context);

        self.emit_stack_load("a0", size_offset);
        let total_ok = self.fresh_label("molecule_table_total_ok");
        self.emit_unaligned_scalar_load(base_reg, "t0", "t2", 0, 4);
        self.emit("sub t2, t0, a0");
        self.emit(format!("beqz t2, {}", total_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&total_ok);

        self.emit_unaligned_scalar_load(base_reg, "t5", "t2", 4 + 4 * field_index, 4);
        self.emit(format!("li t1, {}", header_size));
        self.emit("sltu t2, t5, t1");
        let start_ok = self.fresh_label("molecule_table_start_ok");
        self.emit(format!("beqz t2, {}", start_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&start_ok);

        if field_width > 0 {
            self.emit(format!("li t1, {}", field_width));
            self.emit("add t3, t5, t1");
            self.emit("sltu t2, t3, t5");
            let overflow_ok = self.fresh_label("molecule_table_field_overflow_ok");
            self.emit(format!("beqz t2, {}", overflow_ok));
            self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
            self.emit_label(&overflow_ok);
            self.emit("sltu t2, a0, t3");
            let end_ok = self.fresh_label("molecule_table_end_ok");
            self.emit(format!("beqz t2, {}", end_ok));
            self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
            self.emit_label(&end_ok);
        }
    }

    fn emit_molecule_table_field_span_to_t5_t6(
        &mut self,
        base_reg: &str,
        size_offset: usize,
        field_index: usize,
        field_count: usize,
        context: &str,
    ) {
        self.emit(format!(
            "# cellscript abi: molecule table dynamic field {} index={} field_count={}",
            context, field_index, field_count
        ));
        let header_size = 4 + 4 * field_count;
        self.emit_loaded_schema_bounds_check(size_offset, header_size, context);

        self.emit_stack_load("a0", size_offset);
        let total_ok = self.fresh_label("molecule_table_total_ok");
        self.emit_unaligned_scalar_load(base_reg, "t0", "t2", 0, 4);
        self.emit("sub t2, t0, a0");
        self.emit(format!("beqz t2, {}", total_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&total_ok);

        self.emit_unaligned_scalar_load(base_reg, "t5", "t2", 4 + 4 * field_index, 4);
        if field_index + 1 < field_count {
            self.emit_unaligned_scalar_load(base_reg, "t6", "t2", 4 + 4 * (field_index + 1), 4);
        } else {
            self.emit("add t6, a0, zero");
        }

        self.emit(format!("li t1, {}", header_size));
        self.emit("sltu t2, t5, t1");
        let start_ok = self.fresh_label("molecule_table_start_ok");
        self.emit(format!("beqz t2, {}", start_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&start_ok);

        self.emit("sltu t2, t6, t5");
        let order_ok = self.fresh_label("molecule_table_order_ok");
        self.emit(format!("beqz t2, {}", order_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&order_ok);

        self.emit("sltu t2, a0, t6");
        let end_ok = self.fresh_label("molecule_table_end_ok");
        self.emit(format!("beqz t2, {}", end_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&end_ok);
    }

    fn emit_mutate_replacement_field_hash_check(
        &mut self,
        pattern: &MutatePattern,
        cell_field: u64,
        field_name: &str,
        error: CellScriptRuntimeError,
    ) {
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();

        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("mutate_input_{}", field_name),
            CKB_SOURCE_INPUT,
            pattern.input_index,
            cell_field,
            input_size_offset,
            input_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("mutate_output_{}", field_name),
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            cell_field,
            output_size_offset,
            output_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(input_size_offset, 32, &format!("mutate input {}", field_name));
        self.emit_loaded_schema_exact_size_check(output_size_offset, 32, &format!("mutate output {}", field_name));
        self.emit(format!(
            "# cellscript abi: verify mutate output {} {} Input#{} == Output#{} size=32",
            pattern.ty, field_name, pattern.input_index, pattern.output_index
        ));
        self.emit_sp_addi("t4", input_buffer_offset);
        self.emit_sp_addi("t5", output_buffer_offset);
        for byte_index in 0..32 {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("mutate_identity_byte_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_runtime_error_comment(error);
            self.emit(format!("li a0, {}", error.code()));
            self.emit_epilogue();
            self.emit_label(&ok_label);
        }
    }

    fn emit_cell_metadata_equality(&mut self, left: &IrOperand, right: &IrOperand, field: CellMetadataField) -> Result<()> {
        let Some((left_source, left_index)) = self.operand_cell_location(left) else {
            self.emit("# cellscript abi: fail closed because left cell metadata source cannot be determined");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(());
        };
        let Some((right_source, right_index)) = self.operand_cell_location(right) else {
            self.emit("# cellscript abi: fail closed because right cell metadata source cannot be determined");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(());
        };
        let (cell_field, field_name, width, mismatch_error) = match field {
            CellMetadataField::LockHash => {
                (CKB_CELL_FIELD_LOCK_HASH, "lock_hash", 32usize, CellScriptRuntimeError::LockHashPreservationMismatch)
            }
            CellMetadataField::Capacity => {
                (CKB_CELL_FIELD_CAPACITY, "capacity", 8usize, CellScriptRuntimeError::CapacityPreservationMismatch)
            }
        };

        let left_size_offset = self.runtime_scratch_size_offset();
        let left_buffer_offset = self.runtime_scratch_buffer_offset();
        let right_size_offset = self.runtime_scratch2_size_offset();
        let right_buffer_offset = self.runtime_scratch2_buffer_offset();

        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("cell_metadata_left_{}", field_name),
            left_source,
            left_index,
            cell_field,
            left_size_offset,
            left_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("cell_metadata_right_{}", field_name),
            right_source,
            right_index,
            cell_field,
            right_size_offset,
            right_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(left_size_offset, width, &format!("cell metadata left {}", field_name));
        self.emit_loaded_schema_exact_size_check(right_size_offset, width, &format!("cell metadata right {}", field_name));
        self.emit(format!(
            "# cellscript abi: verify cell metadata {} equality {}#{} == {}#{} size={}",
            field_name,
            ckb_source_name(left_source),
            left_index,
            ckb_source_name(right_source),
            right_index,
            width
        ));
        self.emit_sp_addi("t4", left_buffer_offset);
        self.emit_sp_addi("t5", right_buffer_offset);
        for byte_index in 0..width {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("cell_metadata_byte_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_runtime_error_comment(mismatch_error);
            self.emit(format!("li a0, {}", mismatch_error.code()));
            self.emit_epilogue();
            self.emit_label(&ok_label);
        }
        Ok(())
    }

    fn emit_cell_field_hash_equality(
        &mut self,
        left_reason: &str,
        left_source: u64,
        left_index: usize,
        right_reason: &str,
        right_source: u64,
        right_index: usize,
        cell_field: u64,
        field_name: &str,
        detail: &str,
        error: CellScriptRuntimeError,
    ) {
        let left_size_offset = self.runtime_scratch_size_offset();
        let left_buffer_offset = self.runtime_scratch_buffer_offset();
        let right_size_offset = self.runtime_scratch2_size_offset();
        let right_buffer_offset = self.runtime_scratch2_buffer_offset();

        self.emit_load_cell_by_field_syscall_to_offsets(
            left_reason,
            left_source,
            left_index,
            cell_field,
            left_size_offset,
            left_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(error);
        self.emit_load_cell_by_field_syscall_to_offsets(
            right_reason,
            right_source,
            right_index,
            cell_field,
            right_size_offset,
            right_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(error);
        self.emit_loaded_schema_exact_size_check(left_size_offset, 32, &format!("{} {}", left_reason, field_name));
        self.emit_loaded_schema_exact_size_check(right_size_offset, 32, &format!("{} {}", right_reason, field_name));
        self.emit(format!(
            "# cellscript abi: verify {} {} {}#{} == {}#{} size=32",
            detail,
            field_name,
            ckb_source_name(left_source),
            left_index,
            ckb_source_name(right_source),
            right_index
        ));
        self.emit_sp_addi("a0", left_buffer_offset);
        self.emit_sp_addi("a1", right_buffer_offset);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        let ok_label = self.fresh_label("identity_hash_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_runtime_error_comment(error);
        self.emit(format!("li a0, {}", error.code()));
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn emit_output_type_hash_present_check(&mut self, output_index: usize, context: &str) {
        let size_offset = self.runtime_scratch2_size_offset();
        let buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_by_field_syscall_to_offsets(
            context,
            CKB_SOURCE_OUTPUT,
            output_index,
            CKB_CELL_FIELD_TYPE_HASH,
            size_offset,
            buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::TypeHashMismatch);
        self.emit_loaded_schema_exact_size_check(size_offset, 32, context);
        self.emit(format!("# cellscript abi: verify {} Output#{} TypeHash is present size=32", context, output_index));
    }

    fn emit_loaded_fixed_field_pointer_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        width: usize,
        context: &str,
        pointer_stack_offset: usize,
    ) {
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        self.emit_sp_addi("t5", buffer_offset + layout.offset);
        self.emit_stack_store("t5", pointer_stack_offset);
    }

    fn emit_dynamic_fixed_field_pointer_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        field_count: usize,
        width: usize,
        context: &str,
        pointer_stack_offset: usize,
        len_stack_offset: usize,
    ) {
        self.emit_dynamic_table_field_span_to_stack(
            size_offset,
            buffer_offset,
            layout.index,
            field_count,
            context,
            pointer_stack_offset,
            len_stack_offset,
        );
        self.emit_stack_load("t0", len_stack_offset);
        self.emit(format!("li t1, {}", width));
        self.emit("sub t2, t0, t1");
        let ok_label = self.fresh_label("identity_field_len_ok");
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit_runtime_error_comment(CellScriptRuntimeError::DynamicFieldValueMismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DynamicFieldValueMismatch.code()));
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn emit_fixed_pointer_equality(
        &mut self,
        left_pointer_stack_offset: usize,
        right_pointer_stack_offset: usize,
        width: usize,
        context: &str,
        error: CellScriptRuntimeError,
    ) {
        self.emit(format!("# cellscript abi: verify {} size={}", context, width));
        self.emit_stack_load("a0", left_pointer_stack_offset);
        self.emit_stack_load("a1", right_pointer_stack_offset);
        self.emit(format!("li a2, {}", width));
        self.emit("call __cellscript_memcmp_fixed");
        let ok_label = self.fresh_label("identity_field_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_runtime_error_comment(error);
        self.emit(format!("li a0, {}", error.code()));
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn operand_cell_location(&self, operand: &IrOperand) -> Option<(u64, usize)> {
        let IrOperand::Var(var) = operand else {
            return None;
        };
        if let Some(input_index) = self.consume_indices.get(&var.id).copied() {
            Some((CKB_SOURCE_INPUT, input_index))
        } else if let Some(output_index) = self.operation_output_indices.get(&var.id).copied() {
            Some((CKB_SOURCE_OUTPUT, output_index))
        } else if let Some(dep_index) = self.read_ref_indices.get(&var.id).copied() {
            Some((CKB_SOURCE_CELL_DEP, dep_index))
        } else if let Some(input_index) = self.read_ref_param_input_indices.get(&var.id).copied() {
            Some((CKB_SOURCE_INPUT, input_index))
        } else {
            self.read_ref_param_dep_indices.get(&var.id).copied().map(|dep_index| (CKB_SOURCE_CELL_DEP, dep_index))
        }
    }

    fn emit_destroy_group_output_absence_scan(&mut self, pattern: &CellPattern, input_index: usize) {
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        let loop_label = self.fresh_label("destroy_output_scan");
        let type_hash_label = self.fresh_label("destroy_output_type_hash");
        let next_label = self.fresh_label("destroy_output_next");
        let done_label = self.fresh_label("destroy_output_done");

        self.emit(format!("# cellscript abi: destroy output type-hash absence scan binding={} size=32", pattern.binding));
        self.emit_load_cell_by_field_syscall_to_offsets(
            "destroy_input_type_hash",
            CKB_SOURCE_INPUT,
            input_index,
            CKB_CELL_FIELD_TYPE_HASH,
            input_size_offset,
            input_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(input_size_offset, 32, "destroy input type hash");
        self.emit("li t6, 0");
        self.emit_label(&loop_label);
        self.emit_load_cell_by_field_syscall_to_offsets_dynamic_index(
            "destroy_output_type_hash",
            CKB_SOURCE_OUTPUT,
            "t6",
            CKB_CELL_FIELD_TYPE_HASH,
            output_size_offset,
            output_buffer_offset,
            32,
        );
        self.emit(format!("beqz a0, {}", type_hash_label));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", done_label));
        self.emit(format!("li t0, {}", CKB_ITEM_MISSING));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", next_label));
        self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);

        self.emit_label(&type_hash_label);
        self.emit_loaded_schema_exact_size_check(output_size_offset, 32, "destroy output type hash");
        self.emit(format!("# cellscript abi: reject destroy successor when Output#t6 TypeHash matches consumed {}", pattern.binding));
        self.emit_sp_addi("t4", output_buffer_offset);
        self.emit_sp_addi("t5", input_buffer_offset);
        for byte_index in 0..32 {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", next_label));
        }
        self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);

        self.emit_label(&next_label);
        self.emit("addi t6, t6, 1");
        self.emit(format!("j {}", loop_label));
        self.emit_label(&done_label);
        self.emit("li a0, 0");
    }

    fn mutate_preserved_field_layouts(&self, pattern: &MutatePattern) -> Vec<(String, SchemaFieldLayout, usize)> {
        let Some(type_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return Vec::new();
        };
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return Vec::new();
        }
        pattern
            .preserved_fields
            .iter()
            .filter_map(|field| {
                let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(field)).cloned()?;
                let width = layout_fixed_byte_width(&layout)?;
                (layout.offset + width <= RUNTIME_SCRATCH_BUFFER_SIZE).then(|| (field.clone(), layout, width))
            })
            .collect()
    }

    fn mutate_transition_exclusion_ranges(&self, pattern: &MutatePattern) -> Option<Vec<(usize, usize)>> {
        if pattern.transitions.len() != pattern.fields.len() {
            return None;
        }
        let type_size = self.type_fixed_sizes.get(&pattern.ty).copied()?;
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return None;
        }
        let mut ranges = Vec::new();
        for transition in &pattern.transitions {
            let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field))?;
            let width = layout_fixed_byte_width(layout)?;
            if layout.offset + width > RUNTIME_SCRATCH_BUFFER_SIZE {
                return None;
            }
            ranges.push((layout.offset, layout.offset + width));
        }
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in ranges {
            if start >= end {
                continue;
            }
            if let Some(last) = merged.last_mut() {
                if start <= last.1 {
                    last.1 = last.1.max(end);
                    continue;
                }
            }
            merged.push((start, end));
        }
        Some(merged)
    }

    fn emit_mutate_replacement_preserved_field_checks(&mut self, pattern: &MutatePattern) {
        let preserved_fields = self.mutate_preserved_field_layouts(pattern);
        if !pattern.preserved_fields.is_empty() && preserved_fields.len() != pattern.preserved_fields.len() {
            if self.emit_mutate_replacement_dynamic_table_preserved_field_checks(pattern) {
                return;
            }
            if self.emit_mutate_replacement_data_except_transition_checks(pattern) {
                return;
            }
            self.emit("# cellscript abi: fail closed because not all preserved fields are verifier-addressable");
            self.emit_fail(CellScriptRuntimeError::FieldPreservationMismatch);
            return;
        }
        if preserved_fields.is_empty() {
            return;
        }
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_data",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_data",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(input_size_offset, expected_size, &format!("{} mutate input", pattern.ty));
            self.emit_loaded_schema_exact_size_check(output_size_offset, expected_size, &format!("{} mutate output", pattern.ty));
        }
        self.emit(format!(
            "# cellscript abi: verify mutate preserved fields {} Input#{} == Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        self.emit_sp_addi("t4", input_buffer_offset);
        self.emit_sp_addi("t5", output_buffer_offset);
        for (field, layout, width) in preserved_fields {
            self.emit_loaded_schema_bounds_check(input_size_offset, layout.offset + width, &format!("{} input.{}", pattern.ty, field));
            self.emit_loaded_schema_bounds_check(
                output_size_offset,
                layout.offset + width,
                &format!("{} output.{}", pattern.ty, field),
            );
            self.emit(format!(
                "# cellscript abi: verify mutate preserved field {}.{} Input#{} == Output#{} offset={} size={}",
                pattern.ty, field, pattern.input_index, pattern.output_index, layout.offset, width
            ));
            let mismatch_label = self.fresh_label("mutate_preserved_byte_mismatch");
            for byte_index in 0..width {
                self.emit(format!("lbu t0, {}(t4)", layout.offset + byte_index));
                self.emit(format!("lbu t1, {}(t5)", layout.offset + byte_index));
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch_label));
            }
            self.emit_fixed_byte_mismatch_fail(&mismatch_label, CellScriptRuntimeError::FieldPreservationMismatch);
        }
    }

    fn emit_mutate_replacement_dynamic_table_preserved_field_checks(&mut self, pattern: &MutatePattern) -> bool {
        if self.type_fixed_sizes.contains_key(&pattern.ty) || pattern.preserved_fields.is_empty() {
            return false;
        }
        let Some(layouts) = self.type_layouts.get(&pattern.ty).cloned() else {
            return false;
        };
        let field_count = layouts.len();
        if field_count == 0 || !pattern.preserved_fields.iter().all(|field| layouts.contains_key(field)) {
            return false;
        }

        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_table_preserved",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_table_preserved",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit(format!(
            "# cellscript abi: verify mutate preserved Molecule table fields {} Input#{} == Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        for field in &pattern.preserved_fields {
            let Some(layout) = layouts.get(field).cloned() else {
                return false;
            };
            self.emit_dynamic_table_field_equality_check(
                &pattern.ty,
                field,
                &layout,
                field_count,
                input_size_offset,
                input_buffer_offset,
                output_size_offset,
                output_buffer_offset,
                CellScriptRuntimeError::FieldPreservationMismatch,
            );
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_dynamic_table_field_equality_check(
        &mut self,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
        field_count: usize,
        input_size_offset: usize,
        input_buffer_offset: usize,
        output_size_offset: usize,
        output_buffer_offset: usize,
        fail_code: CellScriptRuntimeError,
    ) {
        let start_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        let output_start_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
        if let Some(width) = layout_fixed_byte_width(layout) {
            self.emit_dynamic_table_fixed_field_pointer_to_stack(
                input_size_offset,
                input_buffer_offset,
                layout,
                width,
                &format!("{} input.{}", type_name, field),
                start_offset,
            );
            self.emit_dynamic_table_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                layout,
                width,
                &format!("{} output.{}", type_name, field),
                output_start_offset,
            );
            self.emit(format!("li t0, {}", width));
            self.emit_stack_store("t0", len_offset);
        } else {
            self.emit_dynamic_table_field_span_to_stack(
                input_size_offset,
                input_buffer_offset,
                layout.index,
                field_count,
                &format!("{} input.{}", type_name, field),
                start_offset,
                len_offset,
            );
            self.emit_dynamic_table_field_span_to_stack(
                output_size_offset,
                output_buffer_offset,
                layout.index,
                field_count,
                &format!("{} output.{}", type_name, field),
                output_start_offset,
                self.runtime_expr_temp_offset(3).expect("runtime temp slot 3"),
            );
            self.emit_stack_load("t0", len_offset);
            self.emit_stack_load("t1", self.runtime_expr_temp_offset(3).expect("runtime temp slot 3"));
            self.emit("sub t2, t0, t1");
            let len_ok = self.fresh_label("mutate_table_field_len_ok");
            self.emit(format!("beqz t2, {}", len_ok));
            self.emit_fail(fail_code);
            self.emit_label(&len_ok);
        }

        self.emit(format!(
            "# cellscript abi: verify mutate preserved Molecule table field {}.{} Input#{} == Output#{}",
            type_name, field, 0, 1
        ));
        let mismatch_label = self.fresh_label("mutate_table_field_mismatch");
        self.emit_stack_load("a0", start_offset);
        self.emit_stack_load("a1", output_start_offset);
        self.emit_stack_load("a2", len_offset);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch_label));
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, fail_code);
    }

    fn emit_dynamic_table_field_span_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        field_index: usize,
        field_count: usize,
        context: &str,
        start_stack_offset: usize,
        len_stack_offset: usize,
    ) {
        self.emit_sp_addi("t4", buffer_offset);
        self.emit_molecule_table_field_span_to_t5_t6("t4", size_offset, field_index, field_count, context);
        self.emit_sp_addi("t4", buffer_offset);
        self.emit("add t5, t4, t5");
        self.emit("add t6, t4, t6");
        self.emit("sub t0, t6, t5");
        self.emit_stack_store("t5", start_stack_offset);
        self.emit_stack_store("t0", len_stack_offset);
    }

    fn emit_dynamic_table_fixed_field_pointer_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        width: usize,
        context: &str,
        start_stack_offset: usize,
    ) {
        self.emit_sp_addi("t4", buffer_offset);
        self.emit_molecule_table_field_bounds_to_t5("t4", size_offset, layout.index, width, context);
        self.emit_sp_addi("t4", buffer_offset);
        self.emit("add t5, t4, t5");
        self.emit_stack_store("t5", start_stack_offset);
    }

    fn emit_mutate_replacement_dynamic_table_append_checks(&mut self, pattern: &MutatePattern) -> bool {
        if self.type_fixed_sizes.contains_key(&pattern.ty) || pattern.transitions.is_empty() {
            return false;
        }
        let Some(layouts) = self.type_layouts.get(&pattern.ty).cloned() else {
            return false;
        };
        let field_count = layouts.len();
        let appends = pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                if transition.op != MutateTransitionOp::Append {
                    return None;
                }
                let layout = layouts.get(&transition.field).cloned()?;
                let element_width = molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)?;
                self.fixed_append_fields(&transition.operand, element_width)
                    .map(|fields| (transition.clone(), layout, element_width, fields))
            })
            .collect::<Vec<_>>();
        if appends.len() != pattern.transitions.len() {
            return false;
        }

        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_table_append",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_table_append",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit(format!(
            "# cellscript abi: verify mutate Molecule table append fields {} Input#{} -> Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        for (transition, layout, element_width, fields) in appends {
            self.emit_dynamic_table_vector_append_check(
                &pattern.ty,
                &transition.field,
                &layout,
                field_count,
                element_width,
                &fields,
                input_size_offset,
                input_buffer_offset,
                output_size_offset,
                output_buffer_offset,
            );
        }
        true
    }

    fn fixed_append_fields(&self, operand: &IrOperand, expected_width: usize) -> Option<Vec<(IrOperand, SchemaFieldLayout, usize)>> {
        if self.expected_fixed_byte_source(operand, expected_width).is_some() {
            let ty = match operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(IrConst::Address(_)) => IrType::Address,
                IrOperand::Const(IrConst::Hash(_)) => IrType::Hash,
                IrOperand::Const(IrConst::Array(items)) => IrType::Array(Box::new(IrType::U8), items.len()),
                IrOperand::Const(_) => return None,
            };
            return Some(vec![(
                operand.clone(),
                SchemaFieldLayout { index: 0, offset: 0, ty, fixed_size: Some(expected_width), fixed_enum_size: None },
                expected_width,
            )]);
        }
        let IrOperand::Var(var) = operand else {
            return None;
        };
        let fields = self.tuple_aggregate_fields.get(&var.id)?;
        let type_name = named_type_name(&var.ty)?;
        let mut layouts = self.type_layouts.get(type_name)?.values().cloned().collect::<Vec<_>>();
        layouts.sort_by_key(|layout| layout.offset);
        if layouts.len() != fields.len() {
            return None;
        }
        let total_width = self.type_fixed_sizes.get(type_name).copied()?;
        if total_width != expected_width {
            return None;
        }
        fields
            .iter()
            .cloned()
            .zip(layouts)
            .map(|(field_operand, layout)| {
                let width = layout_fixed_byte_width(&layout)?;
                self.expected_fixed_byte_source(&field_operand, width)?;
                Some((field_operand, layout, width))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_dynamic_table_vector_append_check(
        &mut self,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
        field_count: usize,
        element_width: usize,
        fields: &[(IrOperand, SchemaFieldLayout, usize)],
        input_size_offset: usize,
        input_buffer_offset: usize,
        output_size_offset: usize,
        output_buffer_offset: usize,
    ) {
        let input_start_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let input_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        let output_start_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
        let output_len_offset = self.runtime_expr_temp_offset(3).expect("runtime temp slot 3");
        self.emit_dynamic_table_field_span_to_stack(
            input_size_offset,
            input_buffer_offset,
            layout.index,
            field_count,
            &format!("{} input.{}", type_name, field),
            input_start_offset,
            input_len_offset,
        );
        self.emit_dynamic_table_field_span_to_stack(
            output_size_offset,
            output_buffer_offset,
            layout.index,
            field_count,
            &format!("{} output.{}", type_name, field),
            output_start_offset,
            output_len_offset,
        );
        self.emit(format!(
            "# cellscript abi: verify mutate Molecule vector append {}.{} element_size={}",
            type_name, field, element_width
        ));
        self.emit_loaded_schema_bounds_check(input_len_offset, 4, &format!("{} input.{} vector", type_name, field));
        self.emit_loaded_schema_bounds_check(output_len_offset, 4 + element_width, &format!("{} output.{} vector", type_name, field));

        self.emit_stack_load("t4", input_start_offset);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, 4);
        self.emit_stack_load("t1", input_len_offset);
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t0, t2");
        self.emit("addi t3, t3, 4");
        self.emit("sub t2, t1, t3");
        let input_size_ok = self.fresh_label("molecule_append_input_size_ok");
        self.emit(format!("beqz t2, {}", input_size_ok));
        self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
        self.emit_label(&input_size_ok);

        self.emit_stack_load("t4", output_start_offset);
        self.emit_unaligned_scalar_load("t4", "t1", "t2", 0, 4);
        self.emit("addi t0, t0, 1");
        self.emit("sub t2, t1, t0");
        let count_ok = self.fresh_label("molecule_append_count_ok");
        self.emit(format!("beqz t2, {}", count_ok));
        self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
        self.emit_label(&count_ok);

        self.emit_stack_load("t0", input_len_offset);
        self.emit(format!("li t1, {}", element_width));
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t1", output_len_offset);
        self.emit("sub t2, t1, t0");
        let len_ok = self.fresh_label("molecule_append_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
        self.emit_label(&len_ok);

        let prefix_ok = self.fresh_label("molecule_append_prefix_ok");
        self.emit_stack_load("a0", input_start_offset);
        self.emit("addi a0, a0, 4");
        self.emit_stack_load("a1", output_start_offset);
        self.emit("addi a1, a1, 4");
        self.emit_stack_load("a2", input_len_offset);
        self.emit("addi a2, a2, -4");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("beqz a0, {}", prefix_ok));
        self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
        self.emit_label(&prefix_ok);

        self.emit_stack_load("t0", output_start_offset);
        self.emit_stack_load("t1", input_len_offset);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", output_start_offset);
        for (operand, field_layout, width) in fields {
            let Some(source) = self.expected_fixed_byte_source(operand, *width) else {
                self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
                continue;
            };
            self.emit_prepare_fixed_byte_source(&source, *width, &format!("append {}.{}", type_name, field));
            self.emit_pointer_fixed_bytes_against_source(
                output_start_offset,
                field_layout.offset,
                &source,
                *width,
                CellScriptRuntimeError::MutateTransitionMismatch,
            );
        }
    }

    fn emit_pointer_fixed_bytes_against_source(
        &mut self,
        output_pointer_stack_offset: usize,
        output_field_offset: usize,
        source: &ExpectedFixedByteSource,
        width: usize,
        fail_code: CellScriptRuntimeError,
    ) {
        let mismatch_label = self.fresh_label("fixed_byte_mismatch");
        match source {
            ExpectedFixedByteSource::Const(bytes) => {
                self.emit_stack_load("t4", output_pointer_stack_offset);
                for (byte_index, byte) in bytes.iter().take(width).enumerate() {
                    self.emit(format!("lbu t0, {}(t4)", output_field_offset + byte_index));
                    self.emit(format!("li t1, {}", byte));
                    self.emit("sub t2, t0, t1");
                    self.emit(format!("bnez t2, {}", mismatch_label));
                }
            }
            ExpectedFixedByteSource::SchemaField(source) => {
                if self.emit_schema_field_source_pointer_to("a1", source, width) {
                    self.emit_stack_load("a0", output_pointer_stack_offset);
                    if output_field_offset != 0 {
                        self.emit_large_addi("a0", "a0", output_field_offset as i64);
                    }
                    self.emit(format!("li a2, {}", width));
                    self.emit("call __cellscript_memcmp_fixed");
                    self.emit(format!("bnez a0, {}", mismatch_label));
                } else {
                    self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
                }
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_stack_load("a0", output_pointer_stack_offset);
                if output_field_offset != 0 {
                    self.emit_large_addi("a0", "a0", output_field_offset as i64);
                }
                self.emit_sp_addi("a1", var_id * 8);
                self.emit(format!("li a2, {}", width));
                self.emit("call __cellscript_memcmp_fixed");
                self.emit(format!("bnez a0, {}", mismatch_label));
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_stack_load("a0", output_pointer_stack_offset);
                if output_field_offset != 0 {
                    self.emit_large_addi("a0", "a0", output_field_offset as i64);
                }
                self.emit_stack_load("a1", var_id * 8);
                self.emit(format!("li a2, {}", width));
                self.emit("call __cellscript_memcmp_fixed");
                self.emit(format!("bnez a0, {}", mismatch_label));
            }
        }
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, fail_code);
    }

    fn emit_mutate_replacement_data_except_transition_checks(&mut self, pattern: &MutatePattern) -> bool {
        let Some(exclusion_ranges) = self.mutate_transition_exclusion_ranges(pattern) else {
            return false;
        };
        if exclusion_ranges.is_empty() {
            return false;
        }
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_preserved_data",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_preserved_data",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(
                input_size_offset,
                expected_size,
                &format!("{} preserved-data input", pattern.ty),
            );
            self.emit_loaded_schema_exact_size_check(
                output_size_offset,
                expected_size,
                &format!("{} preserved-data output", pattern.ty),
            );
        }
        let size_ok_label = self.fresh_label("mutate_preserved_data_size_ok");
        self.emit_stack_load("t0", input_size_offset);
        self.emit_stack_load("t1", output_size_offset);
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", size_ok_label));
        self.emit_fail(CellScriptRuntimeError::FieldPreservationMismatch);
        self.emit_label(&size_ok_label);

        self.emit(format!(
            "# cellscript abi: verify mutate preserved data {} Input#{} == Output#{} except transition ranges {:?}",
            pattern.ty, pattern.input_index, pattern.output_index, exclusion_ranges
        ));
        let loop_label = self.fresh_label("mutate_preserved_data_loop");
        let compare_label = self.fresh_label("mutate_preserved_data_compare");
        let skip_label = self.fresh_label("mutate_preserved_data_skip");
        let done_label = self.fresh_label("mutate_preserved_data_done");
        let mismatch_label = self.fresh_label("mutate_preserved_data_mismatch");
        self.emit_sp_addi("a3", input_buffer_offset);
        self.emit_sp_addi("a4", output_buffer_offset);
        self.emit("li t6, 0");
        self.emit_label(&loop_label);
        self.emit("sltu t2, t6, t0");
        self.emit(format!("beqz t2, {}", done_label));
        for (range_index, (start, end)) in exclusion_ranges.iter().enumerate() {
            let next_range_label = self.fresh_label(&format!("mutate_preserved_data_next_range_{}", range_index));
            self.emit(format!("li t3, {}", start));
            self.emit("sltu t2, t6, t3");
            self.emit(format!("bnez t2, {}", compare_label));
            self.emit(format!("li t3, {}", end));
            self.emit("sltu t2, t6, t3");
            self.emit(format!("beqz t2, {}", next_range_label));
            self.emit(format!("j {}", skip_label));
            self.emit_label(&next_range_label);
        }
        self.emit_label(&compare_label);
        self.emit("add t3, a3, t6");
        self.emit("lbu t4, 0(t3)");
        self.emit("add t3, a4, t6");
        self.emit("lbu t5, 0(t3)");
        self.emit("sub t2, t4, t5");
        self.emit(format!("bnez t2, {}", mismatch_label));
        self.emit_label(&skip_label);
        self.emit("addi t6, t6, 1");
        self.emit(format!("j {}", loop_label));
        self.emit_label(&mismatch_label);
        self.emit_fail(CellScriptRuntimeError::FieldPreservationMismatch);
        self.emit_label(&done_label);
        true
    }

    fn mutate_u128_transition_layouts(&self, pattern: &MutatePattern) -> Vec<(MutateFieldTransition, SchemaFieldLayout)> {
        let Some(type_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return Vec::new();
        };
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return Vec::new();
        }
        pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                if transition.op == MutateTransitionOp::Set {
                    return None;
                }
                let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field)).cloned()?;
                // Only u128 fields (16 bytes) that don't fit in a single register.
                if layout.ty != IrType::U128 || layout.fixed_size != Some(16) {
                    return None;
                }
                if layout.offset + 16 > RUNTIME_SCRATCH_BUFFER_SIZE {
                    return None;
                }
                // u128 transition: the operand must be a u64 value (delta always fits in 64 bits).
                self.prelude_u64_operand_source(&transition.operand)?;
                Some((transition.clone(), layout))
            })
            .collect()
    }

    fn mutate_transition_layouts(&self, pattern: &MutatePattern) -> Vec<(MutateFieldTransition, SchemaFieldLayout, usize)> {
        let Some(type_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return Vec::new();
        };
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return Vec::new();
        }
        pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                if transition.op == MutateTransitionOp::Set {
                    return None;
                }
                let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field)).cloned()?;
                let width = fixed_register_width(&layout.ty, layout.fixed_size)?;
                if layout.offset + width > RUNTIME_SCRATCH_BUFFER_SIZE {
                    return None;
                }
                self.prelude_u64_operand_source(&transition.operand)?;
                Some((transition.clone(), layout, width))
            })
            .collect()
    }

    fn mutate_set_transition_layouts(&self, pattern: &MutatePattern) -> Vec<(MutateFieldTransition, SchemaFieldLayout, usize)> {
        let Some(type_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return Vec::new();
        };
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return Vec::new();
        }
        pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                if transition.op != MutateTransitionOp::Set {
                    return None;
                }
                let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field)).cloned()?;
                let width = layout_fixed_byte_width(&layout)?;
                if layout.offset + width > RUNTIME_SCRATCH_BUFFER_SIZE {
                    return None;
                }
                if layout_fixed_scalar_width(&layout).is_none()
                    && self.expected_fixed_byte_source(&transition.operand, width).is_none()
                {
                    return None;
                }
                Some((transition.clone(), layout, width))
            })
            .collect()
    }

    fn emit_mutate_replacement_transition_checks(&mut self, pattern: &MutatePattern) {
        if self.emit_mutate_replacement_dynamic_table_append_checks(pattern) {
            return;
        }
        if self.emit_mutate_replacement_dynamic_table_transition_checks(pattern) {
            return;
        }
        let transitions = self.mutate_transition_layouts(pattern);
        if transitions.is_empty() {
            return;
        }
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_transition",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_transition",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(
                input_size_offset,
                expected_size,
                &format!("{} mutate transition input", pattern.ty),
            );
            self.emit_loaded_schema_exact_size_check(
                output_size_offset,
                expected_size,
                &format!("{} mutate transition output", pattern.ty),
            );
        }
        self.emit(format!(
            "# cellscript abi: verify mutate transition fields {} Input#{} -> Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        for (transition, layout, width) in transitions {
            let Some(delta) = self.prelude_u64_operand_source(&transition.operand) else {
                continue;
            };
            self.emit_loaded_schema_bounds_check(
                input_size_offset,
                layout.offset + width,
                &format!("{} input.{}", pattern.ty, transition.field),
            );
            self.emit_loaded_schema_bounds_check(
                output_size_offset,
                layout.offset + width,
                &format!("{} output.{}", pattern.ty, transition.field),
            );
            self.emit(format!(
                "# cellscript abi: verify mutate transition field {}.{} {:?} Input#{} -> Output#{} offset={} size={}",
                pattern.ty, transition.field, transition.op, pattern.input_index, pattern.output_index, layout.offset, width
            ));
            self.emit_sp_addi("t4", input_buffer_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            let input_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 2).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve mutate input scalar before transition expression");
            self.emit_stack_store("t0", input_value_offset);
            self.emit_prelude_u64_operand_source_to_t1(&delta);
            self.emit_stack_load("t0", input_value_offset);
            match transition.op {
                MutateTransitionOp::Add => self.emit("add t1, t0, t1"),
                MutateTransitionOp::Sub => self.emit("sub t1, t0, t1"),
                MutateTransitionOp::Set => {
                    unreachable!("set transitions are verified by emit_mutate_replacement_set_transition_checks")
                }
                MutateTransitionOp::Append => {
                    unreachable!("append transitions are verified by emit_mutate_replacement_dynamic_table_append_checks")
                }
            }
            let expected_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 1).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve mutate expected scalar across output field load");
            self.emit_stack_store("t1", expected_value_offset);
            self.emit_sp_addi("t4", output_buffer_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            self.emit_stack_load("t1", expected_value_offset);
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("mutate_transition_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
            self.emit_label(&ok_label);
        }
    }

    fn emit_mutate_replacement_dynamic_table_transition_checks(&mut self, pattern: &MutatePattern) -> bool {
        if self.type_fixed_sizes.contains_key(&pattern.ty) || pattern.transitions.is_empty() {
            return false;
        }
        let Some(layouts) = self.type_layouts.get(&pattern.ty).cloned() else {
            return false;
        };
        let field_count = layouts.len();
        let transitions = pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                let layout = layouts.get(&transition.field).cloned()?;
                let width = layout_fixed_scalar_width(&layout)?;
                (width <= 8 && self.prelude_u64_operand_source(&transition.operand).is_some())
                    .then(|| (transition.clone(), layout, width))
            })
            .collect::<Vec<_>>();
        if transitions.len() != pattern.transitions.len() {
            return false;
        }

        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_table_transition",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_table_transition",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit(format!(
            "# cellscript abi: verify mutate Molecule table transition fields {} Input#{} -> Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        for (transition, layout, width) in transitions {
            let Some(delta) = self.prelude_u64_operand_source(&transition.operand) else {
                continue;
            };
            self.emit_sp_addi("t4", input_buffer_offset);
            self.emit_molecule_table_field_bounds_to_t5(
                "t4",
                input_size_offset,
                layout.index,
                width,
                &format!("{} input.{}", pattern.ty, transition.field),
            );
            self.emit("add t4, t4, t5");
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
            let input_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 2).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve mutate table input scalar before transition expression");
            self.emit_stack_store("t0", input_value_offset);
            self.emit_prelude_u64_operand_source_to_t1(&delta);
            self.emit_stack_load("t0", input_value_offset);
            match transition.op {
                MutateTransitionOp::Add => self.emit("add t1, t0, t1"),
                MutateTransitionOp::Sub => self.emit("sub t1, t0, t1"),
                MutateTransitionOp::Set => {}
                MutateTransitionOp::Append => {}
            }
            let expected_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 1).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve mutate table expected scalar across output field load");
            self.emit_stack_store("t1", expected_value_offset);
            self.emit_sp_addi("t4", output_buffer_offset);
            self.emit_molecule_table_field_bounds_to_t5(
                "t4",
                output_size_offset,
                layout.index,
                width,
                &format!("{} output.{}", pattern.ty, transition.field),
            );
            self.emit("add t4, t4, t5");
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
            self.emit_stack_load("t1", expected_value_offset);
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("mutate_table_transition_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
            self.emit_label(&ok_label);
        }
        let _ = field_count;
        true
    }

    fn emit_mutate_replacement_set_transition_checks(&mut self, pattern: &MutatePattern) {
        let transitions = self.mutate_set_transition_layouts(pattern);
        if transitions.is_empty() {
            return;
        }
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_set_transition",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(
                output_size_offset,
                expected_size,
                &format!("{} mutate set transition output", pattern.ty),
            );
        }
        self.emit(format!("# cellscript abi: verify mutate set transition fields {} Output#{}", pattern.ty, pattern.output_index));
        for (transition, layout, width) in transitions {
            self.emit(format!(
                "# cellscript abi: verify mutate set transition field {}.{} Output#{} offset={} size={}",
                pattern.ty, transition.field, pattern.output_index, layout.offset, width
            ));
            if !self.emit_loaded_field_bytes_equals_expected(
                output_size_offset,
                output_buffer_offset,
                &layout,
                &transition.operand,
                &format!("{} set.{}", pattern.ty, transition.field),
            ) {
                self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
            }
        }
    }

    /// u128 transition verification using 128-bit add/sub with carry.
    /// Layout: field is 16 bytes (low 8 + high 8, little-endian).
    /// Delta is always u64 (fits in a single register).
    /// Verification: output == input +/- delta, with carry propagation.
    fn emit_mutate_replacement_u128_transition_checks(&mut self, pattern: &MutatePattern) {
        let transitions = self.mutate_u128_transition_layouts(pattern);
        if transitions.is_empty() {
            return;
        }
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        // Load Input and Output cell data (already done by the caller for
        // preserved field checks, but we need it for transition checks too).
        // If the scratch buffers were already loaded by the preserved-field
        // path, the syscall results are cached in the buffer; we only need
        // to reload if this function is called independently.
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_u128_transition",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_u128_transition",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(
                input_size_offset,
                expected_size,
                &format!("{} mutate u128 transition input", pattern.ty),
            );
            self.emit_loaded_schema_exact_size_check(
                output_size_offset,
                expected_size,
                &format!("{} mutate u128 transition output", pattern.ty),
            );
        }
        for (transition, layout) in transitions {
            let Some(delta) = self.prelude_u64_operand_source(&transition.operand) else {
                continue;
            };
            self.emit_loaded_schema_bounds_check(
                input_size_offset,
                layout.offset + 16,
                &format!("{} input.{}", pattern.ty, transition.field),
            );
            self.emit_loaded_schema_bounds_check(
                output_size_offset,
                layout.offset + 16,
                &format!("{} output.{}", pattern.ty, transition.field),
            );
            self.emit(format!(
                "# cellscript abi: verify mutate u128 transition field {}.{} {:?} Input#{} -> Output#{} offset={} size=16",
                pattern.ty, transition.field, transition.op, pattern.input_index, pattern.output_index, layout.offset
            ));

            // Load input low 64 bits (little-endian bytes 0..8) into t0
            // Load input high 64 bits (little-endian bytes 8..16) into t3
            self.emit_sp_addi("t4", input_buffer_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, 8);
            self.emit_unaligned_scalar_load("t4", "t3", "t2", layout.offset + 8, 8);

            // Load delta into t1
            self.emit_prelude_u64_operand_source_to_t1(&delta);

            // Compute expected output = input +/- delta with carry
            match transition.op {
                MutateTransitionOp::Add => {
                    // expected_lo = input_lo + delta
                    // expected_hi = input_hi + carry
                    // where carry = (input_lo + delta < input_lo) ? 1 : 0
                    self.emit("add t5, t0, t1"); // expected_lo = input_lo + delta
                    self.emit("sltu t2, t5, t0"); // carry = 1 if addition overflowed
                    self.emit("add t6, t3, t2"); // expected_hi = input_hi + carry
                }
                MutateTransitionOp::Sub => {
                    // expected_lo = input_lo - delta
                    // expected_hi = input_hi - borrow
                    // where borrow = (input_lo < delta) ? 1 : 0
                    self.emit("sub t5, t0, t1"); // expected_lo = input_lo - delta
                    self.emit("sltu t2, t0, t1"); // borrow = 1 if subtraction underflowed
                    self.emit("sub t6, t3, t2"); // expected_hi = input_hi - borrow
                }
                MutateTransitionOp::Set => {
                    unreachable!("set transitions are verified by emit_mutate_replacement_set_transition_checks")
                }
                MutateTransitionOp::Append => {
                    unreachable!("append transitions are verified by emit_mutate_replacement_dynamic_table_append_checks")
                }
            }

            // Load actual output low 64 bits into t0, high 64 bits into t3
            self.emit_sp_addi("t4", output_buffer_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, 8);
            self.emit_unaligned_scalar_load("t4", "t3", "t2", layout.offset + 8, 8);

            // Compare: expected (t5, t6) == actual (t0, t3)
            let ok_label = self.fresh_label("mutate_u128_transition_ok");
            self.emit("sub t2, t0, t5"); // diff_lo = actual_lo - expected_lo
            self.emit("sub t1, t3, t6"); // diff_hi = actual_hi - expected_hi
            self.emit("or t2, t2, t1"); // combined diff = diff_lo | diff_hi
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
            self.emit_label(&ok_label);
        }
    }

    fn emit_loaded_field_equals_expected(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        expected: &IrOperand,
        context: &str,
    ) {
        let Some(width) = layout_fixed_scalar_width(layout) else {
            return;
        };
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        self.emit(format!("# cellscript abi: verify output field {} offset={} size={}", context, layout.offset, width));
        self.emit_sp_addi("t4", buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
        let actual_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 1).expect("runtime temp slot");
        self.emit("# cellscript abi: preserve output scalar before expected expression");
        self.emit_stack_store("t0", actual_value_offset);
        self.emit_expected_operand_to_t1(expected);
        self.emit_stack_load("t0", actual_value_offset);
        self.emit("sub t2, t0, t1");
        let ok_label = self.fresh_label("output_field_ok");
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&ok_label);
    }

    fn emit_loaded_fixed_bytes_against_source(
        &mut self,
        output_buffer_offset: usize,
        output_field_offset: usize,
        source: &ExpectedFixedByteSource,
        width: usize,
        fail_code: CellScriptRuntimeError,
    ) {
        let mismatch_label = self.fresh_label("fixed_byte_mismatch");
        self.emit_sp_addi("t4", output_buffer_offset);
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                if self.emit_schema_field_source_pointer_to("a1", source, width) {
                    self.emit_sp_addi("a0", output_buffer_offset + output_field_offset);
                    self.emit(format!("li a2, {}", width));
                    self.emit("call __cellscript_memcmp_fixed");
                    self.emit(format!("bnez a0, {}", mismatch_label));
                } else {
                    self.emit("# cellscript abi: fail closed because schema field byte source is not addressable");
                    self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
                }
            }
            ExpectedFixedByteSource::Const(bytes) => {
                if width >= 8 && bytes.iter().take(width).all(|byte| *byte == 0) {
                    self.emit_sp_addi("a0", output_buffer_offset + output_field_offset);
                    self.emit(format!("li a1, {}", width));
                    self.emit("call __cellscript_memzero_fixed");
                    self.emit(format!("bnez a0, {}", mismatch_label));
                } else {
                    for (byte_index, byte) in bytes.iter().take(width).enumerate() {
                        self.emit(format!("lbu t0, {}(t4)", output_field_offset + byte_index));
                        self.emit(format!("li t1, {}", byte));
                        self.emit("sub t2, t0, t1");
                        self.emit(format!("bnez t2, {}", mismatch_label));
                    }
                }
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_loaded_fixed_bytes_helper_call(
                    output_buffer_offset,
                    output_field_offset,
                    SourcePointer::StackAddress { offset: var_id * 8 },
                    width,
                    &mismatch_label,
                );
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_loaded_fixed_bytes_helper_call(
                    output_buffer_offset,
                    output_field_offset,
                    SourcePointer::LoadedStackPointer { var_id: *var_id, offset: 0 },
                    width,
                    &mismatch_label,
                );
            }
        }
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, fail_code);
    }

    fn emit_loaded_fixed_bytes_helper_call(
        &mut self,
        output_buffer_offset: usize,
        output_field_offset: usize,
        source: SourcePointer,
        width: usize,
        mismatch_label: &str,
    ) {
        self.emit_sp_addi("a0", output_buffer_offset + output_field_offset);
        match source {
            SourcePointer::LoadedStackPointer { var_id, offset } => {
                self.emit_stack_load("a1", var_id * 8);
                if offset != 0 {
                    self.emit_large_addi("a1", "a1", offset as i64);
                }
            }
            SourcePointer::StackAddress { offset } => {
                self.emit_sp_addi("a1", offset);
            }
        }
        self.emit(format!("li a2, {}", width));
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch_label));
    }

    fn emit_loaded_field_bytes_equals_expected(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        expected: &IrOperand,
        context: &str,
    ) -> bool {
        if layout_fixed_scalar_width(layout).is_some() {
            self.emit_loaded_field_equals_expected(size_offset, buffer_offset, layout, expected, context);
            return true;
        }
        let Some(width) = layout_fixed_byte_width(layout) else {
            return false;
        };
        let Some(source) = self.expected_fixed_byte_source(expected, width) else {
            return false;
        };
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                if let Some(source_size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
                    if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                        self.emit_loaded_schema_exact_size_check(source_size_offset, expected_size, &source.type_name);
                    }
                    self.emit_loaded_schema_bounds_check(
                        source_size_offset,
                        source.layout.offset + width,
                        &format!("{}.{}", source.type_name, source.field),
                    );
                }
                self.emit(format!("# cellscript abi: verify output bytes field {} offset={} size={}", context, layout.offset, width));
                self.emit(format!(
                    "# cellscript abi: expected bytes field {}.{} offset={} size={}",
                    source.type_name, source.field, source.layout.offset, width
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::SchemaField(source),
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::Const(bytes) => {
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against const",
                    context, layout.offset, width
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::Const(bytes),
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::StackSlot { var_id, width } => {
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against stack slot var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::StackSlot { var_id, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::PointerBytes { var_id, width } => {
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against pointer var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::PointerBytes { var_id, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::ParamBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(size_offset, width, &format!("param var{}", var_id));
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against fixed-byte param var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::ParamBytes { var_id, size_offset, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::LoadedBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(size_offset, width, &format!("loaded bytes var{}", var_id));
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against loaded bytes var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::LoadedBytes { var_id, size_offset, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
        }
        true
    }

    fn emit_prepare_fixed_byte_source(&mut self, source: &ExpectedFixedByteSource, width: usize, context: &str) {
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                self.emit_prepare_schema_field_source(source, width);
            }
            ExpectedFixedByteSource::ParamBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(*size_offset, *width, &format!("{} param var{}", context, var_id));
            }
            ExpectedFixedByteSource::LoadedBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(*size_offset, *width, &format!("{} loaded bytes var{}", context, var_id));
            }
            ExpectedFixedByteSource::Const(_)
            | ExpectedFixedByteSource::StackSlot { .. }
            | ExpectedFixedByteSource::PointerBytes { .. } => {}
        }
    }

    fn emit_fixed_byte_source_byte_to(&mut self, dest_reg: &str, base_reg: &str, source: &ExpectedFixedByteSource, byte_index: usize) {
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                if self.emit_schema_field_source_pointer_to(base_reg, source, byte_index + 1) {
                    self.emit(format!("lbu {}, {}({})", dest_reg, byte_index, base_reg));
                } else {
                    self.emit("# cellscript abi: fail closed because schema field byte source is not addressable");
                    self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
                }
            }
            ExpectedFixedByteSource::Const(bytes) => {
                self.emit(format!("li {}, {}", dest_reg, bytes[byte_index]));
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_sp_addi(base_reg, var_id * 8);
                self.emit(format!("lbu {}, {}({})", dest_reg, byte_index, base_reg));
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_stack_load(base_reg, var_id * 8);
                self.emit(format!("lbu {}, {}({})", dest_reg, byte_index, base_reg));
            }
        }
    }

    fn emit_fixed_byte_source_pointer_to(&mut self, dest_reg: &str, source: &ExpectedFixedByteSource) -> bool {
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                let Some(width) = layout_fixed_byte_width(&source.layout).or_else(|| self.fixed_named_type_width(&source.layout.ty))
                else {
                    return false;
                };
                self.emit_schema_field_source_pointer_to(dest_reg, source, width)
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_sp_addi(dest_reg, var_id * 8);
                true
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_stack_load(dest_reg, var_id * 8);
                true
            }
            ExpectedFixedByteSource::Const(_) => false,
        }
    }

    fn emit_fixed_byte_source_pointer_or_const_to(&mut self, dest_reg: &str, source: &ExpectedFixedByteSource) -> bool {
        if let ExpectedFixedByteSource::Const(bytes) = source {
            let label = self.const_data_label_for_bytes(bytes.clone());
            self.emit(format!("la {}, {}", dest_reg, label));
            true
        } else {
            self.emit_fixed_byte_source_pointer_to(dest_reg, source)
        }
    }

    fn emit_fixed_byte_mismatch_fail(&mut self, mismatch_label: &str, fail_code: CellScriptRuntimeError) {
        let done_label = self.fresh_label("fixed_byte_verify_done");
        self.emit(format!("j {}", done_label));
        self.emit_label(mismatch_label);
        self.emit_fail(fail_code);
        self.emit_label(&done_label);
    }

    fn emit_fixed_byte_comparison(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        let Some(width) = operand_fixed_byte_width(left) else {
            return false;
        };
        if operand_fixed_byte_width(right) != Some(width) {
            return false;
        }
        let Some(left_source) = self.expected_fixed_byte_source(left, width) else {
            return false;
        };
        let Some(right_source) = self.expected_fixed_byte_source(right, width) else {
            return false;
        };
        self.emit(format!("# cellscript abi: fixed-byte {:?} comparison size={}", op, width));
        self.emit_prepare_fixed_byte_source(&left_source, width, "left fixed-byte comparison");
        self.emit_prepare_fixed_byte_source(&right_source, width, "right fixed-byte comparison");
        if width >= 8 && self.emit_fixed_byte_comparison_helper(dest, op, &left_source, &right_source, width) {
            return true;
        }
        let mismatch_label = self.fresh_label("fixed_byte_mismatch");
        let done_label = self.fresh_label("fixed_byte_done");
        for byte_index in 0..width {
            self.emit_fixed_byte_source_byte_to("t0", "t4", &left_source, byte_index);
            self.emit_fixed_byte_source_byte_to("t1", "t5", &right_source, byte_index);
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", mismatch_label));
        }
        let equal_value = if matches!(op, BinaryOp::Eq) { 1 } else { 0 };
        let mismatch_value = if matches!(op, BinaryOp::Eq) { 0 } else { 1 };
        self.emit(format!("li t3, {}", equal_value));
        self.emit(format!("j {}", done_label));
        self.emit_label(&mismatch_label);
        self.emit(format!("li t3, {}", mismatch_value));
        self.emit_label(&done_label);
        self.emit_stack_store("t3", dest.id * 8);
        true
    }

    fn emit_fixed_byte_comparison_helper(
        &mut self,
        dest: &IrVar,
        op: BinaryOp,
        left_source: &ExpectedFixedByteSource,
        right_source: &ExpectedFixedByteSource,
        width: usize,
    ) -> bool {
        match (left_source, right_source) {
            (ExpectedFixedByteSource::Const(bytes), source) if bytes.iter().take(width).all(|byte| *byte == 0) => {
                if !self.emit_fixed_byte_source_pointer_to("a0", source) {
                    return false;
                }
                self.emit(format!("li a1, {}", width));
                self.emit("call __cellscript_memzero_fixed");
            }
            (source, ExpectedFixedByteSource::Const(bytes)) if bytes.iter().take(width).all(|byte| *byte == 0) => {
                if !self.emit_fixed_byte_source_pointer_to("a0", source) {
                    return false;
                }
                self.emit(format!("li a1, {}", width));
                self.emit("call __cellscript_memzero_fixed");
            }
            (ExpectedFixedByteSource::Const(_), _) | (_, ExpectedFixedByteSource::Const(_)) => return false,
            _ => {
                if !self.emit_fixed_byte_source_pointer_to("a0", left_source) {
                    return false;
                }
                let Some(left_pointer_offset) = self.runtime_expr_temp_offset(0) else {
                    return false;
                };
                self.emit_stack_store("a0", left_pointer_offset);
                if !self.emit_fixed_byte_source_pointer_to("a1", right_source) {
                    return false;
                }
                self.emit_stack_load("a0", left_pointer_offset);
                self.emit(format!("li a2, {}", width));
                self.emit("call __cellscript_memcmp_fixed");
            }
        }
        if matches!(op, BinaryOp::Eq) {
            self.emit("seqz t3, a0");
        } else {
            self.emit("snez t3, a0");
        }
        self.emit_stack_store("t3", dest.id * 8);
        true
    }

    fn expected_fixed_byte_source(&self, operand: &IrOperand, expected_width: usize) -> Option<ExpectedFixedByteSource> {
        match operand {
            IrOperand::Const(value) => {
                let bytes = fixed_byte_const_bytes(value).or_else(|| {
                    fixed_scalar_const_value(value)
                        .and_then(|value| (expected_width <= 8).then(|| value.to_le_bytes()[..expected_width].to_vec()))
                })?;
                (bytes.len() == expected_width).then_some(ExpectedFixedByteSource::Const(bytes))
            }
            IrOperand::Var(var) if self.fixed_byte_like_width(&var.ty).or_else(|| self.fixed_named_type_width(&var.ty)).is_some() => {
                let var_width = self.fixed_byte_like_width(&var.ty).or_else(|| self.fixed_named_type_width(&var.ty))?;
                if let Some(source) = self.schema_field_value_sources.get(&var.id).cloned() {
                    let source_width =
                        layout_fixed_byte_width(&source.layout).or_else(|| self.fixed_named_type_width(&source.layout.ty))?;
                    if source_width == expected_width {
                        return Some(ExpectedFixedByteSource::SchemaField(source));
                    }
                }
                if let Some(bytes) = self.prelude_fixed_byte_constants.get(&var.id).cloned() {
                    if bytes.len() == expected_width {
                        return Some(ExpectedFixedByteSource::Const(bytes));
                    }
                }
                if self.schema_pointer_vars.contains(&var.id) && var_width == expected_width {
                    if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
                        return Some(ExpectedFixedByteSource::LoadedBytes { var_id: var.id, size_offset, width: expected_width });
                    }
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if let Some(size_offset) = self.cell_buffer_size_offsets.get(&var.id).copied() {
                    if var_width == expected_width {
                        return Some(ExpectedFixedByteSource::LoadedBytes { var_id: var.id, size_offset, width: expected_width });
                    }
                }
                if self.fixed_byte_local_offsets.contains_key(&var.id) && var_width == expected_width {
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if expected_width <= 8
                    && (fixed_scalar_width(&var.ty, type_static_length(&var.ty)).is_some()
                        || (var_width == expected_width && fixed_byte_width(&var.ty, type_static_length(&var.ty)).is_some()))
                    && expected_width <= var_width
                {
                    return Some(ExpectedFixedByteSource::StackSlot { var_id: var.id, width: expected_width });
                }
                if self.u128_value_offsets.contains_key(&var.id)
                    && !self.fixed_byte_param_size_offsets.contains_key(&var.id)
                    && var_width == expected_width
                {
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if self.aggregate_pointer_sources.contains_key(&var.id) && var_width == expected_width {
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if self.param_vars.contains(&var.id) && var_width == expected_width {
                    if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&var.id).copied() {
                        return Some(ExpectedFixedByteSource::ParamBytes { var_id: var.id, size_offset, width: expected_width });
                    }
                }
                if let Some(param_id) = self.param_type_hash_sources.get(&var.id).copied() {
                    if var_width == expected_width {
                        if let Some(size_offset) = self.param_type_hash_size_offsets.get(&param_id).copied() {
                            return Some(ExpectedFixedByteSource::LoadedBytes { var_id: var.id, size_offset, width: expected_width });
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Generic fixed-byte comparison: when `emit_fixed_byte_comparison` can't determine
    /// the source of bytes, this method loads pointers from stack slots and performs
    /// a byte-by-byte comparison. Works for Var operands whose stack slots contain
    /// pointers to the fixed-byte data.
    fn emit_generic_fixed_byte_comparison(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        let left_width = operand_fixed_byte_width(left);
        let right_width = operand_fixed_byte_width(right);

        // Need at least one Var operand with known width for this to work
        let width = match (left_width, right_width) {
            (Some(w), Some(r)) if w == r => w,
            (Some(w), None) | (None, Some(w)) => w,
            _ => return false,
        };

        if width == 0 {
            return false;
        }

        // We need at least one Var operand
        let left_var = match left {
            IrOperand::Var(v) => Some(v),
            _ => None,
        };
        let right_var = match right {
            IrOperand::Var(v) => Some(v),
            _ => None,
        };
        if left_var.is_none() && right_var.is_none() {
            return false;
        }

        self.emit(format!("# cellscript abi: generic fixed-byte {:?} comparison size={}", op, width));

        // Load left pointer to t4
        if let Some(v) = left_var {
            self.emit_stack_load("t4", v.id * 8);
        } else {
            // Left is a constant – store it to scratch buffer and point t4 there
            let size_offset = self.runtime_scratch_size_offset();
            let buffer_offset = self.runtime_scratch_buffer_offset();
            self.emit_store_fixed_byte_const_to_scratch(left, size_offset, buffer_offset, width);
            self.emit_sp_addi("t4", buffer_offset);
        }

        // Load right pointer to t5
        if let Some(v) = right_var {
            self.emit_stack_load("t5", v.id * 8);
        } else {
            let size_offset = self.runtime_scratch2_size_offset();
            let buffer_offset = self.runtime_scratch2_buffer_offset();
            self.emit_store_fixed_byte_const_to_scratch(right, size_offset, buffer_offset, width);
            self.emit_sp_addi("t5", buffer_offset);
        }

        let mismatch_label = self.fresh_label("gen_fb_mismatch");
        let done_label = self.fresh_label("gen_fb_done");
        for byte_index in 0..width {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", mismatch_label));
        }
        let equal_value = if matches!(op, BinaryOp::Eq) { 1 } else { 0 };
        let mismatch_value = if matches!(op, BinaryOp::Eq) { 0 } else { 1 };
        self.emit(format!("li t3, {}", equal_value));
        self.emit(format!("j {}", done_label));
        self.emit_label(&mismatch_label);
        self.emit(format!("li t3, {}", mismatch_value));
        self.emit_label(&done_label);
        self.emit_stack_store("t3", dest.id * 8);
        true
    }

    /// Store fixed-byte constant value to scratch buffer area.
    fn emit_store_fixed_byte_const_to_scratch(&mut self, operand: &IrOperand, size_offset: usize, buffer_offset: usize, width: usize) {
        match operand {
            IrOperand::Const(IrConst::Address(bytes)) | IrOperand::Const(IrConst::Hash(bytes)) => {
                self.emit(format!("# cellscript abi: store fixed-byte const size={}", width));
                self.emit(format!("li t0, {}", width));
                self.emit_stack_store("t0", size_offset);
                for (i, byte) in bytes.iter().enumerate() {
                    self.emit(format!("li t0, {}", byte));
                    if buffer_offset + i <= 2047 {
                        self.emit_stack_store_byte("t0", buffer_offset + i);
                    } else {
                        self.emit(format!("li t6, {}", buffer_offset + i));
                        self.emit("add t6, sp, t6");
                        self.emit("sb t0, 0(t6)");
                    }
                }
            }
            IrOperand::Const(IrConst::U128(value)) => {
                self.emit(format!("# cellscript abi: store u128 const size={}", width));
                self.emit(format!("li t0, {}", width));
                self.emit_stack_store("t0", size_offset);
                for (i, byte) in value.to_le_bytes().iter().enumerate() {
                    self.emit(format!("li t0, {}", byte));
                    self.emit_stack_store_byte("t0", buffer_offset + i);
                }
            }
            IrOperand::Const(IrConst::Array(values)) => {
                self.emit(format!("# cellscript abi: store fixed-byte array const size={}", width));
                self.emit(format!("li t0, {}", width));
                self.emit_stack_store("t0", size_offset);
                for (i, value) in values.iter().enumerate() {
                    if let IrConst::U8(byte) = value {
                        self.emit(format!("li t0, {}", byte));
                        if buffer_offset + i <= 2047 {
                            self.emit_stack_store_byte("t0", buffer_offset + i);
                        } else {
                            self.emit(format!("li t6, {}", buffer_offset + i));
                            self.emit("add t6, sp, t6");
                            self.emit("sb t0, 0(t6)");
                        }
                    }
                }
            }
            _ => {
                self.emit("# cellscript abi: cannot store unknown const type to scratch".to_string());
            }
        }
    }

    fn emit_fixed_byte_source_scalar_to(
        &mut self,
        dest_reg: &str,
        scratch_reg: &str,
        base_reg: &str,
        source: &ExpectedFixedByteSource,
        start: usize,
        width: usize,
    ) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit_fixed_byte_source_byte_to(scratch_reg, base_reg, source, start + byte_index);
            if byte_index != 0 {
                self.emit(format!("slli {}, {}, {}", scratch_reg, scratch_reg, byte_index * 8));
            }
            self.emit(format!("or {}, {}, {}", dest_reg, dest_reg, scratch_reg));
        }
    }

    fn operand_is_u128(&self, operand: &IrOperand) -> bool {
        match operand {
            IrOperand::Const(IrConst::U128(_)) => true,
            IrOperand::Var(var) => var.ty == IrType::U128,
            _ => false,
        }
    }

    fn emit_u128_add_sub_with_u64(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        if dest.ty != IrType::U128 || !matches!(op, BinaryOp::Add | BinaryOp::Sub) {
            return false;
        }
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            return false;
        };

        let (wide_operand, delta_operand) = match op {
            BinaryOp::Add if self.operand_is_u128(left) => (left, right),
            BinaryOp::Add if self.operand_is_u128(right) => (right, left),
            BinaryOp::Sub if self.operand_is_u128(left) => (left, right),
            _ => return false,
        };
        let Some(source) = self.expected_fixed_byte_source(wide_operand, 16) else {
            return false;
        };
        let Some(delta) = self.prelude_u64_operand_source(delta_operand) else {
            return false;
        };

        match op {
            BinaryOp::Add => self.emit("# cellscript abi: u128 add with carry"),
            BinaryOp::Sub => self.emit("# cellscript abi: u128 sub with borrow"),
            _ => unreachable!("guarded u128 binary op"),
        }
        self.emit_fixed_byte_source_scalar_to("t0", "t2", "t4", &source, 0, 8);
        self.emit_fixed_byte_source_scalar_to("t3", "t2", "t4", &source, 8, 8);
        self.emit_prelude_u64_operand_source_to_t1(&delta);
        match op {
            BinaryOp::Add => {
                self.emit("add t5, t0, t1");
                self.emit("sltu t2, t5, t0");
                self.emit("add t6, t3, t2");
            }
            BinaryOp::Sub => {
                self.emit("sub t5, t0, t1");
                self.emit("sltu t2, t0, t1");
                self.emit("sub t6, t3, t2");
            }
            _ => unreachable!("guarded u128 binary op"),
        }
        self.emit_stack_store("t5", dest_offset);
        self.emit_stack_store("t6", dest_offset + 8);
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_expected_operand_to_t1(&mut self, operand: &IrOperand) {
        match operand {
            IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("li t1, {}", if *b { 1 } else { 0 })),
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Var(var) => {
                if let Some(source) = self.schema_field_value_sources.get(&var.id).cloned() {
                    self.emit_schema_field_source_to_t1(&source);
                } else if let Some(source) = self.prelude_u64_value_sources.get(&var.id).cloned() {
                    self.emit_prelude_u64_value_source_to_t1(&source);
                } else if matches!(var.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64) {
                    self.emit_stack_load("t1", var.id * 8);
                } else if let Some(value) = self.prelude_scalar_immediates.get(&var.id).copied() {
                    self.emit(format!("li t1, {}", value));
                } else {
                    self.emit_stack_load("t1", var.id * 8);
                }
            }
            _ => self.emit("li t1, 0"),
        }
    }

    fn emit_prelude_u64_value_source_to_t1(&mut self, source: &PreludeU64ValueSource) {
        self.emit_prelude_u64_value_source_to_t1_at_depth(source, 0);
    }

    fn emit_prelude_u64_value_source_to_t1_at_depth(&mut self, source: &PreludeU64ValueSource, _depth: usize) {
        match source {
            PreludeU64ValueSource::Const(n) => self.emit(format!("li t1, {}", n)),
            PreludeU64ValueSource::ParamVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64ValueSource::StackVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64ValueSource::Field(source) => self.emit_schema_field_source_to_t1(source),
            PreludeU64ValueSource::Binary { op, left, right } => {
                self.emit(format!("# cellscript abi: expected expression u64 {:?}", op));
                let Some(temp_offset) = self.runtime_expr_temp_offset(_depth) else {
                    self.emit("# cellscript abi: fail closed because expression verifier temp stack is exhausted");
                    self.emit_fail(CellScriptRuntimeError::DataPreservationMismatch);
                    return;
                };
                self.emit_prelude_u64_value_source_to_t1_at_depth(left, _depth + 1);
                self.emit_stack_store("t1", temp_offset);
                self.emit_prelude_u64_operand_source_to_t1_at_depth(right, _depth + 1);
                self.emit_stack_load("t3", temp_offset);
                match op {
                    BinaryOp::Add => self.emit("add t1, t3, t1"),
                    BinaryOp::Sub => self.emit("sub t1, t3, t1"),
                    BinaryOp::Mul => self.emit("mul t1, t3, t1"),
                    BinaryOp::Div => self.emit("divu t1, t3, t1"),
                    _ => unreachable!("prelude u64 binary source only supports add/sub/mul/div"),
                }
            }
            PreludeU64ValueSource::Min { left, right } => {
                self.emit("# cellscript abi: expected expression u64 min");
                let Some(temp_offset) = self.runtime_expr_temp_offset(_depth) else {
                    self.emit("# cellscript abi: fail closed because expression verifier temp stack is exhausted");
                    self.emit_fail(CellScriptRuntimeError::DataPreservationMismatch);
                    return;
                };
                self.emit_prelude_u64_value_source_to_t1_at_depth(left, _depth + 1);
                self.emit_stack_store("t1", temp_offset);
                self.emit_prelude_u64_operand_source_to_t1_at_depth(right, _depth + 1);
                self.emit_stack_load("t3", temp_offset);
                self.emit("slt t2, t3, t1");
                let right_ok_label = self.fresh_label("prelude_min_right_ok");
                self.emit(format!("beqz t2, {}", right_ok_label));
                self.emit("add t1, t3, zero");
                self.emit_label(&right_ok_label);
            }
        }
    }

    fn emit_prelude_u64_operand_source_to_t1(&mut self, source: &PreludeU64OperandSource) {
        self.emit_prelude_u64_operand_source_to_t1_at_depth(source, 0);
    }

    fn emit_prelude_u64_operand_source_to_t1_at_depth(&mut self, source: &PreludeU64OperandSource, _depth: usize) {
        match source {
            PreludeU64OperandSource::Const(n) => self.emit(format!("li t1, {}", n)),
            PreludeU64OperandSource::ParamVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64OperandSource::StackVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64OperandSource::Field(source) => self.emit_schema_field_source_to_t1(source),
            PreludeU64OperandSource::Expr(source) => self.emit_prelude_u64_value_source_to_t1_at_depth(source, _depth),
        }
    }

    fn emit_schema_field_source_to_t1(&mut self, source: &SchemaFieldValueSource) {
        let context = format!("{}.{}", source.type_name, source.field);
        let Some(width) = layout_fixed_scalar_width(&source.layout) else {
            self.emit("li t1, 0");
            return;
        };
        if !self.type_fixed_sizes.contains_key(&source.type_name) {
            if self.emit_schema_field_source_pointer_to("t4", source, width) {
                self.emit(format!("# cellscript abi: expected table field {} index={} size={}", context, source.layout.index, width));
                self.emit_unaligned_scalar_load("t4", "t1", "t2", 0, width);
            } else {
                self.emit("li t1, 0");
            }
            return;
        }
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
            }
            self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
        }
        self.emit(format!("# cellscript abi: expected field {} offset={} size={}", context, source.layout.offset, width));
        self.emit_stack_load("t4", source.obj_var_id * 8);
        self.emit_unaligned_scalar_load("t4", "t1", "t2", source.layout.offset, width);
    }

    fn emit_prepare_schema_field_source(&mut self, source: &SchemaFieldValueSource, width: usize) {
        let context = format!("{}.{}", source.type_name, source.field);
        let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() else {
            return;
        };
        if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
            self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
        } else {
            self.emit_stack_load("t4", source.obj_var_id * 8);
            self.emit_molecule_table_field_bounds_to_t5("t4", size_offset, source.layout.index, width, &context);
        }
    }

    fn emit_schema_field_source_pointer_to(&mut self, dest_reg: &str, source: &SchemaFieldValueSource, width: usize) -> bool {
        let context = format!("{}.{}", source.type_name, source.field);
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
                self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
                self.emit_stack_load(dest_reg, source.obj_var_id * 8);
                if source.layout.offset != 0 {
                    self.emit_large_addi(dest_reg, dest_reg, source.layout.offset as i64);
                }
            } else {
                self.emit_stack_load("t4", source.obj_var_id * 8);
                self.emit_molecule_table_field_bounds_to_t5("t4", size_offset, source.layout.index, width, &context);
                self.emit(format!("add {}, t4, t5", dest_reg));
            }
            true
        } else if self.aggregate_pointer_sources.contains_key(&source.obj_var_id)
            || self.type_fixed_sizes.contains_key(&source.type_name)
        {
            self.emit_stack_load(dest_reg, source.obj_var_id * 8);
            if source.layout.offset != 0 {
                self.emit_large_addi(dest_reg, dest_reg, source.layout.offset as i64);
            }
            true
        } else {
            false
        }
    }

    fn can_verify_create_output_fields(&self, pattern: &CreatePattern) -> bool {
        if pattern.fields.is_empty() {
            return false;
        }
        if !self.create_output_fields_cover_type(pattern) {
            return false;
        }
        pattern.fields.iter().all(|(field, value)| {
            self.type_layouts.get(&pattern.ty).and_then(|layouts| layouts.get(field)).is_some_and(|layout| {
                if let Some(width) = layout_fixed_byte_width(layout) {
                    self.is_prelude_available_fixed_value(value, width)
                } else {
                    self.can_verify_dynamic_create_output_field_value(value, layout)
                }
            })
        })
    }

    fn create_output_fields_cover_type(&self, pattern: &CreatePattern) -> bool {
        let Some(layouts) = self.type_layouts.get(&pattern.ty) else {
            return false;
        };
        let covered_fields = pattern.fields.iter().map(|(field, _)| field.as_str()).collect::<BTreeSet<_>>();
        layouts.keys().all(|field| covered_fields.contains(field.as_str()))
    }

    fn can_verify_dynamic_create_output_field_value(&self, value: &IrOperand, layout: &SchemaFieldLayout) -> bool {
        let IrOperand::Var(var) = value else {
            return false;
        };
        (self.schema_pointer_vars.contains(&var.id) && self.schema_pointer_size_offsets.contains_key(&var.id))
            || self.constructed_byte_vectors.contains_key(&var.id)
            || (self.empty_molecule_vector_vars.contains(&var.id)
                && molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).is_some())
    }

    fn can_verify_output_lock(&self, pattern: &CreatePattern) -> bool {
        match &pattern.lock {
            Some(lock) => self.expected_fixed_byte_source(lock, 32).is_some(),
            None => true,
        }
    }

    fn emit_create_output_checks(&mut self, pattern: &CreatePattern) {
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_create_output_checks_at(pattern, size_offset, buffer_offset);
    }

    fn emit_create_output_checks_at(&mut self, pattern: &CreatePattern, size_offset: usize, buffer_offset: usize) {
        let is_fixed_type = self.type_fixed_sizes.contains_key(&pattern.ty);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &pattern.ty);
        }
        for (field, value) in &pattern.fields {
            let Some(layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(field)).cloned() else {
                continue;
            };
            if layout_fixed_byte_width(&layout).is_some() {
                if is_fixed_type {
                    self.emit_loaded_field_bytes_equals_expected(
                        size_offset,
                        buffer_offset,
                        &layout,
                        value,
                        &format!("{}.{}", pattern.ty, field),
                    );
                } else {
                    let Some(field_count) = self.type_layouts.get(&pattern.ty).map(|fields| fields.len()) else {
                        self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                        continue;
                    };
                    if !self.emit_dynamic_create_output_fixed_field_equals_expected(
                        size_offset,
                        buffer_offset,
                        &pattern.ty,
                        field,
                        &layout,
                        field_count,
                        value,
                    ) {
                        self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    }
                }
            } else {
                let Some(field_count) = self.type_layouts.get(&pattern.ty).map(|fields| fields.len()) else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    continue;
                };
                if !self.emit_dynamic_create_output_field_equals_expected(
                    size_offset,
                    buffer_offset,
                    &pattern.ty,
                    field,
                    &layout,
                    field_count,
                    value,
                ) {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                }
            }
        }
        if pattern.operation == "settle" {
            self.emit_settle_final_state_check(pattern, size_offset, buffer_offset);
        } else {
            self.emit_state_transition_check(pattern, size_offset, buffer_offset);
        }
    }

    fn emit_dynamic_create_output_fixed_field_equals_expected(
        &mut self,
        output_size_offset: usize,
        output_buffer_offset: usize,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
        field_count: usize,
        expected: &IrOperand,
    ) -> bool {
        let Some(width) = layout_fixed_byte_width(layout) else {
            return false;
        };
        let output_start_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let output_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        self.emit_dynamic_table_field_span_to_stack(
            output_size_offset,
            output_buffer_offset,
            layout.index,
            field_count,
            &format!("{}.{}", type_name, field),
            output_start_offset,
            output_len_offset,
        );
        self.emit_stack_load("t0", output_len_offset);
        self.emit(format!("li t1, {}", width));
        self.emit("sub t2, t0, t1");
        let len_ok = self.fresh_label("create_fixed_table_field_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&len_ok);

        if layout_fixed_scalar_width(layout).is_some() {
            self.emit(format!(
                "# cellscript abi: verify output Molecule table scalar field {}.{} index={} size={}",
                type_name, field, layout.index, width
            ));
            self.emit_stack_load("t4", output_start_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
            let actual_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 1).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve output table scalar before expected expression");
            self.emit_stack_store("t0", actual_value_offset);
            self.emit_expected_operand_to_t1(expected);
            self.emit_stack_load("t0", actual_value_offset);
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("output_table_field_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            self.emit_label(&ok_label);
            return true;
        }

        let Some(source) = self.expected_fixed_byte_source(expected, width) else {
            return false;
        };
        self.emit(format!(
            "# cellscript abi: verify output Molecule table bytes field {}.{} index={} size={}",
            type_name, field, layout.index, width
        ));
        self.emit_prepare_fixed_byte_source(&source, width, &format!("{}.{}", type_name, field));
        self.emit_pointer_fixed_bytes_against_source(
            output_start_offset,
            0,
            &source,
            width,
            CellScriptRuntimeError::DynamicFieldValueMismatch,
        );
        true
    }

    fn emit_dynamic_create_output_field_equals_expected(
        &mut self,
        output_size_offset: usize,
        output_buffer_offset: usize,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
        field_count: usize,
        expected: &IrOperand,
    ) -> bool {
        let IrOperand::Var(var) = expected else {
            return false;
        };
        let output_start_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let output_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        self.emit_dynamic_table_field_span_to_stack(
            output_size_offset,
            output_buffer_offset,
            layout.index,
            field_count,
            &format!("{}.{}", type_name, field),
            output_start_offset,
            output_len_offset,
        );
        if let Some(parts) = self.constructed_byte_vectors.get(&var.id).cloned() {
            if let Some(element_width) =
                molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
            {
                if parts.is_empty() && element_width != 1 {
                    self.emit_empty_molecule_vector_field_check(type_name, field, output_start_offset, output_len_offset);
                    return true;
                }
                self.emit_constructed_molecule_vector_field_check(
                    type_name,
                    field,
                    output_start_offset,
                    output_len_offset,
                    &parts,
                    element_width,
                );
                return true;
            }
        }
        if self.empty_molecule_vector_vars.contains(&var.id)
            && molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).is_some()
        {
            self.emit_empty_molecule_vector_field_check(type_name, field, output_start_offset, output_len_offset);
            return true;
        }
        if !self.schema_pointer_vars.contains(&var.id) {
            return false;
        }
        let Some(expected_size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() else {
            return false;
        };
        self.emit_stack_load("t0", output_len_offset);
        self.emit_stack_load("t1", expected_size_offset);
        self.emit("sub t2, t0, t1");
        let len_ok = self.fresh_label("create_dynamic_field_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&len_ok);

        self.emit(format!("# cellscript abi: verify output dynamic field {}.{} as Molecule bytes", type_name, field));
        let mismatch_label = self.fresh_label("create_dynamic_field_mismatch");
        self.emit_stack_load("a0", output_start_offset);
        self.emit_stack_load("a1", var.id * 8);
        self.emit_stack_load("a2", output_len_offset);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch_label));
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, CellScriptRuntimeError::CellLoadFailed);
        true
    }

    fn emit_empty_molecule_vector_field_check(
        &mut self,
        type_name: &str,
        field: &str,
        output_start_offset: usize,
        output_len_offset: usize,
    ) {
        self.emit(format!("# cellscript abi: verify output dynamic field {}.{} as empty Molecule vector", type_name, field));
        self.emit_stack_load("t0", output_len_offset);
        self.emit("li t1, 4");
        self.emit("sub t2, t0, t1");
        let len_ok = self.fresh_label("create_empty_vector_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&len_ok);
        self.emit_stack_load("t0", output_start_offset);
        for offset in 0..4 {
            self.emit(format!("lbu t1, {}(t0)", offset));
            let byte_ok = self.fresh_label("create_empty_vector_byte_ok");
            self.emit(format!("beqz t1, {}", byte_ok));
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            self.emit_label(&byte_ok);
        }
    }

    fn emit_constructed_molecule_vector_field_check(
        &mut self,
        type_name: &str,
        field: &str,
        output_start_offset: usize,
        output_len_offset: usize,
        parts: &[IrOperand],
        element_width: usize,
    ) {
        let Some(expected_bytes) =
            parts.iter().try_fold(0usize, |acc, part| self.constructed_byte_vector_part_width(part).map(|width| acc + width))
        else {
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            return;
        };
        if element_width == 0 || expected_bytes % element_width != 0 {
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            return;
        }
        let expected_elements = expected_bytes / element_width;
        let expected_len = 4 + expected_bytes;
        if element_width == 1 {
            self.emit(format!(
                "# cellscript abi: verify output dynamic field {}.{} as constructed Molecule byte vector len={}",
                type_name, field, expected_bytes
            ));
        } else {
            self.emit(format!(
                "# cellscript abi: verify output dynamic field {}.{} as constructed Molecule vector elements={} bytes={} element_size={}",
                type_name, field, expected_elements, expected_bytes, element_width
            ));
        }
        self.emit_stack_load("t0", output_len_offset);
        self.emit(format!("li t1, {}", expected_len));
        self.emit("sub t2, t0, t1");
        let len_ok = self.fresh_label("create_constructed_vector_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&len_ok);

        self.emit_stack_load("t4", output_start_offset);
        for (offset, byte) in (expected_elements as u32).to_le_bytes().iter().enumerate() {
            self.emit(format!("lbu t0, {}(t4)", offset));
            self.emit(format!("li t1, {}", byte));
            self.emit("sub t2, t0, t1");
            let byte_ok = self.fresh_label("create_constructed_vector_count_ok");
            self.emit(format!("beqz t2, {}", byte_ok));
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            self.emit_label(&byte_ok);
        }

        let mut cursor = 4usize;
        for part in parts {
            let Some(width) = self.constructed_byte_vector_part_width(part) else {
                self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
                continue;
            };
            let Some(source) = self.expected_fixed_byte_source(part, width) else {
                self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
                continue;
            };
            self.emit_prepare_fixed_byte_source(&source, width, &format!("constructed {}.{}", type_name, field));
            self.emit_pointer_fixed_bytes_against_source(
                output_start_offset,
                cursor,
                &source,
                width,
                CellScriptRuntimeError::CellLoadFailed,
            );
            cursor += width;
        }
    }

    fn emit_output_lock_hash_check(&mut self, output_index: usize, expected: &IrOperand) -> bool {
        if self.expected_fixed_byte_source(expected, 32).is_none() {
            return false;
        }
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_load_cell_by_field_syscall_to_offsets(
            "output_lock_hash",
            CKB_SOURCE_OUTPUT,
            output_index,
            CKB_CELL_FIELD_LOCK_HASH,
            size_offset,
            buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(size_offset, 32, "output lock hash");
        self.emit("# cellscript abi: verify output lock hash offset=0 size=32");
        let layout = SchemaFieldLayout { index: 0, offset: 0, ty: IrType::Hash, fixed_size: Some(32), fixed_enum_size: None };
        self.emit_loaded_field_bytes_equals_expected(size_offset, buffer_offset, &layout, expected, "output lock hash")
    }

    fn emit_state_transition_check(&mut self, pattern: &CreatePattern, output_size_offset: usize, output_buffer_offset: usize) {
        let Some(states) = self.flow_states.get(&pattern.ty) else {
            return;
        };
        let state_count = states.len();
        let action_edges = self.state_transition_edges_for_pattern(pattern);
        let Some(consumed_var_id) = self.consumed_var_for_state_transition(&pattern.ty, &action_edges) else {
            if !action_edges.is_empty() {
                self.emit_fail(CellScriptRuntimeError::FlowTransitionMismatch);
            }
            return;
        };
        let Some(input_size_offset) = self.cell_buffer_size_offsets.get(&consumed_var_id).copied() else {
            return;
        };
        let Some(input_buffer_offset) = self.cell_buffer_offsets.get(&consumed_var_id).copied() else {
            return;
        };
        let state_field = self.flow_state_fields.get(&pattern.ty).cloned().unwrap_or_else(|| FLOW_STATE_FIELD_NAME.to_string());
        let Some(state_layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&state_field)).cloned() else {
            return;
        };
        let Some(width) = layout_fixed_scalar_width(&state_layout) else {
            return;
        };
        let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return;
        };

        self.emit(format!("# cellscript abi: state transition {}.{} state_count={}", pattern.ty, state_field, state_count));
        self.emit_loaded_schema_exact_size_check(input_size_offset, expected_size, &format!("{} input", pattern.ty));
        self.emit_loaded_schema_bounds_check(
            input_size_offset,
            state_layout.offset + width,
            &format!("{} input.{}", pattern.ty, state_field),
        );
        self.emit_loaded_schema_bounds_check(
            output_size_offset,
            state_layout.offset + width,
            &format!("{} output.{}", pattern.ty, state_field),
        );
        self.emit_sp_addi("t4", input_buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", state_layout.offset, width);
        let old_range_ok_label = self.fresh_label("flow_old_state_range_ok");
        self.emit(format!("li t3, {}", state_count));
        self.emit("sltu t2, t0, t3");
        self.emit(format!("bnez t2, {}", old_range_ok_label));
        self.emit_fail(CellScriptRuntimeError::FlowOldStateInvalid);
        self.emit_label(&old_range_ok_label);

        self.emit_sp_addi("t4", output_buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t1", "t2", state_layout.offset, width);
        let ok_label = self.fresh_label("flow_transition_ok");
        let rules = self.state_transition_rules_for_pattern(pattern, &action_edges);
        if rules.is_empty() {
            self.emit("addi t0, t0, 1");
            self.emit("sub t2, t1, t0");
            self.emit(format!("beqz t2, {}", ok_label));
        } else {
            for rule in rules {
                let next_rule_label = self.fresh_label("flow_transition_next_rule");
                self.emit(format!("li t3, {}", rule.from_index));
                self.emit("sub t2, t0, t3");
                self.emit(format!("bnez t2, {}", next_rule_label));
                self.emit(format!("li t3, {}", rule.to_index));
                self.emit("sub t2, t1, t3");
                self.emit(format!("beqz t2, {}", ok_label));
                self.emit_label(&next_rule_label);
            }
        }
        self.emit_fail(CellScriptRuntimeError::FlowTransitionMismatch);
        self.emit_label(&ok_label);

        let range_ok_label = self.fresh_label("flow_state_range_ok");
        self.emit(format!("li t3, {}", state_count));
        self.emit("sltu t2, t1, t3");
        self.emit(format!("bnez t2, {}", range_ok_label));
        self.emit_fail(CellScriptRuntimeError::FlowNewStateInvalid);
        self.emit_label(&range_ok_label);
    }

    fn state_transition_edges_for_pattern(&self, pattern: &CreatePattern) -> Vec<IrStateTransitionEdge> {
        self.current_state_transition_edges
            .iter()
            .filter(|state_edge| {
                state_edge.type_name == pattern.ty
                    && state_edge.output_binding.as_ref().is_none_or(|binding| binding == &pattern.binding)
            })
            .cloned()
            .collect()
    }

    fn state_transition_rules_for_pattern(&self, pattern: &CreatePattern, action_edges: &[IrStateTransitionEdge]) -> Vec<IrFlowRule> {
        if !action_edges.is_empty() {
            return action_edges
                .iter()
                .map(|state_edge| IrFlowRule {
                    from: state_edge.from.clone(),
                    to: state_edge.to.clone(),
                    from_index: state_edge.from_index,
                    to_index: state_edge.to_index,
                })
                .collect();
        }
        self.flow_rules.get(&pattern.ty).cloned().unwrap_or_default()
    }

    fn consumed_var_for_state_transition(&self, type_name: &str, action_edges: &[IrStateTransitionEdge]) -> Option<usize> {
        if let Some(binding) = action_edges.iter().filter_map(|state_edge| state_edge.input_binding.as_ref()).next() {
            let var_id = self.consume_binding_ids.get(binding).copied()?;
            if self.consume_type_names.get(&var_id).is_some_and(|consumed_type| consumed_type == type_name) {
                return Some(var_id);
            }
            return None;
        }
        self.consumed_var_for_type(type_name)
    }

    fn emit_settle_final_state_check(&mut self, pattern: &CreatePattern, output_size_offset: usize, output_buffer_offset: usize) {
        let Some(states) = self.flow_states.get(&pattern.ty) else {
            return;
        };
        if states.len() < 2 {
            return;
        }
        let final_state = states.len() - 1;
        let Some(consumed_var_id) = self.consumed_var_for_type(&pattern.ty) else {
            return;
        };
        let Some(input_size_offset) = self.cell_buffer_size_offsets.get(&consumed_var_id).copied() else {
            return;
        };
        let Some(input_buffer_offset) = self.cell_buffer_offsets.get(&consumed_var_id).copied() else {
            return;
        };
        let state_field = self.flow_state_fields.get(&pattern.ty).cloned().unwrap_or_else(|| FLOW_STATE_FIELD_NAME.to_string());
        let Some(state_layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&state_field)).cloned() else {
            return;
        };
        let Some(width) = layout_fixed_scalar_width(&state_layout) else {
            return;
        };
        let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return;
        };

        self.emit(format!(
            "# cellscript abi: settle final-state {}.{} final_state={} state_count={}",
            pattern.ty,
            state_field,
            final_state,
            states.len()
        ));
        self.emit_loaded_schema_exact_size_check(input_size_offset, expected_size, &format!("{} input", pattern.ty));
        self.emit_loaded_schema_bounds_check(
            input_size_offset,
            state_layout.offset + width,
            &format!("{} input.{}", pattern.ty, state_field),
        );
        self.emit_loaded_schema_bounds_check(
            output_size_offset,
            state_layout.offset + width,
            &format!("{} output.{}", pattern.ty, state_field),
        );

        self.emit_sp_addi("t4", input_buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", state_layout.offset, width);
        self.emit(format!("li t3, {}", final_state));
        self.emit("sub t2, t0, t3");
        let input_ok_label = self.fresh_label("settle_input_final_state_ok");
        self.emit(format!("beqz t2, {}", input_ok_label));
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        self.emit_label(&input_ok_label);

        self.emit_sp_addi("t4", output_buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t1", "t2", state_layout.offset, width);
        self.emit("sub t2, t1, t3");
        let output_ok_label = self.fresh_label("settle_output_final_state_ok");
        self.emit(format!("beqz t2, {}", output_ok_label));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&output_ok_label);
    }

    fn consumed_var_for_type(&self, type_name: &str) -> Option<usize> {
        self.consume_order
            .iter()
            .copied()
            .find(|var_id| self.consume_type_names.get(var_id).is_some_and(|consumed_type| consumed_type == type_name))
    }

    fn is_prelude_available_scalar(&self, operand: &IrOperand) -> bool {
        match operand {
            IrOperand::Const(IrConst::Bool(_) | IrConst::U8(_) | IrConst::U16(_) | IrConst::U32(_) | IrConst::U64(_)) => true,
            IrOperand::Var(var) => matches!(var.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64),
            _ => false,
        }
    }

    fn is_prelude_available_fixed_value(&self, operand: &IrOperand, expected_width: usize) -> bool {
        if self.is_prelude_available_scalar(operand) {
            return true;
        }
        self.expected_fixed_byte_source(operand, expected_width).is_some()
    }

    fn emit_unaligned_scalar_load(&mut self, base_reg: &str, dest_reg: &str, scratch_reg: &str, offset: usize, width: usize) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit_memory_load_with_avoid("lbu", scratch_reg, base_reg, offset + byte_index, &[dest_reg, scratch_reg, base_reg]);
            if byte_index != 0 {
                self.emit(format!("slli {}, {}, {}", scratch_reg, scratch_reg, byte_index * 8));
            }
            self.emit(format!("or {}, {}, {}", dest_reg, dest_reg, scratch_reg));
        }
    }

    fn emit_sign_extend_i32(&mut self, register: &str) {
        self.emit(format!("# cellscript abi: sign-extend i32 in {}", register));
        self.emit(format!("slli {}, {}, 32", register, register));
        self.emit(format!("srai {}, {}, 32", register, register));
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let label = format!(".L{}_{}", prefix, self.next_runtime_label);
        self.next_runtime_label += 1;
        label
    }

    fn emit_param_spills(&mut self, params: &[IrParam]) -> Result<()> {
        let mut abi_index = 0usize;
        for param in params {
            if named_type_name(&param.ty).is_some() {
                self.emit(format!(
                    "# cellscript abi: schema param {} pointer={} length={}",
                    param.name,
                    abi_arg_label(abi_index),
                    abi_arg_label(abi_index + 1)
                ));
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                if let Some(size_offset) = self.schema_pointer_size_offsets.get(&param.binding.id).copied() {
                    self.emit_spill_abi_arg(abi_index + 1, size_offset);
                }
                abi_index += 2;
                if let (Some(pointer_offset), Some(size_offset)) = (
                    self.param_type_hash_pointer_offsets.get(&param.binding.id).copied(),
                    self.param_type_hash_size_offsets.get(&param.binding.id).copied(),
                ) {
                    self.emit(format!(
                        "# cellscript abi: schema param {} type_hash pointer={} length={} size=32",
                        param.name,
                        abi_arg_label(abi_index),
                        abi_arg_label(abi_index + 1)
                    ));
                    self.emit_spill_abi_arg(abi_index, pointer_offset);
                    self.emit_spill_abi_arg(abi_index + 1, size_offset);
                    abi_index += 2;
                }
            } else if let Some(width) = fixed_byte_pointer_param_width(&param.ty) {
                self.emit(format!(
                    "# cellscript abi: fixed-byte param {} pointer={} length={} size={}",
                    param.name,
                    abi_arg_label(abi_index),
                    abi_arg_label(abi_index + 1),
                    width
                ));
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&param.binding.id).copied() {
                    self.emit_spill_abi_arg(abi_index + 1, size_offset);
                }
                abi_index += 2;
            } else if let Some(width) = fixed_aggregate_pointer_param_width(&param.ty) {
                self.emit(format!(
                    "# cellscript abi: fixed-aggregate param {} pointer={} length={} size={}",
                    param.name,
                    abi_arg_label(abi_index),
                    abi_arg_label(abi_index + 1),
                    width
                ));
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&param.binding.id).copied() {
                    self.emit_spill_abi_arg(abi_index + 1, size_offset);
                }
                abi_index += 2;
            } else {
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                abi_index += 1;
            }
        }

        Ok(())
    }

    fn emit_spill_abi_arg(&mut self, abi_index: usize, stack_offset: usize) {
        if abi_index < 8 {
            self.emit_stack_store(&format!("a{}", abi_index), stack_offset);
        } else {
            let caller_stack_offset = (abi_index - 8) * 8;
            self.emit(format!("# cellscript abi: arg{} loaded from caller stack +{}", abi_index, caller_stack_offset));
            self.emit(format!("ld t0, {}(fp)", caller_stack_offset));
            self.emit_stack_store("t0", stack_offset);
        }
    }

    fn record_instruction_var(&self, instruction: &IrInstruction, max_var_id: &mut Option<usize>) {
        match instruction {
            IrInstruction::LoadConst { dest, .. }
            | IrInstruction::LoadVar { dest, .. }
            | IrInstruction::Unary { dest, .. }
            | IrInstruction::FieldAccess { dest, .. }
            | IrInstruction::Index { dest, .. }
            | IrInstruction::Length { dest, .. }
            | IrInstruction::TypeHash { dest, .. }
            | IrInstruction::Create { dest, .. }
            | IrInstruction::CreateUnique { dest, .. }
            | IrInstruction::ReadRef { dest, .. } => self.record_var(dest, max_var_id),
            IrInstruction::CollectionNew { dest, capacity, .. } => {
                self.record_var(dest, max_var_id);
                if let Some(capacity) = capacity {
                    self.record_operand(capacity, max_var_id);
                }
            }
            IrInstruction::Move { dest, src } => {
                self.record_var(dest, max_var_id);
                self.record_operand(src, max_var_id);
            }
            IrInstruction::Tuple { dest, fields } => {
                self.record_var(dest, max_var_id);
                for field in fields {
                    self.record_operand(field, max_var_id);
                }
            }
            IrInstruction::Binary { dest, left, right, .. } => {
                self.record_var(dest, max_var_id);
                self.record_operand(left, max_var_id);
                self.record_operand(right, max_var_id);
            }
            IrInstruction::StoreVar { src, .. } => self.record_operand(src, max_var_id),
            IrInstruction::Call { dest, args, .. } => {
                if let Some(dest) = dest {
                    self.record_var(dest, max_var_id);
                }
                for arg in args {
                    self.record_operand(arg, max_var_id);
                }
            }
            IrInstruction::Consume { operand } | IrInstruction::Destroy { operand, policy: _ } => {
                self.record_operand(operand, max_var_id)
            }
            IrInstruction::Transfer { dest, operand, to } => {
                self.record_var(dest, max_var_id);
                self.record_operand(operand, max_var_id);
                self.record_operand(to, max_var_id);
            }
            IrInstruction::Claim { dest, receipt } => {
                self.record_var(dest, max_var_id);
                self.record_operand(receipt, max_var_id);
            }
            IrInstruction::Settle { dest, operand } => {
                self.record_var(dest, max_var_id);
                self.record_operand(operand, max_var_id)
            }
            IrInstruction::ReplaceUnique { dest, operand, .. } => {
                self.record_var(dest, max_var_id);
                self.record_operand(operand, max_var_id)
            }
            IrInstruction::CellMetadataEquality { left, right, .. } => {
                self.record_operand(left, max_var_id);
                self.record_operand(right, max_var_id);
            }
            IrInstruction::CollectionPush { collection, value } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(value, max_var_id);
            }
            IrInstruction::CollectionCapacity { dest, collection } => {
                self.record_var(dest, max_var_id);
                self.record_operand(collection, max_var_id);
            }
            IrInstruction::CollectionExtend { collection, slice } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(slice, max_var_id);
            }
            IrInstruction::CollectionClear { collection } => {
                self.record_operand(collection, max_var_id);
            }
            IrInstruction::CollectionReverse { collection } => {
                self.record_operand(collection, max_var_id);
            }
            IrInstruction::CollectionTruncate { collection, len } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(len, max_var_id);
            }
            IrInstruction::CollectionSwap { collection, left, right } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(left, max_var_id);
                self.record_operand(right, max_var_id);
            }
            IrInstruction::CollectionContains { dest, collection, value } => {
                self.record_var(dest, max_var_id);
                self.record_operand(collection, max_var_id);
                self.record_operand(value, max_var_id);
            }
            IrInstruction::CollectionRemove { dest, collection, index } => {
                self.record_var(dest, max_var_id);
                self.record_operand(collection, max_var_id);
                self.record_operand(index, max_var_id);
            }
            IrInstruction::CollectionInsert { collection, index, value } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(index, max_var_id);
                self.record_operand(value, max_var_id);
            }
            IrInstruction::CollectionSet { collection, index, value } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(index, max_var_id);
                self.record_operand(value, max_var_id);
            }
            IrInstruction::CollectionPop { dest, collection } => {
                self.record_var(dest, max_var_id);
                self.record_operand(collection, max_var_id);
            }
        }
    }

    fn record_instruction_fixed_byte_local(&self, instruction: &IrInstruction, offsets: &mut HashMap<usize, usize>) {
        let record = |offsets: &mut HashMap<usize, usize>, var: &IrVar| {
            if var.ty == IrType::U128 {
                offsets.insert(var.id, 16);
            }
            if let Some(width) = fixed_byte_width(&var.ty, type_static_length(&var.ty)).filter(|width| *width > 8) {
                offsets.insert(var.id, width);
            }
            if let Some(width) = self.fixed_named_type_width(&var.ty) {
                offsets.insert(var.id, width);
            }
        };

        match instruction {
            IrInstruction::LoadConst { dest, .. }
            | IrInstruction::LoadVar { dest, .. }
            | IrInstruction::Unary { dest, .. }
            | IrInstruction::FieldAccess { dest, .. }
            | IrInstruction::Index { dest, .. }
            | IrInstruction::Length { dest, .. }
            | IrInstruction::TypeHash { dest, .. }
            | IrInstruction::Create { dest, .. }
            | IrInstruction::CreateUnique { dest, .. }
            | IrInstruction::ReplaceUnique { dest, .. }
            | IrInstruction::Transfer { dest, .. }
            | IrInstruction::Claim { dest, .. }
            | IrInstruction::Settle { dest, .. }
            | IrInstruction::ReadRef { dest, .. }
            | IrInstruction::CollectionCapacity { dest, .. }
            | IrInstruction::CollectionContains { dest, .. }
            | IrInstruction::CollectionRemove { dest, .. }
            | IrInstruction::CollectionPop { dest, .. }
            | IrInstruction::CollectionNew { dest, .. }
            | IrInstruction::Move { dest, .. }
            | IrInstruction::Tuple { dest, .. }
            | IrInstruction::Binary { dest, .. } => record(offsets, dest),
            IrInstruction::Call { dest, func, .. } => {
                if let Some(dest) = dest {
                    if is_ckb_fixed_hash_helper(func) && dest.ty == IrType::Hash {
                        offsets.insert(dest.id, 32);
                    }
                    record(offsets, dest);
                }
            }
            IrInstruction::StoreVar { .. }
            | IrInstruction::Consume { .. }
            | IrInstruction::Destroy { .. }
            | IrInstruction::CellMetadataEquality { .. }
            | IrInstruction::CollectionPush { .. }
            | IrInstruction::CollectionExtend { .. }
            | IrInstruction::CollectionClear { .. }
            | IrInstruction::CollectionReverse { .. }
            | IrInstruction::CollectionTruncate { .. }
            | IrInstruction::CollectionSwap { .. }
            | IrInstruction::CollectionInsert { .. }
            | IrInstruction::CollectionSet { .. } => {}
        }
    }

    fn record_terminator_var(&self, terminator: &IrTerminator, max_var_id: &mut Option<usize>) {
        match terminator {
            IrTerminator::Return(Some(operand)) | IrTerminator::Branch { cond: operand, .. } => {
                self.record_operand(operand, max_var_id)
            }
            IrTerminator::Return(None) | IrTerminator::Jump(_) => {}
        }
    }

    fn collect_u128_instruction_vars(&self, instruction: &IrInstruction, out: &mut BTreeSet<usize>) {
        match instruction {
            IrInstruction::LoadConst { dest, .. }
            | IrInstruction::LoadVar { dest, .. }
            | IrInstruction::Unary { dest, .. }
            | IrInstruction::FieldAccess { dest, .. }
            | IrInstruction::Index { dest, .. }
            | IrInstruction::Length { dest, .. }
            | IrInstruction::TypeHash { dest, .. }
            | IrInstruction::Create { dest, .. }
            | IrInstruction::CreateUnique { dest, .. }
            | IrInstruction::ReplaceUnique { dest, .. }
            | IrInstruction::Claim { dest, .. }
            | IrInstruction::ReadRef { dest, .. }
            | IrInstruction::CollectionCapacity { dest, .. }
            | IrInstruction::CollectionContains { dest, .. }
            | IrInstruction::CollectionRemove { dest, .. }
            | IrInstruction::CollectionPop { dest, .. }
            | IrInstruction::Settle { dest, .. }
            | IrInstruction::Transfer { dest, .. }
            | IrInstruction::Move { dest, .. }
            | IrInstruction::Tuple { dest, .. }
            | IrInstruction::Binary { dest, .. }
            | IrInstruction::Call { dest: Some(dest), .. } => {
                if dest.ty == IrType::U128 {
                    out.insert(dest.id);
                }
            }
            IrInstruction::CollectionNew { dest, .. } => {
                if dest.ty == IrType::U128 {
                    out.insert(dest.id);
                }
            }
            IrInstruction::StoreVar { .. }
            | IrInstruction::Call { dest: None, .. }
            | IrInstruction::Consume { .. }
            | IrInstruction::Destroy { .. }
            | IrInstruction::CellMetadataEquality { .. }
            | IrInstruction::CollectionPush { .. }
            | IrInstruction::CollectionExtend { .. }
            | IrInstruction::CollectionClear { .. }
            | IrInstruction::CollectionReverse { .. }
            | IrInstruction::CollectionTruncate { .. }
            | IrInstruction::CollectionSwap { .. }
            | IrInstruction::CollectionInsert { .. }
            | IrInstruction::CollectionSet { .. } => {}
        }
    }

    fn collect_u128_terminator_vars(&self, terminator: &IrTerminator, out: &mut BTreeSet<usize>) {
        if let IrTerminator::Return(Some(IrOperand::Var(var))) = terminator {
            if var.ty == IrType::U128 {
                out.insert(var.id);
            }
        }
    }

    fn record_operand(&self, operand: &IrOperand, max_var_id: &mut Option<usize>) {
        if let IrOperand::Var(var) = operand {
            self.record_var(var, max_var_id);
        }
    }

    fn record_var(&self, var: &IrVar, max_var_id: &mut Option<usize>) {
        *max_var_id = Some(max_var_id.map(|current| current.max(var.id)).unwrap_or(var.id));
    }

    fn const_as_u128(value: &IrConst) -> Option<u128> {
        match value {
            IrConst::U8(value) => Some((*value).into()),
            IrConst::U16(value) => Some((*value).into()),
            IrConst::U32(value) => Some((*value).into()),
            IrConst::U64(value) => Some((*value).into()),
            IrConst::U128(value) => Some(*value),
            _ => None,
        }
    }

    fn expected_u128_source(&self, operand: &IrOperand) -> Option<ExpectedFixedByteSource> {
        match operand {
            IrOperand::Const(value) => {
                Self::const_as_u128(value).map(|value| ExpectedFixedByteSource::Const(value.to_le_bytes().to_vec()))
            }
            _ => self.expected_fixed_byte_source(operand, 16),
        }
    }

    fn emit_store_byte_to_stack_offset(&mut self, src_reg: &str, offset: usize) {
        self.emit_stack_store_byte(src_reg, offset);
    }

    fn emit_store_u128_const_to_stack_offset(&mut self, value: u128, offset: usize) {
        self.emit(format!("# cellscript abi: materialize u128 const at stack+{}", offset));
        for (index, byte) in value.to_le_bytes().iter().enumerate() {
            self.emit(format!("li t0, {}", byte));
            self.emit_store_byte_to_stack_offset("t0", offset + index);
        }
    }

    fn emit_store_u128_pointer_for_var(&mut self, var_id: usize, offset: usize) {
        self.emit_sp_addi("t0", offset);
        self.emit_stack_store("t0", var_id * 8);
    }

    fn emit_materialize_u128_operand_to_var(&mut self, dest: &IrVar, src: &IrOperand) -> bool {
        let Some(dest_offset) = self.u128_value_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: u128 destination has no 16-byte storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return true;
        };
        if let IrOperand::Const(value) = src {
            if let Some(value) = Self::const_as_u128(value) {
                self.emit_store_u128_const_to_stack_offset(value, dest_offset);
                self.emit_store_u128_pointer_for_var(dest.id, dest_offset);
                return true;
            }
        }
        let Some(source) = self.expected_u128_source(src) else {
            self.emit("# cellscript abi: u128 source is not addressable; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return true;
        };
        self.emit_prepare_fixed_byte_source(&source, 16, "u128 materialize");
        self.emit(format!("# cellscript abi: materialize u128 operand into var{}", dest.id));
        for byte_index in 0..16 {
            self.emit_fixed_byte_source_byte_to("t0", "t4", &source, byte_index);
            self.emit_store_byte_to_stack_offset("t0", dest_offset + byte_index);
        }
        self.emit_store_u128_pointer_for_var(dest.id, dest_offset);
        true
    }

    fn emit_u64_le_from_fixed_byte_source(
        &mut self,
        dest_reg: &str,
        scratch_reg: &str,
        base_reg: &str,
        source: &ExpectedFixedByteSource,
        start: usize,
    ) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_offset in 0..8 {
            self.emit_fixed_byte_source_byte_to(scratch_reg, base_reg, source, start + byte_offset);
            if byte_offset != 0 {
                self.emit(format!("slli {}, {}, {}", scratch_reg, scratch_reg, byte_offset * 8));
            }
            self.emit(format!("or {}, {}, {}", dest_reg, dest_reg, scratch_reg));
        }
    }

    fn emit_u128_operand_limbs(
        &mut self,
        low_reg: &str,
        high_reg: &str,
        scratch_reg: &str,
        base_reg: &str,
        operand: &IrOperand,
        context: &str,
    ) -> bool {
        let Some(source) = self.expected_u128_source(operand) else {
            self.emit(format!("# cellscript abi: {} u128 operand is not addressable; fail closed", context));
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return false;
        };
        self.emit_prepare_fixed_byte_source(&source, 16, context);
        self.emit_u64_le_from_fixed_byte_source(low_reg, scratch_reg, base_reg, &source, 0);
        self.emit_u64_le_from_fixed_byte_source(high_reg, scratch_reg, base_reg, &source, 8);
        true
    }

    fn operand_is_u128_like(&self, operand: &IrOperand) -> bool {
        match operand {
            IrOperand::Var(var) => var.ty == IrType::U128,
            IrOperand::Const(IrConst::U128(_)) => true,
            _ => false,
        }
    }

    fn emit_store_const_bytes_to_stack(&mut self, bytes: &[u8], offset: usize) {
        for (index, byte) in bytes.iter().enumerate() {
            self.emit(format!("li t0, {}", byte));
            self.emit_stack_store_byte("t0", offset + index);
        }
    }

    fn emit_load_const(&mut self, dest: &IrVar, value: &IrConst) -> Result<()> {
        if dest.ty == IrType::U128 {
            self.emit_materialize_u128_operand_to_var(dest, &IrOperand::Const(value.clone()));
            return Ok(());
        }
        match value {
            IrConst::Unit => self.emit("li t0, 0"),
            IrConst::U8(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U16(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U32(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U64(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U128(value) => {
                if let Some(offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() {
                    self.emit_store_const_bytes_to_stack(&value.to_le_bytes(), offset);
                    self.emit_sp_addi("t0", offset);
                    self.emit_stack_store("t0", dest.id * 8);
                    return Ok(());
                }
                let label = self.const_data_label_for_bytes(value.to_le_bytes().to_vec());
                self.emit(format!("la t0, {}", label));
            }
            IrConst::Bool(b) => self.emit(format!("li t0, {}", if *b { 1 } else { 0 })),
            IrConst::Address(_) | IrConst::Hash(_) | IrConst::Array(_) => {
                let Some(bytes) = fixed_byte_const_bytes(value) else {
                    self.emit("# cellscript abi: fail closed because fixed-byte constant bytes are not materializable");
                    self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                    self.emit("li t0, 0");
                    self.emit_stack_store("t0", dest.id * 8);
                    return Ok(());
                };
                let label = self.const_data_label_for_bytes(bytes);
                self.emit(format!("la t0, {}", label));
            }
        }
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_load_var(&mut self, dest: &IrVar, name: &str) -> Result<()> {
        self.emit(format!("# load var {}", name));
        let Some(offset) = self.named_var_offsets.get(name).copied() else {
            self.emit("# cellscript abi: fail closed because named variable slot was not allocated");
            self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
            return Ok(());
        };
        self.emit_stack_load("t0", offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_store_var(&mut self, name: &str, src: &IrOperand) -> Result<()> {
        self.emit(format!("# store var {}", name));
        let Some(offset) = self.named_var_offsets.get(name).copied() else {
            self.emit("# cellscript abi: fail closed because named variable slot was not allocated");
            self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
            return Ok(());
        };
        self.emit_operand_to_register("t0", src);
        self.emit_stack_store("t0", offset);
        Ok(())
    }

    fn emit_binary(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> Result<()> {
        if self.emit_u128_add_sub_with_u64(dest, op, left, right) {
            return Ok(());
        }
        if self.emit_u128_binary(dest, op, left, right) {
            return Ok(());
        }
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) && self.emit_dynamic_byte_comparison(dest, op, left, right) {
            return Ok(());
        }
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
            && (operand_fixed_byte_width(left).is_some() || operand_fixed_byte_width(right).is_some())
        {
            if self.emit_fixed_byte_comparison(dest, op, left, right) {
                return Ok(());
            }
            if self.emit_generic_fixed_byte_comparison(dest, op, left, right) {
                return Ok(());
            }
            // Final fallback: emit a fail-closed trap with specific error code
            self.emit(format!("# binary {:?} over fixed-byte operands (unresolved)", op));
            self.emit("# cellscript abi: fail closed because fixed-byte operand sources are not available");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonMaterializationUnresolved);
            return Ok(());
        }

        if dest.ty == IrType::U128 || self.operand_is_u128(left) || self.operand_is_u128(right) {
            self.emit(format!("# binary {:?} over unsupported u128 operand shape", op));
            self.emit("# cellscript abi: fail closed because generic u128 arithmetic/comparison shape is not lowered");
            self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
            return Ok(());
        }

        self.emit_expected_operand_to_t1(left);
        self.emit_stack_store("t1", dest.id * 8);
        self.emit_expected_operand_to_t1(right);
        self.emit_stack_load("t0", dest.id * 8);

        match op {
            BinaryOp::Add => self.emit("add t0, t0, t1"),
            BinaryOp::Sub => self.emit("sub t0, t0, t1"),
            BinaryOp::Mul => self.emit("mul t0, t0, t1"),
            BinaryOp::Div if binary_operands_signed_i32(left, right) => self.emit("div t0, t0, t1"),
            BinaryOp::Div => self.emit("divu t0, t0, t1"),
            BinaryOp::Mod if binary_operands_signed_i32(left, right) => self.emit("rem t0, t0, t1"),
            BinaryOp::Mod => self.emit("remu t0, t0, t1"),
            BinaryOp::Eq => {
                self.emit("sub t0, t0, t1");
                self.emit("seqz t0, t0");
            }
            BinaryOp::Ne => {
                self.emit("sub t0, t0, t1");
                self.emit("snez t0, t0");
            }
            BinaryOp::Lt if binary_operands_signed_i32(left, right) => self.emit("slt t0, t0, t1"),
            BinaryOp::Lt => self.emit("sltu t0, t0, t1"),
            BinaryOp::Le if binary_operands_signed_i32(left, right) => {
                self.emit("slt t0, t1, t0");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::Le => {
                self.emit("sltu t0, t1, t0");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::Gt if binary_operands_signed_i32(left, right) => self.emit("slt t0, t1, t0"),
            BinaryOp::Gt => self.emit("sltu t0, t1, t0"),
            BinaryOp::Ge if binary_operands_signed_i32(left, right) => {
                self.emit("slt t0, t0, t1");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::Ge => {
                self.emit("sltu t0, t0, t1");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::And => self.emit("and t0, t0, t1"),
            BinaryOp::Or => self.emit("or t0, t0, t1"),
        }

        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_u128_binary(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        let arithmetic_u128 = dest.ty == IrType::U128 || self.operand_is_u128_like(left) || self.operand_is_u128_like(right);
        let comparison_u128 = matches!(op, BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge)
            && (self.operand_is_u128_like(left) || self.operand_is_u128_like(right));
        if !arithmetic_u128 && !comparison_u128 {
            return false;
        }

        match op {
            BinaryOp::Add | BinaryOp::Sub if dest.ty == IrType::U128 => {
                self.emit_u128_add_sub(dest, op, left, right);
                true
            }
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                self.emit_u128_compare(dest, op, left, right);
                true
            }
            BinaryOp::Mul if dest.ty == IrType::U128 => {
                self.emit_u128_mul(dest, left, right);
                true
            }
            BinaryOp::Div if dest.ty == IrType::U128 => {
                self.emit_u128_div(dest, left, right);
                true
            }
            BinaryOp::Mod if arithmetic_u128 => {
                self.emit("# cellscript abi: u128 Mod requires full-width lowering; fail closed");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                true
            }
            BinaryOp::Add | BinaryOp::Sub if arithmetic_u128 => {
                self.emit(format!("# cellscript abi: u128 {:?} result is not materialized as u128; fail closed", op));
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                true
            }
            _ => false,
        }
    }

    fn emit_u128_add_sub(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) {
        let Some(dest_offset) = self.u128_value_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: u128 arithmetic destination has no storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return;
        };
        if !self.emit_u128_operand_limbs("t0", "t1", "t6", "t4", left, "u128 arithmetic left") {
            return;
        }
        if !self.emit_u128_operand_limbs("t2", "t3", "t6", "t5", right, "u128 arithmetic right") {
            return;
        }
        let ok_label = self.fresh_label("u128_arithmetic_ok");
        let overflow_label = self.fresh_label("u128_arithmetic_overflow");
        match op {
            BinaryOp::Add => {
                self.emit("# cellscript abi: u128 add with carry");
                self.emit("add t4, t0, t2");
                self.emit("sltu t6, t4, t0");
                self.emit("add t5, t1, t3");
                self.emit("sltu a6, t5, t1");
                self.emit(format!("bnez a6, {}", overflow_label));
                self.emit("add t5, t5, t6");
                self.emit("sltu a6, t5, t6");
                self.emit(format!("bnez a6, {}", overflow_label));
            }
            BinaryOp::Sub => {
                self.emit("# cellscript abi: u128 sub with borrow");
                self.emit("sltu t6, t0, t2");
                self.emit("sltu a6, t1, t3");
                self.emit(format!("bnez a6, {}", overflow_label));
                self.emit("sub t4, t0, t2");
                self.emit("sub t5, t1, t3");
                self.emit(format!("beqz t6, {}", ok_label));
                self.emit(format!("beqz t5, {}", overflow_label));
                self.emit("addi t5, t5, -1");
            }
            _ => unreachable!("u128 add/sub only"),
        }
        self.emit_label(&ok_label);
        self.emit_stack_store("t4", dest_offset);
        self.emit_stack_store("t5", dest_offset + 8);
        self.emit_store_u128_pointer_for_var(dest.id, dest_offset);
        let done_label = self.fresh_label("u128_arithmetic_done");
        self.emit(format!("j {}", done_label));
        self.emit_label(&overflow_label);
        self.emit_runtime_error_comment(CellScriptRuntimeError::AggregateAmountMismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_epilogue();
        self.emit_label(&done_label);
    }

    fn emit_u128_compare(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) {
        if !self.emit_u128_operand_limbs("t0", "t1", "t6", "t4", left, "u128 compare left") {
            return;
        }
        if !self.emit_u128_operand_limbs("t2", "t3", "t6", "t5", right, "u128 compare right") {
            return;
        }
        self.emit("# cellscript abi: u128 compare high limb first");
        let high_lt = self.fresh_label("u128_compare_high_lt");
        let high_gt = self.fresh_label("u128_compare_high_gt");
        let same_high = self.fresh_label("u128_compare_same_high");
        let done = self.fresh_label("u128_compare_done");
        self.emit("sltu t4, t1, t3");
        self.emit(format!("bnez t4, {}", high_lt));
        self.emit("sltu t4, t3, t1");
        self.emit(format!("bnez t4, {}", high_gt));
        self.emit_label(&same_high);
        match op {
            BinaryOp::Eq => {
                self.emit("sub t4, t0, t2");
                self.emit("seqz t0, t4");
            }
            BinaryOp::Ne => {
                self.emit("sub t4, t0, t2");
                self.emit("snez t0, t4");
            }
            BinaryOp::Lt => self.emit("sltu t0, t0, t2"),
            BinaryOp::Le => {
                self.emit("sltu t0, t2, t0");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::Gt => self.emit("sltu t0, t2, t0"),
            BinaryOp::Ge => {
                self.emit("sltu t0, t0, t2");
                self.emit("xori t0, t0, 1");
            }
            _ => unreachable!("u128 compare only"),
        }
        self.emit(format!("j {}", done));
        self.emit_label(&high_lt);
        let high_lt_value = matches!(op, BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le);
        self.emit(format!("li t0, {}", u8::from(high_lt_value)));
        self.emit(format!("j {}", done));
        self.emit_label(&high_gt);
        let high_gt_value = matches!(op, BinaryOp::Ne | BinaryOp::Gt | BinaryOp::Ge);
        self.emit(format!("li t0, {}", u8::from(high_gt_value)));
        self.emit_label(&done);
        self.emit_stack_store("t0", dest.id * 8);
    }

    fn emit_u128_mul(&mut self, dest: &IrVar, left: &IrOperand, right: &IrOperand) {
        let Some(dest_offset) = self.u128_value_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: u128 multiplication destination has no storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return;
        };
        if !self.emit_u128_operand_limbs("t0", "t1", "t6", "t4", left, "u128 multiplication left") {
            return;
        }
        if !self.emit_u128_operand_limbs("t2", "t3", "t6", "t5", right, "u128 multiplication right") {
            return;
        }
        self.emit("# cellscript abi: checked u128 multiplication");
        let overflow_label = self.fresh_label("u128_mul_overflow");
        let high_left_zero = self.fresh_label("u128_mul_high_left_zero");
        let high_pair_ok = self.fresh_label("u128_mul_high_pair_ok");
        let done_label = self.fresh_label("u128_mul_done");

        self.emit(format!("beqz t1, {}", high_left_zero));
        self.emit(format!("bnez t3, {}", overflow_label));
        self.emit_label(&high_left_zero);
        self.emit(format!("beqz t3, {}", high_pair_ok));
        self.emit(format!("bnez t1, {}", overflow_label));
        self.emit_label(&high_pair_ok);

        self.emit("mul t4, t0, t2");
        self.emit("mulhu a2, t0, t2");

        self.emit("mul a3, t0, t3");
        self.emit("mulhu a4, t0, t3");
        self.emit(format!("bnez a4, {}", overflow_label));

        self.emit("mul a5, t1, t2");
        self.emit("mulhu a6, t1, t2");
        self.emit(format!("bnez a6, {}", overflow_label));

        self.emit("add t5, a2, a3");
        self.emit("sltu a7, t5, a2");
        self.emit(format!("bnez a7, {}", overflow_label));
        self.emit("add t5, t5, a5");
        self.emit("sltu a7, t5, a5");
        self.emit(format!("bnez a7, {}", overflow_label));

        self.emit_stack_store("t4", dest_offset);
        self.emit_stack_store("t5", dest_offset + 8);
        self.emit_store_u128_pointer_for_var(dest.id, dest_offset);
        self.emit(format!("j {}", done_label));

        self.emit_label(&overflow_label);
        self.emit_runtime_error_comment(CellScriptRuntimeError::AggregateAmountMismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_epilogue();
        self.emit_label(&done_label);
    }

    fn emit_u128_div(&mut self, dest: &IrVar, left: &IrOperand, right: &IrOperand) {
        let Some(dest_offset) = self.u128_value_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: u128 division destination has no storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return;
        };
        if !self.emit_u128_operand_limbs("t0", "t1", "t6", "t4", left, "u128 division numerator") {
            return;
        }
        if !self.emit_u128_operand_limbs("t2", "t3", "t6", "t5", right, "u128 division denominator") {
            return;
        }
        self.emit("# cellscript abi: checked u128 division by restoring long division");
        let ok_label = self.fresh_label("u128_div_denominator_ok");
        let loop_label = self.fresh_label("u128_div_loop");
        let skip_sub_label = self.fresh_label("u128_div_skip_subtract");
        let subtract_label = self.fresh_label("u128_div_subtract");
        let done_label = self.fresh_label("u128_div_done");
        let fail_label = self.fresh_label("u128_div_zero_denominator");

        self.emit("or t4, t2, t3");
        self.emit(format!("bnez t4, {}", ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&ok_label);
        self.emit("li t4, 0"); // remainder low
        self.emit("li t5, 0"); // remainder high
        self.emit("li a2, 0"); // quotient low
        self.emit("li a3, 0"); // quotient high
        self.emit("li a4, 128");
        self.emit_label(&loop_label);

        self.emit("slt a5, t1, zero"); // next numerator bit
        self.emit("slt a6, t4, zero"); // carry from remainder low
        self.emit("slli t4, t4, 1");
        self.emit("or t4, t4, a5");
        self.emit("slli t5, t5, 1");
        self.emit("or t5, t5, a6");

        self.emit("slt a5, t0, zero"); // carry from numerator low
        self.emit("slli t0, t0, 1");
        self.emit("slli t1, t1, 1");
        self.emit("or t1, t1, a5");

        self.emit("slt a5, a2, zero"); // carry from quotient low
        self.emit("slli a2, a2, 1");
        self.emit("slli a3, a3, 1");
        self.emit("or a3, a3, a5");

        self.emit("sltu a5, t5, t3");
        self.emit(format!("bnez a5, {}", skip_sub_label));
        self.emit("sltu a5, t3, t5");
        self.emit(format!("bnez a5, {}", subtract_label));
        self.emit("sltu a5, t4, t2");
        self.emit(format!("bnez a5, {}", skip_sub_label));

        self.emit_label(&subtract_label);
        self.emit("sltu a5, t4, t2");
        self.emit("sub t4, t4, t2");
        self.emit("sub t5, t5, t3");
        self.emit("sub t5, t5, a5");
        self.emit("addi a2, a2, 1");

        self.emit_label(&skip_sub_label);
        self.emit("addi a4, a4, -1");
        self.emit(format!("bnez a4, {}", loop_label));
        self.emit_stack_store("a2", dest_offset);
        self.emit_stack_store("a3", dest_offset + 8);
        self.emit_store_u128_pointer_for_var(dest.id, dest_offset);
        self.emit(format!("j {}", done_label));

        self.emit_label(&fail_label);
        self.emit_runtime_error_comment(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()));
        self.emit_epilogue();
        self.emit_label(&done_label);
    }

    fn emit_dynamic_byte_comparison(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        let (IrOperand::Var(left_var), IrOperand::Var(right_var)) = (left, right) else {
            return false;
        };
        let Some(left_len_offset) = self.dynamic_value_size_offsets.get(&left_var.id).copied() else {
            return false;
        };
        let Some(right_len_offset) = self.dynamic_value_size_offsets.get(&right_var.id).copied() else {
            return false;
        };

        let equal_value = if matches!(op, BinaryOp::Eq) { 1 } else { 0 };
        let mismatch_value = if matches!(op, BinaryOp::Eq) { 0 } else { 1 };
        let len_equal_label = self.fresh_label("dynamic_bytes_len_equal");
        let bytes_equal_label = self.fresh_label("dynamic_bytes_equal");
        let done_label = self.fresh_label("dynamic_bytes_cmp_done");

        self.emit(format!("# binary {:?} over dynamic byte operands", op));
        self.emit_stack_load("t0", left_len_offset);
        self.emit_stack_load("t1", right_len_offset);
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", len_equal_label));
        self.emit(format!("li t0, {}", mismatch_value));
        self.emit_stack_store("t0", dest.id * 8);
        self.emit(format!("j {}", done_label));

        self.emit_label(&len_equal_label);
        self.emit_stack_load("a0", left_var.id * 8);
        self.emit_stack_load("a1", right_var.id * 8);
        self.emit_stack_load("a2", left_len_offset);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("beqz a0, {}", bytes_equal_label));
        self.emit(format!("li t0, {}", mismatch_value));
        self.emit_stack_store("t0", dest.id * 8);
        self.emit(format!("j {}", done_label));

        self.emit_label(&bytes_equal_label);
        self.emit(format!("li t0, {}", equal_value));
        self.emit_stack_store("t0", dest.id * 8);
        self.emit_label(&done_label);
        true
    }

    fn emit_unary(&mut self, dest: &IrVar, op: UnaryOp, operand: &IrOperand) -> Result<()> {
        match operand {
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t0, {}", n)),
            IrOperand::Var(v) => self.emit_stack_load("t0", v.id * 8),
            _ => self.emit("li t0, 0"),
        }

        match op {
            UnaryOp::Neg => self.emit("neg t0, t0"),
            UnaryOp::Not => self.emit("xori t0, t0, 1"),
            UnaryOp::Ref | UnaryOp::Deref => self.emit("# reference conversion (no-op in asm backend)"),
        }

        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> Result<()> {
        if self.emit_fixed_byte_field_access(dest, obj, field) {
            return Ok(());
        }
        if self.emit_schema_field_access(dest, obj, field) {
            return Ok(());
        }
        if self.emit_aggregate_field_access(dest, obj, field) {
            return Ok(());
        }
        if self.emit_tuple_call_return_field_access(dest, obj, field) {
            return Ok(());
        }
        if self.emit_generic_field_access(dest, obj, field) {
            return Ok(());
        }

        self.emit(format!("# field access .{} (unresolved)", field));
        self.emit("# cellscript abi: fail closed because field offset is not computable from available type layout");
        self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
        Ok(())
    }

    fn emit_fixed_byte_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let layout = aggregate_field_layout(&var.ty, field).or_else(|| {
            named_type_name(&var.ty)
                .and_then(|type_name| self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned())
        });
        let Some(layout) = layout else {
            return false;
        };
        let Some(parent_width) = self.fixed_byte_like_width(&var.ty) else {
            return false;
        };
        let Some(source) = self.expected_fixed_byte_source(obj, parent_width) else {
            return false;
        };
        if is_fixed_scalar_ir_type(&dest.ty) {
            let Some(width) = layout_fixed_scalar_width(&layout) else {
                return false;
            };
            self.emit(format!(
                "# cellscript abi: fixed-byte scalar field {}.{} offset={} size={}",
                aggregate_type_label(&var.ty),
                field,
                layout.offset,
                width
            ));
            self.emit_prepare_fixed_byte_source(&source, parent_width, "fixed-byte scalar field access");
            if !self.emit_fixed_byte_source_pointer_or_const_to("t4", &source) {
                return false;
            }
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            if layout.ty == IrType::I32 {
                self.emit_sign_extend_i32("t0");
            }
            self.emit_stack_store("t0", dest.id * 8);
            if let ExpectedFixedByteSource::SchemaField(parent) = &source {
                let mut nested_layout = layout.clone();
                nested_layout.offset += parent.layout.offset;
                let nested_source = SchemaFieldValueSource {
                    obj_var_id: parent.obj_var_id,
                    type_name: parent.type_name.clone(),
                    field: format!("{}.{}", parent.field, field),
                    layout: nested_layout,
                };
                self.schema_field_value_sources.insert(dest.id, nested_source.clone());
                if dest.ty == IrType::U64 {
                    self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Field(nested_source));
                }
            }
            return true;
        }
        let Some(width) = layout_fixed_byte_width(&layout).or_else(|| self.fixed_named_type_width(&layout.ty)) else {
            return false;
        };
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            return false;
        };

        self.emit(format!(
            "# cellscript abi: fixed-byte field {}.{} offset={} size={}",
            aggregate_type_label(&var.ty),
            field,
            layout.offset,
            width
        ));
        self.emit_prepare_fixed_byte_source(&source, parent_width, "fixed-byte field access");
        if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &source) {
            return false;
        }
        self.emit(format!("addi a0, a0, {}", layout.offset));
        self.emit_sp_addi("a1", dest_offset);
        self.emit(format!("li a2, {}", width));
        self.emit("call __cellscript_memcpy_fixed");
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        if let ExpectedFixedByteSource::SchemaField(parent) = &source {
            let mut nested_layout = layout.clone();
            nested_layout.offset += parent.layout.offset;
            let nested_source = SchemaFieldValueSource {
                obj_var_id: parent.obj_var_id,
                type_name: parent.type_name.clone(),
                field: format!("{}.{}", parent.field, field),
                layout: nested_layout,
            };
            self.schema_field_value_sources.insert(dest.id, nested_source);
        }
        true
    }

    fn emit_schema_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        if !self.schema_pointer_vars.contains(&var.id) {
            return false;
        }
        let Some(type_name) = named_type_name(&var.ty) else {
            return false;
        };
        let Some(layout) = self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
            return false;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            return self.emit_dynamic_schema_field_access(dest, var, type_name, field, &layout);
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: schema field {}.{} offset={} size={}", type_name, field, layout.offset, width));
        self.emit_stack_load("t4", var.id * 8);
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, type_name);
                self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
                if layout_fixed_scalar_width(&layout).is_some() {
                    self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
                } else {
                    self.emit(format!("addi t0, t4, {}", layout.offset));
                }
            } else {
                self.emit_molecule_table_field_bounds_to_t5(
                    "t4",
                    size_offset,
                    layout.index,
                    width,
                    &format!("{}.{}", type_name, field),
                );
                self.emit("add t4, t4, t5");
                if layout_fixed_scalar_width(&layout).is_some() {
                    self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
                } else {
                    self.emit("addi t0, t4, 0");
                }
            }
        } else {
            if !self.type_fixed_sizes.contains_key(type_name) {
                return false;
            }
            if layout_fixed_scalar_width(&layout).is_some() {
                self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            } else {
                self.emit(format!("addi t0, t4, {}", layout.offset));
            }
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_dynamic_schema_field_access(
        &mut self,
        dest: &IrVar,
        obj: &IrVar,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
    ) -> bool {
        if molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).is_none() {
            return false;
        }
        let Some(size_offset) = self.schema_pointer_size_offsets.get(&obj.id).copied() else {
            return false;
        };
        let Some(dest_size_offset) = self.dynamic_value_size_offsets.get(&dest.id).copied() else {
            return false;
        };
        let Some(field_count) = self.type_layouts.get(type_name).map(|fields| fields.len()) else {
            return false;
        };

        let context = format!("{}.{}", type_name, field);
        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: dynamic schema field {} index={} as Molecule vector bytes", context, layout.index));
        self.emit_stack_load("t4", obj.id * 8);
        self.emit_molecule_table_field_span_to_t5_t6("t4", size_offset, layout.index, field_count, &context);
        self.emit("add t0, t4, t5");
        self.emit("sub t1, t6, t5");
        self.emit_stack_store("t0", dest.id * 8);
        self.emit_stack_store("t1", dest_size_offset);
        true
    }

    fn emit_aggregate_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let Some(source) = self.aggregate_pointer_sources.get(&var.id) else {
            return false;
        };
        let source_ty = source.ty.clone();
        let Some(layout) = aggregate_field_layout(&source_ty, field) else {
            return false;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            return false;
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!(
            "# cellscript abi: fixed aggregate field {}.{} offset={} size={}",
            aggregate_type_label(&source_ty),
            field,
            layout.offset,
            width
        ));
        self.emit_stack_load("t4", var.id * 8);
        if layout_fixed_scalar_width(&layout).is_some() {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            if layout.ty == IrType::I32 {
                self.emit_sign_extend_i32("t0");
            }
        } else {
            self.emit(format!("addi t0, t4, {}", layout.offset));
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_tuple_call_return_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let Some(slot_var_id) = self.tuple_call_return_field_slots.get(&(var.id, field.to_string())).copied() else {
            return false;
        };
        if slot_var_id != dest.id {
            return false;
        }
        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: tuple call return field .{} projected from return register", field));
        true
    }

    /// Generic field access: when specialized paths don't match, try to compute the
    /// field offset from type_layouts and emit an unaligned load from the pointer
    /// stored in the object's stack slot. This works for any named-type variable
    /// whose type has a registered layout, even if it wasn't classified as a
    /// schema_pointer_var or aggregate_pointer_source.
    fn emit_generic_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let Some(type_name) = named_type_name(&var.ty) else {
            return false;
        };
        if !self.type_fixed_sizes.contains_key(type_name) {
            return false;
        }
        let Some(layout) = self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
            return false;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            return false;
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: generic field {}.{} offset={} size={}", type_name, field, layout.offset, width));

        // Bounds check: if the object has a known size offset, verify the data
        // is large enough to contain this field.
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
            self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
        } else if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&var.id).copied() {
            self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
        }

        // Load the object pointer from the stack slot
        self.emit_stack_load("t4", var.id * 8);
        if layout_fixed_scalar_width(&layout).is_some() {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
        } else {
            self.emit(format!("addi t0, t4, {}", layout.offset));
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> Result<()> {
        if self.emit_fixed_aggregate_index(dest, arr, idx) {
            return Ok(());
        }
        if self.emit_dynamic_molecule_vector_index(dest, arr, idx) {
            return Ok(());
        }
        if self.emit_stack_collection_index(dest, arr, idx) {
            return Ok(());
        }
        if self.emit_dynamic_index_access(dest, arr, idx) {
            return Ok(());
        }

        self.emit("# index access (unresolved)");
        self.emit("# cellscript abi: fail closed because element layout is not statically computable");
        self.emit_fail(CellScriptRuntimeError::TypeHashMismatch);
        Ok(())
    }

    fn emit_fixed_aggregate_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> bool {
        let (IrOperand::Var(arr_var), Some(index)) = (arr, const_usize_operand(idx)) else {
            return false;
        };
        if !self.aggregate_pointer_sources.contains_key(&arr_var.id) {
            return false;
        }
        let IrType::Array(inner, len) = &arr_var.ty else {
            return false;
        };
        if index >= *len {
            return false;
        }
        let Some(element_width) = type_static_length(inner) else {
            return false;
        };
        let Some(total_width) = type_static_length(&arr_var.ty) else {
            return false;
        };
        let offset = index * element_width;
        self.emit(format!("# index access [{}]", index));
        self.emit(format!("# cellscript abi: fixed aggregate index element_offset={} element_size={}", offset, element_width));
        if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&arr_var.id).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, total_width, "fixed aggregate param");
            self.emit_loaded_schema_bounds_check(size_offset, offset + element_width, "fixed aggregate index");
        }
        self.emit_stack_load("t4", arr_var.id * 8);
        if let Some(width) = fixed_scalar_width(inner, Some(element_width)) {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", offset, width);
        } else {
            self.emit(format!("addi t0, t4, {}", offset));
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_dynamic_molecule_vector_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> bool {
        let IrOperand::Var(arr_var) = arr else {
            return false;
        };
        let Some(size_offset) = self
            .dynamic_value_size_offsets
            .get(&arr_var.id)
            .copied()
            .or_else(|| self.schema_pointer_size_offsets.get(&arr_var.id).copied())
        else {
            return false;
        };
        let Some(element_width) = molecule_vector_element_fixed_width(&arr_var.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };

        self.emit("# index access");
        self.emit(format!(
            "# cellscript abi: dynamic Molecule vector index element_size={} size_offset={}",
            element_width, size_offset
        ));
        self.emit_loaded_schema_bounds_check(size_offset, 4, "dynamic Molecule vector index");
        self.emit_stack_load("t4", arr_var.id * 8);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, 4);

        self.emit_stack_load("t3", size_offset);
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t5, t0, t2");
        self.emit("addi t5, t5, 4");
        self.emit("sub t2, t3, t5");
        let size_ok = self.fresh_label("molecule_vector_index_size_ok");
        self.emit(format!("beqz t2, {}", size_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&size_ok);

        match idx {
            IrOperand::Var(v) => self.emit_stack_load("t1", v.id * 8),
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t1, {}", n)),
            _ => self.emit("li t1, 0"),
        }

        let bounds_ok = self.fresh_label("molecule_vector_index_bounds_ok");
        self.emit("sltu t2, t1, t0");
        self.emit(format!("bnez t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t1, t1, t2");
        self.emit("addi t1, t1, 4");
        self.emit("add t4, t4, t1");
        if fixed_scalar_width(&dest.ty, Some(element_width)).is_some() {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, element_width.min(8));
        } else {
            self.emit("addi t0, t4, 0");
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_stack_collection_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> bool {
        let IrOperand::Var(arr_var) = arr else {
            return false;
        };
        if !self.stack_collection_vars.contains(&arr_var.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&arr_var.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        let dest_scalar = fixed_scalar_width(&dest.ty, Some(element_width)).is_some();
        let dest_fixed_bytes = self.fixed_byte_like_width(&dest.ty).is_some_and(|width| width == element_width);
        if !dest_scalar && !dest_fixed_bytes {
            return false;
        }

        self.emit("# index access");
        self.emit(format!("# cellscript abi: stack collection index element_size={}", element_width));
        self.emit_stack_load("t4", arr_var.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", idx);

        let bounds_ok = self.fresh_label("stack_collection_index_bounds_ok");
        self.emit("sltu t2, t1, t0");
        self.emit(format!("bnez t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t1, t1, t2");
        self.emit("add t4, t4, t1");
        if dest_scalar {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, element_width);
        } else {
            self.emit("addi t0, t4, 0");
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    /// Dynamic index access: compute element offset from array type layout.
    /// Handles cases where the index is not a constant or the array is not in
    /// aggregate_pointer_sources, but the element size is still statically known.
    fn emit_dynamic_index_access(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> bool {
        let IrOperand::Var(arr_var) = arr else {
            return false;
        };
        let IrType::Array(inner, len) = &arr_var.ty else {
            return false;
        };
        let Some(element_width) = type_static_length(inner) else {
            return false;
        };
        let Some(total_width) = type_static_length(&arr_var.ty) else {
            return false;
        };

        self.emit("# index access");
        self.emit(format!("# cellscript abi: dynamic index element_size={}", element_width));

        // Bounds check: if we have a size offset, verify total data is large enough
        if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&arr_var.id).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, total_width, "dynamic index aggregate");
        }

        // Load array base pointer
        self.emit_stack_load("t4", arr_var.id * 8);

        // Load index value into t1
        match idx {
            IrOperand::Var(v) => self.emit_stack_load("t1", v.id * 8),
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t1, {}", n)),
            _ => self.emit("li t1, 0"),
        }

        // Bounds check: index < len
        let bounds_ok = self.fresh_label("idx_bounds_ok");
        self.emit(format!("li t2, {}", len));
        self.emit("slt t3, t1, t2");
        self.emit(format!("bnez t3, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&bounds_ok);

        // Compute offset = index * element_width
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t1, t1, t2");

        if fixed_scalar_width(inner, Some(element_width)).is_some() {
            // Scalar element: load from base + offset
            self.emit("add t4, t4, t1");
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, element_width.min(8));
        } else {
            // Pointer-sized element: compute base + offset
            self.emit("add t0, t4, t1");
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_length(&mut self, dest: &IrVar, operand: &IrOperand) -> Result<()> {
        self.emit("# length");
        if let Some(static_len) = self.static_length(operand) {
            self.emit(format!("li t0, {}", static_len));
        } else if self.emit_stack_collection_length(operand) || self.emit_dynamic_molecule_vector_length(operand) {
        } else if let Some(size_offset) = self.dynamic_length_from_size_offset(operand) {
            // For schema-backed or fixed-byte params, the actual size word is already
            // stored at the size offset; load it directly.
            self.emit(format!("# cellscript abi: dynamic length from size word at offset={}", size_offset));
            self.emit_stack_load("t0", size_offset);
        } else {
            self.emit("# cellscript abi: fail closed because dynamic length is not available");
            self.emit_fail(CellScriptRuntimeError::CollectionRuntimeUnsupported);
            return Ok(());
        }
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_stack_collection_length(&mut self, operand: &IrOperand) -> bool {
        let IrOperand::Var(var) = operand else {
            return false;
        };
        if !self.stack_collection_vars.contains(&var.id) {
            return false;
        }
        self.emit("# cellscript abi: stack collection length");
        self.emit_stack_load("t4", var.id * 8);
        self.emit("ld t0, -8(t4)");
        true
    }

    fn emit_dynamic_molecule_vector_length(&mut self, operand: &IrOperand) -> bool {
        let IrOperand::Var(var) = operand else {
            return false;
        };
        let Some(size_offset) =
            self.dynamic_value_size_offsets.get(&var.id).copied().or_else(|| self.schema_pointer_size_offsets.get(&var.id).copied())
        else {
            return false;
        };
        let Some(element_width) = molecule_vector_element_fixed_width(&var.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes) else {
            return false;
        };

        self.emit(format!(
            "# cellscript abi: dynamic Molecule vector length element_size={} size_offset={}",
            element_width, size_offset
        ));
        self.emit_loaded_schema_bounds_check(size_offset, 4, "dynamic Molecule vector length");
        self.emit_stack_load("t4", var.id * 8);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, 4);

        self.emit_stack_load("t1", size_offset);
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t0, t2");
        self.emit("addi t3, t3, 4");
        self.emit("sub t2, t1, t3");
        let size_ok = self.fresh_label("molecule_vector_size_ok");
        self.emit(format!("beqz t2, {}", size_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&size_ok);
        true
    }

    /// Try to obtain the size offset for a dynamically-sized operand.
    fn dynamic_length_from_size_offset(&self, operand: &IrOperand) -> Option<usize> {
        let IrOperand::Var(var) = operand else {
            return None;
        };
        // Check schema pointer size offsets (named-type params, consumed inputs, read_refs)
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
            return Some(size_offset);
        }
        // Check fixed-byte param size offsets
        if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&var.id).copied() {
            return Some(size_offset);
        }
        // Check cell buffer size offsets (consumed inputs, read_refs, type_hash)
        if let Some(size_offset) = self.cell_buffer_size_offsets.get(&var.id).copied() {
            return Some(size_offset);
        }
        None
    }

    fn emit_type_hash(&mut self, dest: &IrVar, operand: &IrOperand) -> Result<()> {
        if let Some(output_index) = self.output_type_hash_sources.get(&dest.id).copied() {
            let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() else {
                return Ok(());
            };
            let Some(buffer_offset) = self.cell_buffer_offsets.get(&dest.id).copied() else {
                return Ok(());
            };
            self.emit("# type_hash");
            self.emit_operand_comment("type_hash source", operand);
            self.emit_load_cell_by_field_syscall_to_offsets(
                "output_type_hash",
                CKB_SOURCE_OUTPUT,
                output_index,
                CKB_CELL_FIELD_TYPE_HASH,
                size_offset,
                buffer_offset,
                32,
            );
            self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
            self.emit_loaded_schema_exact_size_check(size_offset, 32, "output type hash");
            self.emit_sp_addi("t0", buffer_offset);
            self.emit_stack_store("t0", dest.id * 8);
            return Ok(());
        }
        if self.emit_runtime_type_hash(dest, operand) {
            return Ok(());
        }
        if let Some(param_id) = self.param_type_hash_sources.get(&dest.id).copied() {
            let Some(pointer_offset) = self.param_type_hash_pointer_offsets.get(&param_id).copied() else {
                return Ok(());
            };
            let Some(size_offset) = self.param_type_hash_size_offsets.get(&param_id).copied() else {
                return Ok(());
            };
            self.emit("# type_hash");
            self.emit_operand_comment("type_hash source", operand);
            self.emit_loaded_schema_exact_size_check(size_offset, 32, "param type hash");
            self.emit_stack_load("t0", pointer_offset);
            self.emit_stack_store("t0", dest.id * 8);
            return Ok(());
        }

        self.emit("# type_hash (unresolved)");
        self.emit("# cellscript abi: fail closed because type_hash source cell cannot be determined");
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        Ok(())
    }

    /// Runtime type_hash: try to load the type hash from a cell identified by the operand's
    /// association with a consumed input, created output, or read_ref cell dep.
    fn emit_runtime_type_hash(&mut self, dest: &IrVar, operand: &IrOperand) -> bool {
        let IrOperand::Var(var) = operand else {
            return false;
        };

        // Try to find which cell this var is associated with
        let (source, index) = if let Some(input_index) = self.consume_indices.get(&var.id).copied() {
            (CKB_SOURCE_INPUT, input_index)
        } else if let Some(output_index) = self.operation_output_indices.get(&var.id).copied() {
            (CKB_SOURCE_OUTPUT, output_index)
        } else if let Some(dep_index) = self.read_ref_indices.get(&var.id).copied() {
            (CKB_SOURCE_CELL_DEP, dep_index)
        } else {
            return false;
        };

        let size_offset = self.cell_buffer_size_offsets.get(&dest.id).copied().unwrap_or_else(|| self.runtime_scratch_size_offset());
        let buffer_offset = self.cell_buffer_offsets.get(&dest.id).copied().unwrap_or_else(|| self.runtime_scratch_buffer_offset());

        self.emit("# type_hash");
        self.emit_operand_comment("type_hash source", operand);
        self.emit_load_cell_by_field_syscall_to_offsets(
            "runtime_type_hash",
            source,
            index,
            CKB_CELL_FIELD_TYPE_HASH,
            size_offset,
            buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(size_offset, 32, "runtime type hash");
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_collection_new(&mut self, dest: &IrVar, ty: &str, capacity: Option<&IrOperand>) -> Result<()> {
        // Stack-allocated collection: the stack slot stores a pointer to the
        // collection buffer area, with the length word immediately before the buffer.
        // Layout: [length: u64][buffer: RUNTIME_COLLECTION_BUFFER_SIZE bytes]
        // We allocate space in the stack frame and initialize length to 0.
        let collection_slot_size = 8 + RUNTIME_COLLECTION_BUFFER_SIZE;
        let length_offset = self.collection_region_start + collection_slot_size * self.next_collection_slot;
        let buffer_offset = length_offset + 8;

        self.emit(format!("# collection new {}", ty));
        self.emit(format!(
            "# cellscript abi: stack collection buffer_offset={} max_size={}",
            buffer_offset, RUNTIME_COLLECTION_BUFFER_SIZE
        ));
        if let Some(capacity) = capacity {
            self.emit("# cellscript abi: stack collection with_capacity uses fixed backing buffer");
            self.emit_operand_comment("capacity", capacity);
        }

        // Initialize length to 0
        self.emit_stack_store("zero", length_offset);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);
        self.empty_molecule_vector_vars.insert(dest.id);
        self.stack_collection_vars.insert(dest.id);
        self.next_collection_slot += 1;
        Ok(())
    }

    fn emit_collection_capacity(&mut self, dest: &IrVar, collection: &IrOperand) -> Result<()> {
        self.emit("# collection capacity");
        self.emit_operand_comment("collection", collection);
        if self.emit_stack_collection_capacity(dest, collection) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection capacity is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_capacity(&mut self, dest: &IrVar, collection: &IrOperand) -> bool {
        if dest.ty != IrType::U64 {
            return false;
        }
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width == 0 {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection capacity element_size={}", element_width));
        self.emit(format!("li t0, {}", RUNTIME_COLLECTION_BUFFER_SIZE / element_width));
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_collection_push(&mut self, collection: &IrOperand, value: &IrOperand) -> Result<()> {
        self.emit("# collection push");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("value", value);
        if matches!(value, IrOperand::Var(var) if self.verified_collection_push_values.contains(&var.id)) {
            self.emit("# cellscript abi: collection push is covered by mutate append verifier");
            return Ok(());
        }
        if matches!(collection, IrOperand::Var(var) if self.verified_collection_construction_vectors.contains(&var.id)) {
            self.emit("# cellscript abi: collection push is covered by create-output vector verifier");
            return Ok(());
        }
        if self.emit_stack_collection_push(collection, value) {
            return Ok(());
        }
        // In the verifier context, collection push is used for building output data.
        // The verifier doesn't need to actually build the data; it needs to verify
        // that the output cell data matches expectations. The collection operations
        // in the verifier body are vestigial from the source-level specification.
        // For now, emit a fail-closed trap because runtime collection mutation is not
        // needed in the verifier path – the prelude already verified the output.
        self.emit("# cellscript abi: collection push is not needed for verifier execution");
        self.emit("# cellscript abi: if this path is reached, the source program uses dynamic collections");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_push(&mut self, collection: &IrOperand, value: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(width) = self.constructed_byte_vector_part_width(value) else {
            return false;
        };
        if width > RUNTIME_COLLECTION_BUFFER_SIZE {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection push element_size={}", width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit(format!("li t1, {}", width));
        self.emit("mul t2, t0, t1");
        self.emit(format!("li t3, {}", RUNTIME_COLLECTION_BUFFER_SIZE));
        self.emit("sub t5, t3, t2");
        self.emit("sltu t5, t5, t1");
        let capacity_ok = self.fresh_label("stack_collection_push_capacity_ok");
        self.emit(format!("beqz t5, {}", capacity_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&capacity_ok);

        self.emit("add t5, t4, t2");
        if width <= 8 && fixed_scalar_operand_width(value).is_some() {
            self.emit_operand_to_register("t1", value);
            match width {
                1 => self.emit("sb t1, 0(t5)"),
                2 => self.emit("sh t1, 0(t5)"),
                4 => self.emit("sw t1, 0(t5)"),
                8 => self.emit("sd t1, 0(t5)"),
                _ => return false,
            }
        } else {
            let Some(source) = self.expected_fixed_byte_source(value, width) else {
                return false;
            };
            self.emit_prepare_fixed_byte_source(&source, width, "stack collection push");
            self.emit(format!("# cellscript abi: stack collection copy fixed bytes size={}", width));
            for byte_index in 0..width {
                self.emit_fixed_byte_source_byte_to("t1", "t6", &source, byte_index);
                self.emit_stack_load("t4", collection.id * 8);
                self.emit("ld t0, -8(t4)");
                self.emit(format!("li t2, {}", width));
                self.emit("mul t2, t0, t2");
                self.emit("add t4, t4, t2");
                if byte_index <= 2047 {
                    self.emit(format!("sb t1, {}(t4)", byte_index));
                } else {
                    self.emit_large_addi("t0", "t4", byte_index as i64);
                    self.emit("sb t1, 0(t0)");
                }
            }
        }
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit("addi t0, t0, 1");
        self.emit("sd t0, -8(t4)");
        true
    }

    fn emit_collection_extend(&mut self, collection: &IrOperand, slice: &IrOperand) -> Result<()> {
        self.emit("# collection extend_from_slice");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("slice", slice);
        if matches!(collection, IrOperand::Var(var) if self.verified_collection_construction_vectors.contains(&var.id)) {
            self.emit("# cellscript abi: collection extend is covered by create-output vector verifier");
            return Ok(());
        }
        if self.emit_stack_collection_extend(collection, slice) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection extend is not needed for verifier execution");
        self.emit("# cellscript abi: if this path is reached, the source program uses dynamic collections");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_extend(&mut self, collection: &IrOperand, slice: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(width) = operand_fixed_byte_width(slice) else {
            return false;
        };
        let element_width =
            molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).unwrap_or(1);
        if element_width == 0 || width % element_width != 0 {
            return false;
        }
        let element_count = width / element_width;
        if width > RUNTIME_COLLECTION_BUFFER_SIZE {
            return false;
        }
        let Some(source) = self.expected_fixed_byte_source(slice, width) else {
            return false;
        };

        self.emit(format!(
            "# cellscript abi: stack collection extend bytes={} elements={} element_size={}",
            width, element_count, element_width
        ));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit(format!("li t1, {}", element_width));
        self.emit("mul t2, t0, t1");
        self.emit(format!("li t3, {}", RUNTIME_COLLECTION_BUFFER_SIZE));
        self.emit("sub t5, t3, t2");
        self.emit(format!("li t1, {}", width));
        self.emit("sltu t5, t5, t1");
        let capacity_ok = self.fresh_label("stack_collection_extend_capacity_ok");
        self.emit(format!("beqz t5, {}", capacity_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&capacity_ok);

        self.emit_prepare_fixed_byte_source(&source, width, "stack collection extend");
        self.emit(format!("# cellscript abi: stack collection extend copy fixed bytes size={}", width));
        for byte_index in 0..width {
            self.emit_fixed_byte_source_byte_to("t1", "t6", &source, byte_index);
            self.emit_stack_load("t4", collection.id * 8);
            self.emit("ld t0, -8(t4)");
            self.emit(format!("li t2, {}", element_width));
            self.emit("mul t2, t0, t2");
            self.emit("add t4, t4, t2");
            if byte_index <= 2047 {
                self.emit(format!("sb t1, {}(t4)", byte_index));
            } else {
                self.emit_large_addi("t0", "t4", byte_index as i64);
                self.emit("sb t1, 0(t0)");
            }
        }
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit(format!("addi t0, t0, {}", element_count));
        self.emit("sd t0, -8(t4)");
        true
    }

    fn emit_collection_clear(&mut self, collection: &IrOperand) -> Result<()> {
        self.emit("# collection clear");
        self.emit_operand_comment("collection", collection);
        if matches!(collection, IrOperand::Var(var) if self.verified_collection_construction_vectors.contains(&var.id)) {
            self.emit("# cellscript abi: collection clear is covered by create-output vector verifier");
            return Ok(());
        }
        if self.emit_stack_collection_clear(collection) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection clear is not needed for verifier execution");
        self.emit("# cellscript abi: if this path is reached, the source program uses dynamic collections");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_clear(&mut self, collection: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        self.emit("# cellscript abi: stack collection clear");
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("sd zero, -8(t4)");
        true
    }

    fn emit_collection_reverse(&mut self, collection: &IrOperand) -> Result<()> {
        self.emit("# collection reverse");
        self.emit_operand_comment("collection", collection);
        if self.emit_stack_collection_reverse(collection) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection reverse is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_reverse(&mut self, collection: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width == 0 || element_width > RUNTIME_COLLECTION_BUFFER_SIZE {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection reverse element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        let done_label = self.fresh_label("stack_collection_reverse_done");
        self.emit("li t1, 2");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", done_label));

        let left_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let right_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        self.emit_stack_store("zero", left_offset);
        self.emit("addi t0, t0, -1");
        self.emit_stack_store("t0", right_offset);

        let loop_label = self.fresh_label("stack_collection_reverse_loop");
        self.emit_label(&loop_label);
        self.emit_stack_load("t0", left_offset);
        self.emit_stack_load("t1", right_offset);
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", done_label));

        self.emit_stack_load("t4", collection.id * 8);
        self.emit(format!("li t3, {}", element_width));
        self.emit("mul t5, t0, t3");
        self.emit("add t5, t4, t5");
        self.emit("mul t6, t1, t3");
        self.emit("add t6, t4, t6");
        self.emit(format!("# cellscript abi: stack collection reverse swap element_size={}", element_width));
        for byte_index in 0..element_width {
            if byte_index <= 2047 {
                self.emit(format!("lbu t0, {}(t5)", byte_index));
                self.emit(format!("lbu t1, {}(t6)", byte_index));
                self.emit(format!("sb t1, {}(t5)", byte_index));
                self.emit(format!("sb t0, {}(t6)", byte_index));
            } else {
                self.emit_large_addi("t2", "t5", byte_index as i64);
                self.emit_large_addi("t3", "t6", byte_index as i64);
                self.emit("lbu t0, 0(t2)");
                self.emit("lbu t1, 0(t3)");
                self.emit("sb t1, 0(t2)");
                self.emit("sb t0, 0(t3)");
            }
        }
        self.emit_stack_load("t0", left_offset);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", left_offset);
        self.emit_stack_load("t1", right_offset);
        self.emit("addi t1, t1, -1");
        self.emit_stack_store("t1", right_offset);
        self.emit(format!("j {}", loop_label));
        self.emit_label(&done_label);
        true
    }

    fn emit_collection_truncate(&mut self, collection: &IrOperand, len: &IrOperand) -> Result<()> {
        self.emit("# collection truncate");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("len", len);
        if self.emit_stack_collection_truncate(collection, len) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection truncate is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_truncate(&mut self, collection: &IrOperand, len: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }

        self.emit("# cellscript abi: stack collection truncate");
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", len);
        let done_label = self.fresh_label("stack_collection_truncate_done");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", done_label));
        self.emit("sd t1, -8(t4)");
        self.emit_label(&done_label);
        true
    }

    fn emit_collection_swap(&mut self, collection: &IrOperand, left: &IrOperand, right: &IrOperand) -> Result<()> {
        self.emit("# collection swap");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("left", left);
        self.emit_operand_comment("right", right);
        if self.emit_stack_collection_swap(collection, left, right) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection swap is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_swap(&mut self, collection: &IrOperand, left: &IrOperand, right: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width == 0 || element_width > RUNTIME_COLLECTION_BUFFER_SIZE {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection swap element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", left);
        self.emit_operand_to_register("t2", right);

        let left_ok = self.fresh_label("stack_collection_swap_left_ok");
        self.emit("sltu t3, t1, t0");
        self.emit(format!("bnez t3, {}", left_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&left_ok);

        let right_ok = self.fresh_label("stack_collection_swap_right_ok");
        self.emit("sltu t3, t2, t0");
        self.emit(format!("bnez t3, {}", right_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&right_ok);

        self.emit(format!("li t3, {}", element_width));
        self.emit("mul t5, t1, t3");
        self.emit("add t5, t4, t5");
        self.emit("mul t6, t2, t3");
        self.emit("add t6, t4, t6");
        self.emit(format!("# cellscript abi: stack collection swap bytes element_size={}", element_width));
        for byte_index in 0..element_width {
            if byte_index <= 2047 {
                self.emit(format!("lbu t0, {}(t5)", byte_index));
                self.emit(format!("lbu t1, {}(t6)", byte_index));
                self.emit(format!("sb t1, {}(t5)", byte_index));
                self.emit(format!("sb t0, {}(t6)", byte_index));
            } else {
                self.emit_large_addi("t2", "t5", byte_index as i64);
                self.emit_large_addi("t3", "t6", byte_index as i64);
                self.emit("lbu t0, 0(t2)");
                self.emit("lbu t1, 0(t3)");
                self.emit("sb t1, 0(t2)");
                self.emit("sb t0, 0(t3)");
            }
        }
        true
    }

    fn emit_collection_contains(&mut self, dest: &IrVar, collection: &IrOperand, value: &IrOperand) -> Result<()> {
        self.emit("# collection contains");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("value", value);
        if self.emit_stack_collection_contains(dest, collection, value) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection contains is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_contains(&mut self, dest: &IrVar, collection: &IrOperand, value: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(value_width) = self.constructed_byte_vector_part_width(value) else {
            return false;
        };
        let element_width =
            molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).unwrap_or(value_width);
        if element_width == 0 || element_width != value_width {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection contains element_size={}", element_width));
        let index_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        self.emit_stack_store("zero", index_offset);
        self.emit_stack_store("zero", dest.id * 8);
        let loop_label = self.fresh_label("stack_collection_contains_loop");
        let next_label = self.fresh_label("stack_collection_contains_next");
        let found_label = self.fresh_label("stack_collection_contains_found");
        let done_label = self.fresh_label("stack_collection_contains_done");
        self.emit_label(&loop_label);
        self.emit_stack_load("t1", index_offset);
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t2, -8(t4)");
        self.emit(format!("beq t1, t2, {}", done_label));

        if element_width <= 8 && fixed_scalar_operand_width(value).is_some() {
            self.emit(format!("li t2, {}", element_width));
            self.emit("mul t3, t1, t2");
            self.emit("add t4, t4, t3");
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, element_width);
            self.emit_operand_to_register("t5", value);
            self.emit("sub t6, t0, t5");
            self.emit(format!("beqz t6, {}", found_label));
        } else {
            let Some(source) = self.expected_fixed_byte_source(value, element_width) else {
                return false;
            };
            self.emit_prepare_fixed_byte_source(&source, element_width, "stack collection contains");
            for byte_index in 0..element_width {
                self.emit_stack_load("t1", index_offset);
                self.emit_stack_load("t4", collection.id * 8);
                self.emit(format!("li t2, {}", element_width));
                self.emit("mul t3, t1, t2");
                self.emit("add t4, t4, t3");
                if byte_index <= 2047 {
                    self.emit(format!("lbu t0, {}(t4)", byte_index));
                } else {
                    self.emit_large_addi("t2", "t4", byte_index as i64);
                    self.emit("lbu t0, 0(t2)");
                }
                self.emit_fixed_byte_source_byte_to("t5", "t6", &source, byte_index);
                self.emit("sub t0, t0, t5");
                self.emit(format!("bnez t0, {}", next_label));
            }
            self.emit(format!("j {}", found_label));
        }

        self.emit_label(&next_label);
        self.emit_stack_load("t1", index_offset);
        self.emit("addi t1, t1, 1");
        self.emit_stack_store("t1", index_offset);
        self.emit(format!("j {}", loop_label));
        self.emit_label(&found_label);
        self.emit("li t0, 1");
        self.emit_stack_store("t0", dest.id * 8);
        self.emit_label(&done_label);
        true
    }

    fn emit_collection_remove(&mut self, dest: &IrVar, collection: &IrOperand, index: &IrOperand) -> Result<()> {
        self.emit("# collection remove");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("index", index);
        if self.emit_stack_collection_remove(dest, collection, index) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection remove is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_remove(&mut self, dest: &IrVar, collection: &IrOperand, index: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        let dest_scalar = fixed_scalar_width(&dest.ty, Some(element_width)).is_some();
        let dest_fixed_bytes = self.fixed_byte_like_width(&dest.ty).is_some_and(|width| width == element_width);
        if !dest_scalar && !dest_fixed_bytes {
            return false;
        }
        let removed_value_slots = if dest_fixed_bytes { element_width.div_ceil(8) } else { 0 };
        if dest_fixed_bytes && removed_value_slots + 1 > RUNTIME_EXPR_TEMP_SLOTS {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection remove element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", index);

        let bounds_ok = self.fresh_label("stack_collection_remove_bounds_ok");
        self.emit("sltu t2, t1, t0");
        self.emit(format!("bnez t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t1, t2");
        self.emit("add t5, t4, t3");
        if dest_scalar {
            self.emit_unaligned_scalar_load("t5", "t6", "t2", 0, element_width);
            self.emit_stack_store("t6", dest.id * 8);
        } else {
            let removed_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
            self.emit(format!("# cellscript abi: stack collection remove snapshot fixed bytes size={}", element_width));
            for byte_index in 0..element_width {
                if byte_index <= 2047 {
                    self.emit(format!("lbu t6, {}(t5)", byte_index));
                } else {
                    self.emit_large_addi("t2", "t5", byte_index as i64);
                    self.emit("lbu t6, 0(t2)");
                }
                self.emit_sp_addi("t2", removed_offset + byte_index);
                self.emit("sb t6, 0(t2)");
            }
            self.emit_sp_addi("t6", removed_offset);
            self.emit_stack_store("t6", dest.id * 8);
        }

        let index_offset = self.runtime_expr_temp_offset(removed_value_slots).expect("runtime temp slot");
        self.emit_stack_store("t1", index_offset);
        let shift_loop = self.fresh_label("stack_collection_remove_shift_loop");
        let shift_done = self.fresh_label("stack_collection_remove_shift_done");
        self.emit(format!("# cellscript abi: stack collection remove shift element_size={}", element_width));
        self.emit_label(&shift_loop);
        self.emit_stack_load("t1", index_offset);
        self.emit("addi t2, t1, 1");
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit("sltu t3, t2, t0");
        self.emit(format!("beqz t3, {}", shift_done));
        self.emit(format!("li t3, {}", element_width));
        self.emit("mul t5, t1, t3");
        self.emit("add t5, t4, t5");
        self.emit("mul t6, t2, t3");
        self.emit("add t6, t4, t6");
        for byte_index in 0..element_width {
            if byte_index <= 2047 {
                self.emit(format!("lbu t0, {}(t6)", byte_index));
                self.emit(format!("sb t0, {}(t5)", byte_index));
            } else {
                self.emit_large_addi("t0", "t6", byte_index as i64);
                self.emit("lbu t0, 0(t0)");
                self.emit_large_addi("t2", "t5", byte_index as i64);
                self.emit("sb t0, 0(t2)");
            }
        }
        self.emit_stack_load("t1", index_offset);
        self.emit("addi t1, t1, 1");
        self.emit_stack_store("t1", index_offset);
        self.emit(format!("j {}", shift_loop));
        self.emit_label(&shift_done);
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit("addi t0, t0, -1");
        self.emit("sd t0, -8(t4)");
        true
    }

    fn emit_collection_pop(&mut self, dest: &IrVar, collection: &IrOperand) -> Result<()> {
        self.emit("# collection pop");
        self.emit_operand_comment("collection", collection);
        if self.emit_stack_collection_pop(dest, collection) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection pop is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_pop(&mut self, dest: &IrVar, collection: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        let dest_scalar = fixed_scalar_width(&dest.ty, Some(element_width)).is_some();
        let dest_fixed_bytes = self.fixed_byte_like_width(&dest.ty).is_some_and(|width| width == element_width);
        if !dest_scalar && !dest_fixed_bytes {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection pop element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        let bounds_ok = self.fresh_label("stack_collection_pop_bounds_ok");
        self.emit(format!("bnez t0, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit("addi t1, t0, -1");
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t1, t2");
        self.emit("add t5, t4, t3");
        if dest_scalar {
            self.emit_unaligned_scalar_load("t5", "t6", "t2", 0, element_width);
            self.emit_stack_store("t6", dest.id * 8);
        } else {
            self.emit("# cellscript abi: stack collection pop fixed bytes");
            self.emit_stack_store("t5", dest.id * 8);
        }
        self.emit("sd t1, -8(t4)");
        true
    }

    fn emit_collection_insert(&mut self, collection: &IrOperand, index: &IrOperand, value: &IrOperand) -> Result<()> {
        self.emit("# collection insert");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("index", index);
        self.emit_operand_comment("value", value);
        if self.emit_stack_collection_insert(collection, index, value) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection insert is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_insert(&mut self, collection: &IrOperand, index: &IrOperand, value: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(value_width) = self.constructed_byte_vector_part_width(value) else {
            return false;
        };
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width != value_width {
            return false;
        }
        let value_scalar = element_width <= 8 && fixed_scalar_operand_width(value).is_some();
        let fixed_byte_source = if value_scalar {
            None
        } else {
            if element_width > (RUNTIME_EXPR_TEMP_SLOTS - 2) * 8 {
                return false;
            }
            let Some(source) = self.expected_fixed_byte_source(value, element_width) else {
                return false;
            };
            Some(source)
        };

        self.emit(format!("# cellscript abi: stack collection insert element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", index);

        let bounds_ok = self.fresh_label("stack_collection_insert_bounds_ok");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t0, t2");
        self.emit(format!("li t5, {}", RUNTIME_COLLECTION_BUFFER_SIZE));
        self.emit("sub t6, t5, t3");
        self.emit("sltu t6, t6, t2");
        let capacity_ok = self.fresh_label("stack_collection_insert_capacity_ok");
        self.emit(format!("beqz t6, {}", capacity_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&capacity_ok);

        let index_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let current_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        self.emit_stack_store("t1", index_offset);
        self.emit_stack_store("t0", current_offset);
        if let Some(source) = fixed_byte_source.as_ref() {
            self.emit_prepare_fixed_byte_source(source, element_width, "stack collection insert");
            let value_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
            self.emit(format!("# cellscript abi: stack collection insert snapshot fixed bytes size={}", element_width));
            for byte_index in 0..element_width {
                self.emit_fixed_byte_source_byte_to("t1", "t6", source, byte_index);
                self.emit_sp_addi("t6", value_offset + byte_index);
                self.emit("sb t1, 0(t6)");
            }
        }
        let shift_loop = self.fresh_label("stack_collection_insert_shift_loop");
        let shift_done = self.fresh_label("stack_collection_insert_shift_done");
        self.emit(format!("# cellscript abi: stack collection insert shift element_size={}", element_width));
        self.emit_label(&shift_loop);
        self.emit_stack_load("t0", current_offset);
        self.emit_stack_load("t1", index_offset);
        self.emit(format!("beq t0, t1, {}", shift_done));
        self.emit("addi t2, t0, -1");
        self.emit_stack_load("t4", collection.id * 8);
        self.emit(format!("li t3, {}", element_width));
        self.emit("mul t5, t0, t3");
        self.emit("add t5, t4, t5");
        self.emit("mul t6, t2, t3");
        self.emit("add t6, t4, t6");
        if element_width <= 8 {
            self.emit_unaligned_scalar_load("t6", "t0", "t2", 0, element_width);
            match element_width {
                1 => self.emit("sb t0, 0(t5)"),
                2 => self.emit("sh t0, 0(t5)"),
                4 => self.emit("sw t0, 0(t5)"),
                8 => self.emit("sd t0, 0(t5)"),
                _ => return false,
            }
        } else {
            for byte_index in 0..element_width {
                if byte_index <= 2047 {
                    self.emit(format!("lbu t0, {}(t6)", byte_index));
                    self.emit(format!("sb t0, {}(t5)", byte_index));
                } else {
                    self.emit_large_addi("t0", "t6", byte_index as i64);
                    self.emit("lbu t0, 0(t0)");
                    self.emit_large_addi("t2", "t5", byte_index as i64);
                    self.emit("sb t0, 0(t2)");
                }
            }
        }
        self.emit_stack_load("t0", current_offset);
        self.emit("addi t0, t0, -1");
        self.emit_stack_store("t0", current_offset);
        self.emit(format!("j {}", shift_loop));
        self.emit_label(&shift_done);

        self.emit_stack_load("t4", collection.id * 8);
        self.emit_stack_load("t0", index_offset);
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t0, t2");
        self.emit("add t5, t4, t3");
        if value_scalar {
            self.emit_operand_to_register("t1", value);
            match element_width {
                1 => self.emit("sb t1, 0(t5)"),
                2 => self.emit("sh t1, 0(t5)"),
                4 => self.emit("sw t1, 0(t5)"),
                8 => self.emit("sd t1, 0(t5)"),
                _ => return false,
            }
        } else {
            let value_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
            self.emit(format!("# cellscript abi: stack collection insert copy fixed bytes size={}", element_width));
            for byte_index in 0..element_width {
                self.emit_sp_addi("t6", value_offset + byte_index);
                self.emit("lbu t1, 0(t6)");
                if byte_index <= 2047 {
                    self.emit(format!("sb t1, {}(t5)", byte_index));
                } else {
                    self.emit_large_addi("t0", "t5", byte_index as i64);
                    self.emit("sb t1, 0(t0)");
                }
            }
        }
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit("addi t0, t0, 1");
        self.emit("sd t0, -8(t4)");
        true
    }

    fn emit_collection_set(&mut self, collection: &IrOperand, index: &IrOperand, value: &IrOperand) -> Result<()> {
        self.emit("# collection set");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("index", index);
        self.emit_operand_comment("value", value);
        if self.emit_stack_collection_set(collection, index, value) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection set is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_set(&mut self, collection: &IrOperand, index: &IrOperand, value: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(value_width) = self.constructed_byte_vector_part_width(value) else {
            return false;
        };
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width == 0 || element_width > RUNTIME_COLLECTION_BUFFER_SIZE || element_width != value_width {
            return false;
        }
        let value_scalar = element_width <= 8 && fixed_scalar_operand_width(value).is_some();
        let fixed_byte_source = if value_scalar {
            None
        } else {
            let Some(source) = self.expected_fixed_byte_source(value, element_width) else {
                return false;
            };
            Some(source)
        };

        self.emit(format!("# cellscript abi: stack collection set element_size={}", element_width));
        if let Some(source) = fixed_byte_source.as_ref() {
            self.emit_prepare_fixed_byte_source(source, element_width, "stack collection set");
        }
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", index);

        let bounds_ok = self.fresh_label("stack_collection_set_bounds_ok");
        self.emit("sltu t2, t1, t0");
        self.emit(format!("bnez t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t1, t2");
        self.emit("add t5, t4, t3");
        if value_scalar {
            self.emit_operand_to_register("t1", value);
            match element_width {
                1 => self.emit("sb t1, 0(t5)"),
                2 => self.emit("sh t1, 0(t5)"),
                4 => self.emit("sw t1, 0(t5)"),
                8 => self.emit("sd t1, 0(t5)"),
                _ => return false,
            }
        } else {
            let source = fixed_byte_source.as_ref().expect("fixed byte source");
            self.emit(format!("# cellscript abi: stack collection set copy fixed bytes size={}", element_width));
            for byte_index in 0..element_width {
                self.emit_fixed_byte_source_byte_to("t1", "t6", source, byte_index);
                if byte_index <= 2047 {
                    self.emit(format!("sb t1, {}(t5)", byte_index));
                } else {
                    self.emit_large_addi("t0", "t5", byte_index as i64);
                    self.emit("sb t1, 0(t0)");
                }
            }
        }
        true
    }

    fn emit_ckb_fixed_hash_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !is_ckb_fixed_hash_helper(func) {
            return Ok(false);
        }
        self.emit(format!("# call {}", func));
        let Some(dest) = dest else {
            self.emit("# cellscript abi: fail closed because hash helper result has no destination");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: fail closed because hash helper output buffer was not allocated");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        if func == "__ckb_hash_pair" {
            if args.len() != 2 {
                self.emit("# cellscript abi: fail closed because hash_pair needs two inputs");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return Ok(true);
            }
            let Some(left) = self.expected_fixed_byte_source(&args[0], 32) else {
                self.emit("# cellscript abi: fail closed because hash_pair left input is not a 32-byte value");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return Ok(true);
            };
            let Some(right) = self.expected_fixed_byte_source(&args[1], 32) else {
                self.emit("# cellscript abi: fail closed because hash_pair right input is not a 32-byte value");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return Ok(true);
            };
            self.emit_prepare_fixed_byte_source(&left, 32, "hash_pair left input");
            self.emit_prepare_fixed_byte_source(&right, 32, "hash_pair right input");
            if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &left) {
                self.emit("# cellscript abi: fail closed because hash_pair left pointer is not materializable");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return Ok(true);
            }
            if !self.emit_fixed_byte_source_pointer_or_const_to("a1", &right) {
                self.emit("# cellscript abi: fail closed because hash_pair right pointer is not materializable");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return Ok(true);
            }
            self.emit_sp_addi("a2", dest_offset);
            self.emit("call __ckb_hash_pair");
            self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
            self.emit_sp_addi("t0", dest_offset);
            self.emit_stack_store("t0", dest.id * 8);
            return Ok(true);
        }
        if func == "__ckb_hash_blake2b_packed" {
            let Some(arg) = args.first() else {
                self.emit("# cellscript abi: fail closed because hash_blake2b_packed is missing input");
                self.emit_fail(CellScriptRuntimeError::PackedHashPreimageMaterializationUnresolved);
                return Ok(true);
            };
            let Some(width) = operand_fixed_byte_width(arg).or_else(|| match arg {
                IrOperand::Var(var) => self.fixed_byte_like_width(&var.ty),
                _ => None,
            }) else {
                self.emit("# cellscript abi: fail closed because hash_blake2b_packed input has no static packed width");
                self.emit_fail(CellScriptRuntimeError::PackedHashPreimageMaterializationUnresolved);
                return Ok(true);
            };
            let Some(source) = self.expected_fixed_byte_source(arg, width) else {
                self.emit("# cellscript abi: fail closed because hash_blake2b_packed input is not materializable");
                self.emit_fail(CellScriptRuntimeError::PackedHashPreimageMaterializationUnresolved);
                return Ok(true);
            };
            let type_name = match arg {
                IrOperand::Var(var) => named_type_name(&var.ty).map(str::to_string).unwrap_or_else(|| aggregate_type_label(&var.ty)),
                IrOperand::Const(_) => "const".to_string(),
            };
            let mut header = b"CellScriptPackedHashV0\0".to_vec();
            header.extend_from_slice(type_name.as_bytes());
            header.push(0);
            header.extend_from_slice(&(width as u32).to_le_bytes());
            let total_width = header.len() + width;
            if total_width > RUNTIME_SCRATCH_BUFFER_SIZE {
                self.emit("# cellscript abi: fail closed because hash_blake2b_packed preimage exceeds scratch buffer");
                self.emit_fail(CellScriptRuntimeError::PackedHashPreimageMaterializationUnresolved);
                return Ok(true);
            }
            let buffer_offset = self.runtime_scratch_buffer_offset();
            for (index, byte) in header.iter().enumerate() {
                self.emit(format!("li t0, {}", byte));
                self.emit_stack_store_byte("t0", buffer_offset + index);
            }
            self.emit_prepare_fixed_byte_source(&source, width, "hash_blake2b_packed input");
            if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &source) {
                self.emit("# cellscript abi: fail closed because hash_blake2b_packed input pointer is not materializable");
                self.emit_fail(CellScriptRuntimeError::PackedHashPreimageMaterializationUnresolved);
                return Ok(true);
            }
            self.emit_sp_addi("a1", buffer_offset + header.len());
            self.emit(format!("li a2, {}", width));
            self.emit("call __cellscript_memcpy_fixed");
            self.emit_sp_addi("a0", buffer_offset);
            self.emit(format!("li a1, {}", total_width));
            self.emit_sp_addi("a2", dest_offset);
            self.emit("call __ckb_hash_blake2b_var");
            self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
            self.emit_sp_addi("t0", dest_offset);
            self.emit_stack_store("t0", dest.id * 8);
            return Ok(true);
        }
        if func == "__ckb_hash_data_packed" {
            let Some(arg) = args.first() else {
                self.emit("# cellscript abi: fail closed because hash_data_packed is missing input");
                self.emit_fail(CellScriptRuntimeError::PackedHashPreimageMaterializationUnresolved);
                return Ok(true);
            };
            let Some(width) = operand_fixed_byte_width(arg).or_else(|| match arg {
                IrOperand::Var(var) => self.fixed_byte_like_width(&var.ty),
                _ => None,
            }) else {
                self.emit("# cellscript abi: fail closed because hash_data_packed input has no static packed width");
                self.emit_fail(CellScriptRuntimeError::PackedHashPreimageMaterializationUnresolved);
                return Ok(true);
            };
            let Some(source) = self.expected_fixed_byte_source(arg, width) else {
                self.emit("# cellscript abi: fail closed because hash_data_packed input is not materializable");
                self.emit_fail(CellScriptRuntimeError::PackedHashPreimageMaterializationUnresolved);
                return Ok(true);
            };
            self.emit_prepare_fixed_byte_source(&source, width, "hash_data_packed input");
            if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &source) {
                self.emit("# cellscript abi: fail closed because hash_data_packed input pointer is not materializable");
                self.emit_fail(CellScriptRuntimeError::PackedHashPreimageMaterializationUnresolved);
                return Ok(true);
            }
            self.emit(format!("li a1, {}", width));
            self.emit_sp_addi("a2", dest_offset);
            self.emit("call __ckb_hash_blake2b_var");
            self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
            self.emit_sp_addi("t0", dest_offset);
            self.emit_stack_store("t0", dest.id * 8);
            return Ok(true);
        }
        let Some(arg) = args.first() else {
            self.emit("# cellscript abi: fail closed because hash helper is missing input");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(source) = self.expected_fixed_byte_source(arg, 32) else {
            self.emit("# cellscript abi: fail closed because hash helper input is not a 32-byte value");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        self.emit_prepare_fixed_byte_source(&source, 32, "hash_blake2b input");
        if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &source) {
            self.emit("# cellscript abi: fail closed because hash helper input pointer is not materializable");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        }
        self.emit_sp_addi("a1", dest_offset);
        self.emit("call __ckb_hash_blake2b");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    fn emit_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<()> {
        if self.emit_ckb_fixed_hash_call(dest, func, args)? {
            return Ok(());
        }
        if func == "__novaseal_bip340_require_signature" {
            self.emit(format!("# call {} args={}", func, args.len()));
            if args.len() != 3 {
                self.emit("# cellscript abi: fail closed because BIP340 verifier requires message, pubkey, signature");
                self.emit_fail(CellScriptRuntimeError::Bip340MessageMaterializationUnresolved);
                return Ok(());
            }
            let Some(message) = self.expected_fixed_byte_source(&args[0], 32) else {
                self.emit("# cellscript abi: fail closed because BIP340 message is not a 32-byte value");
                self.emit_fail(CellScriptRuntimeError::Bip340MessageMaterializationUnresolved);
                return Ok(());
            };
            let Some(pubkey) = self.expected_fixed_byte_source(&args[1], 32) else {
                self.emit("# cellscript abi: fail closed because BIP340 pubkey is not a 32-byte value");
                self.emit_fail(CellScriptRuntimeError::Bip340PubkeyMaterializationUnresolved);
                return Ok(());
            };
            let Some(signature) = self.expected_fixed_byte_source(&args[2], 64) else {
                self.emit("# cellscript abi: fail closed because BIP340 signature is not a 64-byte value");
                self.emit_fail(CellScriptRuntimeError::Bip340SignatureMaterializationUnresolved);
                return Ok(());
            };
            self.emit_prepare_fixed_byte_source(&message, 32, "novaseal bip340 message");
            self.emit_prepare_fixed_byte_source(&pubkey, 32, "novaseal bip340 pubkey");
            self.emit_prepare_fixed_byte_source(&signature, 64, "novaseal bip340 signature");
            let Some(read_fd_offset) = self.runtime_expr_temp_offset(0) else {
                self.emit_fail(CellScriptRuntimeError::Bip340MessageMaterializationUnresolved);
                return Ok(());
            };
            let Some(write_fd_offset) = self.runtime_expr_temp_offset(1) else {
                self.emit_fail(CellScriptRuntimeError::Bip340MessageMaterializationUnresolved);
                return Ok(());
            };
            let Some(child_pid_offset) = self.runtime_expr_temp_offset(2) else {
                self.emit_fail(CellScriptRuntimeError::Bip340MessageMaterializationUnresolved);
                return Ok(());
            };
            let ipc_buffer_offset = self.runtime_scratch_buffer_offset();
            let ipc_size_offset = self.runtime_scratch_size_offset();
            self.emit("# cellscript abi: NovaSeal BIP340 verifier IPC envelope via VM2 pipe/spawn/wait");
            let pipe_ok = self.fresh_label("novaseal_bip340_pipe_ok");
            self.emit("call __ckb_pipe");
            self.emit(format!("beqz a0, {}", pipe_ok));
            self.emit_fail(CellScriptRuntimeError::Bip340PipeCreateFailed);
            self.emit_label(&pipe_ok);
            self.emit_stack_store("a1", read_fd_offset);
            self.emit_stack_store("a2", write_fd_offset);
            self.emit("# cellscript abi: materialize cellscript-btc-bip340-ipc-v0 envelope in scratch");
            for (index, byte) in b"NSBV0IPC".iter().enumerate() {
                self.emit(format!("li t0, {}", byte));
                self.emit_stack_store_byte("t0", ipc_buffer_offset + index);
            }
            for (index, byte) in [0u8, 0, 1, 0, 0, 0, 0, 0].iter().enumerate() {
                self.emit(format!("li t0, {}", byte));
                self.emit_stack_store_byte("t0", ipc_buffer_offset + 8 + index);
            }
            if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &message) {
                self.emit_fail(CellScriptRuntimeError::Bip340MessageMaterializationUnresolved);
                return Ok(());
            }
            self.emit_sp_addi("a1", ipc_buffer_offset + 16);
            self.emit("li a2, 32");
            self.emit("call __cellscript_memcpy_fixed");
            if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &pubkey) {
                self.emit_fail(CellScriptRuntimeError::Bip340PubkeyMaterializationUnresolved);
                return Ok(());
            }
            self.emit_sp_addi("a1", ipc_buffer_offset + 48);
            self.emit("li a2, 32");
            self.emit("call __cellscript_memcpy_fixed");
            if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &signature) {
                self.emit_fail(CellScriptRuntimeError::Bip340SignatureMaterializationUnresolved);
                return Ok(());
            }
            self.emit_sp_addi("a1", ipc_buffer_offset + 80);
            self.emit("li a2, 64");
            self.emit("call __cellscript_memcpy_fixed");
            self.emit("# cellscript abi: spawn manifest-bound CellDep#0 verifier with prepared read fd inherited");
            self.emit("li a0, 0");
            self.emit_stack_load("a1", read_fd_offset);
            self.emit("call __ckb_spawn_with_fd1");
            let spawn_ok = self.fresh_label("novaseal_bip340_spawn_ok");
            self.emit(format!("beqz a0, {}", spawn_ok));
            self.emit_fail(CellScriptRuntimeError::Bip340SpawnFailed);
            self.emit_label(&spawn_ok);
            self.emit_stack_store("a1", child_pid_offset);
            self.emit("# cellscript abi: BIP340 IPC write canonical 144-byte envelope");
            self.emit("li t0, 144");
            self.emit_stack_store("t0", ipc_size_offset);
            self.emit_stack_load("a0", write_fd_offset);
            self.emit_sp_addi("a1", ipc_buffer_offset);
            self.emit_sp_addi("a2", ipc_size_offset);
            self.emit(format!("li a7, {}", ckb_abi::syscall::WRITE));
            self.emit("ecall");
            let write_ok = self.fresh_label("novaseal_bip340_write_ok");
            self.emit(format!("beqz a0, {}", write_ok));
            self.emit_fail(CellScriptRuntimeError::Bip340MessageWriteFailed);
            self.emit_label(&write_ok);
            self.emit_stack_load("a0", write_fd_offset);
            self.emit("call __ckb_close");
            let close_ok = self.fresh_label("novaseal_bip340_close_ok");
            self.emit(format!("beqz a0, {}", close_ok));
            self.emit_fail(CellScriptRuntimeError::Bip340VerifierReadFailed);
            self.emit_label(&close_ok);
            self.emit_stack_load("a0", child_pid_offset);
            self.emit("call __ckb_wait");
            let wait_ok = self.fresh_label("novaseal_bip340_wait_ok");
            self.emit(format!("beqz a0, {}", wait_ok));
            self.emit_fail(CellScriptRuntimeError::Bip340ChildRejected);
            self.emit_label(&wait_ok);
            return Ok(());
        }
        if func.contains("::") {
            return Err(CompileError::new(
                format!("qualified function call '{}' reached codegen without IR label normalization; this is a compiler bug", func),
                crate::error::Span::default(),
            ));
        }
        self.emit(format!("# call {}", func));

        if self.emit_runtime_fixed_hash_requirement_call(func, args)? {
            return Ok(());
        }
        if self.emit_runtime_cell_script_args_exact_requirement_call(func, args)? {
            return Ok(());
        }
        if self.emit_runtime_cell_script_hash_type_requirement_call(func, args)? {
            return Ok(());
        }
        if self.emit_runtime_input_out_point_requirement_call(func, args)? {
            return Ok(());
        }
        if self.emit_runtime_xudt_type_args_requirement_call(func, args)? {
            return Ok(());
        }
        if self.emit_runtime_xudt_group_amount_delta_call(func, args)? {
            return Ok(());
        }
        if self.emit_runtime_metapoint_filtered_pair_call(func, args)? {
            return Ok(());
        }
        if self.emit_runtime_c256_product_requirement_call(func, args)? {
            return Ok(());
        }
        if self.emit_runtime_c256_sum2_product_requirement_call(func, args)? {
            return Ok(());
        }
        if self.emit_runtime_current_script_hash_call(dest, func, args)? {
            return Ok(());
        }
        if self.emit_runtime_input_out_point_tx_hash_call(dest, func, args)? {
            return Ok(());
        }
        if self.emit_runtime_cell_data_hash_at_call(dest, func, args)? {
            return Ok(());
        }
        if self.emit_runtime_cell_script_hash_field_call(dest, func, args)? {
            return Ok(());
        }
        if self.emit_runtime_witness_hash_call(dest, func, args)? {
            return Ok(());
        }

        let abi = self.callable_abis.get(func).cloned();
        let outgoing_stack_arg_bytes = align_stack_arg_bytes(call_abi_arg_count(abi.as_ref(), args).saturating_sub(8) * 8);
        let mut abi_index = 0usize;
        for (arg_index, arg) in args.iter().enumerate() {
            if let Some(abi) = &abi {
                if let Some(param) = abi.params.get(arg_index) {
                    let needs_type_hash = abi.type_hash_param_indices.contains(&arg_index);
                    if !self.emit_call_param_arg(func, param, needs_type_hash, &mut abi_index, arg, outgoing_stack_arg_bytes) {
                        return Ok(());
                    }
                    continue;
                }
            }
            if !self.emit_call_scalar_arg(func, &format!("arg{}", arg_index), &mut abi_index, arg, outgoing_stack_arg_bytes) {
                return Ok(());
            }
        }

        if outgoing_stack_arg_bytes > 0 {
            self.emit(format!("# cellscript abi: reserve {} bytes for outgoing stack call arguments", outgoing_stack_arg_bytes));
            self.emit_large_addi("sp", "sp", -(outgoing_stack_arg_bytes as i64));
        }
        self.emit(format!("call {}", func));
        if outgoing_stack_arg_bytes > 0 {
            self.emit_large_addi("sp", "sp", outgoing_stack_arg_bytes as i64);
        }

        if is_runtime_scalar_failclosed_call(func) {
            let ok_label = self.fresh_label("runtime_scalar_ok");
            self.emit("# cellscript abi: scalar runtime helper status check (a1 == 0)");
            self.emit(format!("beqz a1, {}", ok_label));
            self.emit("addi a0, a1, 0");
            self.emit_epilogue();
            self.emit_label(&ok_label);
        }

        if dest.is_none() && is_void_runtime_requirement_call(func) {
            let ok_label = self.fresh_label("runtime_requirement_ok");
            self.emit(format!("beqz a0, {}", ok_label));
            self.emit_epilogue();
            self.emit_label(&ok_label);
        }

        if let Some(d) = dest {
            if d.ty == IrType::U128 {
                if let Some(offset) = self.u128_value_offsets.get(&d.id).copied() {
                    self.emit("# cellscript abi: receive u128 return from a0(low)/a1(high)");
                    self.emit_stack_store("a0", offset);
                    self.emit_stack_store("a1", offset + 8);
                    self.emit_store_u128_pointer_for_var(d.id, offset);
                } else {
                    self.emit("# cellscript abi: u128 call destination has no storage; fail closed");
                    self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                }
            } else if let IrType::Tuple(items) = &d.ty {
                self.emit_stack_store("a0", d.id * 8);
                for index in 0..items.len().min(8) {
                    let field = index.to_string();
                    if let Some(field_var_id) = self.tuple_call_return_field_slots.get(&(d.id, field)).copied() {
                        self.emit_stack_store(&format!("a{}", index), field_var_id * 8);
                    }
                }
            } else {
                self.emit_stack_store("a0", d.id * 8);
            }
        }

        Ok(())
    }

    fn emit_runtime_current_script_hash_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if func != "__ckb_current_script_hash" {
            return Ok(false);
        }
        let Some(dest) = dest else {
            return Ok(false);
        };
        if !args.is_empty() || dest.ty != IrType::Hash {
            return Ok(false);
        }
        let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: current script hash destination has no 32-byte storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(buffer_offset) = self.cell_buffer_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: current script hash destination has no buffer storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };

        self.emit("# cellscript abi: load current script hash into addressable Hash");
        self.emit("li t0, 32");
        self.emit_stack_store("t0", size_offset);
        self.emit_sp_addi("a0", buffer_offset);
        self.emit_sp_addi("a1", size_offset);
        self.emit("call __ckb_current_script_hash");
        let ok_label = self.fresh_label("current_script_hash_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    fn emit_runtime_input_out_point_tx_hash_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if func != "__ckb_input_out_point_tx_hash" {
            return Ok(false);
        }
        let Some(dest) = dest else {
            return Ok(false);
        };
        if args.len() != 1 || dest.ty != IrType::Hash {
            return Ok(false);
        }
        let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: input OutPoint tx hash destination has no 32-byte storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(buffer_offset) = self.cell_buffer_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: input OutPoint tx hash destination has no buffer storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };

        self.emit("# cellscript abi: load SourceView input OutPoint tx hash into addressable Hash");
        self.emit("li t0, 32");
        self.emit_stack_store("t0", size_offset);
        self.emit_operand_to_register("a0", &args[0]);
        self.emit_sp_addi("a1", buffer_offset);
        self.emit_sp_addi("a2", size_offset);
        self.emit("call __ckb_input_out_point_tx_hash");
        let ok_label = self.fresh_label("input_out_point_tx_hash_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    fn emit_runtime_cell_script_hash_field_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(
            func,
            "__ckb_cell_lock_hash"
                | "__ckb_cell_type_hash"
                | "__ckb_cell_data_hash"
                | "__ckb_cell_lock_code_hash"
                | "__ckb_cell_type_code_hash"
                | "__ckb_cell_lock_args_hash"
                | "__ckb_cell_type_args_hash"
        ) {
            return Ok(false);
        }
        let Some(dest) = dest else {
            return Ok(false);
        };
        if args.len() != 1 || dest.ty != IrType::Hash {
            return Ok(false);
        }
        let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: ScriptRef hash destination has no 32-byte storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(buffer_offset) = self.cell_buffer_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: ScriptRef hash destination has no buffer storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };

        self.emit("# cellscript abi: load SourceView ScriptRef hash field into addressable Hash");
        self.emit("li t0, 32");
        self.emit_stack_store("t0", size_offset);
        self.emit_operand_to_register("a0", &args[0]);
        self.emit_sp_addi("a1", buffer_offset);
        self.emit_sp_addi("a2", size_offset);
        self.emit(format!("call {}", func));
        let ok_label = self.fresh_label("script_ref_hash_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    fn emit_runtime_cell_data_hash_at_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if func != "__ckb_cell_data_hash_at" {
            return Ok(false);
        }
        let Some(dest) = dest else {
            return Ok(false);
        };
        if args.len() != 2 || dest.ty != IrType::Hash {
            return Ok(false);
        }
        let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: cell data hash-at destination has no 32-byte storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(buffer_offset) = self.cell_buffer_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: cell data hash-at destination has no buffer storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };

        self.emit("# cellscript abi: load 32 bytes from SourceView cell data into addressable Hash");
        self.emit("li t0, 32");
        self.emit_stack_store("t0", size_offset);
        self.emit_operand_to_register("a0", &args[0]);
        self.emit_operand_to_register("a1", &args[1]);
        self.emit_sp_addi("a2", buffer_offset);
        self.emit_sp_addi("a3", size_offset);
        self.emit("call __ckb_cell_data_hash_at");
        let ok_label = self.fresh_label("cell_data_hash_at_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    fn emit_runtime_witness_hash_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(func, "__ckb_witness_raw" | "__ckb_witness_lock" | "__ckb_witness_input_type" | "__ckb_witness_output_type") {
            return Ok(false);
        }
        let Some(dest) = dest else {
            return Ok(false);
        };
        if args.len() != 1 || dest.ty != IrType::Hash {
            return Ok(false);
        }
        let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: witness hash destination has no 32-byte storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(buffer_offset) = self.cell_buffer_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: witness hash destination has no buffer storage; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };

        self.emit("# cellscript abi: load witness hash into addressable Hash");
        self.emit("li t0, 32");
        self.emit_stack_store("t0", size_offset);
        self.emit_operand_to_register("a0", &args[0]);
        self.emit_sp_addi("a1", buffer_offset);
        self.emit(format!("call {}", func));
        let ok_label = self.fresh_label("witness_hash_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    fn emit_runtime_fixed_hash_requirement_call(&mut self, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(
            func,
            "__ckb_require_cell_lock_hash"
                | "__ckb_require_cell_type_hash"
                | "__ckb_require_cell_lock_args_hash"
                | "__ckb_require_cell_type_args_hash"
                | "__ckb_require_cell_lock_args_prefix_hash"
                | "__ckb_require_cell_type_args_prefix_hash"
                | "__ckb_require_cell_lock_args_suffix_hash"
                | "__ckb_require_cell_type_args_suffix_hash"
                | "__ckb_require_input_out_point_tx_hash"
                | "__xudt_require_owner_mode_input_type"
        ) {
            return Ok(false);
        }
        if args.len() != 2 {
            return Ok(false);
        }

        let expected = self.expected_fixed_byte_source(&args[1], 32);
        match expected {
            Some(ExpectedFixedByteSource::Const(bytes)) => {
                let size_offset = self.runtime_scratch_size_offset();
                let buffer_offset = self.runtime_scratch_buffer_offset();
                let hash: [u8; 32] = bytes.as_slice().try_into().expect("expected fixed hash width");
                self.emit_store_fixed_byte_const_to_scratch(&IrOperand::Const(IrConst::Hash(hash)), size_offset, buffer_offset, 32);
                self.emit_sp_addi("a1", buffer_offset);
                self.emit("li a2, 32");
            }
            Some(source) => {
                self.emit_prepare_fixed_byte_source(&source, 32, "runtime expected hash");
                if self.emit_fixed_byte_source_pointer_to("a1", &source) {
                    self.emit("li a2, 32");
                } else {
                    self.emit("# cellscript abi: runtime expected hash source is not addressable; pass null to fail closed");
                    self.emit("li a1, 0");
                    self.emit("li a2, 0");
                }
            }
            None => {
                self.emit("# cellscript abi: runtime expected hash source is unavailable; pass null to fail closed");
                self.emit("li a1, 0");
                self.emit("li a2, 0");
            }
        }

        self.emit_operand_to_register("a0", &args[0]);
        self.emit("call ".to_string() + func);
        let ok_label = self.fresh_label("runtime_hash_requirement_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        Ok(true)
    }

    fn emit_runtime_cell_script_hash_type_requirement_call(&mut self, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(func, "__ckb_require_cell_lock_script_hash_type" | "__ckb_require_cell_type_script_hash_type") {
            return Ok(false);
        }
        if args.len() != 3 {
            return Ok(false);
        }

        let expected = self.expected_fixed_byte_source(&args[1], 32);
        match expected {
            Some(ExpectedFixedByteSource::Const(bytes)) => {
                let size_offset = self.runtime_scratch_size_offset();
                let buffer_offset = self.runtime_scratch_buffer_offset();
                let hash: [u8; 32] = bytes.as_slice().try_into().expect("expected fixed hash width");
                self.emit_store_fixed_byte_const_to_scratch(&IrOperand::Const(IrConst::Hash(hash)), size_offset, buffer_offset, 32);
                self.emit_sp_addi("a1", buffer_offset);
                self.emit("li a2, 32");
            }
            Some(source) => {
                self.emit_prepare_fixed_byte_source(&source, 32, "runtime expected Script code hash");
                if self.emit_fixed_byte_source_pointer_to("a1", &source) {
                    self.emit("li a2, 32");
                } else {
                    self.emit(
                        "# cellscript abi: runtime expected Script code hash source is not addressable; pass null to fail closed",
                    );
                    self.emit("li a1, 0");
                    self.emit("li a2, 0");
                }
            }
            None => {
                self.emit("# cellscript abi: runtime expected Script code hash is unavailable; pass null to fail closed");
                self.emit("li a1, 0");
                self.emit("li a2, 0");
            }
        }

        self.emit_operand_to_register("a0", &args[0]);
        self.emit_operand_to_register("a3", &args[2]);
        self.emit("call ".to_string() + func);
        let ok_label = self.fresh_label("runtime_script_identity_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        Ok(true)
    }

    fn emit_runtime_cell_script_args_exact_requirement_call(&mut self, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(func, "__ckb_require_cell_lock_args_exact" | "__ckb_require_cell_type_args_exact") {
            return Ok(false);
        }
        if args.len() != 2 {
            return Ok(false);
        }
        let Some(width) = operand_fixed_byte_width(&args[1]) else {
            self.emit("# cellscript abi: runtime expected Script args source has no fixed byte width; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(expected) = self.expected_fixed_byte_source(&args[1], width) else {
            self.emit("# cellscript abi: runtime expected Script args source is unavailable; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };

        match expected {
            ExpectedFixedByteSource::Const(bytes) => {
                let size_offset = self.runtime_scratch_size_offset();
                let buffer_offset = self.runtime_scratch_buffer_offset();
                self.emit_store_fixed_byte_const_to_scratch(
                    &IrOperand::Const(IrConst::Array(bytes.into_iter().map(IrConst::U8).collect())),
                    size_offset,
                    buffer_offset,
                    width,
                );
                self.emit_sp_addi("a1", buffer_offset);
            }
            source => {
                self.emit_prepare_fixed_byte_source(&source, width, "runtime expected Script args");
                if !self.emit_fixed_byte_source_pointer_to("a1", &source) {
                    self.emit("# cellscript abi: runtime expected Script args source is not addressable; fail closed");
                    self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                    return Ok(true);
                }
            }
        }

        self.emit_operand_to_register("a0", &args[0]);
        self.emit(format!("li a2, {}", width));
        self.emit("call ".to_string() + func);
        let ok_label = self.fresh_label("runtime_script_args_exact_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        Ok(true)
    }

    fn emit_runtime_input_out_point_requirement_call(&mut self, func: &str, args: &[IrOperand]) -> Result<bool> {
        if func != "__ckb_require_input_out_point" {
            return Ok(false);
        }
        if args.len() != 3 {
            return Ok(false);
        }

        let expected = self.expected_fixed_byte_source(&args[1], 32);
        match expected {
            Some(ExpectedFixedByteSource::Const(bytes)) => {
                let size_offset = self.runtime_scratch_size_offset();
                let buffer_offset = self.runtime_scratch_buffer_offset();
                let hash: [u8; 32] = bytes.as_slice().try_into().expect("expected fixed hash width");
                self.emit_store_fixed_byte_const_to_scratch(&IrOperand::Const(IrConst::Hash(hash)), size_offset, buffer_offset, 32);
                self.emit_sp_addi("a1", buffer_offset);
                self.emit("li a2, 32");
            }
            Some(source) => {
                self.emit_prepare_fixed_byte_source(&source, 32, "runtime expected input out point tx hash");
                if self.emit_fixed_byte_source_pointer_to("a1", &source) {
                    self.emit("li a2, 32");
                } else {
                    self.emit(
                        "# cellscript abi: runtime expected input out point hash source is not addressable; pass null to fail closed",
                    );
                    self.emit("li a1, 0");
                    self.emit("li a2, 0");
                }
            }
            None => {
                self.emit("# cellscript abi: runtime expected input out point hash source is unavailable; pass null to fail closed");
                self.emit("li a1, 0");
                self.emit("li a2, 0");
            }
        }

        self.emit_operand_to_register("a3", &args[2]);
        self.emit_operand_to_register("a0", &args[0]);
        self.emit("call __ckb_require_input_out_point");
        let ok_label = self.fresh_label("runtime_input_out_point_requirement_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        Ok(true)
    }

    fn emit_runtime_xudt_type_args_requirement_call(&mut self, func: &str, args: &[IrOperand]) -> Result<bool> {
        if func != "__xudt_require_owner_mode_type_args" {
            return Ok(false);
        }
        if args.len() != 3 {
            return Ok(false);
        }

        let expected = self.expected_fixed_byte_source(&args[1], 32);
        match expected {
            Some(ExpectedFixedByteSource::Const(bytes)) => {
                let size_offset = self.runtime_scratch_size_offset();
                let buffer_offset = self.runtime_scratch_buffer_offset();
                let hash: [u8; 32] = bytes.as_slice().try_into().expect("expected fixed hash width");
                self.emit_store_fixed_byte_const_to_scratch(&IrOperand::Const(IrConst::Hash(hash)), size_offset, buffer_offset, 32);
                self.emit_sp_addi("a1", buffer_offset);
                self.emit("li a2, 32");
            }
            Some(source) => {
                self.emit_prepare_fixed_byte_source(&source, 32, "runtime expected xUDT owner hash");
                if self.emit_fixed_byte_source_pointer_to("a1", &source) {
                    self.emit("li a2, 32");
                } else {
                    self.emit("# cellscript abi: runtime xUDT owner hash source is not addressable; pass null to fail closed");
                    self.emit("li a1, 0");
                    self.emit("li a2, 0");
                }
            }
            None => {
                self.emit("# cellscript abi: runtime xUDT owner hash source is unavailable; pass null to fail closed");
                self.emit("li a1, 0");
                self.emit("li a2, 0");
            }
        }

        self.emit_operand_to_register("a0", &args[0]);
        self.emit_operand_to_register("a3", &args[2]);
        self.emit("call __xudt_require_owner_mode_type_args");
        let ok_label = self.fresh_label("runtime_xudt_args_requirement_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        Ok(true)
    }

    fn emit_runtime_xudt_group_amount_delta_call(&mut self, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(func, "__xudt_require_group_amount_minted" | "__xudt_require_group_amount_burned") {
            return Ok(false);
        }
        if args.len() != 1 {
            return Ok(false);
        }

        let source = self.expected_fixed_byte_source(&args[0], 16);
        match source {
            Some(ExpectedFixedByteSource::Const(bytes)) => {
                let value = u128::from_le_bytes(bytes.as_slice().try_into().expect("expected fixed u128 width"));
                let buffer_offset = self.runtime_scratch_buffer_offset();
                self.emit_store_fixed_byte_const_to_scratch(
                    &IrOperand::Const(IrConst::U128(value)),
                    self.runtime_scratch_size_offset(),
                    buffer_offset,
                    16,
                );
                self.emit_sp_addi("a0", buffer_offset);
            }
            Some(source) => {
                self.emit_prepare_fixed_byte_source(&source, 16, "runtime xUDT group amount delta");
                if !self.emit_fixed_byte_source_pointer_to("a0", &source) {
                    self.emit("# cellscript abi: xUDT group amount delta is not addressable; pass null to fail closed");
                    self.emit("li a0, 0");
                }
            }
            None => {
                self.emit("# cellscript abi: xUDT group amount delta is unavailable; pass null to fail closed");
                self.emit("li a0, 0");
            }
        }

        self.emit("call ".to_string() + func);
        let ok_label = self.fresh_label("runtime_xudt_delta_requirement_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        Ok(true)
    }

    fn emit_runtime_metapoint_filtered_pair_call(&mut self, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(
            func,
            "__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered"
                | "__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered"
        ) {
            return Ok(false);
        }
        if args.len() != 4 {
            return Ok(false);
        }

        let expected = self.expected_fixed_byte_source(&args[2], 32);
        match expected {
            Some(ExpectedFixedByteSource::Const(bytes)) => {
                let size_offset = self.runtime_scratch_size_offset();
                let buffer_offset = self.runtime_scratch_buffer_offset();
                let hash: [u8; 32] = bytes.as_slice().try_into().expect("expected fixed hash width");
                self.emit_store_fixed_byte_const_to_scratch(&IrOperand::Const(IrConst::Hash(hash)), size_offset, buffer_offset, 32);
                self.emit_sp_addi("a2", buffer_offset);
                self.emit("li a3, 32");
            }
            Some(source) => {
                self.emit_prepare_fixed_byte_source(&source, 32, "runtime filtered MetaPoint related type hash");
                if self.emit_fixed_byte_source_pointer_to("a2", &source) {
                    self.emit("li a3, 32");
                } else {
                    self.emit("# cellscript abi: filtered MetaPoint expected type hash is not addressable; pass null to fail closed");
                    self.emit("li a2, 0");
                    self.emit("li a3, 0");
                }
            }
            None => {
                self.emit("# cellscript abi: filtered MetaPoint expected type hash is unavailable; pass null to fail closed");
                self.emit("li a2, 0");
                self.emit("li a3, 0");
            }
        }

        self.emit_operand_to_register("a0", &args[0]);
        self.emit_operand_to_register("a1", &args[1]);
        self.emit_operand_to_register("a4", &args[3]);
        self.emit("call ".to_string() + func);
        let ok_label = self.fresh_label("runtime_metapoint_filtered_pair_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        Ok(true)
    }

    fn emit_runtime_c256_product_requirement_call(&mut self, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(func, "__c256_require_u128_product_lte" | "__c256_require_u128_product_eq") {
            return Ok(false);
        }
        if args.len() != 4 {
            return Ok(false);
        }

        let scratch_base = self.runtime_scratch_buffer_offset();
        for (index, (register, arg)) in ["a0", "a1", "a2", "a3"].into_iter().zip(args.iter()).enumerate() {
            let source = self.expected_fixed_byte_source(arg, 16);
            match source {
                Some(ExpectedFixedByteSource::Const(bytes)) => {
                    let value = u128::from_le_bytes(bytes.as_slice().try_into().expect("expected fixed u128 width"));
                    let buffer_offset = scratch_base + index * 16;
                    self.emit_store_fixed_byte_const_to_scratch(
                        &IrOperand::Const(IrConst::U128(value)),
                        self.runtime_scratch_size_offset(),
                        buffer_offset,
                        16,
                    );
                    self.emit_sp_addi(register, buffer_offset);
                }
                Some(source) => {
                    self.emit_prepare_fixed_byte_source(&source, 16, "runtime c256 u128 product operand");
                    if !self.emit_fixed_byte_source_pointer_to(register, &source) {
                        self.emit(format!(
                            "# cellscript abi: c256 product operand {} is not addressable; pass null to fail closed",
                            index
                        ));
                        self.emit(format!("li {}, 0", register));
                    }
                }
                None => {
                    self.emit(format!("# cellscript abi: c256 product operand {} is unavailable; pass null to fail closed", index));
                    self.emit(format!("li {}, 0", register));
                }
            }
        }

        self.emit("call ".to_string() + func);
        let ok_label = self.fresh_label("runtime_c256_requirement_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        Ok(true)
    }

    fn emit_runtime_c256_sum2_product_requirement_call(&mut self, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(func, "__c256_require_u128_sum2_products_lte" | "__c256_require_u128_sum2_products_eq") {
            return Ok(false);
        }
        if args.len() != 8 {
            return Ok(false);
        }

        let scratch_base = self.runtime_scratch_buffer_offset();
        for (index, (register, arg)) in ["a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"].into_iter().zip(args.iter()).enumerate() {
            let source = self.expected_fixed_byte_source(arg, 16);
            match source {
                Some(ExpectedFixedByteSource::Const(bytes)) => {
                    let value = u128::from_le_bytes(bytes.as_slice().try_into().expect("expected fixed u128 width"));
                    let buffer_offset = scratch_base + index * 16;
                    self.emit_store_fixed_byte_const_to_scratch(
                        &IrOperand::Const(IrConst::U128(value)),
                        self.runtime_scratch_size_offset(),
                        buffer_offset,
                        16,
                    );
                    self.emit_sp_addi(register, buffer_offset);
                }
                Some(source) => {
                    self.emit_prepare_fixed_byte_source(&source, 16, "runtime c256 sum-product operand");
                    if !self.emit_fixed_byte_source_pointer_to(register, &source) {
                        self.emit(format!(
                            "# cellscript abi: c256 sum-product operand {} is not addressable; pass null to fail closed",
                            index
                        ));
                        self.emit(format!("li {}, 0", register));
                    }
                }
                None => {
                    self.emit(format!(
                        "# cellscript abi: c256 sum-product operand {} is unavailable; pass null to fail closed",
                        index
                    ));
                    self.emit(format!("li {}, 0", register));
                }
            }
        }

        self.emit("call ".to_string() + func);
        let ok_label = self.fresh_label("runtime_c256_sum_requirement_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_epilogue();
        self.emit_label(&ok_label);
        Ok(true)
    }

    fn emit_call_param_arg(
        &mut self,
        func: &str,
        param: &IrParam,
        needs_type_hash: bool,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        if named_type_name(&param.ty).is_some() {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} pointer={} length={}",
                func,
                param.name,
                abi_arg_label(*abi_index),
                abi_arg_label(*abi_index + 1)
            ));
            if !self.emit_call_pointer_arg(func, &param.name, abi_index, arg, None, outgoing_stack_arg_bytes) {
                return false;
            }
            if !self.emit_call_length_arg(func, &param.name, abi_index, arg, CallLengthKind::Schema, outgoing_stack_arg_bytes) {
                return false;
            }
            if needs_type_hash {
                self.emit(format!(
                    "# cellscript abi: call {} schema param {} type_hash pointer={} length={} size=32",
                    func,
                    param.name,
                    abi_arg_label(*abi_index),
                    abi_arg_label(*abi_index + 1)
                ));
                if !self.emit_call_type_hash_pointer_arg(func, &param.name, abi_index, arg, outgoing_stack_arg_bytes) {
                    return false;
                }
                if !self.emit_call_type_hash_length_arg(func, &param.name, abi_index, arg, outgoing_stack_arg_bytes) {
                    return false;
                }
            }
            return true;
        }

        let fixed_pointer_width = fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty));
        if let Some(width) = fixed_pointer_width {
            self.emit(format!(
                "# cellscript abi: call {} fixed-byte param {} pointer={} length={} size={}",
                func,
                param.name,
                abi_arg_label(*abi_index),
                abi_arg_label(*abi_index + 1),
                width
            ));
            if !self.emit_call_pointer_arg(func, &param.name, abi_index, arg, Some(width), outgoing_stack_arg_bytes) {
                return false;
            }
            if !self.emit_call_length_arg(func, &param.name, abi_index, arg, CallLengthKind::FixedBytes, outgoing_stack_arg_bytes) {
                return false;
            }
            return true;
        }

        self.emit_call_scalar_arg(func, &param.name, abi_index, arg, outgoing_stack_arg_bytes)
    }

    fn emit_call_scalar_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        self.emit(format!("# cellscript abi: call {} scalar {} -> {}", func, label, register));
        self.emit_operand_to_register(&register, arg);
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_pointer_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        const_width: Option<usize>,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        if const_width.is_some() && matches!(arg, IrOperand::Const(_)) {
            self.emit(format!(
                "# cellscript abi: call {} pointer param {} uses a constant unsupported by the call ABI; pass null pointer",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        } else {
            self.emit_operand_to_register(&register, arg);
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_length_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        kind: CallLengthKind,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        let size_offset = match (arg, kind) {
            (IrOperand::Var(var), CallLengthKind::Schema) => self.schema_pointer_size_offsets.get(&var.id).copied(),
            (IrOperand::Var(var), CallLengthKind::FixedBytes) => self.fixed_byte_param_size_offsets.get(&var.id).copied(),
            _ => None,
        };
        if let Some(size_offset) = size_offset {
            self.emit_stack_load(&register, size_offset);
        } else if let CallLengthKind::FixedBytes = kind {
            if matches!(arg, IrOperand::Const(_)) {
                self.emit(format!(
                    "# cellscript abi: call {} fixed-byte const param {} has no materialized pointer; pass zero length to fail closed",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            } else {
                self.emit(format!(
                    "# cellscript abi: call {} fixed-byte param {} has no tracked ABI length; pass zero length to fail closed",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            }
        } else {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} has no tracked ABI length; pass zero length to fail closed",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_type_hash_pointer_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        if let IrOperand::Var(var) = arg {
            if let Some(pointer_offset) = self.param_type_hash_pointer_offsets.get(&var.id).copied() {
                self.emit_stack_load(&register, pointer_offset);
            } else {
                self.emit(format!(
                    "# cellscript abi: call {} schema param {} has no tracked TypeHash pointer; pass null pointer",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            }
        } else {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} TypeHash source is not a variable; pass null pointer",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_type_hash_length_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        if let IrOperand::Var(var) = arg {
            if let Some(size_offset) = self.param_type_hash_size_offsets.get(&var.id).copied() {
                self.emit_stack_load(&register, size_offset);
            } else {
                self.emit(format!(
                    "# cellscript abi: call {} schema param {} has no tracked TypeHash length; pass zero length to fail closed",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            }
        } else {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} TypeHash length source is not a variable; pass zero length",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_outgoing_call_stack_arg_store(&mut self, register: &str, abi_index: usize, outgoing_stack_arg_bytes: usize) {
        if abi_index < 8 {
            return;
        }
        let stack_slot_offset = (abi_index - 8) * 8;
        let offset = i64::try_from(stack_slot_offset).expect("call stack slot should fit in i64")
            - i64::try_from(outgoing_stack_arg_bytes).expect("call stack argument area should fit in i64");
        self.emit(format!(
            "# cellscript abi: stage outgoing stack arg{} at pre-call sp{}{}",
            abi_index,
            if offset < 0 { "" } else { "+" },
            offset
        ));
        self.emit_sp_store_signed(register, offset);
    }

    fn emit_sp_store_signed(&mut self, register: &str, offset: i64) {
        if small_signed_immediate(offset) {
            self.emit(format!("sd {}, {}(sp)", register, offset));
        } else {
            let scratch = scratch_register_avoiding(&[register]);
            self.emit(format!("li {}, {}", scratch, offset));
            self.emit(format!("add {}, sp, {}", scratch, scratch));
            self.emit(format!("sd {}, 0({})", register, scratch));
        }
    }

    fn call_abi_register(&self, abi_index: usize) -> String {
        if abi_index < 8 {
            format!("a{}", abi_index)
        } else {
            "t0".to_string()
        }
    }

    fn emit_read_ref(&mut self, dest: &IrVar, ty: &str) -> Result<()> {
        if self.cell_buffer_offsets.contains_key(&dest.id) {
            self.emit(format!("# read_ref {} (preloaded from CellDep)", ty));
            return Ok(());
        }

        // Runtime fallback: emit LOAD_CELL_DATA syscall to load the cell dep data
        // into the scratch buffer and store the pointer.
        let Some(dep_index) = self.read_ref_indices.get(&dest.id).copied() else {
            self.emit("# cellscript abi: fail closed because read_ref CellDep index was not allocated");
            self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
            return Ok(());
        };
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();

        self.emit(format!("# read_ref {}", ty));
        self.emit(format!("# cellscript abi: runtime read_ref CellDep index={}", dep_index));
        self.emit_load_cell_data_syscall_to_offsets(
            "read_ref",
            CKB_SOURCE_CELL_DEP,
            dep_index,
            size_offset,
            buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);

        // Also store the size so that subsequent schema operations can use it
        self.schema_pointer_size_offsets.insert(dest.id, size_offset);
        self.cell_buffer_size_offsets.insert(dest.id, size_offset);
        self.cell_buffer_offsets.insert(dest.id, buffer_offset);

        Ok(())
    }

    fn emit_move(&mut self, dest: &IrVar, src: &IrOperand) -> Result<()> {
        if dest.ty == IrType::U128 {
            self.emit_materialize_u128_operand_to_var(dest, src);
            return Ok(());
        }
        if let Some(width) = self.fixed_byte_like_width(&dest.ty).filter(|width| *width > 8) {
            if self.emit_materialize_fixed_byte_operand_to_var(dest, src, width) {
                return Ok(());
            }
        }
        self.emit_operand_to_register("t0", src);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_materialize_fixed_byte_operand_to_var(&mut self, dest: &IrVar, src: &IrOperand, width: usize) -> bool {
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            return false;
        };
        let Some(source) = self.expected_fixed_byte_source(src, width) else {
            self.emit("# cellscript abi: fail closed because fixed-byte move source is unavailable");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return true;
        };
        self.emit(format!("# cellscript abi: materialize fixed-byte move var{} size={}", dest.id, width));
        self.emit_prepare_fixed_byte_source(&source, width, "fixed-byte move");
        if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &source) {
            self.emit("# cellscript abi: fail closed because fixed-byte move pointer is unavailable");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return true;
        }
        self.emit_sp_addi("a1", dest_offset);
        self.emit(format!("li a2, {}", width));
        self.emit("call __cellscript_memcpy_fixed");
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_tuple(&mut self, dest: &IrVar, fields: &[IrOperand]) -> Result<()> {
        self.emit(format!("# cellscript abi: construct tuple aggregate var{} fields={}", dest.id, fields.len()));
        if self.emit_fixed_named_tuple(dest, fields) {
            return Ok(());
        }
        self.emit_stack_store("zero", dest.id * 8);
        Ok(())
    }

    fn emit_fixed_named_tuple(&mut self, dest: &IrVar, fields: &[IrOperand]) -> bool {
        let IrType::Named(type_name) = &dest.ty else {
            return false;
        };
        let Some(width) = self.type_fixed_sizes.get(type_name).copied() else {
            return false;
        };
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            return false;
        };
        let Some(layouts) = self.type_layouts.get(type_name) else {
            return false;
        };
        let mut ordered = layouts.values().cloned().collect::<Vec<_>>();
        ordered.sort_by_key(|layout| layout.offset);
        if ordered.len() != fields.len() {
            return false;
        }

        self.emit(format!("# cellscript abi: materialize fixed aggregate {} var{} size={}", type_name, dest.id, width));
        for (field, layout) in fields.iter().zip(ordered.iter()) {
            let Some(field_width) = layout_fixed_byte_width(layout).or_else(|| self.fixed_named_type_width(&layout.ty)) else {
                return false;
            };
            let Some(source) = self.expected_fixed_byte_source(field, field_width) else {
                self.emit("# cellscript abi: fail closed because fixed aggregate field source is unavailable");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return true;
            };
            self.emit_prepare_fixed_byte_source(&source, field_width, &format!("{} aggregate field", type_name));
            if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &source) {
                self.emit("# cellscript abi: fail closed because fixed aggregate field pointer is unavailable");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return true;
            }
            self.emit_sp_addi("a1", dest_offset + layout.offset);
            self.emit(format!("li a2, {}", field_width));
            self.emit("call __cellscript_memcpy_fixed");
        }
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_operand_to_register(&mut self, register: &str, operand: &IrOperand) {
        match operand {
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("li {}, {}", register, if *b { 1 } else { 0 })),
            IrOperand::Const(value) => {
                if let Some(bytes) = fixed_byte_const_bytes(value) {
                    let label = self.const_data_label_for_bytes(bytes);
                    self.emit(format!("la {}, {}", register, label));
                } else {
                    self.emit(format!("li {}, 0", register));
                }
            }
            IrOperand::Var(v) => self.emit_stack_load(register, v.id * 8),
        }
    }

    /// consume
    fn emit_consume(&mut self, operand: &IrOperand) -> Result<()> {
        self.emit("# consume");
        if let IrOperand::Var(var) = operand {
            if self.consume_indices.contains_key(&var.id) {
                self.emit("# cellscript abi: consumed input pointer retained for verifier field checks");
                return Ok(());
            }
            // Consume a local variable: the actual LOAD_CELL input data loading
            // already happened in the action prelude (generate_consume).
            // Here we only zero out the local binding to enforce linear ownership.
            self.emit_stack_store("zero", var.id * 8);
            return Ok(());
        }
        // Non-Var consume: this should not happen in valid IR, but fail with
        // a specific error code instead of blocking ELF emission.
        self.emit("# cellscript abi: fail closed because consume operand is not a variable");
        self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
        Ok(())
    }

    /// create
    fn emit_create(&mut self, dest: &IrVar, pattern: &CreatePattern) -> Result<()> {
        let output_index = self.operation_output_indices.get(&dest.id).copied().unwrap_or(self.next_virtual_output);
        if pattern.operation == "output" {
            self.emit(format!("# constrain named output {}", pattern.ty));
            for (field, value) in &pattern.fields {
                match value {
                    IrOperand::Const(IrConst::U64(n)) => self.emit(format!("#   field {} = {}", field, n)),
                    IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("#   field {} = {}", field, b)),
                    IrOperand::Var(var) => self.emit(format!("#   field {} <- {}", field, var.name)),
                    _ => self.emit(format!("#   field {} <- <value>", field)),
                }
            }
            if pattern.lock.is_some() {
                self.emit("#   with_lock <expr>");
            }
            if let Some(var_id) = self.output_param_ids.get(&pattern.binding).copied() {
                let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                };
                let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                };
                if pattern.fields.is_empty() {
                    self.emit_state_transition_check(pattern, size_offset, buffer_offset);
                } else if self.can_verify_create_output_fields(pattern) {
                    self.emit_create_output_checks_at(pattern, size_offset, buffer_offset);
                } else {
                    self.emit("# cellscript abi: ordered named output field verification incomplete");
                    self.emit("# cellscript abi: fail closed because the output state is not fully verified");
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                }
                if let Some(lock) = &pattern.lock {
                    if !(self.can_verify_output_lock(pattern) && self.emit_output_lock_hash_check(output_index, lock)) {
                        self.emit("# cellscript abi: output lock verification incomplete for this named output");
                        self.emit("# cellscript abi: fail closed because the output lock is not fully verified");
                        self.emit_fail(CellScriptRuntimeError::EntryWitnessMagicMismatch);
                        return Ok(());
                    }
                }
            } else {
                self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                return Ok(());
            }
            self.emit(format!("li t0, {}", output_index));
            self.emit_stack_store("t0", dest.id * 8);
            self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
            return Ok(());
        }

        self.generate_create(pattern, output_index, false, false)?;
        self.emit(format!("# create {}", pattern.ty));
        for (field, value) in &pattern.fields {
            match value {
                IrOperand::Const(IrConst::U64(n)) => self.emit(format!("#   field {} = {}", field, n)),
                IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("#   field {} = {}", field, b)),
                IrOperand::Var(var) => self.emit(format!("#   field {} <- {}", field, var.name)),
                _ => self.emit(format!("#   field {} <- <value>", field)),
            }
        }
        if pattern.lock.is_some() {
            self.emit("#   with_lock <expr>");
        }
        self.emit(format!("li t0, {}", output_index));
        self.emit_stack_store("t0", dest.id * 8);
        self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
        Ok(())
    }

    fn emit_create_unique_identity_check(&mut self, output_index: usize, pattern: &CreatePattern, identity: &IrIdentityPolicy) {
        self.emit(format!(
            "# cellscript abi: create_unique identity policy {} for Output#{}",
            identity_policy_label(identity),
            output_index
        ));
        match identity {
            IrIdentityPolicy::None => {}
            IrIdentityPolicy::CkbTypeId => {
                self.emit_output_type_hash_present_check(output_index, "create_unique_ckb_type_id_output_type_hash");
            }
            IrIdentityPolicy::Field(field) => {
                self.emit_create_unique_field_identity_anchor(output_index, pattern, field);
            }
            IrIdentityPolicy::ScriptArgs => {
                self.emit_cell_field_hash_equality(
                    "create_unique_group_input_lock_hash",
                    CKB_SOURCE_GROUP_INPUT,
                    0,
                    "create_unique_output_lock_hash",
                    CKB_SOURCE_OUTPUT,
                    output_index,
                    CKB_CELL_FIELD_LOCK_HASH,
                    "LockHash",
                    "create_unique script_args identity anchor",
                    CellScriptRuntimeError::LockHashPreservationMismatch,
                );
            }
            IrIdentityPolicy::SingletonType => {
                self.emit_cell_field_hash_equality(
                    "create_unique_group_input_type_hash",
                    CKB_SOURCE_GROUP_INPUT,
                    0,
                    "create_unique_output_type_hash",
                    CKB_SOURCE_OUTPUT,
                    output_index,
                    CKB_CELL_FIELD_TYPE_HASH,
                    "TypeHash",
                    "create_unique singleton_type identity anchor",
                    CellScriptRuntimeError::TypeHashMismatch,
                );
            }
        }
    }

    fn emit_create_unique_field_identity_anchor(&mut self, output_index: usize, pattern: &CreatePattern, field: &str) {
        let Some(layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(field)).cloned() else {
            self.emit(format!(
                "# cellscript abi: fail closed because create_unique identity field {}.{} has no layout",
                pattern.ty, field
            ));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            self.emit(format!(
                "# cellscript abi: fail closed because create_unique identity field {}.{} is not fixed-width",
                pattern.ty, field
            ));
            self.emit_fail(CellScriptRuntimeError::DynamicFieldValueMismatch);
            return;
        };
        let output_size_offset = self.runtime_scratch_size_offset();
        let output_buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_load_cell_data_syscall("create_unique_identity_field", CKB_SOURCE_OUTPUT, output_index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::CellLoadFailed);
        let output_pointer_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let output_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        let context = format!("create_unique identity field {}.{}", pattern.ty, field);
        if self.type_fixed_sizes.contains_key(&pattern.ty) {
            self.emit_loaded_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                &layout,
                width,
                &context,
                output_pointer_offset,
            );
        } else if let Some(field_count) = self.type_layouts.get(&pattern.ty).map(|fields| fields.len()) {
            self.emit_dynamic_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                &layout,
                field_count,
                width,
                &context,
                output_pointer_offset,
                output_len_offset,
            );
        } else {
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        }
        self.emit(format!(
            "# cellscript abi: create_unique field identity anchored by verified Output#{} {}.{} size={}",
            output_index, pattern.ty, field, width
        ));
    }

    fn emit_replace_unique_identity_check(
        &mut self,
        output_index: usize,
        operand: &IrOperand,
        pattern: &CreatePattern,
        identity: &IrIdentityPolicy,
    ) {
        self.emit(format!(
            "# cellscript abi: replace_unique identity policy {} for Output#{}",
            identity_policy_label(identity),
            output_index
        ));
        let input_index = match operand {
            IrOperand::Var(var) => self.consume_indices.get(&var.id).copied().unwrap_or(0),
            _ => 0,
        };
        match identity {
            IrIdentityPolicy::None => {}
            IrIdentityPolicy::CkbTypeId | IrIdentityPolicy::SingletonType => {
                self.emit_cell_field_hash_equality(
                    "replace_unique_input_type_hash",
                    CKB_SOURCE_INPUT,
                    input_index,
                    "replace_unique_output_type_hash",
                    CKB_SOURCE_OUTPUT,
                    output_index,
                    CKB_CELL_FIELD_TYPE_HASH,
                    "TypeHash",
                    "replace_unique type identity preservation",
                    CellScriptRuntimeError::TypeHashMismatch,
                );
            }
            IrIdentityPolicy::ScriptArgs => {
                self.emit_cell_field_hash_equality(
                    "replace_unique_input_lock_hash",
                    CKB_SOURCE_INPUT,
                    input_index,
                    "replace_unique_output_lock_hash",
                    CKB_SOURCE_OUTPUT,
                    output_index,
                    CKB_CELL_FIELD_LOCK_HASH,
                    "LockHash",
                    "replace_unique script_args identity preservation",
                    CellScriptRuntimeError::LockHashPreservationMismatch,
                );
            }
            IrIdentityPolicy::Field(field) => {
                self.emit_replace_unique_field_identity_check(output_index, operand, pattern, field);
            }
        }
    }

    fn emit_replace_unique_field_identity_check(
        &mut self,
        output_index: usize,
        operand: &IrOperand,
        pattern: &CreatePattern,
        field: &str,
    ) {
        let input_var = match operand {
            IrOperand::Var(var) => var,
            _ => {
                self.emit("# cellscript abi: fail closed because replace_unique identity input is not a cell variable");
                self.emit_fail(CellScriptRuntimeError::DestroyInvalidOperand);
                return;
            }
        };
        let (Some(input_size_offset), Some(input_buffer_offset)) =
            (self.cell_buffer_size_offsets.get(&input_var.id).copied(), self.cell_buffer_offsets.get(&input_var.id).copied())
        else {
            self.emit("# cellscript abi: fail closed because replace_unique identity input cell data is unavailable");
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            return;
        };
        let Some(layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(field)).cloned() else {
            self.emit(format!(
                "# cellscript abi: fail closed because replace_unique identity field {}.{} has no layout",
                pattern.ty, field
            ));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            self.emit(format!(
                "# cellscript abi: fail closed because replace_unique identity field {}.{} is not fixed-width",
                pattern.ty, field
            ));
            self.emit_fail(CellScriptRuntimeError::DynamicFieldValueMismatch);
            return;
        };

        let output_size_offset = self.runtime_scratch_size_offset();
        let output_buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_load_cell_data_syscall("replace_unique_identity_field_output", CKB_SOURCE_OUTPUT, output_index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::CellLoadFailed);
        let input_pointer_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let input_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        let output_pointer_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
        let output_len_offset = self.runtime_expr_temp_offset(3).expect("runtime temp slot 3");
        let input_context = format!("replace_unique input identity field {}.{}", pattern.ty, field);
        let output_context = format!("replace_unique output identity field {}.{}", pattern.ty, field);
        if self.type_fixed_sizes.contains_key(&pattern.ty) {
            self.emit_loaded_fixed_field_pointer_to_stack(
                input_size_offset,
                input_buffer_offset,
                &layout,
                width,
                &input_context,
                input_pointer_offset,
            );
            self.emit_loaded_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                &layout,
                width,
                &output_context,
                output_pointer_offset,
            );
        } else if let Some(field_count) = self.type_layouts.get(&pattern.ty).map(|fields| fields.len()) {
            self.emit_dynamic_fixed_field_pointer_to_stack(
                input_size_offset,
                input_buffer_offset,
                &layout,
                field_count,
                width,
                &input_context,
                input_pointer_offset,
                input_len_offset,
            );
            self.emit_dynamic_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                &layout,
                field_count,
                width,
                &output_context,
                output_pointer_offset,
                output_len_offset,
            );
        } else {
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        }
        self.emit_fixed_pointer_equality(
            input_pointer_offset,
            output_pointer_offset,
            width,
            &format!("replace_unique identity field {}.{} Input == Output#{}", pattern.ty, field, output_index),
            CellScriptRuntimeError::DynamicFieldValueMismatch,
        );
    }

    /// create_unique
    fn emit_create_unique(&mut self, dest: &IrVar, pattern: &CreatePattern, identity: &IrIdentityPolicy) -> Result<()> {
        let output_index = self.operation_output_indices.get(&dest.id).copied().unwrap_or(self.next_virtual_output);
        self.generate_create(pattern, output_index, false, false)?;
        self.emit_create_unique_identity_check(output_index, pattern, identity);
        self.emit(format!("# create_unique {} identity={}", pattern.ty, identity_policy_label(identity)));
        for (field, value) in &pattern.fields {
            match value {
                IrOperand::Const(IrConst::U64(n)) => self.emit(format!("#   field {} = {}", field, n)),
                IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("#   field {} = {}", field, b)),
                IrOperand::Var(var) => self.emit(format!("#   field {} <- {}", field, var.name)),
                _ => self.emit(format!("#   field {} <- <value>", field)),
            }
        }
        if pattern.lock.is_some() {
            self.emit("#   with_lock <expr>");
        }
        self.emit(format!("li t0, {}", output_index));
        self.emit_stack_store("t0", dest.id * 8);
        self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
        Ok(())
    }

    /// replace_unique
    fn emit_replace_unique(
        &mut self,
        dest: &IrVar,
        operand: &IrOperand,
        pattern: &CreatePattern,
        identity: &IrIdentityPolicy,
    ) -> Result<()> {
        let output_index = self.operation_output_indices.get(&dest.id).copied().unwrap_or(self.next_virtual_output);
        self.emit(format!("# replace_unique {} identity={}", pattern.ty, identity_policy_label(identity)));
        self.emit_operand_comment("input", operand);
        for (field, value) in &pattern.fields {
            match value {
                IrOperand::Const(IrConst::U64(n)) => self.emit(format!("#   field {} = {}", field, n)),
                IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("#   field {} = {}", field, b)),
                IrOperand::Var(var) => self.emit(format!("#   field {} <- {}", field, var.name)),
                _ => self.emit(format!("#   field {} <- <value>", field)),
            }
        }
        // replace_unique is a consume + create with identity preservation.
        // The output occupies a virtual output slot, similar to transfer.
        self.generate_create(pattern, output_index, false, false)?;
        self.emit_replace_unique_identity_check(output_index, operand, pattern, identity);
        if self.emit_verified_operation_output_handle(dest, "replace_unique") {
            return Ok(());
        }
        self.emit(format!("# cellscript abi: replace_unique output handle Output#{}", output_index));
        self.emit(format!("li t0, {}", output_index));
        self.emit_stack_store("t0", dest.id * 8);
        self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
        Ok(())
    }

    /// transfer
    fn emit_transfer(&mut self, dest: &IrVar, operand: &IrOperand, to: &IrOperand) -> Result<()> {
        self.emit("# transfer");
        self.emit_operand_comment("asset", operand);
        self.emit_operand_comment("to", to);
        if self.emit_verified_operation_output_handle(dest, "transfer") {
            return Ok(());
        }
        if let Some(output_index) = self.operation_output_indices.get(&dest.id).copied() {
            self.emit(format!("# cellscript abi: transfer output handle Output#{} (unverified)", output_index));
            self.emit(format!("li t0, {}", output_index));
            self.emit_stack_store("t0", dest.id * 8);
            self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
            return Ok(());
        }
        self.emit("# cellscript abi: fail closed because transfer output relation is unknown");
        self.emit_fail(CellScriptRuntimeError::DestroyInvalidOperand);
        Ok(())
    }

    /// claim
    fn emit_claim(&mut self, dest: &IrVar, receipt: &IrOperand) -> Result<()> {
        self.emit("# claim");
        self.emit_operand_comment("receipt", receipt);
        if self.emit_verified_operation_output_handle(dest, "claim") {
            return Ok(());
        }
        if let Some(output_index) = self.operation_output_indices.get(&dest.id).copied() {
            self.emit(format!("# cellscript abi: claim output handle Output#{} (unverified)", output_index));
            self.emit(format!("li t0, {}", output_index));
            self.emit_stack_store("t0", dest.id * 8);
            self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
            return Ok(());
        }
        self.emit("# cellscript abi: fail closed because claim output relation is unknown");
        self.emit_fail(CellScriptRuntimeError::DestroyInvalidOperand);
        Ok(())
    }

    /// settle
    fn emit_settle(&mut self, dest: &IrVar, operand: &IrOperand) -> Result<()> {
        self.emit("# settle");
        self.emit_operand_comment("value", operand);
        if self.emit_verified_operation_output_handle(dest, "settle") {
            return Ok(());
        }
        if let Some(output_index) = self.operation_output_indices.get(&dest.id).copied() {
            self.emit(format!("# cellscript abi: settle output handle Output#{} (unverified)", output_index));
            self.emit(format!("li t0, {}", output_index));
            self.emit_stack_store("t0", dest.id * 8);
            self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
            return Ok(());
        }
        self.emit("# cellscript abi: fail closed because settle output relation is unknown");
        self.emit_fail(CellScriptRuntimeError::DestroyInvalidOperand);
        Ok(())
    }

    fn emit_verified_operation_output_handle(&mut self, dest: &IrVar, operation: &str) -> bool {
        if !self.verified_operation_outputs.contains(&dest.id) {
            return false;
        }
        let output_index = self.operation_output_indices.get(&dest.id).copied().unwrap_or(self.next_virtual_output);
        self.emit(format!("# cellscript abi: {} output relation verified by prelude Output#{}", operation, output_index));
        self.emit(format!("li t0, {}", output_index));
        self.emit_stack_store("t0", dest.id * 8);
        self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
        true
    }

    /// destroy
    fn emit_destroy(&mut self, operand: &IrOperand) -> Result<()> {
        self.emit("# destroy");
        if let IrOperand::Var(_) = operand {
            self.emit_operand_comment("destroyed input retained for verifier field checks", operand);
            self.emit("# cellscript abi: destroy consumed input is checked by Output absence scan");
            self.emit("# cellscript abi: retain consumed input pointer for post-destroy output verification");
            return Ok(());
        }
        // Non-Var destroy: this should not happen in valid IR, fail with specific error.
        self.emit("# cellscript abi: fail closed because destroy operand is not a variable");
        self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
        Ok(())
    }

    fn emit_operand_comment(&mut self, label: &str, operand: &IrOperand) {
        let rendered = match operand {
            IrOperand::Var(var) => format!("{}: {}", label, var.name),
            IrOperand::Const(IrConst::U64(n)) => format!("{}: {}", label, n),
            IrOperand::Const(IrConst::Bool(b)) => format!("{}: {}", label, b),
            IrOperand::Const(IrConst::Address(_)) => format!("{}: <address>", label),
            IrOperand::Const(IrConst::Hash(_)) => format!("{}: <hash>", label),
            IrOperand::Const(IrConst::Array(items)) => format!("{}: <array:{}>", label, items.len()),
            IrOperand::Const(_) => format!("{}: <const>", label),
        };
        self.emit(format!("#   {}", rendered));
    }

    fn static_length(&self, operand: &IrOperand) -> Option<usize> {
        match operand {
            IrOperand::Var(var) => Self::static_length_from_type(&var.ty),
            IrOperand::Const(IrConst::Array(items)) => Some(items.len()),
            _ => None,
        }
    }

    fn static_length_from_type(ty: &IrType) -> Option<usize> {
        match ty {
            IrType::Array(_, size) => Some(*size),
            IrType::Ref(inner) | IrType::MutRef(inner) => Self::static_length_from_type(inner),
            _ => None,
        }
    }

    fn generate_runtime_support(&mut self, ir: &IrModule) {
        self.emit_section(".text");
        self.emit_runtime_memcmp_fixed();
        self.emit_runtime_memzero_fixed();
        self.emit_runtime_memcpy_fixed();
        self.emit_runtime_size_guards();
        // CKB exposes epoch-number based timepoints here, not Unix timestamps.
        self.emit_runtime_header_field_u64(
            "__env_current_timepoint",
            "ckb_epoch_number",
            CKB_HEADER_FIELD_EPOCH_NUMBER,
            true,
            "env::current_timepoint is required for CKB profile",
        );
        self.emit_runtime_header_field_u64(
            "__ckb_header_epoch_number",
            "ckb_epoch_number",
            CKB_HEADER_FIELD_EPOCH_NUMBER,
            self.options.target_profile == TargetProfile::Ckb,
            "ckb::header_epoch_number is rejected outside the ckb target profile",
        );
        self.emit_runtime_header_field_u64(
            "__ckb_header_epoch_start_block_number",
            "ckb_epoch_start_block_number",
            CKB_HEADER_FIELD_EPOCH_START_BLOCK_NUMBER,
            self.options.target_profile == TargetProfile::Ckb,
            "ckb::header_epoch_start_block_number is rejected outside the ckb target profile",
        );
        self.emit_runtime_header_field_u64(
            "__ckb_header_epoch_length",
            "ckb_epoch_length",
            CKB_HEADER_FIELD_EPOCH_LENGTH,
            self.options.target_profile == TargetProfile::Ckb,
            "ckb::header_epoch_length is rejected outside the ckb target profile",
        );
        self.emit_runtime_input_field_u64(
            "__ckb_input_since",
            "ckb_input_since",
            CKB_INPUT_FIELD_SINCE,
            self.options.target_profile == TargetProfile::Ckb,
            "ckb::input_since is rejected outside the ckb target profile",
        );
        let v014_helpers = referenced_v014_runtime_helpers(ir);
        self.emit_runtime_ckb_v014_surface_helpers(&v014_helpers);
    }

    fn emit_runtime_ckb_v014_surface_helpers(&mut self, referenced_helpers: &BTreeSet<String>) {
        let enabled = self.options.target_profile == TargetProfile::Ckb;
        for (name, syscall, detail) in [
            ("__ckb_spawn", ckb_abi::syscall::SPAWN, "spawn bounded verifier child"),
            ("__ckb_wait", ckb_abi::syscall::WAIT, "wait for bounded verifier child"),
            ("__ckb_process_id", ckb_abi::syscall::PROCESS_ID, "current process id"),
            ("__ckb_pipe", ckb_abi::syscall::PIPE, "create IPC pipe; returns read fd in a0 and write fd in a1"),
            ("__ckb_pipe_write", ckb_abi::syscall::WRITE, "write u64 payload to IPC pipe"),
            ("__ckb_pipe_read", ckb_abi::syscall::READ, "read u64 payload from IPC pipe"),
            ("__ckb_inherited_fd", ckb_abi::syscall::INHERITED_FDS, "resolve inherited fd"),
            ("__ckb_close", ckb_abi::syscall::CLOSE, "close fd"),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_global(name);
            self.emit_label(name);
            self.emit(format!("# cellscript abi: CKB VM v2 syscall {} ({})", syscall, detail));
            if !enabled {
                self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            } else {
                match name {
                    "__ckb_pipe" => {
                        self.emit("addi sp, sp, -32");
                        self.emit("sd ra, 24(sp)");
                        self.emit("addi a0, sp, 8");
                        self.emit(format!("li a7, {}", syscall));
                        self.emit("ecall");
                        let failed = self.fresh_label("ckb_pipe_failed");
                        let done = self.fresh_label("ckb_pipe_done");
                        self.emit(format!("bnez a0, {}", failed));
                        self.emit("ld a1, 8(sp)");
                        self.emit("ld a2, 16(sp)");
                        self.emit("li a0, 0");
                        self.emit(format!("j {}", done));
                        self.emit_label(&failed);
                        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                        self.emit_label(&done);
                        self.emit("ld ra, 24(sp)");
                        self.emit("addi sp, sp, 32");
                        self.emit("ret");
                    }
                    "__ckb_pipe_write" => {
                        self.emit("addi sp, sp, -32");
                        self.emit("sd ra, 24(sp)");
                        self.emit("sd a1, 8(sp)");
                        self.emit("li t0, 8");
                        self.emit("sd t0, 16(sp)");
                        self.emit("addi a1, sp, 8");
                        self.emit("addi a2, sp, 16");
                        self.emit(format!("li a7, {}", syscall));
                        self.emit("ecall");
                        let done = self.fresh_label("ckb_pipe_write_done");
                        self.emit(format!("beqz a0, {}", done));
                        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                        self.emit_label(&done);
                        self.emit("ld ra, 24(sp)");
                        self.emit("addi sp, sp, 32");
                        self.emit("ret");
                    }
                    "__ckb_close" => {
                        self.emit(format!("li a7, {}", syscall));
                        self.emit("ecall");
                        let done = self.fresh_label("ckb_close_done");
                        self.emit(format!("beqz a0, {}", done));
                        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                        self.emit_label(&done);
                        self.emit("ret");
                    }
                    "__ckb_wait" => {
                        self.emit("addi sp, sp, -32");
                        self.emit("sd ra, 24(sp)");
                        self.emit("sd zero, 8(sp)");
                        self.emit("addi a1, sp, 8");
                        self.emit(format!("li a7, {}", syscall));
                        self.emit("ecall");
                        let failed = self.fresh_label("ckb_wait_failed");
                        let exit_ok = self.fresh_label("ckb_wait_exit_ok");
                        let child_failed = self.fresh_label("ckb_wait_child_failed");
                        let done = self.fresh_label("ckb_wait_done");
                        self.emit(format!("bnez a0, {}", failed));
                        self.emit("lbu t0, 8(sp)");
                        self.emit(format!("beqz t0, {}", exit_ok));
                        self.emit_label(&child_failed);
                        self.emit("addi a0, t0, 0");
                        self.emit(format!("j {}", done));
                        self.emit_label(&failed);
                        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                        self.emit(format!("j {}", done));
                        self.emit_label(&exit_ok);
                        self.emit("li a0, 0");
                        self.emit_label(&done);
                        self.emit("ld ra, 24(sp)");
                        self.emit("addi sp, sp, 32");
                        self.emit("ret");
                    }
                    "__ckb_spawn" => {
                        self.emit("li a0, 0");
                        self.emit("ret");
                    }
                    _ => {
                        self.emit(format!("li a7, {}", syscall));
                        self.emit("ecall");
                        self.emit("ret");
                    }
                }
            }
        }
        if referenced_helpers.contains("__ckb_spawn_with_fd1") {
            self.emit_global("__ckb_spawn_with_fd1");
            self.emit_label("__ckb_spawn_with_fd1");
            self.emit("# cellscript abi: CKB VM v2 spawn CellDep#0/code with one inherited fd from a1");
            if !enabled {
                self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            } else {
                self.emit("addi sp, sp, -96");
                self.emit("sd ra, 88(sp)");
                self.emit("sd a1, 8(sp)");
                self.emit("sd zero, 16(sp)");
                self.emit("sd zero, 32(sp)");
                self.emit("sd zero, 40(sp)");
                self.emit("addi t0, sp, 24");
                self.emit("sd t0, 48(sp)");
                self.emit("addi t0, sp, 8");
                self.emit("sd t0, 56(sp)");
                self.emit("li a0, 0");
                self.emit(format!("li a1, {}", ckb_abi::source::CELL_DEP));
                self.emit("li a2, 0");
                self.emit(format!("li a3, {}", ckb_abi::place::CELL));
                self.emit("addi a4, sp, 32");
                self.emit(format!("li a7, {}", ckb_abi::syscall::SPAWN));
                self.emit("ecall");
                let failed = self.fresh_label("ckb_spawn_with_fd_failed");
                let done = self.fresh_label("ckb_spawn_with_fd_done");
                self.emit(format!("bnez a0, {}", failed));
                self.emit("ld a1, 24(sp)");
                self.emit("li a0, 0");
                self.emit(format!("j {}", done));
                self.emit_label(&failed);
                self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                self.emit_label(&done);
                self.emit("ld ra, 88(sp)");
                self.emit("addi sp, sp, 96");
                self.emit("ret");
            }
        }

        for (name, source_view, detail) in [
            ("__ckb_source_input", CKB_SOURCE_VIEW_INPUT, "Source::Input"),
            ("__ckb_source_output", CKB_SOURCE_VIEW_OUTPUT, "Source::Output"),
            ("__ckb_source_cell_dep", CKB_SOURCE_VIEW_CELL_DEP, "Source::CellDep"),
            ("__ckb_source_header_dep", CKB_SOURCE_VIEW_HEADER_DEP, "Source::HeaderDep"),
            ("__ckb_source_group_input", CKB_SOURCE_VIEW_GROUP_INPUT, "Source::GroupInput"),
            ("__ckb_source_group_output", CKB_SOURCE_VIEW_GROUP_OUTPUT, "Source::GroupOutput"),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_runtime_source_view_helper(name, source_view, detail, enabled);
        }

        for (name, relative, detail) in [
            ("__ckb_since_epoch_absolute", false, "CKB RFC0017 absolute epoch since encoder"),
            ("__ckb_since_epoch_relative", true, "CKB RFC0017 relative epoch since encoder"),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_runtime_ckb_since_epoch_helper(name, relative, detail, enabled);
        }

        let needs_c256_product = referenced_helpers.contains("__c256_require_u128_product_lte")
            || referenced_helpers.contains("__c256_require_u128_product_eq")
            || referenced_helpers.contains("__c256_require_u128_sum2_products_lte")
            || referenced_helpers.contains("__c256_require_u128_sum2_products_eq");
        let needs_c256_sum = referenced_helpers.contains("__c256_require_u128_sum2_products_lte")
            || referenced_helpers.contains("__c256_require_u128_sum2_products_eq");
        if needs_c256_product {
            self.emit_runtime_load_u64_le_helper();
            self.emit_runtime_mul_u128_to_u256_helper();
            if needs_c256_sum {
                self.emit_runtime_add_u256_helper();
            }
        }
        if referenced_helpers.contains("__ckb_require_lock_type_metapoint_pairs")
            || referenced_helpers.contains("__ckb_require_type_lock_metapoint_pairs")
            || referenced_helpers.contains("__ckb_require_lock_type_metapoint_pairs_from_i32_data")
            || referenced_helpers.contains("__ckb_require_type_lock_metapoint_pairs_from_i32_data")
            || referenced_helpers.contains("__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered")
            || referenced_helpers.contains("__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered")
            || referenced_helpers.contains("__ckb_require_lock_match_master_out_point_pairs_from_data")
        {
            self.emit_runtime_current_script_role_at_helper(enabled);
        }

        for (name, detail) in [
            ("__ckb_current_role", "current script role inferred from group input lock/type hashes"),
            ("__ckb_current_script_hash", "current script hash loaded via LOAD_SCRIPT_HASH"),
            ("__ckb_cell_capacity", "SourceView cell capacity field"),
            ("__ckb_cell_occupied_capacity", "SourceView occupied capacity from CellOutput scripts and data bytes"),
            ("__ckb_cell_unoccupied_capacity", "SourceView capacity minus occupied capacity"),
            ("__ckb_cell_output_index", "SourceView output index"),
            ("__ckb_input_out_point_index", "SourceView input OutPoint index"),
            ("__ckb_input_out_point_tx_hash_low", "SourceView input OutPoint tx hash low word"),
            ("__ckb_input_out_point_tx_hash", "SourceView input OutPoint full tx hash read"),
            ("__ckb_require_input_out_point_tx_hash", "SourceView input OutPoint full tx-hash binding check"),
            ("__ckb_require_input_out_point", "SourceView input OutPoint full tx-hash and index binding check"),
            ("__ckb_require_metapoint_relative", "SourceView MetaPoint relative-distance binding check"),
            ("__ckb_require_lock_type_metapoint_pairs", "current-script lock-only to type-only MetaPoint pair cardinality check"),
            ("__ckb_require_type_lock_metapoint_pairs", "current-script type-only to lock-only MetaPoint pair cardinality check"),
            (
                "__ckb_require_lock_type_metapoint_pairs_from_i32_data",
                "current-script lock-only to type-only MetaPoint pair cardinality check using signed i32 distance loaded from base cell data",
            ),
            (
                "__ckb_require_type_lock_metapoint_pairs_from_i32_data",
                "current-script type-only to lock-only MetaPoint pair cardinality check using signed i32 distance loaded from base cell data",
            ),
            (
                "__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered",
                "current-script lock-only to type-only filtered MetaPoint pair cardinality check using signed i32 distance loaded from base cell data",
            ),
            (
                "__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered",
                "current-script type-only to lock-only filtered MetaPoint pair cardinality check using signed i32 distance loaded from base cell data",
            ),
            (
                "__ckb_require_lock_match_master_out_point_pairs_from_data",
                "current-script lock-only match order input/output pairing using master OutPoint loaded from order data",
            ),
            ("__ckb_cell_lock_hash_low", "SourceView lock hash low word"),
            ("__ckb_cell_type_hash_low", "SourceView type hash low word"),
            ("__ckb_cell_lock_hash", "SourceView lock hash full 32-byte read"),
            ("__ckb_cell_type_hash", "SourceView type hash full 32-byte read"),
            ("__ckb_cell_data_hash", "SourceView data hash full 32-byte read"),
            ("__ckb_cell_data_hash_at", "SourceView cell data 32-byte read at byte offset"),
            ("__ckb_cell_lock_code_hash", "SourceView lock Script code_hash read"),
            ("__ckb_cell_type_code_hash", "SourceView type Script code_hash read"),
            ("__ckb_cell_lock_hash_type", "SourceView lock Script hash_type read"),
            ("__ckb_cell_type_hash_type", "SourceView type Script hash_type read"),
            ("__ckb_cell_lock_args_empty", "SourceView lock Script args_empty read"),
            ("__ckb_cell_type_args_empty", "SourceView type Script args_empty read"),
            ("__ckb_cell_lock_args_hash", "SourceView lock Script 32-byte args read"),
            ("__ckb_cell_type_args_hash", "SourceView type Script 32-byte args read"),
            ("__ckb_require_cell_lock_hash", "SourceView lock hash full 32-byte binding check"),
            ("__ckb_require_cell_type_hash", "SourceView type hash full 32-byte binding check"),
            ("__ckb_require_current_script_args_empty", "current Script empty args requirement"),
            ("__ckb_require_cell_lock_args_empty", "SourceView lock Script empty args requirement"),
            ("__ckb_require_cell_type_args_empty", "SourceView type Script empty args requirement"),
            ("__ckb_require_cell_lock_args_hash", "SourceView lock Script 32-byte args binding check"),
            ("__ckb_require_cell_type_args_hash", "SourceView type Script 32-byte args binding check"),
            ("__ckb_require_cell_lock_args_exact", "SourceView lock Script arbitrary exact args binding check"),
            ("__ckb_require_cell_type_args_exact", "SourceView type Script arbitrary exact args binding check"),
            ("__ckb_require_cell_lock_args_prefix_hash", "SourceView lock Script 32-byte args prefix binding check"),
            ("__ckb_require_cell_type_args_prefix_hash", "SourceView type Script 32-byte args prefix binding check"),
            ("__ckb_require_cell_lock_args_suffix_hash", "SourceView lock Script 32-byte args suffix binding check"),
            ("__ckb_require_cell_type_args_suffix_hash", "SourceView type Script 32-byte args suffix binding check"),
            ("__ckb_require_cell_lock_script_hash_type", "SourceView lock Script code_hash/hash_type binding check"),
            ("__ckb_require_cell_type_script_hash_type", "SourceView type Script code_hash/hash_type binding check"),
            ("__c256_require_u128_product_lte", "C256 u128 product <= requirement"),
            ("__c256_require_u128_product_eq", "C256 u128 product == requirement"),
            ("__c256_require_u128_sum2_products_lte", "C256 u128 product-sum <= requirement"),
            ("__c256_require_u128_sum2_products_eq", "C256 u128 product-sum == requirement"),
            ("__ckb_cell_data_size", "SourceView cell data byte length"),
            ("__ckb_cell_data_u32_le", "SourceView cell data little-endian u32 read"),
            ("__ckb_cell_data_u64_le", "SourceView cell data little-endian u64 read"),
            ("__dao_accumulated_rate", "DAO accumulated rate from HeaderDep SourceView"),
            (
                "__dao_input_accumulated_rate",
                "DAO accumulated rate from an Input/GroupInput committed header",
            ),
            ("__dao_has_dao_type", "DAO type hash classifier"),
            ("__dao_is_deposit_data", "DAO deposit data classifier"),
            ("__dao_is_withdrawal_request_data", "DAO withdrawal request data classifier"),
            ("__dao_require_header_dep_for_input", "DAO input header to HeaderDep lineage requirement"),
            ("__dao_require_input_since_at_least", "DAO input since lower-bound requirement"),
            ("__dao_require_input_relative_epoch_since_at_least", "DAO relative epoch since maturity requirement"),
            ("__xudt_amount_low", "xUDT amount low 64 bits"),
            ("__xudt_amount_high", "xUDT amount high 64 bits"),
            ("__xudt_owner_mode_input_type_hash", "xUDT owner-mode input-type hash low word"),
            ("__xudt_require_owner_mode_input_type", "xUDT owner-mode input-type binding check"),
            ("__xudt_require_owner_mode_type_args", "xUDT owner-mode type args binding check"),
            (
                "__xudt_require_owner_mode_type_args_current_script",
                "xUDT owner-mode type args binding check against current script hash",
            ),
            ("__xudt_require_group_amount_conserved", "xUDT group input/output amount conservation"),
            ("__xudt_require_group_amount_minted", "xUDT group output-input amount delta check"),
            ("__xudt_require_group_amount_burned", "xUDT group input-output amount delta check"),
            ("__ckb_witness_raw", "raw witness bytes"),
            ("__ckb_witness_lock", "WitnessArgs.lock"),
            ("__ckb_witness_input_type", "WitnessArgs.input_type"),
            ("__ckb_witness_output_type", "WitnessArgs.output_type"),
            ("__ckb_witness_size", "witness byte size"),
            ("__ckb_require_witness_size_at_least", "require witness size lower bound"),
            ("__ckb_sighash_all", "CKB sighash-all digest"),
            ("__ckb_require_maturity", "CKB block-number since maturity"),
            ("__ckb_require_time", "CKB timestamp since"),
            ("__ckb_require_epoch_after", "CKB absolute epoch since"),
            ("__ckb_require_epoch_relative", "CKB relative epoch since"),
            ("__ckb_occupied_capacity", "compile-visible occupied capacity floor"),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            match name {
                "__ckb_current_role" => self.emit_runtime_current_role_helper(enabled),
                "__ckb_current_script_hash" => self.emit_runtime_current_script_hash_helper(enabled),
                "__ckb_cell_capacity" => {
                    self.emit_runtime_cell_field_u64_helper(name, detail, CKB_CELL_FIELD_CAPACITY, enabled);
                }
                "__ckb_cell_occupied_capacity" => self.emit_runtime_cell_occupied_capacity_helper(enabled),
                "__ckb_cell_unoccupied_capacity" => self.emit_runtime_cell_unoccupied_capacity_helper(enabled),
                "__ckb_cell_output_index" => self.emit_runtime_cell_output_index_helper(enabled),
                "__ckb_input_out_point_index" => self.emit_runtime_input_out_point_word_helper(name, detail, 32, 4, enabled),
                "__ckb_input_out_point_tx_hash_low" => self.emit_runtime_input_out_point_word_helper(name, detail, 0, 8, enabled),
                "__ckb_input_out_point_tx_hash" => self.emit_runtime_input_out_point_tx_hash_helper(enabled),
                "__ckb_require_input_out_point_tx_hash" => self.emit_runtime_input_out_point_tx_hash_requirement_helper(enabled),
                "__ckb_require_input_out_point" => self.emit_runtime_input_out_point_requirement_helper(enabled),
                "__ckb_require_metapoint_relative" => self.emit_runtime_metapoint_relative_requirement_helper(enabled),
                "__ckb_require_lock_type_metapoint_pairs" => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, true, false, false, enabled)
                }
                "__ckb_require_type_lock_metapoint_pairs" => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, false, false, false, enabled)
                }
                "__ckb_require_lock_type_metapoint_pairs_from_i32_data" => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, true, true, false, enabled)
                }
                "__ckb_require_type_lock_metapoint_pairs_from_i32_data" => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, false, true, false, enabled)
                }
                "__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered" => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, true, true, true, enabled)
                }
                "__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered" => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, false, true, true, enabled)
                }
                "__ckb_require_lock_match_master_out_point_pairs_from_data" => {
                    self.emit_runtime_lock_match_master_out_point_pairs_from_data_helper(enabled)
                }
                "__ckb_cell_lock_hash_low" => {
                    self.emit_runtime_cell_field_low_word_helper(name, detail, CKB_CELL_FIELD_LOCK_HASH, enabled);
                }
                "__ckb_cell_type_hash_low" => {
                    self.emit_runtime_cell_field_low_word_helper(name, detail, CKB_CELL_FIELD_TYPE_HASH, enabled);
                }
                "__ckb_cell_lock_hash" => {
                    self.emit_runtime_cell_hash_field_helper(name, detail, CKB_CELL_FIELD_LOCK_HASH, enabled);
                }
                "__ckb_cell_type_hash" => {
                    self.emit_runtime_cell_hash_field_helper(name, detail, CKB_CELL_FIELD_TYPE_HASH, enabled);
                }
                "__ckb_cell_data_hash" => {
                    self.emit_runtime_cell_data_hash_helper(name, detail, enabled);
                }
                "__ckb_cell_data_hash_at" => {
                    self.emit_runtime_cell_data_hash_at_helper(name, detail, enabled);
                }
                "__ckb_cell_lock_code_hash" => {
                    self.emit_runtime_cell_script_hash_field_helper(name, detail, CKB_CELL_FIELD_LOCK, ScriptHashFieldRead::CodeHash, enabled);
                }
                "__ckb_cell_type_code_hash" => {
                    self.emit_runtime_cell_script_hash_field_helper(name, detail, CKB_CELL_FIELD_TYPE, ScriptHashFieldRead::CodeHash, enabled);
                }
                "__ckb_cell_lock_args_hash" => {
                    self.emit_runtime_cell_script_hash_field_helper(name, detail, CKB_CELL_FIELD_LOCK, ScriptHashFieldRead::Args32, enabled);
                }
                "__ckb_cell_type_args_hash" => {
                    self.emit_runtime_cell_script_hash_field_helper(name, detail, CKB_CELL_FIELD_TYPE, ScriptHashFieldRead::Args32, enabled);
                }
                "__ckb_cell_lock_hash_type" => {
                    self.emit_runtime_cell_script_scalar_field_helper(name, detail, CKB_CELL_FIELD_LOCK, ScriptScalarFieldRead::HashType, enabled);
                }
                "__ckb_cell_type_hash_type" => {
                    self.emit_runtime_cell_script_scalar_field_helper(name, detail, CKB_CELL_FIELD_TYPE, ScriptScalarFieldRead::HashType, enabled);
                }
                "__ckb_cell_lock_args_empty" => {
                    self.emit_runtime_cell_script_scalar_field_helper(name, detail, CKB_CELL_FIELD_LOCK, ScriptScalarFieldRead::ArgsEmpty, enabled);
                }
                "__ckb_cell_type_args_empty" => {
                    self.emit_runtime_cell_script_scalar_field_helper(name, detail, CKB_CELL_FIELD_TYPE, ScriptScalarFieldRead::ArgsEmpty, enabled);
                }
                "__ckb_require_cell_lock_hash" => self.emit_runtime_cell_hash_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_LOCK_HASH,
                    CellScriptRuntimeError::ScriptRoleMismatch,
                    enabled,
                ),
                "__ckb_require_cell_type_hash" => self.emit_runtime_cell_hash_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_TYPE_HASH,
                    CellScriptRuntimeError::TypeHashMismatch,
                    enabled,
                ),
                "__ckb_require_current_script_args_empty" => self.emit_runtime_current_script_args_empty_requirement_helper(enabled),
                "__ckb_require_cell_lock_args_empty" => {
                    self.emit_runtime_cell_script_args_empty_requirement_helper(name, detail, CKB_CELL_FIELD_LOCK, enabled)
                }
                "__ckb_require_cell_type_args_empty" => {
                    self.emit_runtime_cell_script_args_empty_requirement_helper(name, detail, CKB_CELL_FIELD_TYPE, enabled)
                }
                "__ckb_require_cell_lock_args_hash" => {
                    self.emit_runtime_cell_script_args_hash_requirement_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_LOCK,
                        ScriptArgsHashRequirementMode::Exact32,
                        enabled,
                    )
                }
                "__ckb_require_cell_type_args_hash" => {
                    self.emit_runtime_cell_script_args_hash_requirement_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_TYPE,
                        ScriptArgsHashRequirementMode::Exact32,
                        enabled,
                    )
                }
                "__ckb_require_cell_lock_args_exact" => {
                    self.emit_runtime_cell_script_args_exact_requirement_helper(name, detail, CKB_CELL_FIELD_LOCK, enabled)
                }
                "__ckb_require_cell_type_args_exact" => {
                    self.emit_runtime_cell_script_args_exact_requirement_helper(name, detail, CKB_CELL_FIELD_TYPE, enabled)
                }
                "__ckb_require_cell_lock_args_prefix_hash" => {
                    self.emit_runtime_cell_script_args_hash_requirement_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_LOCK,
                        ScriptArgsHashRequirementMode::Prefix32,
                        enabled,
                    )
                }
                "__ckb_require_cell_type_args_prefix_hash" => {
                    self.emit_runtime_cell_script_args_hash_requirement_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_TYPE,
                        ScriptArgsHashRequirementMode::Prefix32,
                        enabled,
                    )
                }
                "__ckb_require_cell_lock_args_suffix_hash" => {
                    self.emit_runtime_cell_script_args_hash_requirement_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_LOCK,
                        ScriptArgsHashRequirementMode::Suffix32,
                        enabled,
                    )
                }
                "__ckb_require_cell_type_args_suffix_hash" => {
                    self.emit_runtime_cell_script_args_hash_requirement_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_TYPE,
                        ScriptArgsHashRequirementMode::Suffix32,
                        enabled,
                    )
                }
                "__ckb_require_cell_lock_script_hash_type" => {
                    self.emit_runtime_cell_script_hash_type_requirement_helper(name, detail, CKB_CELL_FIELD_LOCK, enabled)
                }
                "__ckb_require_cell_type_script_hash_type" => {
                    self.emit_runtime_cell_script_hash_type_requirement_helper(name, detail, CKB_CELL_FIELD_TYPE, enabled)
                }
                "__c256_require_u128_product_lte" => self.emit_runtime_c256_product_requirement_helper(name, detail, false),
                "__c256_require_u128_product_eq" => self.emit_runtime_c256_product_requirement_helper(name, detail, true),
                "__c256_require_u128_sum2_products_lte" => self.emit_runtime_c256_sum2_product_requirement_helper(name, detail, false),
                "__c256_require_u128_sum2_products_eq" => self.emit_runtime_c256_sum2_product_requirement_helper(name, detail, true),
                "__ckb_cell_data_size" => self.emit_runtime_cell_data_size_helper(enabled),
                "__ckb_cell_data_u32_le" => self.emit_runtime_cell_data_word_le_helper(name, detail, 4, enabled),
                "__ckb_cell_data_u64_le" => self.emit_runtime_cell_data_word_le_helper(name, detail, 8, enabled),
                "__dao_accumulated_rate" => self.emit_runtime_dao_accumulated_rate_helper(enabled),
                "__dao_input_accumulated_rate" => self.emit_runtime_dao_input_accumulated_rate_helper(enabled),
                "__dao_has_dao_type" => self.emit_runtime_dao_type_classifier_helper(enabled),
                "__dao_is_deposit_data" => self.emit_runtime_dao_cell_data_classifier_helper(name, detail, true, enabled),
                "__dao_is_withdrawal_request_data" => {
                    self.emit_runtime_dao_cell_data_classifier_helper(name, detail, false, enabled);
                }
                "__dao_require_header_dep_for_input" => self.emit_runtime_dao_require_header_dep_for_input_helper(enabled),
                "__dao_require_input_since_at_least" => self.emit_runtime_dao_require_input_since_at_least_helper(enabled),
                "__dao_require_input_relative_epoch_since_at_least" => {
                    self.emit_runtime_dao_require_input_relative_epoch_since_at_least_helper(enabled);
                }
                "__xudt_amount_low" => self.emit_runtime_xudt_amount_word_helper(name, detail, 0, enabled),
                "__xudt_amount_high" => self.emit_runtime_xudt_amount_word_helper(name, detail, 8, enabled),
                "__xudt_owner_mode_input_type_hash" => {
                    self.emit_runtime_cell_field_low_word_helper(name, detail, CKB_CELL_FIELD_TYPE_HASH, enabled);
                }
                "__xudt_require_owner_mode_input_type" => self.emit_runtime_xudt_require_owner_mode_input_type_helper(enabled),
                "__xudt_require_owner_mode_type_args" => self.emit_runtime_xudt_require_owner_mode_type_args_helper(enabled),
                "__xudt_require_owner_mode_type_args_current_script" => {
                    self.emit_runtime_xudt_require_owner_mode_type_args_current_script_helper(enabled)
                }
                "__xudt_require_group_amount_conserved" => self.emit_runtime_xudt_require_group_amount_conserved_helper(enabled),
                "__xudt_require_group_amount_minted" => {
                    self.emit_runtime_xudt_require_group_amount_delta_helper(name, true, enabled);
                }
                "__xudt_require_group_amount_burned" => {
                    self.emit_runtime_xudt_require_group_amount_delta_helper(name, false, enabled);
                }
                "__ckb_witness_size" => self.emit_runtime_witness_size_helper(enabled),
                "__ckb_require_witness_size_at_least" => {
                    self.emit_runtime_require_witness_size_at_least_helper(enabled)
                }
                "__ckb_witness_raw" => self.emit_runtime_witness_raw_helper(enabled),
                "__ckb_witness_lock" => self.emit_runtime_witness_args_field_helper(name, detail, 0, enabled),
                "__ckb_witness_input_type" => self.emit_runtime_witness_args_field_helper(name, detail, 1, enabled),
                "__ckb_witness_output_type" => self.emit_runtime_witness_args_field_helper(name, detail, 2, enabled),
                _ => {
                    self.emit_global(name);
                    self.emit_label(name);
                    self.emit(format!("# cellscript abi: v0.14 CKB semantic helper ({})", detail));
                    if !enabled {
                        self.emit_fail(CellScriptRuntimeError::SyscallFailed);
                    } else {
                        self.emit("li a0, 0");
                        self.emit("ret");
                    }
                }
            }
        }

        if referenced_helpers.contains("__ckb_hash_chain") {
            self.emit_global("__ckb_hash_chain");
            self.emit_label("__ckb_hash_chain");
            self.emit("# cellscript abi: hash_chain aliases CKB Blake2b-256 over one 32-byte Hash input");
            if !enabled {
                self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            } else {
                self.emit("j __ckb_hash_blake2b");
            }
        }
        if referenced_helpers.contains("__ckb_hash_pair") {
            self.emit_runtime_blake2b_hash_pair(enabled);
        }
        if referenced_helpers.contains("__ckb_hash_chain")
            || referenced_helpers.contains("__ckb_hash_blake2b")
            || referenced_helpers.contains("__ckb_hash_data_packed")
        {
            self.emit_runtime_blake2b_hash32(enabled);
        }
        if referenced_helpers.contains("__ckb_hash_blake2b_var")
            || referenced_helpers.contains("__ckb_hash_data_packed")
            || referenced_helpers.contains("__ckb_hash_blake2b_packed")
            || referenced_helpers.contains("__ckb_cell_data_hash")
        {
            self.emit_runtime_blake2b_hash_var(enabled);
        }
    }

    fn emit_runtime_blake2b_hash_var(&mut self, enabled: bool) {
        self.emit_global("__ckb_hash_blake2b_var");
        self.emit_label("__ckb_hash_blake2b_var");
        self.emit("# cellscript abi: CKB Blake2b-256 variable helper; a0=input, a1=len, a2=output[32], returns a0=0");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }

        const IV: [u64; 8] = [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ];
        const SIGMA: [[usize; 16]; 12] = [
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
            [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
            [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
            [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
            [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
            [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
            [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
            [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
            [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        ];

        const H_BASE: usize = 0;
        const V_BASE: usize = 64;
        const M_BASE: usize = 192;
        const PTR: usize = 320;
        const LEN: usize = 328;
        const OUT: usize = 336;
        const POS: usize = 344;
        const CHUNK: usize = 352;
        const FRAME: usize = 384;

        let personal0 = u64::from_le_bytes(*b"ckb-defa");
        let personal1 = u64::from_le_bytes(*b"ult-hash");
        let h = [IV[0] ^ 0x01010020, IV[1], IV[2], IV[3], IV[4], IV[5], IV[6] ^ personal0, IV[7] ^ personal1];

        self.emit_large_addi("sp", "sp", -(FRAME as i64));
        self.emit_stack_store("a0", PTR);
        self.emit_stack_store("a1", LEN);
        self.emit_stack_store("a2", OUT);
        self.emit_stack_store("zero", POS);
        for (index, value) in h.iter().enumerate() {
            self.emit_blake2b_store_const(*value, H_BASE + index * 8);
        }

        let block_label = self.fresh_label("blake2b_var_block");
        let done_label = self.fresh_label("blake2b_var_done");
        self.emit_label(&block_label);
        self.emit_stack_load("t0", POS);
        self.emit_stack_load("t1", LEN);
        self.emit("sub t2, t1, t0");
        let empty_first_block_label = self.fresh_label("blake2b_var_empty_first_block");
        self.emit(format!("bnez t2, {}", empty_first_block_label));
        self.emit(format!("beqz t0, {}", empty_first_block_label));
        self.emit(format!("j {}", done_label));
        self.emit_label(&empty_first_block_label);
        self.emit("li t3, 128");
        self.emit("sltu t4, t3, t2");
        let chunk_rem_label = self.fresh_label("blake2b_var_chunk_rem");
        let chunk_set_label = self.fresh_label("blake2b_var_chunk_set");
        self.emit(format!("beqz t4, {}", chunk_rem_label));
        self.emit("li t2, 128");
        self.emit(format!("j {}", chunk_set_label));
        self.emit_label(&chunk_rem_label);
        self.emit("# chunk already in t2");
        self.emit_label(&chunk_set_label);
        self.emit_stack_store("t2", CHUNK);
        let zero_loop = self.fresh_label("blake2b_var_zero_loop");
        let zero_done = self.fresh_label("blake2b_var_zero_done");
        self.emit("li t0, 0");
        self.emit_label(&zero_loop);
        self.emit("li t1, 128");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", zero_done));
        self.emit(format!("li t3, {}", M_BASE));
        self.emit("add t3, sp, t3");
        self.emit("add t3, t3, t0");
        self.emit("sb zero, 0(t3)");
        self.emit("addi t0, t0, 1");
        self.emit(format!("j {}", zero_loop));
        self.emit_label(&zero_done);

        let copy_loop = self.fresh_label("blake2b_var_copy_loop");
        let copy_done = self.fresh_label("blake2b_var_copy_done");
        self.emit("li t0, 0");
        self.emit_label(&copy_loop);
        self.emit_stack_load("t1", CHUNK);
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", copy_done));
        self.emit_stack_load("t3", PTR);
        self.emit_stack_load("t4", POS);
        self.emit("add t3, t3, t4");
        self.emit("add t3, t3, t0");
        self.emit("lbu t5, 0(t3)");
        self.emit(format!("li t6, {}", M_BASE));
        self.emit("add t6, sp, t6");
        self.emit("add t6, t6, t0");
        self.emit("sb t5, 0(t6)");
        self.emit("addi t0, t0, 1");
        self.emit(format!("j {}", copy_loop));
        self.emit_label(&copy_done);

        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_store("t0", V_BASE + index * 8);
        }
        for (index, value) in IV.iter().enumerate() {
            self.emit_blake2b_store_const(*value, V_BASE + (index + 8) * 8);
        }
        self.emit_stack_load("t0", POS);
        self.emit_stack_load("t1", CHUNK);
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t2", V_BASE + 12 * 8);
        self.emit("xor t2, t2, t0");
        self.emit_stack_store("t2", V_BASE + 12 * 8);
        self.emit_stack_load("t2", V_BASE + 13 * 8);
        self.emit_stack_store("t2", V_BASE + 13 * 8);
        let not_final_label = self.fresh_label("blake2b_var_not_final");
        self.emit_stack_load("t3", LEN);
        self.emit("sub t4, t3, t0");
        self.emit(format!("bnez t4, {}", not_final_label));
        self.emit_stack_load("t5", V_BASE + 14 * 8);
        self.emit("xori t5, t5, -1");
        self.emit_stack_store("t5", V_BASE + 14 * 8);
        self.emit_label(&not_final_label);

        for round in SIGMA {
            self.emit_blake2b_g(V_BASE, M_BASE, 0, 4, 8, 12, round[0], round[1]);
            self.emit_blake2b_g(V_BASE, M_BASE, 1, 5, 9, 13, round[2], round[3]);
            self.emit_blake2b_g(V_BASE, M_BASE, 2, 6, 10, 14, round[4], round[5]);
            self.emit_blake2b_g(V_BASE, M_BASE, 3, 7, 11, 15, round[6], round[7]);
            self.emit_blake2b_g(V_BASE, M_BASE, 0, 5, 10, 15, round[8], round[9]);
            self.emit_blake2b_g(V_BASE, M_BASE, 1, 6, 11, 12, round[10], round[11]);
            self.emit_blake2b_g(V_BASE, M_BASE, 2, 7, 8, 13, round[12], round[13]);
            self.emit_blake2b_g(V_BASE, M_BASE, 3, 4, 9, 14, round[14], round[15]);
        }
        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_load("t1", V_BASE + index * 8);
            self.emit("xor t0, t0, t1");
            self.emit_stack_load("t1", V_BASE + (index + 8) * 8);
            self.emit("xor t0, t0, t1");
            self.emit_stack_store("t0", H_BASE + index * 8);
        }
        self.emit_stack_load("t0", POS);
        self.emit_stack_load("t1", CHUNK);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", POS);
        self.emit(format!("beqz t1, {}", done_label));
        self.emit(format!("j {}", block_label));

        self.emit_label(&done_label);
        self.emit_stack_load("t6", OUT);
        for index in 0..4 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit(format!("sd t0, {}(t6)", index * 8));
        }
        self.emit_large_addi("sp", "sp", FRAME as i64);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    fn emit_runtime_blake2b_hash32(&mut self, enabled: bool) {
        self.emit_global("__ckb_hash_blake2b");
        self.emit_label("__ckb_hash_blake2b");
        self.emit("# cellscript abi: CKB Blake2b-256 helper; a0=input[32], a1=output[32], returns a0=0");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }

        const IV: [u64; 8] = [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ];
        const SIGMA: [[usize; 16]; 12] = [
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
            [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
            [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
            [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
            [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
            [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
            [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
            [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
            [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        ];

        const H_BASE: usize = 0;
        const V_BASE: usize = 64;
        const M_BASE: usize = 192;
        const FRAME: usize = 320;

        let personal0 = u64::from_le_bytes(*b"ckb-defa");
        let personal1 = u64::from_le_bytes(*b"ult-hash");
        let h = [IV[0] ^ 0x01010020, IV[1], IV[2], IV[3], IV[4], IV[5], IV[6] ^ personal0, IV[7] ^ personal1];

        self.emit_large_addi("sp", "sp", -(FRAME as i64));
        for (index, value) in h.iter().enumerate() {
            self.emit_blake2b_store_const(*value, H_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_blake2b_load_input_word(index, M_BASE + index * 8);
        }
        for index in 4..16 {
            self.emit_stack_store("zero", M_BASE + index * 8);
        }
        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_store("t0", V_BASE + index * 8);
        }
        for (index, value) in IV.iter().enumerate() {
            self.emit_blake2b_store_const(*value, V_BASE + (index + 8) * 8);
        }
        self.emit_stack_load("t0", V_BASE + 12 * 8);
        self.emit("xori t0, t0, 32");
        self.emit_stack_store("t0", V_BASE + 12 * 8);
        self.emit_stack_load("t0", V_BASE + 14 * 8);
        self.emit("xori t0, t0, -1");
        self.emit_stack_store("t0", V_BASE + 14 * 8);

        for round in SIGMA {
            self.emit_blake2b_g(V_BASE, M_BASE, 0, 4, 8, 12, round[0], round[1]);
            self.emit_blake2b_g(V_BASE, M_BASE, 1, 5, 9, 13, round[2], round[3]);
            self.emit_blake2b_g(V_BASE, M_BASE, 2, 6, 10, 14, round[4], round[5]);
            self.emit_blake2b_g(V_BASE, M_BASE, 3, 7, 11, 15, round[6], round[7]);
            self.emit_blake2b_g(V_BASE, M_BASE, 0, 5, 10, 15, round[8], round[9]);
            self.emit_blake2b_g(V_BASE, M_BASE, 1, 6, 11, 12, round[10], round[11]);
            self.emit_blake2b_g(V_BASE, M_BASE, 2, 7, 8, 13, round[12], round[13]);
            self.emit_blake2b_g(V_BASE, M_BASE, 3, 4, 9, 14, round[14], round[15]);
        }

        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_load("t1", V_BASE + index * 8);
            self.emit("xor t0, t0, t1");
            self.emit_stack_load("t1", V_BASE + (index + 8) * 8);
            self.emit("xor t0, t0, t1");
            self.emit_stack_store("t0", H_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit(format!("sd t0, {}(a1)", index * 8));
        }
        self.emit_large_addi("sp", "sp", FRAME as i64);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    fn emit_runtime_blake2b_hash_pair(&mut self, enabled: bool) {
        self.emit_global("__ckb_hash_pair");
        self.emit_label("__ckb_hash_pair");
        self.emit("# cellscript abi: hash_pair combines two 32-byte Hash inputs with CKB Blake2b-256; a0=left[32], a1=right[32], a2=output[32]");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }

        const IV: [u64; 8] = [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ];
        const SIGMA: [[usize; 16]; 12] = [
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
            [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
            [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
            [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
            [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
            [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
            [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
            [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
            [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        ];

        const H_BASE: usize = 0;
        const V_BASE: usize = 64;
        const M_BASE: usize = 192;
        const FRAME: usize = 320;

        let personal0 = u64::from_le_bytes(*b"ckb-defa");
        let personal1 = u64::from_le_bytes(*b"ult-hash");
        let h = [IV[0] ^ 0x01010020, IV[1], IV[2], IV[3], IV[4], IV[5], IV[6] ^ personal0, IV[7] ^ personal1];

        self.emit_large_addi("sp", "sp", -(FRAME as i64));
        for (index, value) in h.iter().enumerate() {
            self.emit_blake2b_store_const(*value, H_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_blake2b_load_input_word(index, M_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_blake2b_load_input_word_from("a1", index, M_BASE + (index + 4) * 8);
        }
        for index in 8..16 {
            self.emit_stack_store("zero", M_BASE + index * 8);
        }
        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_store("t0", V_BASE + index * 8);
        }
        for (index, value) in IV.iter().enumerate() {
            self.emit_blake2b_store_const(*value, V_BASE + (index + 8) * 8);
        }
        self.emit_stack_load("t0", V_BASE + 12 * 8);
        self.emit("xori t0, t0, 64");
        self.emit_stack_store("t0", V_BASE + 12 * 8);
        self.emit_stack_load("t0", V_BASE + 14 * 8);
        self.emit("xori t0, t0, -1");
        self.emit_stack_store("t0", V_BASE + 14 * 8);

        for round in SIGMA {
            self.emit_blake2b_g(V_BASE, M_BASE, 0, 4, 8, 12, round[0], round[1]);
            self.emit_blake2b_g(V_BASE, M_BASE, 1, 5, 9, 13, round[2], round[3]);
            self.emit_blake2b_g(V_BASE, M_BASE, 2, 6, 10, 14, round[4], round[5]);
            self.emit_blake2b_g(V_BASE, M_BASE, 3, 7, 11, 15, round[6], round[7]);
            self.emit_blake2b_g(V_BASE, M_BASE, 0, 5, 10, 15, round[8], round[9]);
            self.emit_blake2b_g(V_BASE, M_BASE, 1, 6, 11, 12, round[10], round[11]);
            self.emit_blake2b_g(V_BASE, M_BASE, 2, 7, 8, 13, round[12], round[13]);
            self.emit_blake2b_g(V_BASE, M_BASE, 3, 4, 9, 14, round[14], round[15]);
        }

        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_load("t1", V_BASE + index * 8);
            self.emit("xor t0, t0, t1");
            self.emit_stack_load("t1", V_BASE + (index + 8) * 8);
            self.emit("xor t0, t0, t1");
            self.emit_stack_store("t0", H_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit(format!("sd t0, {}(a2)", index * 8));
        }
        self.emit_large_addi("sp", "sp", FRAME as i64);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    fn emit_blake2b_store_const(&mut self, value: u64, stack_offset: usize) {
        self.emit(format!("li t0, 0x{:016x}", value));
        self.emit_stack_store("t0", stack_offset);
    }

    fn emit_blake2b_load_input_word(&mut self, word_index: usize, stack_offset: usize) {
        self.emit_blake2b_load_input_word_from("a0", word_index, stack_offset);
    }

    fn emit_blake2b_load_input_word_from(&mut self, source_reg: &str, word_index: usize, stack_offset: usize) {
        self.emit("li t0, 0");
        for byte_index in 0..8 {
            let absolute = word_index * 8 + byte_index;
            self.emit(format!("lbu t1, {}({})", absolute, source_reg));
            if byte_index > 0 {
                self.emit(format!("slli t1, t1, {}", byte_index * 8));
            }
            self.emit("or t0, t0, t1");
        }
        self.emit_stack_store("t0", stack_offset);
    }

    fn emit_blake2b_rotr(&mut self, register: &str, bits: usize) {
        self.emit(format!("srli t1, {}, {}", register, bits));
        self.emit(format!("slli {}, {}, {}", register, register, 64 - bits));
        self.emit(format!("or {}, {}, t1", register, register));
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_blake2b_g(&mut self, v_base: usize, m_base: usize, a: usize, b: usize, c: usize, d: usize, mx: usize, my: usize) {
        let va = v_base + a * 8;
        let vb = v_base + b * 8;
        let vc = v_base + c * 8;
        let vd = v_base + d * 8;
        let vmx = m_base + mx * 8;
        let vmy = m_base + my * 8;

        self.emit_stack_load("t0", va);
        self.emit_stack_load("t1", vb);
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t1", vmx);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", va);
        self.emit_stack_load("t0", vd);
        self.emit_stack_load("t1", va);
        self.emit("xor t0, t0, t1");
        self.emit_blake2b_rotr("t0", 32);
        self.emit_stack_store("t0", vd);

        self.emit_stack_load("t0", vc);
        self.emit_stack_load("t1", vd);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", vc);
        self.emit_stack_load("t0", vb);
        self.emit_stack_load("t1", vc);
        self.emit("xor t0, t0, t1");
        self.emit_blake2b_rotr("t0", 24);
        self.emit_stack_store("t0", vb);

        self.emit_stack_load("t0", va);
        self.emit_stack_load("t1", vb);
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t1", vmy);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", va);
        self.emit_stack_load("t0", vd);
        self.emit_stack_load("t1", va);
        self.emit("xor t0, t0, t1");
        self.emit_blake2b_rotr("t0", 16);
        self.emit_stack_store("t0", vd);

        self.emit_stack_load("t0", vc);
        self.emit_stack_load("t1", vd);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", vc);
        self.emit_stack_load("t0", vb);
        self.emit_stack_load("t1", vc);
        self.emit("xor t0, t0, t1");
        self.emit_blake2b_rotr("t0", 63);
        self.emit_stack_store("t0", vb);
    }

    fn emit_runtime_witness_size_helper(&mut self, enabled: bool) {
        const SIZE_OFFSET: usize = 8;
        const RA_OFFSET: usize = 24;
        const FRAME_SIZE: usize = 32;

        self.emit_global("__ckb_witness_size");
        self.emit_label("__ckb_witness_size");
        self.emit("# cellscript abi: witness byte size via LOAD_WITNESS");
        self.emit("# cellscript abi: args a0=SourceView; returns a0=size, a1=0 on success, a1=error_code on failure");
        if !enabled {
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("witness_size_source_invalid");
        let failed = self.fresh_label("witness_size_load_failed");
        let status_ok = self.fresh_label("witness_size_status_ok");
        let done = self.fresh_label("witness_size_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 0");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit("li a0, 0");
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", status_ok));
        self.emit(format!("beqz a0, {}", status_ok));
        self.emit(format!("j {}", failed));

        self.emit_label(&status_ok);
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&failed);
        self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&done);
        self.emit(format!("ld a0, {}(sp)", SIZE_OFFSET));
        self.emit("li a1, 0");
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_require_witness_size_at_least_helper(&mut self, enabled: bool) {
        const SIZE_OFFSET: usize = 8;
        const MIN_SIZE_OFFSET: usize = 16;
        const RA_OFFSET: usize = 24;
        const FRAME_SIZE: usize = 32;

        self.emit_global("__ckb_require_witness_size_at_least");
        self.emit_label("__ckb_require_witness_size_at_least");
        self.emit("# cellscript abi: require witness size >= min_size");
        self.emit("# cellscript abi: args a0=SourceView, a1=min_size; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("witness_req_size_source_invalid");
        let failed = self.fresh_label("witness_req_size_load_failed");
        let too_small = self.fresh_label("witness_req_size_too_small");
        let status_ok = self.fresh_label("witness_req_size_status_ok");
        let done = self.fresh_label("witness_req_size_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit("# cellscript abi: preserve min_size before LOAD_WITNESS size probe");
        self.emit(format!("sd a1, {}(sp)", MIN_SIZE_OFFSET));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 0");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit("li a0, 0");
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", status_ok));
        self.emit(format!("beqz a0, {}", status_ok));
        self.emit(format!("j {}", failed));

        self.emit_label(&status_ok);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("ld t1, {}(sp)", MIN_SIZE_OFFSET));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", done));

        self.emit_label(&too_small);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::WitnessMalformed.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&done);
        self.emit("li a0, 0");
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_witness_raw_helper(&mut self, enabled: bool) {
        const OUTPTR_OFFSET: usize = 8;
        const SIZE_OFFSET: usize = 16;
        const RA_OFFSET: usize = 24;
        const FRAME_SIZE: usize = 32;

        self.emit_global("__ckb_witness_raw");
        self.emit_label("__ckb_witness_raw");
        self.emit("# cellscript abi: load raw witness bytes (first 32) into caller buffer");
        self.emit("# cellscript abi: args a0=SourceView, a1=out32_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("witness_raw_source_invalid");
        let failed = self.fresh_label("witness_raw_load_failed");
        let status_ok = self.fresh_label("witness_raw_status_ok");
        let done = self.fresh_label("witness_raw_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", OUTPTR_OFFSET));
        self.emit("# cellscript abi: zero-fill caller witness Hash buffer before raw prefix load");
        self.emit(format!("ld t0, {}(sp)", OUTPTR_OFFSET));
        self.emit("li t1, 0");
        let zero_loop = self.fresh_label("witness_raw_zero_loop");
        let zero_done = self.fresh_label("witness_raw_zero_done");
        self.emit_label(&zero_loop);
        self.emit("li t2, 32");
        self.emit("sltu t3, t1, t2");
        self.emit(format!("beqz t3, {}", zero_done));
        self.emit("add t4, t0, t1");
        self.emit("sb zero, 0(t4)");
        self.emit("addi t1, t1, 1");
        self.emit(format!("j {}", zero_loop));
        self.emit_label(&zero_done);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("# cellscript abi: LOAD_WITNESS raw first 32 bytes into caller buffer");
        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("ld a0, {}(sp)", OUTPTR_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", status_ok));
        self.emit(format!("beqz a0, {}", status_ok));
        self.emit(format!("j {}", failed));

        self.emit_label(&status_ok);
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&done);
        self.emit("li a0, 0");
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_witness_args_field_helper(&mut self, symbol: &str, detail: &str, field_index: u64, enabled: bool) {
        const OUTPTR_OFFSET: usize = 0;
        const SIZE_OFFSET: usize = 8;
        const FULL_BUFFER_OFFSET: usize = 16;
        const FULL_BUFFER_SIZE: usize = 512;
        const FIELD_BUF_OFFSET: usize = FULL_BUFFER_OFFSET + FULL_BUFFER_SIZE;
        const FIELD_BUF_SIZE: usize = 128;
        const HEADER_READ_OFFSET: usize = FIELD_BUF_OFFSET + FIELD_BUF_SIZE;
        const HEADER_READ_SIZE: usize = 24;
        const RA_OFFSET: usize = HEADER_READ_OFFSET + HEADER_READ_SIZE;
        const FRAME_SIZE: usize = RA_OFFSET + 8;

        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: extract WitnessArgs field {} ({})", field_index, detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=out32_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("witness_field_source_invalid");
        let failed = self.fresh_label("witness_field_load_failed");
        let malformed = self.fresh_label("witness_field_malformed");
        let truncated = self.fresh_label("witness_field_truncated");
        let field_absent = self.fresh_label("witness_field_absent");
        let ok = self.fresh_label("witness_field_ok");
        let done = self.fresh_label("witness_field_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", OUTPTR_OFFSET));
        self.emit("# cellscript abi: zero-fill extracted WitnessArgs Hash buffer before parsing");
        self.emit("li t0, 0");
        let zero_field_loop = self.fresh_label("witness_field_prezero_loop");
        let zero_field_done = self.fresh_label("witness_field_prezero_done");
        self.emit_label(&zero_field_loop);
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", zero_field_done));
        self.emit(format!("addi t3, sp, {}", FIELD_BUF_OFFSET));
        self.emit("add t3, t3, t0");
        self.emit("sb zero, 0(t3)");
        self.emit("addi t0, t0, 1");
        self.emit(format!("j {}", zero_field_loop));
        self.emit_label(&zero_field_done);
        self.emit_decode_source_view_to_t1_t2(&invalid);

        // Load full witness
        self.emit(format!("li t0, {}", FULL_BUFFER_SIZE));
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", FULL_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", ok));
        self.emit(format!("j {}", failed));

        self.emit_label(&ok);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));

        // Parse Molecule WitnessArgs table header (minimum 4 + 3*4 = 16 bytes)
        // Table encoding: total_size (4 bytes) + offsets[0..N-1] (4 bytes each)
        // field_count = (offset0 / 4) - 1
        self.emit("li t1, 16");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit(format!("addi t3, sp, {}", FULL_BUFFER_OFFSET));
        self.emit("# cellscript abi: WitnessArgs total_size must match loaded witness size");
        self.emit_u32_le_from_base_to("t4", "t3", 0, "t5");
        self.emit("sub t2, t4, t0");
        self.emit(format!("bnez t2, {}", malformed));

        // For the current 3-field WitnessArgs table, offset0 must be 16.
        self.emit_u32_le_from_base_to("t4", "t3", 4, "t5");
        self.emit("li t5, 16");
        self.emit("sub t2, t4, t5");
        self.emit(format!("bnez t2, {}", malformed));

        // Read field offsets from header (offsets at bytes 4, 8, 12)
        self.emit_u32_le_from_base_to("t4", "t3", 4, "t2");
        self.emit(format!("sd t4, {}(sp)", HEADER_READ_OFFSET));
        self.emit_u32_le_from_base_to("t5", "t3", 8, "t2");
        self.emit(format!("sd t5, {}(sp)", HEADER_READ_OFFSET + 8));
        self.emit_u32_le_from_base_to("t6", "t3", 12, "t2");
        self.emit(format!("sd t6, {}(sp)", HEADER_READ_OFFSET + 16));

        self.emit("# cellscript abi: validate all WitnessArgs field offsets are monotonic and in bounds");
        self.emit(format!("ld t4, {}(sp)", HEADER_READ_OFFSET));
        self.emit(format!("ld t5, {}(sp)", HEADER_READ_OFFSET + 8));
        self.emit("sltu t2, t5, t4");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit(format!("ld t4, {}(sp)", HEADER_READ_OFFSET + 8));
        self.emit(format!("ld t5, {}(sp)", HEADER_READ_OFFSET + 16));
        self.emit("sltu t2, t5, t4");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("sltu t2, t0, t5");
        self.emit(format!("bnez t2, {}", truncated));

        // Select field offset and next field offset
        let field_offsets_offset = HEADER_READ_OFFSET + (field_index * 8) as usize;
        let next_offsets_offset = HEADER_READ_OFFSET + ((field_index + 1) * 8) as usize;
        self.emit(format!("ld t4, {}(sp)", field_offsets_offset));
        if field_index < 2 {
            self.emit(format!("ld t5, {}(sp)", next_offsets_offset));
        } else {
            self.emit("addi t5, t0, 0".to_string());
        }

        // Check field offset bounds: field_offset <= next_offset <= total_size.
        // BytesOpt None is an empty span, so adjacent offsets may be equal.
        self.emit("sltu t2, t5, t4");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("sltu t2, t0, t5");
        self.emit(format!("bnez t2, {}", truncated));

        // Calculate BytesOpt field span. Empty span is None.
        self.emit("sub t2, t5, t4");
        self.emit(format!("beqz t2, {}", field_absent));
        self.emit("li t6, 4");
        self.emit("sltu t3, t2, t6");
        self.emit(format!("bnez t3, {}", malformed));
        self.emit("addi t2, t2, -4");

        // Read Some(Bytes) length at field_offset and require exact Bytes size.
        self.emit(format!("addi t3, sp, {}", FULL_BUFFER_OFFSET));
        self.emit("add t6, t3, t4");
        self.emit_u32_le_from_base_to("t1", "t6", 0, "t3");
        self.emit("sub t3, t2, t1");
        self.emit(format!("bnez t3, {}", malformed));

        // Copy field bytes to output buffer (max 32 bytes for Hash)
        self.emit("li t3, 32");
        self.emit("sltu t5, t3, t1");
        let copy_count_ready = self.fresh_label("witness_field_copy_count_ready");
        self.emit(format!("beqz t5, {}", copy_count_ready));
        self.emit("addi t1, t3, 0");
        self.emit_label(&copy_count_ready);
        self.emit(format!("addi t2, sp, {}", FIELD_BUF_OFFSET));
        self.emit("addi t4, t6, 4");
        // Copy loop
        self.emit("li t3, 0");
        let copy_loop = self.fresh_label("witness_field_copy_loop");
        let copy_done = self.fresh_label("witness_field_copy_done");
        self.emit_label(&copy_loop);
        self.emit("sltu t5, t3, t1");
        self.emit(format!("beqz t5, {}", copy_done));
        self.emit("add t5, t4, t3");
        self.emit("lbu t6, 0(t5)");
        self.emit("add t5, t2, t3");
        self.emit("sb t6, 0(t5)");
        self.emit("addi t3, t3, 1");
        self.emit(format!("j {}", copy_loop));
        self.emit_label(&copy_done);
        self.emit(format!("j {}", done));

        self.emit_label(&field_absent);
        self.emit("# cellscript abi: BytesOpt None leaves pre-zeroed Hash buffer");
        self.emit(format!("j {}", done));

        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::WitnessMalformed.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&truncated);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::WitnessFieldTruncated.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");

        self.emit_label(&done);
        // Copy 32 bytes from FIELD_BUF_OFFSET to caller's buffer (outptr)
        self.emit(format!("ld t0, {}(sp)", OUTPTR_OFFSET));
        self.emit("li t1, 0");
        let copy_out = self.fresh_label("witness_field_copy_out_loop");
        let copy_out_done = self.fresh_label("witness_field_copy_out_done");
        self.emit_label(&copy_out);
        self.emit("li t2, 32");
        self.emit("sltu t3, t1, t2");
        self.emit(format!("beqz t3, {}", copy_out_done));
        self.emit(format!("addi t2, sp, {}", FIELD_BUF_OFFSET));
        self.emit("add t2, t2, t1");
        self.emit("lbu t3, 0(t2)");
        self.emit("add t4, t0, t1");
        self.emit("sb t3, 0(t4)");
        self.emit("addi t1, t1, 1");
        self.emit(format!("j {}", copy_out));
        self.emit_label(&copy_out_done);
        self.emit("li a0, 0");
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_current_script_hash_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_current_script_hash");
        self.emit_label("__ckb_current_script_hash");
        self.emit("# cellscript abi: current script Hash via LOAD_SCRIPT_HASH");
        self.emit("# cellscript abi: args a0=out32_ptr, a1=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let failed = self.fresh_label("current_script_hash_load_failed");
        let malformed = self.fresh_label("current_script_hash_malformed");
        let done = self.fresh_label("current_script_hash_done");
        let abi = self.runtime_abi();
        self.emit("addi sp, sp, -24");
        self.emit("sd ra, 16(sp)");
        self.emit("sd a1, 8(sp)");
        self.emit("li t0, 32");
        self.emit("sd t0, 0(a1)");
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit("ld t6, 8(sp)");
        self.emit("ld t0, 0(t6)");
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit_label(&done);
        self.emit("ld ra, 16(sp)");
        self.emit("addi sp, sp, 24");
        self.emit("ret");
    }

    fn emit_runtime_source_view_helper(&mut self, symbol: &str, source_view: u64, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView helper ({})", detail));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        self.emit(format!("li t0, {}", source_view));
        self.emit(format!("li t1, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("mul t0, t0, t1");
        self.emit("add a0, a0, t0");
        self.emit("li a1, 0");
        self.emit("ret");
    }

    fn emit_runtime_ckb_since_epoch_helper(&mut self, symbol: &str, relative: bool, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {}", detail));
        self.emit("# cellscript abi: args a0=number(<2^24), a1=index(<2^16), a2=length(<2^16); requires length>0 and index<length");
        self.emit("# cellscript abi: encodes CKB RFC0017 EpochNumberWithFraction as number | index<<24 | length<<40");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let malformed = self.fresh_label("ckb_since_epoch_malformed");
        let done = self.fresh_label("ckb_since_epoch_done");
        self.emit(format!("li t0, {}", CKB_EPOCH_NUMBER_BOUND));
        self.emit("sltu t1, a0, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("li t0, {}", CKB_EPOCH_FRACTION_BOUND));
        self.emit("sltu t1, a1, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit("sltu t1, a2, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("beqz a2, {}", malformed));
        self.emit("sltu t1, a1, a2");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit("addi t2, a0, 0");
        self.emit("slli t0, a1, 24");
        self.emit("or t2, t2, t0");
        self.emit("slli t0, a2, 40");
        self.emit("or t2, t2, t0");
        self.emit(format!("li t0, {}", CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG));
        self.emit("or t2, t2, t0");
        if relative {
            self.emit("li t0, 1");
            self.emit("slli t0, t0, 63");
            self.emit("or t2, t2, t0");
        }
        self.emit("addi a0, t2, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    fn emit_decode_source_view_to_t1_t2(&mut self, invalid_label: &str) {
        let done = self.fresh_label("source_view_decoded");
        self.emit(format!("li t6, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("div t0, a0, t6");
        self.emit("rem t1, a0, t6");
        for (view, source) in [
            (CKB_SOURCE_VIEW_INPUT, CKB_SOURCE_INPUT),
            (CKB_SOURCE_VIEW_OUTPUT, CKB_SOURCE_OUTPUT),
            (CKB_SOURCE_VIEW_CELL_DEP, CKB_SOURCE_CELL_DEP),
            (CKB_SOURCE_VIEW_HEADER_DEP, CKB_SOURCE_HEADER_DEP),
            (CKB_SOURCE_VIEW_GROUP_INPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT),
            (CKB_SOURCE_VIEW_GROUP_OUTPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT),
        ] {
            let next = self.fresh_label("source_view_next");
            self.emit(format!("li t5, {}", view));
            self.emit("sub t4, t0, t5");
            self.emit(format!("bnez t4, {}", next));
            self.emit(format!("li t2, {}", source));
            self.emit(format!("j {}", done));
            self.emit_label(&next);
        }
        self.emit(format!("j {}", invalid_label));
        self.emit_label(&done);
    }

    fn emit_decode_input_source_view_to_t1_t2(&mut self, invalid_label: &str) {
        let done = self.fresh_label("input_source_view_decoded");
        self.emit_decode_source_view_to_t1_t2(invalid_label);
        self.emit(format!("li t0, {}", CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", done));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", done));
        self.emit(format!("j {}", invalid_label));
        self.emit_label(&done);
    }

    fn emit_runtime_cell_field_u64_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView LOAD_CELL_BY_FIELD ({})", detail));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("source_view_invalid");
        let done = self.fresh_label("cell_field_done");
        let failed = self.fresh_label("cell_field_failed");
        let abi = self.runtime_abi();
        self.emit("addi sp, sp, -48");
        self.emit("sd ra, 40(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit("ld a0, 16(sp)");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit("addi sp, sp, 48");
        self.emit("ret");
    }

    fn emit_runtime_cell_field_low_word_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_runtime_cell_field_u64_helper(symbol, detail, field_id, enabled);
    }

    fn emit_runtime_cell_hash_field_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView LOAD_CELL_BY_FIELD full hash ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=out32_ptr, a2=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const BUFFER_OFFSET: usize = 16;
        const OUT_PTR_OFFSET: usize = 56;
        const SIZE_PTR_OFFSET: usize = 64;
        const RA_OFFSET: usize = 72;
        const FRAME_SIZE: usize = 80;

        let invalid = self.fresh_label("cell_hash_source_invalid");
        let bad_output = self.fresh_label("cell_hash_output_invalid");
        let failed = self.fresh_label("cell_hash_load_failed");
        let copy_loop = self.fresh_label("cell_hash_copy");
        let copy_done = self.fresh_label("cell_hash_copy_done");
        let done = self.fresh_label("cell_hash_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", OUT_PTR_OFFSET));
        self.emit(format!("sd a2, {}(sp)", SIZE_PTR_OFFSET));
        self.emit(format!("beqz a1, {}", bad_output));
        self.emit(format!("beqz a2, {}", bad_output));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));

        self.emit("li t1, 0");
        self.emit_label(&copy_loop);
        self.emit("li t2, 32");
        self.emit("sltu t3, t1, t2");
        self.emit(format!("beqz t3, {}", copy_done));
        self.emit(format!("addi t6, sp, {}", BUFFER_OFFSET));
        self.emit("add t6, t6, t1");
        self.emit("lbu t5, 0(t6)");
        self.emit(format!("ld t6, {}(sp)", OUT_PTR_OFFSET));
        self.emit("add t6, t6, t1");
        self.emit("sb t5, 0(t6)");
        self.emit("addi t1, t1, 1");
        self.emit(format!("j {}", copy_loop));

        self.emit_label(&copy_done);
        self.emit(format!("ld t6, {}(sp)", SIZE_PTR_OFFSET));
        self.emit("li t0, 32");
        self.emit("sd t0, 0(t6)");
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_output);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_input_out_point_word_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        out_point_offset: usize,
        width: usize,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView LOAD_INPUT_BY_FIELD OutPoint ({})", detail));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("input_out_point_source_invalid");
        let failed = self.fresh_label("input_out_point_load_failed");
        let done = self.fresh_label("input_out_point_done");
        let abi = self.runtime_abi();
        self.emit("addi sp, sp, -80");
        self.emit("sd ra, 72(sp)");
        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 36");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit("ld t0, 8(sp)");
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit("addi t4, sp, 16");
        self.emit_unaligned_scalar_load("t4", "a0", "t3", out_point_offset, width);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 72(sp)");
        self.emit("addi sp, sp, 80");
        self.emit("ret");
    }

    fn emit_runtime_input_out_point_tx_hash_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_input_out_point_tx_hash");
        self.emit_label("__ckb_input_out_point_tx_hash");
        self.emit("# cellscript abi: CKB SourceView LOAD_INPUT_BY_FIELD OutPoint full tx-hash read");
        self.emit("# cellscript abi: args a0=input SourceView, a1=out32_ptr, a2=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const OUT_PTR_OFFSET: usize = 8;
        const SIZE_PTR_OFFSET: usize = 16;
        const OUT_POINT_SIZE_OFFSET: usize = 24;
        const OUT_POINT_OFFSET: usize = 32;
        const RA_OFFSET: usize = 72;
        const FRAME_SIZE: usize = 80;

        let invalid = self.fresh_label("input_out_point_hash_source_invalid");
        let bad_output = self.fresh_label("input_out_point_hash_output_invalid");
        let failed = self.fresh_label("input_out_point_hash_load_failed");
        let copy_loop = self.fresh_label("input_out_point_hash_copy");
        let copy_done = self.fresh_label("input_out_point_hash_copy_done");
        let done = self.fresh_label("input_out_point_hash_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", OUT_PTR_OFFSET));
        self.emit(format!("sd a2, {}(sp)", SIZE_PTR_OFFSET));
        self.emit(format!("beqz a1, {}", bad_output));
        self.emit(format!("beqz a2, {}", bad_output));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 36");
        self.emit(format!("sd t0, {}(sp)", OUT_POINT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", OUT_POINT_OFFSET));
        self.emit(format!("addi a1, sp, {}", OUT_POINT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("ld t0, {}(sp)", OUT_POINT_SIZE_OFFSET));
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));

        self.emit("li t0, 0");
        self.emit_label(&copy_loop);
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", copy_done));
        self.emit(format!("addi t3, sp, {}", OUT_POINT_OFFSET));
        self.emit("add t3, t3, t0");
        self.emit("lbu t4, 0(t3)");
        self.emit(format!("ld t5, {}(sp)", OUT_PTR_OFFSET));
        self.emit("add t5, t5, t0");
        self.emit("sb t4, 0(t5)");
        self.emit("addi t0, t0, 1");
        self.emit(format!("j {}", copy_loop));
        self.emit_label(&copy_done);
        self.emit(format!("ld t0, {}(sp)", SIZE_PTR_OFFSET));
        self.emit("li t1, 32");
        self.emit("sd t1, 0(t0)");
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_output);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_input_out_point_tx_hash_requirement_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_require_input_out_point_tx_hash");
        self.emit_label("__ckb_require_input_out_point_tx_hash");
        self.emit("# cellscript abi: CKB SourceView LOAD_INPUT_BY_FIELD OutPoint full tx-hash requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=expected_tx_hash_ptr, a2=expected_tx_hash_len");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("input_out_point_source_invalid");
        let bad_expected = self.fresh_label("input_out_point_expected_invalid");
        let failed = self.fresh_label("input_out_point_load_failed");
        let mismatch = self.fresh_label("input_out_point_tx_hash_mismatch");
        let done = self.fresh_label("input_out_point_tx_hash_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -96");
        self.emit("sd ra, 88(sp)");
        self.emit("sd a1, 80(sp)");
        self.emit("sd a2, 72(sp)");

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 36");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit("ld t0, 8(sp)");
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit("addi a0, sp, 16");
        self.emit("ld a1, 80(sp)");
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 88(sp)");
        self.emit("addi sp, sp, 96");
        self.emit("ret");
    }

    fn emit_runtime_input_out_point_requirement_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_require_input_out_point");
        self.emit_label("__ckb_require_input_out_point");
        self.emit("# cellscript abi: CKB SourceView LOAD_INPUT_BY_FIELD OutPoint full tx-hash + index requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=expected_tx_hash_ptr, a2=expected_tx_hash_len, a3=expected_index");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("input_out_point_source_invalid");
        let bad_expected = self.fresh_label("input_out_point_expected_invalid");
        let failed = self.fresh_label("input_out_point_load_failed");
        let mismatch = self.fresh_label("input_out_point_mismatch");
        let done = self.fresh_label("input_out_point_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -112");
        self.emit("sd ra, 104(sp)");
        self.emit("sd a1, 96(sp)");
        self.emit("sd a2, 88(sp)");
        self.emit("sd a3, 80(sp)");

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 36");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit("ld t0, 8(sp)");
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));

        self.emit("addi a0, sp, 16");
        self.emit("ld a1, 96(sp)");
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));

        self.emit("addi t0, sp, 16");
        self.emit_unaligned_scalar_load("t0", "t1", "t2", 32, 4);
        self.emit("ld t3, 80(sp)");
        self.emit("sub t4, t1, t3");
        self.emit(format!("bnez t4, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 104(sp)");
        self.emit("addi sp, sp, 112");
        self.emit("ret");
    }

    fn emit_runtime_metapoint_relative_requirement_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_require_metapoint_relative");
        self.emit_label("__ckb_require_metapoint_relative");
        self.emit("# cellscript abi: CKB SourceView MetaPoint relative-distance requirement");
        self.emit("# cellscript abi: args a0=base SourceView, a1=related SourceView, a2=signed i32 distance");
        self.emit("# cellscript abi: input metapoint = input OutPoint(tx_hash,index); output metapoint = output index");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const BASE_VIEW_OFFSET: usize = 8;
        const RELATED_VIEW_OFFSET: usize = 16;
        const DISTANCE_OFFSET: usize = 24;
        const BASE_SOURCE_OFFSET: usize = 32;
        const BASE_INDEX_OFFSET: usize = 40;
        const RELATED_SOURCE_OFFSET: usize = 48;
        const RELATED_INDEX_OFFSET: usize = 56;
        const BASE_SIZE_OFFSET: usize = 64;
        const RELATED_SIZE_OFFSET: usize = 72;
        const BASE_OUT_POINT_OFFSET: usize = 80;
        const RELATED_OUT_POINT_OFFSET: usize = 120;

        let invalid = self.fresh_label("metapoint_source_invalid");
        let input_pair = self.fresh_label("metapoint_input_pair");
        let output_pair = self.fresh_label("metapoint_output_pair");
        let load_failed = self.fresh_label("metapoint_load_failed");
        let mismatch = self.fresh_label("metapoint_mismatch");
        let done = self.fresh_label("metapoint_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -192");
        self.emit("sd ra, 184(sp)");
        self.emit(format!("sd a0, {}(sp)", BASE_VIEW_OFFSET));
        self.emit(format!("sd a1, {}(sp)", RELATED_VIEW_OFFSET));
        self.emit_sign_extend_i32("a2");
        self.emit(format!("sd a2, {}(sp)", DISTANCE_OFFSET));

        self.emit("# cellscript abi: decode base MetaPoint SourceView");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("sd t2, {}(sp)", BASE_SOURCE_OFFSET));
        self.emit(format!("sd t1, {}(sp)", BASE_INDEX_OFFSET));

        self.emit("# cellscript abi: decode related MetaPoint SourceView");
        self.emit(format!("ld a0, {}(sp)", RELATED_VIEW_OFFSET));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("sd t2, {}(sp)", RELATED_SOURCE_OFFSET));
        self.emit(format!("sd t1, {}(sp)", RELATED_INDEX_OFFSET));

        self.emit("# cellscript abi: MetaPoint relation requires both views from the same source class");
        self.emit(format!("ld t0, {}(sp)", BASE_SOURCE_OFFSET));
        self.emit(format!("ld t1, {}(sp)", RELATED_SOURCE_OFFSET));
        self.emit("sub t3, t0, t1");
        self.emit(format!("bnez t3, {}", mismatch));

        self.emit(format!("li t4, {}", CKB_SOURCE_INPUT));
        self.emit("sub t3, t0, t4");
        self.emit(format!("beqz t3, {}", input_pair));
        self.emit(format!("li t4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t3, t0, t4");
        self.emit(format!("beqz t3, {}", input_pair));
        self.emit(format!("li t4, {}", CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t0, t4");
        self.emit(format!("beqz t3, {}", output_pair));
        self.emit(format!("li t4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t0, t4");
        self.emit(format!("beqz t3, {}", output_pair));
        self.emit(format!("j {}", invalid));

        self.emit_label(&output_pair);
        self.emit("# cellscript abi: output MetaPoint compare base_output_index + distance == related_output_index");
        self.emit(format!("ld t0, {}(sp)", BASE_INDEX_OFFSET));
        self.emit(format!("ld t1, {}(sp)", DISTANCE_OFFSET));
        self.emit("add t0, t0, t1");
        self.emit("slt t4, t0, zero");
        self.emit(format!("bnez t4, {}", mismatch));
        self.emit(format!("ld t2, {}(sp)", RELATED_INDEX_OFFSET));
        self.emit("sub t3, t0, t2");
        self.emit(format!("bnez t3, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&input_pair);
        self.emit("# cellscript abi: input MetaPoint compare OutPoint tx_hash and base_out_index + distance");
        self.emit("li t0, 36");
        self.emit(format!("sd t0, {}(sp)", BASE_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", BASE_OUT_POINT_OFFSET));
        self.emit(format!("addi a1, sp, {}", BASE_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", BASE_INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", BASE_SOURCE_OFFSET));
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", load_failed));
        self.emit(format!("ld t0, {}(sp)", BASE_SIZE_OFFSET));
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", load_failed));

        self.emit("li t0, 36");
        self.emit(format!("sd t0, {}(sp)", RELATED_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", RELATED_OUT_POINT_OFFSET));
        self.emit(format!("addi a1, sp, {}", RELATED_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", RELATED_INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", RELATED_SOURCE_OFFSET));
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", load_failed));
        self.emit(format!("ld t0, {}(sp)", RELATED_SIZE_OFFSET));
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", load_failed));

        self.emit(format!("addi a0, sp, {}", BASE_OUT_POINT_OFFSET));
        self.emit(format!("addi a1, sp, {}", RELATED_OUT_POINT_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit(format!("addi t0, sp, {}", BASE_OUT_POINT_OFFSET));
        self.emit_unaligned_scalar_load("t0", "t1", "t2", 32, 4);
        self.emit(format!("ld t3, {}(sp)", DISTANCE_OFFSET));
        self.emit("add t1, t1, t3");
        self.emit("slt t4, t1, zero");
        self.emit(format!("bnez t4, {}", mismatch));
        self.emit(format!("addi t0, sp, {}", RELATED_OUT_POINT_OFFSET));
        self.emit_unaligned_scalar_load("t0", "t2", "t3", 32, 4);
        self.emit("sub t4, t1, t2");
        self.emit(format!("bnez t4, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&load_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::MetaPointMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 184(sp)");
        self.emit("addi sp, sp, 192");
        self.emit("ret");
    }

    fn emit_runtime_current_script_role_at_helper(&mut self, enabled: bool) {
        self.emit_global("__cellscript_current_script_role_at");
        self.emit_label("__cellscript_current_script_role_at");
        self.emit("# cellscript abi: classify one cell against current script hash");
        self.emit("# cellscript abi: args a0=source, a1=index, a2=current_script_hash_ptr; returns a0=role(0 none,1 lock-only,2 type-only,3 both), a1=status");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SOURCE_OFFSET: usize = 8;
        const INDEX_OFFSET: usize = 16;
        const SCRIPT_HASH_PTR_OFFSET: usize = 24;
        const SIZE_OFFSET: usize = 32;
        const HASH_BUFFER_OFFSET: usize = 40;
        const LOCK_MATCH_OFFSET: usize = 72;
        const TYPE_MATCH_OFFSET: usize = 80;

        let bad_args = self.fresh_label("current_script_role_bad_args");
        let lock_loaded = self.fresh_label("current_script_role_lock_loaded");
        let lock_not_match = self.fresh_label("current_script_role_lock_not_match");
        let type_loaded = self.fresh_label("current_script_role_type_loaded");
        let type_missing = self.fresh_label("current_script_role_type_missing");
        let type_not_match = self.fresh_label("current_script_role_type_not_match");
        let build_role = self.fresh_label("current_script_role_build");
        let out_of_bound = self.fresh_label("current_script_role_oob");
        let failed = self.fresh_label("current_script_role_failed");
        let done = self.fresh_label("current_script_role_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -96");
        self.emit("sd ra, 88(sp)");
        self.emit(format!("sd a0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("sd a1, {}(sp)", INDEX_OFFSET));
        self.emit(format!("sd a2, {}(sp)", SCRIPT_HASH_PTR_OFFSET));
        self.emit(format!("sd zero, {}(sp)", LOCK_MATCH_OFFSET));
        self.emit(format!("sd zero, {}(sp)", TYPE_MATCH_OFFSET));
        self.emit(format!("beqz a2, {}", bad_args));

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", HASH_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_LOCK_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", lock_loaded));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", out_of_bound));
        self.emit(format!("j {}", failed));

        self.emit_label(&lock_loaded);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit(format!("addi a0, sp, {}", HASH_BUFFER_OFFSET));
        self.emit(format!("ld a1, {}(sp)", SCRIPT_HASH_PTR_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", lock_not_match));
        self.emit("li t0, 1");
        self.emit(format!("sd t0, {}(sp)", LOCK_MATCH_OFFSET));
        self.emit_label(&lock_not_match);

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", HASH_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", type_loaded));
        self.emit(format!("li t0, {}", CKB_ITEM_MISSING));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", type_missing));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", out_of_bound));
        self.emit(format!("j {}", failed));

        self.emit_label(&type_loaded);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit(format!("addi a0, sp, {}", HASH_BUFFER_OFFSET));
        self.emit(format!("ld a1, {}(sp)", SCRIPT_HASH_PTR_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", type_not_match));
        self.emit("li t0, 1");
        self.emit(format!("sd t0, {}(sp)", TYPE_MATCH_OFFSET));
        self.emit_label(&type_not_match);
        self.emit(format!("j {}", build_role));

        self.emit_label(&type_missing);
        self.emit(format!("j {}", build_role));

        self.emit_label(&build_role);
        self.emit(format!("ld t0, {}(sp)", LOCK_MATCH_OFFSET));
        self.emit(format!("ld t1, {}(sp)", TYPE_MATCH_OFFSET));
        self.emit("slli t1, t1, 1");
        self.emit("add a0, t0, t1");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&out_of_bound);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_args);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&done);
        self.emit("ld ra, 88(sp)");
        self.emit("addi sp, sp, 96");
        self.emit("ret");
    }

    fn emit_runtime_metapoint_pair_cardinality_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        lock_to_type: bool,
        distance_from_base_data: bool,
        related_filter: bool,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {}", detail));
        self.emit("# cellscript abi: scans current-script lock-only/type-only cells and requires one-to-one MetaPoint pairing");
        if related_filter {
            self.emit("# cellscript abi: related role cells must match expected TypeHash and generic data rule");
            self.emit("# cellscript abi: filtered data rules: 0=no data check, 1=exact 8-byte zero u64, 2=exact 8-byte nonzero u64");
        }
        if distance_from_base_data {
            self.emit("# cellscript abi: args a0=SourceView selecting Input/Output source class, a1=base-cell data offset containing signed i32 distance");
        } else {
            self.emit("# cellscript abi: args a0=SourceView selecting Input/Output source class, a1=signed i32 distance");
        }
        if related_filter {
            self.emit("# cellscript abi: filtered args a2=expected_related_type_hash_ptr, a3=hash_len, a4=related_data_rule");
        }
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const INPUT_VIEW_OFFSET: usize = 8;
        const SOURCE_OFFSET: usize = 16;
        const VIEW_KIND_OFFSET: usize = 24;
        const DISTANCE_OFFSET: usize = 32;
        const BASE_INDEX_OFFSET: usize = 40;
        const RELATED_INDEX_OFFSET: usize = 48;
        const BASE_COUNT_OFFSET: usize = 56;
        const RELATED_COUNT_OFFSET: usize = 64;
        const MATCH_COUNT_OFFSET: usize = 72;
        const SIZE_OFFSET: usize = 80;
        const DATA_OFFSET_OFFSET: usize = 88;
        const FILTER_RULE_OFFSET: usize = 96;
        const EXPECTED_HASH_PTR_OFFSET: usize = 104;
        const EXPECTED_HASH_LEN_OFFSET: usize = 112;
        const SCRIPT_HASH_OFFSET: usize = 128;
        const TYPE_HASH_BUFFER_OFFSET: usize = 160;
        const DATA_BUFFER_OFFSET: usize = 192;
        const RA_OFFSET: usize = 216;
        const STACK_SIZE: usize = 224;

        let invalid = self.fresh_label("metapoint_pair_source_invalid");
        let source_input = self.fresh_label("metapoint_pair_source_input");
        let source_group_input = self.fresh_label("metapoint_pair_source_group_input");
        let source_output = self.fresh_label("metapoint_pair_source_output");
        let source_group_output = self.fresh_label("metapoint_pair_source_group_output");
        let source_ready = self.fresh_label("metapoint_pair_source_ready");
        let hash_failed = self.fresh_label("metapoint_pair_hash_failed");
        let outer_loop = self.fresh_label("metapoint_pair_outer_loop");
        let outer_done = self.fresh_label("metapoint_pair_outer_done");
        let outer_role_ok = self.fresh_label("metapoint_pair_outer_role_ok");
        let maybe_related = self.fresh_label("metapoint_pair_maybe_related");
        let inner_loop = self.fresh_label("metapoint_pair_inner_loop");
        let inner_done = self.fresh_label("metapoint_pair_inner_done");
        let inner_role_candidate = self.fresh_label("metapoint_pair_inner_role_candidate");
        let relation_matched = self.fresh_label("metapoint_pair_relation_matched");
        let advance_related = self.fresh_label("metapoint_pair_advance_related");
        let increment_outer = self.fresh_label("metapoint_pair_increment_outer");
        let status_failed = self.fresh_label("metapoint_pair_status_failed");
        let relation_failed = self.fresh_label("metapoint_pair_relation_failed");
        let role_mismatch = self.fresh_label("metapoint_pair_role_mismatch");
        let bad_expected = self.fresh_label("metapoint_pair_filter_expected_invalid");
        let bad_data_rule = self.fresh_label("metapoint_pair_filter_data_rule_invalid");
        let related_type_mismatch = self.fresh_label("metapoint_pair_related_type_mismatch");
        let related_data_mismatch = self.fresh_label("metapoint_pair_related_data_mismatch");
        let data_loaded = self.fresh_label("metapoint_pair_data_loaded");
        let data_len_enough = self.fresh_label("metapoint_pair_data_len_enough");
        let data_malformed = self.fresh_label("metapoint_pair_data_malformed");
        let distance_ready = self.fresh_label("metapoint_pair_distance_ready");
        let cardinality = self.fresh_label("metapoint_pair_cardinality");
        let done = self.fresh_label("metapoint_pair_done");
        let abi = self.runtime_abi();
        let base_role = if lock_to_type { 1 } else { 2 };
        let related_role = if lock_to_type { 2 } else { 1 };

        self.emit(format!("addi sp, sp, -{}", STACK_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a0, {}(sp)", INPUT_VIEW_OFFSET));
        if distance_from_base_data {
            self.emit(format!("sd a1, {}(sp)", DATA_OFFSET_OFFSET));
            self.emit(format!("sd zero, {}(sp)", DISTANCE_OFFSET));
        } else {
            self.emit_sign_extend_i32("a1");
            self.emit(format!("sd a1, {}(sp)", DISTANCE_OFFSET));
        }
        self.emit(format!("sd zero, {}(sp)", BASE_INDEX_OFFSET));
        self.emit(format!("sd zero, {}(sp)", BASE_COUNT_OFFSET));
        self.emit(format!("sd zero, {}(sp)", RELATED_COUNT_OFFSET));
        if related_filter {
            self.emit(format!("sd a2, {}(sp)", EXPECTED_HASH_PTR_OFFSET));
            self.emit(format!("sd a3, {}(sp)", EXPECTED_HASH_LEN_OFFSET));
            self.emit(format!("sd a4, {}(sp)", FILTER_RULE_OFFSET));
            self.emit(format!("beqz a2, {}", bad_expected));
            self.emit("li t0, 32");
            self.emit("sub t1, a3, t0");
            self.emit(format!("bnez t1, {}", bad_expected));
            self.emit("li t0, 2");
            self.emit("sltu t1, t0, a4");
            self.emit(format!("bnez t1, {}", bad_data_rule));
        }

        self.emit("# cellscript abi: decode SourceView source class; index component is ignored for group scan");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", source_input));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", source_group_input));
        self.emit(format!("li t0, {}", CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", source_output));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", source_group_output));
        self.emit(format!("j {}", invalid));

        for (label, source, view) in [
            (&source_input, CKB_SOURCE_INPUT, CKB_SOURCE_VIEW_INPUT),
            (&source_group_input, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT, CKB_SOURCE_VIEW_GROUP_INPUT),
            (&source_output, CKB_SOURCE_OUTPUT, CKB_SOURCE_VIEW_OUTPUT),
            (&source_group_output, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT, CKB_SOURCE_VIEW_GROUP_OUTPUT),
        ] {
            self.emit_label(label.as_str());
            self.emit(format!("li t0, {}", source));
            self.emit(format!("sd t0, {}(sp)", SOURCE_OFFSET));
            self.emit(format!("li t0, {}", view));
            self.emit(format!("sd t0, {}(sp)", VIEW_KIND_OFFSET));
            self.emit(format!("j {}", source_ready));
        }

        self.emit_label(&source_ready);
        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", hash_failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", hash_failed));

        self.emit_label(&outer_loop);
        self.emit(format!("ld a0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("ld a1, {}(sp)", BASE_INDEX_OFFSET));
        self.emit(format!("addi a2, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", outer_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit(format!("li t2, {}", base_role));
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", outer_role_ok));
        self.emit(format!("j {}", maybe_related));

        self.emit_label(&outer_role_ok);
        self.emit(format!("ld t0, {}(sp)", BASE_COUNT_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", BASE_COUNT_OFFSET));
        self.emit(format!("sd zero, {}(sp)", MATCH_COUNT_OFFSET));
        self.emit(format!("sd zero, {}(sp)", RELATED_INDEX_OFFSET));
        if distance_from_base_data {
            self.emit("# cellscript abi: load signed i32 MetaPoint distance from the base cell data");
            self.emit("li t0, 4");
            self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
            self.emit(format!("addi a0, sp, {}", DATA_BUFFER_OFFSET));
            self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
            self.emit(format!("ld a2, {}(sp)", DATA_OFFSET_OFFSET));
            self.emit(format!("ld a3, {}(sp)", BASE_INDEX_OFFSET));
            self.emit(format!("ld a4, {}(sp)", SOURCE_OFFSET));
            self.emit(format!("li a7, {}", abi.load_cell_data));
            self.emit("ecall");
            self.emit(format!("beqz a0, {}", data_loaded));
            self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
            self.emit("sub t1, a0, t0");
            self.emit(format!("beqz t1, {}", data_len_enough));
            self.emit(format!("j {}", data_malformed));
            self.emit_label(&data_loaded);
            self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
            self.emit("li t1, 4");
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", data_malformed));
            self.emit(format!("j {}", distance_ready));
            self.emit_label(&data_len_enough);
            self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
            self.emit("li t1, 4");
            self.emit("sltu t2, t0, t1");
            self.emit(format!("bnez t2, {}", data_malformed));
            self.emit_label(&distance_ready);
            self.emit_stack_u32_le_to("t0", DATA_BUFFER_OFFSET);
            self.emit_sign_extend_i32("t0");
            self.emit(format!("sd t0, {}(sp)", DISTANCE_OFFSET));
        }

        self.emit_label(&inner_loop);
        self.emit(format!("ld a0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("ld a1, {}(sp)", RELATED_INDEX_OFFSET));
        self.emit(format!("addi a2, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", inner_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit(format!("li t2, {}", related_role));
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", inner_role_candidate));
        self.emit(format!("j {}", advance_related));

        self.emit_label(&inner_role_candidate);
        if related_filter {
            self.emit_metapoint_related_cell_filter_check(
                SOURCE_OFFSET,
                RELATED_INDEX_OFFSET,
                EXPECTED_HASH_PTR_OFFSET,
                FILTER_RULE_OFFSET,
                SIZE_OFFSET,
                TYPE_HASH_BUFFER_OFFSET,
                DATA_BUFFER_OFFSET,
                &status_failed,
                &related_type_mismatch,
                &related_data_mismatch,
                &bad_data_rule,
            );
        }
        self.emit(format!("ld t0, {}(sp)", VIEW_KIND_OFFSET));
        self.emit(format!("li t1, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("mul t0, t0, t1");
        self.emit(format!("ld a0, {}(sp)", BASE_INDEX_OFFSET));
        self.emit("add a0, a0, t0");
        self.emit(format!("ld a1, {}(sp)", RELATED_INDEX_OFFSET));
        self.emit("add a1, a1, t0");
        self.emit(format!("ld a2, {}(sp)", DISTANCE_OFFSET));
        self.emit("call __ckb_require_metapoint_relative");
        self.emit(format!("beqz a0, {}", relation_matched));
        self.emit(format!("li t0, {}", CellScriptRuntimeError::MetaPointMismatch.code()));
        self.emit("sub t1, a0, t0");
        self.emit(format!("bnez t1, {}", relation_failed));
        self.emit(format!("j {}", advance_related));

        self.emit_label(&advance_related);
        self.emit(format!("ld t0, {}(sp)", RELATED_INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", RELATED_INDEX_OFFSET));
        self.emit(format!("j {}", inner_loop));

        self.emit_label(&relation_matched);
        self.emit(format!("ld t0, {}(sp)", MATCH_COUNT_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", MATCH_COUNT_OFFSET));
        self.emit(format!("ld t1, {}(sp)", RELATED_INDEX_OFFSET));
        self.emit("addi t1, t1, 1");
        self.emit(format!("sd t1, {}(sp)", RELATED_INDEX_OFFSET));
        self.emit(format!("j {}", inner_loop));

        self.emit_label(&inner_done);
        self.emit(format!("ld t0, {}(sp)", MATCH_COUNT_OFFSET));
        self.emit("li t1, 1");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", cardinality));
        self.emit(format!("j {}", increment_outer));

        self.emit_label(&maybe_related);
        self.emit(format!("li t2, {}", related_role));
        self.emit("sub t3, t0, t2");
        self.emit(format!("bnez t3, {}", increment_outer));
        if related_filter {
            self.emit_metapoint_related_cell_filter_check(
                SOURCE_OFFSET,
                BASE_INDEX_OFFSET,
                EXPECTED_HASH_PTR_OFFSET,
                FILTER_RULE_OFFSET,
                SIZE_OFFSET,
                TYPE_HASH_BUFFER_OFFSET,
                DATA_BUFFER_OFFSET,
                &status_failed,
                &related_type_mismatch,
                &related_data_mismatch,
                &bad_data_rule,
            );
        }
        self.emit(format!("ld t4, {}(sp)", RELATED_COUNT_OFFSET));
        self.emit("addi t4, t4, 1");
        self.emit(format!("sd t4, {}(sp)", RELATED_COUNT_OFFSET));

        self.emit_label(&increment_outer);
        self.emit(format!("ld t0, {}(sp)", BASE_INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", BASE_INDEX_OFFSET));
        self.emit(format!("j {}", outer_loop));

        self.emit_label(&outer_done);
        self.emit(format!("ld t0, {}(sp)", BASE_COUNT_OFFSET));
        self.emit(format!("ld t1, {}(sp)", RELATED_COUNT_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", cardinality));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&hash_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&status_failed);
        self.emit("addi a0, t1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&relation_failed);
        self.emit("addi t1, a0, 0");
        self.emit(format!("j {}", status_failed));
        self.emit_label(&role_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptRoleMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_data_rule);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&related_type_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::TypeHashMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&related_data_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&data_malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&cardinality);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::MetaPointCardinalityMismatch.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", STACK_SIZE));
        self.emit("ret");
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_metapoint_related_cell_filter_check(
        &mut self,
        source_offset: usize,
        related_index_offset: usize,
        expected_hash_ptr_offset: usize,
        filter_rule_offset: usize,
        size_offset: usize,
        type_hash_buffer_offset: usize,
        data_buffer_offset: usize,
        status_failed: &str,
        type_mismatch: &str,
        data_mismatch: &str,
        bad_data_rule: &str,
    ) {
        let type_loaded = self.fresh_label("metapoint_filter_type_loaded");
        let type_size_ok = self.fresh_label("metapoint_filter_type_size_ok");
        let data_rule_done = self.fresh_label("metapoint_filter_data_rule_done");
        let data_rule_zero = self.fresh_label("metapoint_filter_data_rule_zero");
        let data_rule_nonzero = self.fresh_label("metapoint_filter_data_rule_nonzero");
        let abi = self.runtime_abi();

        self.emit("# cellscript abi: filtered MetaPoint related cell type hash and data-rule check");
        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", size_offset));
        self.emit(format!("addi a0, sp, {}", type_hash_buffer_offset));
        self.emit(format!("addi a1, sp, {}", size_offset));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", related_index_offset));
        self.emit(format!("ld a4, {}(sp)", source_offset));
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", type_loaded));
        self.emit("addi t1, a0, 0");
        self.emit(format!("j {}", status_failed));
        self.emit_label(&type_loaded);
        self.emit(format!("ld t0, {}(sp)", size_offset));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", type_size_ok));
        self.emit(format!("j {}", type_mismatch));
        self.emit_label(&type_size_ok);
        self.emit(format!("addi a0, sp, {}", type_hash_buffer_offset));
        self.emit(format!("ld a1, {}(sp)", expected_hash_ptr_offset));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", type_mismatch));

        self.emit(format!("ld t0, {}(sp)", filter_rule_offset));
        self.emit(format!("beqz t0, {}", data_rule_done));
        self.emit("li t1, 1");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", data_rule_zero));
        self.emit("li t1, 2");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", data_rule_nonzero));
        self.emit(format!("j {}", bad_data_rule));

        for (label, require_nonzero) in [(&data_rule_zero, false), (&data_rule_nonzero, true)] {
            let data_loaded = self.fresh_label("metapoint_filter_data_loaded");
            let data_size_ok = self.fresh_label("metapoint_filter_data_size_ok");
            let data_value_ok = self.fresh_label("metapoint_filter_data_value_ok");
            self.emit_label(label);
            self.emit("li t0, 8");
            self.emit(format!("sd t0, {}(sp)", size_offset));
            self.emit(format!("addi a0, sp, {}", data_buffer_offset));
            self.emit(format!("addi a1, sp, {}", size_offset));
            self.emit("li a2, 0");
            self.emit(format!("ld a3, {}(sp)", related_index_offset));
            self.emit(format!("ld a4, {}(sp)", source_offset));
            self.emit(format!("li a7, {}", abi.load_cell_data));
            self.emit("ecall");
            self.emit(format!("beqz a0, {}", data_loaded));
            self.emit(format!("j {}", data_mismatch));
            self.emit_label(&data_loaded);
            self.emit(format!("ld t0, {}(sp)", size_offset));
            self.emit("li t1, 8");
            self.emit("sub t2, t0, t1");
            self.emit(format!("beqz t2, {}", data_size_ok));
            self.emit(format!("j {}", data_mismatch));
            self.emit_label(&data_size_ok);
            self.emit(format!("ld t0, {}(sp)", data_buffer_offset));
            if require_nonzero {
                self.emit(format!("bnez t0, {}", data_value_ok));
                self.emit(format!("j {}", data_mismatch));
            } else {
                self.emit(format!("beqz t0, {}", data_value_ok));
                self.emit(format!("j {}", data_mismatch));
            }
            self.emit_label(&data_value_ok);
            self.emit(format!("j {}", data_rule_done));
        }

        self.emit_label(&data_rule_done);
    }

    fn emit_runtime_lock_match_master_out_point_pairs_from_data_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_require_lock_match_master_out_point_pairs_from_data");
        self.emit_label("__ckb_require_lock_match_master_out_point_pairs_from_data");
        self.emit("# cellscript abi: Limit-Order-style lock-only match order master OutPoint pairing");
        self.emit(
            "# cellscript abi: args a0=input SourceView, a1=output SourceView, a2=action_offset, a3=tx_hash_offset, a4=index_offset",
        );
        self.emit("# cellscript abi: input orders may encode master as Mint(relative i32) or Match(absolute OutPoint)");
        self.emit("# cellscript abi: output orders must encode master as Match(absolute OutPoint)");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const INPUT_VIEW_OFFSET: usize = 8;
        const OUTPUT_VIEW_OFFSET: usize = 16;
        const INPUT_SOURCE_OFFSET: usize = 24;
        const OUTPUT_SOURCE_OFFSET: usize = 32;
        const ACTION_OFFSET_OFFSET: usize = 40;
        const TX_HASH_OFFSET_OFFSET: usize = 48;
        const INDEX_OFFSET_OFFSET: usize = 56;
        const INPUT_INDEX_OFFSET: usize = 64;
        const OUTPUT_INDEX_OFFSET: usize = 72;
        const INPUT_COUNT_OFFSET: usize = 80;
        const OUTPUT_COUNT_OFFSET: usize = 88;
        const MATCH_COUNT_OFFSET: usize = 96;
        const SIZE_OFFSET: usize = 104;
        const SCRIPT_HASH_OFFSET: usize = 112;
        const INPUT_MASTER_TX_OFFSET: usize = 144;
        const OUTPUT_MASTER_TX_OFFSET: usize = 184;
        const INPUT_MASTER_INDEX_OFFSET: usize = 224;
        const OUTPUT_MASTER_INDEX_OFFSET: usize = 232;
        const DATA_BUFFER_OFFSET: usize = 240;
        const FRAME_SIZE: usize = 304;
        const RA_OFFSET: usize = 296;

        let invalid = self.fresh_label("match_master_source_invalid");
        let input_source_ok = self.fresh_label("match_master_input_source_ok");
        let output_source_ok = self.fresh_label("match_master_output_source_ok");
        let hash_failed = self.fresh_label("match_master_hash_failed");
        let output_count_loop = self.fresh_label("match_master_output_count_loop");
        let output_count_done = self.fresh_label("match_master_output_count_done");
        let output_count_lock = self.fresh_label("match_master_output_count_lock");
        let output_count_advance = self.fresh_label("match_master_output_count_advance");
        let input_loop = self.fresh_label("match_master_input_loop");
        let input_lock = self.fresh_label("match_master_input_lock");
        let input_advance = self.fresh_label("match_master_input_advance");
        let input_done = self.fresh_label("match_master_input_done");
        let output_match_loop = self.fresh_label("match_master_output_match_loop");
        let output_match_done = self.fresh_label("match_master_output_match_done");
        let output_match_candidate = self.fresh_label("match_master_output_match_candidate");
        let output_match_advance = self.fresh_label("match_master_output_match_advance");
        let output_match_equal = self.fresh_label("match_master_output_match_equal");
        let status_failed = self.fresh_label("match_master_status_failed");
        let role_mismatch = self.fresh_label("match_master_role_mismatch");
        let invalid_action = self.fresh_label("match_master_invalid_action");
        let malformed = self.fresh_label("match_master_malformed");
        let out_point_failed = self.fresh_label("match_master_out_point_failed");
        let cardinality = self.fresh_label("match_master_cardinality");
        let done = self.fresh_label("match_master_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a0, {}(sp)", INPUT_VIEW_OFFSET));
        self.emit(format!("sd a1, {}(sp)", OUTPUT_VIEW_OFFSET));
        self.emit(format!("sd a2, {}(sp)", ACTION_OFFSET_OFFSET));
        self.emit(format!("sd a3, {}(sp)", TX_HASH_OFFSET_OFFSET));
        self.emit(format!("sd a4, {}(sp)", INDEX_OFFSET_OFFSET));

        self.emit("# cellscript abi: decode input source class for match-order scan");
        self.emit(format!("ld a0, {}(sp)", INPUT_VIEW_OFFSET));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", input_source_ok));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("bnez t3, {}", invalid));
        self.emit_label(&input_source_ok);
        self.emit(format!("sd t2, {}(sp)", INPUT_SOURCE_OFFSET));

        self.emit("# cellscript abi: decode output source class for match-order scan");
        self.emit(format!("ld a0, {}(sp)", OUTPUT_VIEW_OFFSET));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", output_source_ok));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("bnez t3, {}", invalid));
        self.emit_label(&output_source_ok);
        self.emit(format!("sd t2, {}(sp)", OUTPUT_SOURCE_OFFSET));

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", hash_failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", hash_failed));

        self.emit(format!("sd zero, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit(format!("sd zero, {}(sp)", OUTPUT_COUNT_OFFSET));
        self.emit_label(&output_count_loop);
        self.emit(format!("ld a0, {}(sp)", OUTPUT_SOURCE_OFFSET));
        self.emit(format!("ld a1, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit(format!("addi a2, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", output_count_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit("li t2, 1");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", output_count_lock));
        self.emit(format!("j {}", output_count_advance));
        self.emit_label(&output_count_lock);
        self.emit_load_order_master_out_point_from_data(
            OUTPUT_SOURCE_OFFSET,
            OUTPUT_INDEX_OFFSET,
            ACTION_OFFSET_OFFSET,
            TX_HASH_OFFSET_OFFSET,
            INDEX_OFFSET_OFFSET,
            OUTPUT_MASTER_TX_OFFSET,
            OUTPUT_MASTER_INDEX_OFFSET,
            DATA_BUFFER_OFFSET,
            SIZE_OFFSET,
            false,
            &invalid_action,
            &malformed,
            &out_point_failed,
        );
        self.emit(format!("ld t0, {}(sp)", OUTPUT_COUNT_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", OUTPUT_COUNT_OFFSET));
        self.emit_label(&output_count_advance);
        self.emit(format!("ld t0, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit(format!("j {}", output_count_loop));

        self.emit_label(&output_count_done);
        self.emit(format!("sd zero, {}(sp)", INPUT_INDEX_OFFSET));
        self.emit(format!("sd zero, {}(sp)", INPUT_COUNT_OFFSET));
        self.emit_label(&input_loop);
        self.emit(format!("ld a0, {}(sp)", INPUT_SOURCE_OFFSET));
        self.emit(format!("ld a1, {}(sp)", INPUT_INDEX_OFFSET));
        self.emit(format!("addi a2, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", input_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit("li t2, 1");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", input_lock));
        self.emit(format!("j {}", input_advance));

        self.emit_label(&input_lock);
        self.emit(format!("ld t0, {}(sp)", INPUT_COUNT_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", INPUT_COUNT_OFFSET));
        self.emit(format!("sd zero, {}(sp)", MATCH_COUNT_OFFSET));
        self.emit_load_order_master_out_point_from_data(
            INPUT_SOURCE_OFFSET,
            INPUT_INDEX_OFFSET,
            ACTION_OFFSET_OFFSET,
            TX_HASH_OFFSET_OFFSET,
            INDEX_OFFSET_OFFSET,
            INPUT_MASTER_TX_OFFSET,
            INPUT_MASTER_INDEX_OFFSET,
            DATA_BUFFER_OFFSET,
            SIZE_OFFSET,
            true,
            &invalid_action,
            &malformed,
            &out_point_failed,
        );
        self.emit(format!("sd zero, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit_label(&output_match_loop);
        self.emit(format!("ld a0, {}(sp)", OUTPUT_SOURCE_OFFSET));
        self.emit(format!("ld a1, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit(format!("addi a2, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", output_match_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit("li t2, 1");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", output_match_candidate));
        self.emit(format!("j {}", output_match_advance));

        self.emit_label(&output_match_candidate);
        self.emit_load_order_master_out_point_from_data(
            OUTPUT_SOURCE_OFFSET,
            OUTPUT_INDEX_OFFSET,
            ACTION_OFFSET_OFFSET,
            TX_HASH_OFFSET_OFFSET,
            INDEX_OFFSET_OFFSET,
            OUTPUT_MASTER_TX_OFFSET,
            OUTPUT_MASTER_INDEX_OFFSET,
            DATA_BUFFER_OFFSET,
            SIZE_OFFSET,
            false,
            &invalid_action,
            &malformed,
            &out_point_failed,
        );
        for word in 0..4 {
            self.emit(format!("ld t0, {}(sp)", INPUT_MASTER_TX_OFFSET + word * 8));
            self.emit(format!("ld t1, {}(sp)", OUTPUT_MASTER_TX_OFFSET + word * 8));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", output_match_advance));
        }
        self.emit(format!("ld t0, {}(sp)", INPUT_MASTER_INDEX_OFFSET));
        self.emit(format!("ld t1, {}(sp)", OUTPUT_MASTER_INDEX_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_match_equal));
        self.emit(format!("j {}", output_match_advance));
        self.emit_label(&output_match_equal);
        self.emit(format!("ld t0, {}(sp)", MATCH_COUNT_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", MATCH_COUNT_OFFSET));

        self.emit_label(&output_match_advance);
        self.emit(format!("ld t0, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit(format!("j {}", output_match_loop));

        self.emit_label(&output_match_done);
        self.emit(format!("ld t0, {}(sp)", MATCH_COUNT_OFFSET));
        self.emit("li t1, 1");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", cardinality));

        self.emit_label(&input_advance);
        self.emit(format!("ld t0, {}(sp)", INPUT_INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", INPUT_INDEX_OFFSET));
        self.emit(format!("j {}", input_loop));

        self.emit_label(&input_done);
        self.emit(format!("ld t0, {}(sp)", INPUT_COUNT_OFFSET));
        self.emit(format!("ld t1, {}(sp)", OUTPUT_COUNT_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", cardinality));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&hash_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&status_failed);
        self.emit("addi a0, t1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&role_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptRoleMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&invalid_action);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&out_point_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&cardinality);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::MetaPointCardinalityMismatch.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_load_order_master_out_point_from_data(
        &mut self,
        source_offset: usize,
        cell_index_offset: usize,
        action_offset_offset: usize,
        tx_hash_offset_offset: usize,
        index_offset_offset: usize,
        tx_dest_offset: usize,
        index_dest_offset: usize,
        data_buffer_offset: usize,
        size_offset: usize,
        allow_mint_relative: bool,
        invalid_action: &str,
        malformed: &str,
        out_point_failed: &str,
    ) {
        let action_match = self.fresh_label("order_master_action_match");
        let action_mint = self.fresh_label("order_master_action_mint");
        let size_status_ok = self.fresh_label("order_master_data_size_status_ok");
        let done = self.fresh_label("order_master_loaded");
        let abi = self.runtime_abi();

        self.emit("# cellscript abi: iCKB Limit Order data is exact-length order fields; trailing bytes are malformed");
        self.emit(format!("ld t0, {}(sp)", index_offset_offset));
        self.emit("li t1, 37");
        self.emit("add t0, t0, t1");
        self.emit(format!("sd t0, {}(sp)", data_buffer_offset));
        self.emit("li t1, 0");
        self.emit(format!("sd t1, {}(sp)", size_offset));
        self.emit(format!("addi a0, sp, {}", data_buffer_offset + 8));
        self.emit(format!("addi a1, sp, {}", size_offset));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", cell_index_offset));
        self.emit(format!("ld a4, {}(sp)", source_offset));
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", size_status_ok));
        self.emit(format!("beqz a0, {}", size_status_ok));
        self.emit(format!("j {}", malformed));
        self.emit_label(&size_status_ok);
        self.emit(format!("ld t0, {}(sp)", data_buffer_offset));
        self.emit(format!("ld t1, {}(sp)", size_offset));
        self.emit("sub t2, t1, t0");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit_load_cell_data_prefix_to_stack(
            source_offset,
            cell_index_offset,
            action_offset_offset,
            data_buffer_offset,
            4,
            size_offset,
            malformed,
        );
        self.emit_stack_u32_le_to("t0", data_buffer_offset);
        self.emit("li t1, 1");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", action_match));
        if allow_mint_relative {
            self.emit(format!("beqz t0, {}", action_mint));
        }
        self.emit(format!("j {}", invalid_action));

        self.emit_label(&action_match);
        self.emit_load_cell_data_prefix_to_stack(
            source_offset,
            cell_index_offset,
            tx_hash_offset_offset,
            tx_dest_offset,
            32,
            size_offset,
            malformed,
        );
        self.emit_load_cell_data_prefix_to_stack(
            source_offset,
            cell_index_offset,
            index_offset_offset,
            data_buffer_offset,
            4,
            size_offset,
            malformed,
        );
        self.emit_stack_u32_le_to("t0", data_buffer_offset);
        self.emit(format!("sd t0, {}(sp)", index_dest_offset));
        self.emit(format!("j {}", done));

        if allow_mint_relative {
            self.emit_label(&action_mint);
            self.emit_load_cell_data_prefix_to_stack(
                source_offset,
                cell_index_offset,
                tx_hash_offset_offset,
                tx_dest_offset,
                32,
                size_offset,
                malformed,
            );
            for word in 0..4 {
                self.emit(format!("ld t0, {}(sp)", tx_dest_offset + word * 8));
                self.emit(format!("bnez t0, {}", malformed));
            }
            self.emit_load_cell_data_prefix_to_stack(
                source_offset,
                cell_index_offset,
                index_offset_offset,
                data_buffer_offset,
                4,
                size_offset,
                malformed,
            );
            self.emit_stack_u32_le_to("t3", data_buffer_offset);
            self.emit_sign_extend_i32("t3");
            self.emit(format!("sd t3, {}(sp)", data_buffer_offset));
            self.emit_load_input_out_point_to_stack(
                source_offset,
                cell_index_offset,
                tx_dest_offset,
                index_dest_offset,
                size_offset,
                out_point_failed,
            );
            self.emit(format!("ld t3, {}(sp)", data_buffer_offset));
            self.emit(format!("ld t0, {}(sp)", index_dest_offset));
            self.emit("add t0, t0, t3");
            self.emit("slt t1, t0, zero");
            self.emit(format!("bnez t1, {}", out_point_failed));
            self.emit(format!("sd t0, {}(sp)", index_dest_offset));
        }

        self.emit_label(&done);
    }

    fn emit_load_cell_data_prefix_to_stack(
        &mut self,
        source_offset: usize,
        cell_index_offset: usize,
        data_offset_offset: usize,
        dest_offset: usize,
        width: usize,
        size_offset: usize,
        malformed: &str,
    ) {
        let loaded = self.fresh_label("cell_data_prefix_loaded");
        let len_enough = self.fresh_label("cell_data_prefix_len_enough");
        let ready = self.fresh_label("cell_data_prefix_ready");
        let abi = self.runtime_abi();

        self.emit(format!("li t0, {}", width));
        self.emit(format!("sd t0, {}(sp)", size_offset));
        self.emit(format!("addi a0, sp, {}", dest_offset));
        self.emit(format!("addi a1, sp, {}", size_offset));
        self.emit(format!("ld a2, {}(sp)", data_offset_offset));
        self.emit(format!("ld a3, {}(sp)", cell_index_offset));
        self.emit(format!("ld a4, {}(sp)", source_offset));
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", len_enough));
        self.emit(format!("j {}", malformed));
        self.emit_label(&loaded);
        self.emit(format!("ld t0, {}(sp)", size_offset));
        self.emit(format!("li t1, {}", width));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit(format!("j {}", ready));
        self.emit_label(&len_enough);
        self.emit(format!("ld t0, {}(sp)", size_offset));
        self.emit(format!("li t1, {}", width));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_label(&ready);
    }

    fn emit_load_input_out_point_to_stack(
        &mut self,
        source_offset: usize,
        cell_index_offset: usize,
        tx_dest_offset: usize,
        index_dest_offset: usize,
        size_offset: usize,
        failed: &str,
    ) {
        let abi = self.runtime_abi();

        self.emit("li t0, 36");
        self.emit(format!("sd t0, {}(sp)", size_offset));
        self.emit(format!("addi a0, sp, {}", tx_dest_offset));
        self.emit(format!("addi a1, sp, {}", size_offset));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", cell_index_offset));
        self.emit(format!("ld a4, {}(sp)", source_offset));
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("ld t0, {}(sp)", size_offset));
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_stack_u32_le_to("t0", tx_dest_offset + 32);
        self.emit(format!("sd t0, {}(sp)", index_dest_offset));
    }

    fn emit_runtime_cell_hash_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        mismatch_error: CellScriptRuntimeError,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView full-hash requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=expected_hash_ptr, a2=expected_hash_len");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("source_view_invalid");
        let bad_expected = self.fresh_label("expected_hash_invalid");
        let failed = self.fresh_label("cell_hash_load_failed");
        let mismatch = self.fresh_label("cell_hash_mismatch");
        let done = self.fresh_label("cell_hash_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -80");
        self.emit("sd ra, 72(sp)");
        self.emit("sd a1, 64(sp)");
        self.emit("sd a2, 56(sp)");

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 32");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit("ld t0, 8(sp)");
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit("addi a0, sp, 16");
        self.emit("ld a1, 64(sp)");
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", mismatch_error.code()));
        self.emit_label(&done);
        self.emit("ld ra, 72(sp)");
        self.emit("addi sp, sp, 80");
        self.emit("ret");
    }

    fn emit_runtime_cell_script_hash_field_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        read: ScriptHashFieldRead,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView read-only ScriptRef Hash field ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=out32_ptr, a2=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 8;
        const SCRIPT_BUFFER_OFFSET: usize = 16;
        const OUT_PTR_OFFSET: usize = 152;
        const SIZE_PTR_OFFSET: usize = 160;
        const RA_OFFSET: usize = 184;
        const FRAME_SIZE: usize = 192;

        let requested_size = match read {
            ScriptHashFieldRead::CodeHash => 53u64,
            ScriptHashFieldRead::Args32 => 128u64,
        };
        let payload_offset = match read {
            ScriptHashFieldRead::CodeHash => SCRIPT_BUFFER_OFFSET + 16,
            ScriptHashFieldRead::Args32 => SCRIPT_BUFFER_OFFSET + 53,
        };
        let invalid = self.fresh_label("script_ref_hash_source_invalid");
        let failed = self.fresh_label("script_ref_hash_load_failed");
        let loaded = self.fresh_label("script_ref_hash_loaded");
        let malformed = self.fresh_label("script_ref_hash_malformed");
        let args_mismatch = self.fresh_label("script_ref_hash_args_mismatch");
        let copy_loop = self.fresh_label("script_ref_hash_copy");
        let copy_done = self.fresh_label("script_ref_hash_copy_done");
        let done = self.fresh_label("script_ref_hash_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", OUT_PTR_OFFSET));
        self.emit(format!("sd a2, {}(sp)", SIZE_PTR_OFFSET));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", requested_size));
        self.emit(format!("sd t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        if matches!(read, ScriptHashFieldRead::CodeHash) {
            self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
            self.emit("sub t1, a0, t0");
            self.emit(format!("beqz t1, {}", loaded));
        }
        self.emit(format!("j {}", failed));

        self.emit_label(&loaded);
        self.emit(format!("ld t3, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit("li t1, 49");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        self.emit("li t1, 53");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }
        if matches!(read, ScriptHashFieldRead::Args32) {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + 49);
            self.emit("li t1, 32");
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", args_mismatch));
            self.emit("li t1, 85");
            self.emit("sub t2, t3, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit("li t1, 0");
        self.emit_label(&copy_loop);
        self.emit("li t2, 32");
        self.emit("sltu t3, t1, t2");
        self.emit(format!("beqz t3, {}", copy_done));
        self.emit(format!("addi t6, sp, {}", payload_offset));
        self.emit("add t6, t6, t1");
        self.emit("lbu t5, 0(t6)");
        self.emit(format!("ld t6, {}(sp)", OUT_PTR_OFFSET));
        self.emit("add t6, t6, t1");
        self.emit("sb t5, 0(t6)");
        self.emit("addi t1, t1, 1");
        self.emit(format!("j {}", copy_loop));
        self.emit_label(&copy_done);
        self.emit(format!("ld t6, {}(sp)", SIZE_PTR_OFFSET));
        self.emit("li t0, 32");
        self.emit("sd t0, 0(t6)");
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&args_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_cell_script_scalar_field_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        read: ScriptScalarFieldRead,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView read-only ScriptRef scalar field ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView; returns a0=value, a1=status");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 8;
        const SCRIPT_BUFFER_OFFSET: usize = 16;
        const RA_OFFSET: usize = 152;
        const FRAME_SIZE: usize = 160;
        let requested_size = match read {
            ScriptScalarFieldRead::HashType => 53u64,
            ScriptScalarFieldRead::ArgsEmpty => 128u64,
        };
        let invalid = self.fresh_label("script_ref_scalar_source_invalid");
        let failed = self.fresh_label("script_ref_scalar_load_failed");
        let loaded = self.fresh_label("script_ref_scalar_loaded");
        let malformed = self.fresh_label("script_ref_scalar_malformed");
        let nonempty = self.fresh_label("script_ref_scalar_nonempty");
        let done = self.fresh_label("script_ref_scalar_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", requested_size));
        self.emit(format!("sd t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", loaded));
        self.emit(format!("j {}", failed));

        self.emit_label(&loaded);
        self.emit(format!("ld t3, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit("li t1, 49");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        if matches!(read, ScriptScalarFieldRead::HashType) {
            self.emit("li t1, 53");
            self.emit("sltu t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        } else {
            self.emit("sub t2, t0, t3");
            self.emit(format!("bnez t2, {}", malformed));
        }
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }
        match read {
            ScriptScalarFieldRead::HashType => {
                self.emit(format!("lbu a0, {}(sp)", SCRIPT_BUFFER_OFFSET + 48));
                self.emit("li a1, 0");
                self.emit(format!("j {}", done));
            }
            ScriptScalarFieldRead::ArgsEmpty => {
                self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + 49);
                self.emit(format!("bnez t0, {}", nonempty));
                self.emit("li t1, 53");
                self.emit("sub t2, t3, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit("li a0, 1");
                self.emit("li a1, 0");
                self.emit(format!("j {}", done));
                self.emit_label(&nonempty);
                self.emit("li a0, 0");
                self.emit("li a1, 0");
                self.emit(format!("j {}", done));
            }
        }

        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_cell_script_args_empty_requirement_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView Script empty-args requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView; expects Molecule Script args Bytes length == 0");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 8;
        const SCRIPT_BUFFER_OFFSET: usize = 16;
        const EMPTY_SCRIPT_SIZE: u64 = 53;

        let invalid = self.fresh_label("script_args_source_invalid");
        let failed = self.fresh_label("script_args_load_failed");
        let nonempty = self.fresh_label("script_args_nonempty");
        let malformed = self.fresh_label("script_args_malformed");
        let done = self.fresh_label("script_args_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -160");
        self.emit("sd ra, 152(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 128");
        self.emit(format!("sd t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));

        self.emit(format!("ld t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("li t1, {}", EMPTY_SCRIPT_SIZE));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", nonempty));

        for (offset, expected) in [(0usize, EMPTY_SCRIPT_SIZE), (4, 16), (8, 48), (12, 49), (49, 0)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&nonempty);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit("ld ra, 152(sp)");
        self.emit("addi sp, sp, 160");
        self.emit("ret");
    }

    fn emit_runtime_cell_script_args_exact_requirement_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView Script arbitrary exact args requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=expected_args_ptr, a2=expected_args_len");
        self.emit("# cellscript abi: validates Molecule packed::Script args Bytes exactly, not only 32-byte hash args");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 8;
        const SCRIPT_BUFFER_OFFSET: usize = 16;
        const EXPECTED_PTR_OFFSET: usize = 72;
        const EXPECTED_LEN_OFFSET: usize = 80;
        const ARGS_OFFSET_OFFSET: usize = 88;
        const CHUNK_LEN_OFFSET: usize = 96;
        const SOURCE_INDEX_OFFSET: usize = 104;
        const SOURCE_KIND_OFFSET: usize = 112;
        const RA_OFFSET: usize = 120;
        const FRAME_SIZE: usize = 128;
        const SCRIPT_PREFIX_SIZE: u64 = 53;
        const CHUNK_SIZE: u64 = 32;

        let invalid = self.fresh_label("script_args_exact_source_invalid");
        let bad_expected = self.fresh_label("script_args_exact_expected_invalid");
        let prefix_loaded = self.fresh_label("script_args_exact_prefix_loaded");
        let load_failed = self.fresh_label("script_args_exact_load_failed");
        let malformed = self.fresh_label("script_args_exact_malformed");
        let mismatch = self.fresh_label("script_args_exact_mismatch");
        let chunk_loop = self.fresh_label("script_args_exact_chunk_loop");
        let chunk_tail = self.fresh_label("script_args_exact_chunk_tail");
        let chunk_loaded = self.fresh_label("script_args_exact_chunk_loaded");
        let success = self.fresh_label("script_args_exact_success");
        let done = self.fresh_label("script_args_exact_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", EXPECTED_PTR_OFFSET));
        self.emit(format!("sd a2, {}(sp)", EXPECTED_LEN_OFFSET));
        self.emit(format!("beqz a2, {}", bad_expected));
        self.emit(format!("beqz a1, {}", bad_expected));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("sd t1, {}(sp)", SOURCE_INDEX_OFFSET));
        self.emit(format!("sd t2, {}(sp)", SOURCE_KIND_OFFSET));

        self.emit(format!("li t0, {}", SCRIPT_PREFIX_SIZE));
        self.emit(format!("sd t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", SOURCE_INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_KIND_OFFSET));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", prefix_loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", prefix_loaded));
        self.emit(format!("j {}", load_failed));

        self.emit_label(&prefix_loaded);
        self.emit(format!("ld t3, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("li t1, {}", SCRIPT_PREFIX_SIZE));
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        self.emit(format!("ld t1, {}(sp)", EXPECTED_LEN_OFFSET));
        self.emit(format!("li t2, {}", SCRIPT_PREFIX_SIZE));
        self.emit("add t1, t1, t2");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }
        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + 49);
        self.emit(format!("ld t1, {}(sp)", EXPECTED_LEN_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit(format!("sd zero, {}(sp)", ARGS_OFFSET_OFFSET));

        self.emit_label(&chunk_loop);
        self.emit(format!("ld t0, {}(sp)", ARGS_OFFSET_OFFSET));
        self.emit(format!("ld t1, {}(sp)", EXPECTED_LEN_OFFSET));
        self.emit("sub t2, t1, t0");
        self.emit(format!("beqz t2, {}", success));
        self.emit(format!("li t3, {}", CHUNK_SIZE));
        self.emit("sltu t4, t2, t3");
        self.emit(format!("bnez t4, {}", chunk_tail));
        self.emit(format!("li t2, {}", CHUNK_SIZE));
        self.emit_label(&chunk_tail);
        self.emit(format!("sd t2, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("sd t2, {}(sp)", CHUNK_LEN_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit(format!("li a2, {}", SCRIPT_PREFIX_SIZE));
        self.emit("add a2, a2, t0");
        self.emit(format!("ld a3, {}(sp)", SOURCE_INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_KIND_OFFSET));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", chunk_loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", chunk_loaded));
        self.emit(format!("j {}", load_failed));
        self.emit_label(&chunk_loaded);
        self.emit(format!("ld t2, {}(sp)", CHUNK_LEN_OFFSET));
        self.emit(format!("ld t0, {}(sp)", ARGS_OFFSET_OFFSET));
        self.emit(format!("ld t1, {}(sp)", EXPECTED_PTR_OFFSET));
        self.emit("add a1, t1, t0");
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit("addi a2, t2, 0");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit(format!("ld t0, {}(sp)", ARGS_OFFSET_OFFSET));
        self.emit(format!("ld t2, {}(sp)", CHUNK_LEN_OFFSET));
        self.emit("add t0, t0, t2");
        self.emit(format!("sd t0, {}(sp)", ARGS_OFFSET_OFFSET));
        self.emit(format!("j {}", chunk_loop));

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&load_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_current_script_args_empty_requirement_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_require_current_script_args_empty");
        self.emit_label("__ckb_require_current_script_args_empty");
        self.emit("# cellscript abi: current-script empty-args requirement via LOAD_SCRIPT plus output lock scan");
        self.emit("# cellscript abi: expects current Script args empty and same-code/hash-type Output locks args empty");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const CURRENT_SIZE_OFFSET: usize = 8;
        const CURRENT_BUFFER_OFFSET: usize = 16;
        const OUTPUT_INDEX_OFFSET: usize = 144;
        const OUTPUT_SIZE_OFFSET: usize = 152;
        const OUTPUT_BUFFER_OFFSET: usize = 160;
        const OUTPUT_TRUNCATED_OFFSET: usize = 288;
        const EMPTY_SCRIPT_SIZE: u64 = 53;
        const FRAME_SIZE: usize = 320;
        const RA_OFFSET: usize = 312;

        let failed = self.fresh_label("current_script_args_load_failed");
        let current_loaded = self.fresh_label("current_script_args_loaded");
        let nonempty = self.fresh_label("current_script_args_nonempty");
        let malformed = self.fresh_label("current_script_args_malformed");
        let output_loop = self.fresh_label("current_script_args_output_loop");
        let output_loaded = self.fresh_label("current_script_args_output_loaded");
        let output_prefix_loaded = self.fresh_label("current_script_args_output_prefix_loaded");
        let output_advance = self.fresh_label("current_script_args_output_advance");
        let output_done = self.fresh_label("current_script_args_output_done");
        let output_same_hash = self.fresh_label("current_script_args_output_same_hash");
        let output_same_script = self.fresh_label("current_script_args_output_same_script");
        let output_failed = self.fresh_label("current_script_args_output_failed");
        let done = self.fresh_label("current_script_args_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit("li t0, 128");
        self.emit(format!("sd t0, {}(sp)", CURRENT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", CURRENT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", CURRENT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", current_loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", nonempty));
        self.emit(format!("j {}", failed));

        self.emit_label(&current_loaded);
        self.emit(format!("ld t0, {}(sp)", CURRENT_SIZE_OFFSET));
        self.emit(format!("li t1, {}", EMPTY_SCRIPT_SIZE));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", nonempty));

        for (offset, expected) in [(0usize, EMPTY_SCRIPT_SIZE), (4, 16), (8, 48), (12, 49), (49, 0)] {
            self.emit_stack_u32_le_to("t0", CURRENT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit("# cellscript abi: require matching output lock scripts to keep empty args");
        self.emit(format!("sd zero, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit_label(&output_loop);
        self.emit("li t0, 128");
        self.emit(format!("sd t0, {}(sp)", OUTPUT_SIZE_OFFSET));
        self.emit(format!("sd zero, {}(sp)", OUTPUT_TRUNCATED_OFFSET));
        self.emit(format!("addi a0, sp, {}", OUTPUT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", OUTPUT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit(format!("li a4, {}", CKB_SOURCE_OUTPUT));
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_LOCK));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", output_loaded));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", output_done));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", output_prefix_loaded));
        self.emit(format!("j {}", output_failed));

        self.emit_label(&output_prefix_loaded);
        self.emit("li t0, 1");
        self.emit(format!("sd t0, {}(sp)", OUTPUT_TRUNCATED_OFFSET));
        self.emit_label(&output_loaded);
        self.emit(format!("ld t0, {}(sp)", OUTPUT_SIZE_OFFSET));
        self.emit("li t1, 49");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit(format!("addi a0, sp, {}", CURRENT_BUFFER_OFFSET + 16));
        self.emit(format!("addi a1, sp, {}", OUTPUT_BUFFER_OFFSET + 16));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("beqz a0, {}", output_same_hash));
        self.emit(format!("j {}", output_advance));

        self.emit_label(&output_same_hash);
        self.emit(format!("lbu t0, {}(sp)", CURRENT_BUFFER_OFFSET + 48));
        self.emit(format!("lbu t1, {}(sp)", OUTPUT_BUFFER_OFFSET + 48));
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_same_script));
        self.emit(format!("j {}", output_advance));

        self.emit_label(&output_same_script);
        self.emit(format!("ld t0, {}(sp)", OUTPUT_TRUNCATED_OFFSET));
        self.emit(format!("bnez t0, {}", nonempty));
        self.emit(format!("ld t0, {}(sp)", OUTPUT_SIZE_OFFSET));
        self.emit(format!("li t1, {}", EMPTY_SCRIPT_SIZE));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", nonempty));
        for (offset, expected) in [(0usize, EMPTY_SCRIPT_SIZE), (4, 16), (8, 48), (12, 49), (49, 0)] {
            self.emit_stack_u32_le_to("t0", OUTPUT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit_label(&output_advance);
        self.emit(format!("ld t0, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", OUTPUT_INDEX_OFFSET));
        self.emit(format!("j {}", output_loop));

        self.emit_label(&output_done);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&output_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&nonempty);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_cell_script_args_hash_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        mode: ScriptArgsHashRequirementMode,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView Script 32-byte args requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=expected_args_hash_ptr, a2=expected_args_hash_len");
        match mode {
            ScriptArgsHashRequirementMode::Exact32 => {
                self.emit("# cellscript abi: expects Molecule Script args Bytes length == 32 and payload == expected hash");
            }
            ScriptArgsHashRequirementMode::Prefix32 => {
                self.emit("# cellscript abi: expects Molecule Script args Bytes length >= 32 and first 32 bytes == expected hash");
            }
            ScriptArgsHashRequirementMode::Suffix32 => {
                self.emit("# cellscript abi: expects Molecule Script args Bytes length >= 32 and last 32 bytes == expected hash");
            }
        }
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 8;
        const SCRIPT_BUFFER_OFFSET: usize = 16;
        const ARGS_PAYLOAD_OFFSET: usize = SCRIPT_BUFFER_OFFSET + 53;
        const SOURCE_INDEX_OFFSET: usize = 152;
        const SOURCE_KIND_OFFSET: usize = 160;
        const EXPECTED_HASH_LEN_OFFSET: usize = 168;
        const EXPECTED_HASH_PTR_OFFSET: usize = 176;
        const RA_OFFSET: usize = 184;
        const FRAME_SIZE: usize = 192;
        const SCRIPT_PREFIX_SIZE: u64 = 53;
        const HASH_ARGS_SCRIPT_SIZE: u64 = 85;

        let invalid = self.fresh_label("script_args_hash_source_invalid");
        let bad_expected = self.fresh_label("script_args_hash_expected_invalid");
        let loaded = self.fresh_label("script_args_hash_loaded");
        let suffix_loaded = self.fresh_label("script_args_hash_suffix_loaded");
        let failed = self.fresh_label("script_args_hash_load_failed");
        let mismatch = self.fresh_label("script_args_hash_mismatch");
        let malformed = self.fresh_label("script_args_hash_malformed");
        let done = self.fresh_label("script_args_hash_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", EXPECTED_HASH_PTR_OFFSET));
        self.emit(format!("sd a2, {}(sp)", EXPECTED_HASH_LEN_OFFSET));

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("sd t1, {}(sp)", SOURCE_INDEX_OFFSET));
        self.emit(format!("sd t2, {}(sp)", SOURCE_KIND_OFFSET));
        let requested_size = match mode {
            ScriptArgsHashRequirementMode::Exact32 | ScriptArgsHashRequirementMode::Prefix32 => 128u64,
            ScriptArgsHashRequirementMode::Suffix32 => SCRIPT_PREFIX_SIZE,
        };
        self.emit(format!("li t0, {}", requested_size));
        self.emit(format!("sd t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", loaded));
        self.emit(format!("j {}", failed));

        self.emit_label(&loaded);
        self.emit(format!("ld t3, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit("li t1, 53");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        self.emit("sub t2, t0, t3");
        self.emit(format!("bnez t2, {}", malformed));
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + 49);
        match mode {
            ScriptArgsHashRequirementMode::Exact32 => {
                self.emit("li t1, 32");
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
                self.emit(format!("li t1, {}", HASH_ARGS_SCRIPT_SIZE));
                self.emit("sub t2, t3, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit(format!("addi a0, sp, {}", ARGS_PAYLOAD_OFFSET));
            }
            ScriptArgsHashRequirementMode::Prefix32 => {
                self.emit("li t1, 32");
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
                self.emit(format!("li t1, {}", SCRIPT_PREFIX_SIZE));
                self.emit("add t1, t1, t0");
                self.emit("sub t2, t3, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit(format!("addi a0, sp, {}", ARGS_PAYLOAD_OFFSET));
            }
            ScriptArgsHashRequirementMode::Suffix32 => {
                self.emit("li t1, 32");
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
                self.emit(format!("li t1, {}", SCRIPT_PREFIX_SIZE));
                self.emit("add t1, t1, t0");
                self.emit("sub t2, t3, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit("addi t1, t1, -32");
                self.emit("li t0, 32");
                self.emit(format!("sd t0, {}(sp)", SCRIPT_SIZE_OFFSET));
                self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
                self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
                self.emit("addi a2, t1, 0");
                self.emit(format!("ld a3, {}(sp)", SOURCE_INDEX_OFFSET));
                self.emit(format!("ld a4, {}(sp)", SOURCE_KIND_OFFSET));
                self.emit(format!("li a5, {}", field_id));
                self.emit(format!("li a7, {}", abi.load_cell_by_field));
                self.emit("ecall");
                self.emit(format!("beqz a0, {}", suffix_loaded));
                self.emit(format!("j {}", failed));
                self.emit_label(&suffix_loaded);
                self.emit(format!("ld t0, {}(sp)", SCRIPT_SIZE_OFFSET));
                self.emit("li t1, 32");
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
            }
        }
        self.emit(format!("ld a1, {}(sp)", EXPECTED_HASH_PTR_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_cell_script_hash_type_requirement_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView Script code_hash/hash_type requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=expected_code_hash_ptr, a2=expected_code_hash_len, a3=expected_hash_type");
        self.emit("# cellscript abi: validates Molecule Script table prefix without constraining args length");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 8;
        const SCRIPT_BUFFER_OFFSET: usize = 16;
        const EXPECTED_CODE_HASH_PTR_OFFSET: usize = 80;
        const EXPECTED_CODE_HASH_LEN_OFFSET: usize = 88;
        const EXPECTED_HASH_TYPE_OFFSET: usize = 96;
        const RA_OFFSET: usize = 120;
        const FRAME_SIZE: usize = 128;
        const SCRIPT_PREFIX_SIZE: u64 = 53;

        let invalid = self.fresh_label("script_identity_source_invalid");
        let bad_expected = self.fresh_label("script_identity_expected_invalid");
        let bad_hash_type = self.fresh_label("script_identity_hash_type_invalid");
        let loaded = self.fresh_label("script_identity_loaded");
        let prefix_loaded = self.fresh_label("script_identity_prefix_loaded");
        let failed = self.fresh_label("script_identity_load_failed");
        let malformed = self.fresh_label("script_identity_malformed");
        let mismatch = self.fresh_label("script_identity_mismatch");
        let done = self.fresh_label("script_identity_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", EXPECTED_CODE_HASH_PTR_OFFSET));
        self.emit(format!("sd a2, {}(sp)", EXPECTED_CODE_HASH_LEN_OFFSET));
        self.emit(format!("sd a3, {}(sp)", EXPECTED_HASH_TYPE_OFFSET));

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));
        self.emit("li t0, 256");
        self.emit("sltu t1, a3, t0");
        self.emit(format!("beqz t1, {}", bad_hash_type));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", SCRIPT_PREFIX_SIZE));
        self.emit(format!("sd t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", prefix_loaded));
        self.emit(format!("j {}", failed));

        self.emit_label(&loaded);
        self.emit(format!("j {}", prefix_loaded));
        self.emit_label(&prefix_loaded);
        self.emit(format!("ld t3, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit("li t1, 49");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        self.emit(format!("li t1, {}", SCRIPT_PREFIX_SIZE));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET + 16));
        self.emit(format!("ld a1, {}(sp)", EXPECTED_CODE_HASH_PTR_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));

        self.emit(format!("lbu t0, {}(sp)", SCRIPT_BUFFER_OFFSET + 48));
        self.emit(format!("ld t1, {}(sp)", EXPECTED_HASH_TYPE_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_hash_type);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptIdentityMismatch.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_load_u64_le_helper(&mut self) {
        self.emit_global("__cellscript_load_u64_le");
        self.emit_label("__cellscript_load_u64_le");
        self.emit("# cellscript abi: load unaligned little-endian u64 from pointer a0");
        self.emit("li a1, 0");
        for byte_index in 0..8 {
            self.emit(format!("lbu t0, {}(a0)", byte_index));
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit("or a1, a1, t0");
        }
        self.emit("addi a0, a1, 0");
        self.emit("ret");
    }

    fn emit_runtime_mul_u128_to_u256_helper(&mut self) {
        self.emit_global("__cellscript_mul_u128_to_u256");
        self.emit_label("__cellscript_mul_u128_to_u256");
        self.emit("# cellscript abi: u128*u128 -> u256 limbs; args a0=left_ptr a1=right_ptr a2=out32_ptr");
        self.emit("addi sp, sp, -96");
        self.emit("sd ra, 88(sp)");
        self.emit("sd a0, 0(sp)");
        self.emit("sd a1, 8(sp)");
        self.emit("sd a2, 16(sp)");

        self.emit("ld a0, 0(sp)");
        self.emit("call __cellscript_load_u64_le");
        self.emit("sd a0, 24(sp)");
        self.emit("ld a0, 0(sp)");
        self.emit("addi a0, a0, 8");
        self.emit("call __cellscript_load_u64_le");
        self.emit("sd a0, 32(sp)");
        self.emit("ld a0, 8(sp)");
        self.emit("call __cellscript_load_u64_le");
        self.emit("sd a0, 40(sp)");
        self.emit("ld a0, 8(sp)");
        self.emit("addi a0, a0, 8");
        self.emit("call __cellscript_load_u64_le");
        self.emit("sd a0, 48(sp)");

        self.emit("ld t0, 24(sp)");
        self.emit("ld t1, 40(sp)");
        self.emit("mul t2, t0, t1");
        self.emit("mulhu t3, t0, t1");
        self.emit("sd t2, 56(sp)");

        self.emit("ld t0, 24(sp)");
        self.emit("ld t1, 48(sp)");
        self.emit("mul t4, t0, t1");
        self.emit("mulhu t5, t0, t1");

        self.emit("ld t0, 32(sp)");
        self.emit("ld t1, 40(sp)");
        self.emit("mul t6, t0, t1");
        self.emit("mulhu a3, t0, t1");

        self.emit("add t0, t3, t4");
        self.emit("sltu a4, t0, t3");
        self.emit("add t1, t0, t6");
        self.emit("sltu a5, t1, t0");
        self.emit("add a4, a4, a5");
        self.emit("sd t1, 64(sp)");

        self.emit("ld t0, 32(sp)");
        self.emit("ld t1, 48(sp)");
        self.emit("mul a5, t0, t1");
        self.emit("mulhu a6, t0, t1");

        self.emit("add t2, t5, a3");
        self.emit("sltu a7, t2, t5");
        self.emit("add t3, t2, a5");
        self.emit("sltu t4, t3, t2");
        self.emit("add t5, t3, a4");
        self.emit("sltu t6, t5, t3");
        self.emit("sd t5, 72(sp)");
        self.emit("add t0, a6, a7");
        self.emit("add t0, t0, t4");
        self.emit("add t0, t0, t6");
        self.emit("sd t0, 80(sp)");

        self.emit("ld t0, 16(sp)");
        self.emit("ld t1, 56(sp)");
        self.emit("sd t1, 0(t0)");
        self.emit("ld t1, 64(sp)");
        self.emit("sd t1, 8(t0)");
        self.emit("ld t1, 72(sp)");
        self.emit("sd t1, 16(t0)");
        self.emit("ld t1, 80(sp)");
        self.emit("sd t1, 24(t0)");
        self.emit("ld ra, 88(sp)");
        self.emit("addi sp, sp, 96");
        self.emit("ret");
    }

    fn emit_runtime_add_u256_helper(&mut self) {
        self.emit_global("__cellscript_add_u256");
        self.emit_label("__cellscript_add_u256");
        self.emit("# cellscript abi: checked u256 addition; args a0=left32_ptr a1=right32_ptr a2=out32_ptr, returns carry in a0");
        self.emit("li a3, 0");
        for limb_offset in [0, 8, 16, 24] {
            self.emit(format!("ld t0, {}(a0)", limb_offset));
            self.emit(format!("ld t1, {}(a1)", limb_offset));
            self.emit("add t2, t0, t1");
            self.emit("sltu t3, t2, t0");
            self.emit("add t2, t2, a3");
            self.emit("sltu t4, t2, a3");
            self.emit(format!("sd t2, {}(a2)", limb_offset));
            self.emit("add a3, t3, t4");
        }
        self.emit("addi a0, a3, 0");
        self.emit("ret");
    }

    fn emit_runtime_c256_product_requirement_helper(&mut self, symbol: &str, detail: &str, equality: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} with overflow-safe C256 product comparison", detail));
        self.emit("# cellscript abi: args a0..a3 are u128 little-endian pointers");
        let bad_expected = self.fresh_label("c256_operand_invalid");
        let mismatch = self.fresh_label("c256_product_mismatch");
        let success = self.fresh_label("c256_product_ok");
        let done = self.fresh_label("c256_product_done");

        self.emit("addi sp, sp, -128");
        self.emit("sd ra, 120(sp)");
        self.emit("sd a0, 0(sp)");
        self.emit("sd a1, 8(sp)");
        self.emit("sd a2, 16(sp)");
        self.emit("sd a3, 24(sp)");
        self.emit(format!("beqz a0, {}", bad_expected));
        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit(format!("beqz a2, {}", bad_expected));
        self.emit(format!("beqz a3, {}", bad_expected));

        self.emit("ld a0, 0(sp)");
        self.emit("ld a1, 8(sp)");
        self.emit("addi a2, sp, 32");
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit("ld a0, 16(sp)");
        self.emit("ld a1, 24(sp)");
        self.emit("addi a2, sp, 64");
        self.emit("call __cellscript_mul_u128_to_u256");

        for limb_offset in [24, 16, 8, 0] {
            self.emit(format!("ld t0, {}(sp)", 32 + limb_offset));
            self.emit(format!("ld t1, {}(sp)", 64 + limb_offset));
            if equality {
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
            } else {
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", success));
                self.emit("sltu t2, t1, t0");
                self.emit(format!("bnez t2, {}", mismatch));
            }
        }

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 120(sp)");
        self.emit("addi sp, sp, 128");
        self.emit("ret");
    }

    fn emit_runtime_c256_sum2_product_requirement_helper(&mut self, symbol: &str, detail: &str, equality: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} with checked u256 product sums", detail));
        self.emit("# cellscript abi: args a0..a7 are u128 little-endian pointers; compares a0*a1+a2*a3 with a4*a5+a6*a7");
        let bad_expected = self.fresh_label("c256_sum_operand_invalid");
        let mismatch = self.fresh_label("c256_sum_mismatch");
        let success = self.fresh_label("c256_sum_ok");
        let done = self.fresh_label("c256_sum_done");

        self.emit("addi sp, sp, -320");
        self.emit("sd ra, 312(sp)");
        for (index, register) in ["a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"].into_iter().enumerate() {
            self.emit(format!("sd {}, {}(sp)", register, index * 8));
            self.emit(format!("beqz {}, {}", register, bad_expected));
        }

        self.emit("ld a0, 0(sp)");
        self.emit("ld a1, 8(sp)");
        self.emit("addi a2, sp, 64");
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit("ld a0, 16(sp)");
        self.emit("ld a1, 24(sp)");
        self.emit("addi a2, sp, 96");
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit("addi a0, sp, 64");
        self.emit("addi a1, sp, 96");
        self.emit("addi a2, sp, 128");
        self.emit("call __cellscript_add_u256");
        self.emit(format!("bnez a0, {}", mismatch));

        self.emit("ld a0, 32(sp)");
        self.emit("ld a1, 40(sp)");
        self.emit("addi a2, sp, 160");
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit("ld a0, 48(sp)");
        self.emit("ld a1, 56(sp)");
        self.emit("addi a2, sp, 192");
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit("addi a0, sp, 160");
        self.emit("addi a1, sp, 192");
        self.emit("addi a2, sp, 224");
        self.emit("call __cellscript_add_u256");
        self.emit(format!("bnez a0, {}", mismatch));

        for limb_offset in [24, 16, 8, 0] {
            self.emit(format!("ld t0, {}(sp)", 128 + limb_offset));
            self.emit(format!("ld t1, {}(sp)", 224 + limb_offset));
            if equality {
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
            } else {
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", success));
                self.emit("sltu t2, t1, t0");
                self.emit(format!("bnez t2, {}", mismatch));
            }
        }

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 312(sp)");
        self.emit("addi sp, sp, 320");
        self.emit("ret");
    }

    fn emit_runtime_current_role_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_current_role");
        self.emit_label("__ckb_current_role");
        self.emit("# cellscript abi: current role helper; normal lowering folds role to a compile-time lock/type constant");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
        } else {
            self.emit(format!("li a0, {}", CKB_ROLE_UNKNOWN));
            self.emit("li a1, 0");
        }
        self.emit("ret");
    }

    fn emit_runtime_cell_occupied_capacity_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_cell_occupied_capacity");
        self.emit_label("__ckb_cell_occupied_capacity");
        self.emit("# cellscript abi: CKB occupied capacity via LOAD_CELL_BY_FIELD CellField::OccupiedCapacity");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("occupied_capacity_source_invalid");
        let failed = self.fresh_label("occupied_capacity_load_failed");
        let malformed = self.fresh_label("occupied_capacity_field_malformed");
        let done = self.fresh_label("occupied_capacity_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -48");
        self.emit("sd ra, 40(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("sd t1, 0(sp)");
        self.emit("sd t2, 8(sp)");
        self.emit("li t0, 8");
        self.emit("sd t0, 16(sp)");
        self.emit("addi a0, sp, 24");
        self.emit("addi a1, sp, 16");
        self.emit("li a2, 0");
        self.emit("ld a3, 0(sp)");
        self.emit("ld a4, 8(sp)");
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_OCCUPIED_CAPACITY));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit("ld t0, 16(sp)");
        self.emit(format!("bnez a0, {}", failed));
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("ld a0, 24(sp)");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit("addi sp, sp, 48");
        self.emit("ret");
    }

    fn emit_runtime_cell_unoccupied_capacity_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_cell_unoccupied_capacity");
        self.emit_label("__ckb_cell_unoccupied_capacity");
        self.emit("# cellscript abi: SourceView unoccupied capacity = capacity - occupied_capacity");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }

        let failed = self.fresh_label("unoccupied_capacity_failed");
        let failed_status_ok = self.fresh_label("unoccupied_capacity_failed_status_ok");
        let underflow = self.fresh_label("unoccupied_capacity_underflow");
        let done = self.fresh_label("unoccupied_capacity_done");

        self.emit("addi sp, sp, -48");
        self.emit("sd ra, 40(sp)");
        self.emit("sd a0, 32(sp)");
        self.emit("call __ckb_cell_capacity");
        self.emit(format!("bnez a1, {}", failed));
        self.emit("sd a0, 24(sp)");
        self.emit("ld a0, 32(sp)");
        self.emit("call __ckb_cell_occupied_capacity");
        self.emit(format!("bnez a1, {}", failed));
        self.emit("ld t0, 24(sp)");
        self.emit("sltu t1, t0, a0");
        self.emit(format!("bnez t1, {}", underflow));
        self.emit("sub a0, t0, a0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit("addi a0, a1, 0");
        self.emit(format!("bnez a0, {}", failed_status_ok));
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&failed_status_ok);
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&underflow);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit("addi sp, sp, 48");
        self.emit("ret");
    }

    fn emit_runtime_cell_output_index_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_cell_output_index");
        self.emit_label("__ckb_cell_output_index");
        self.emit("# cellscript abi: SourceView output index extractor");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("source_view_invalid");
        let output = self.fresh_label("source_view_output");
        let done = self.fresh_label("source_view_output_index_done");
        self.emit(format!("li t6, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("div t0, a0, t6");
        self.emit("rem t1, a0, t6");
        self.emit(format!("li t5, {}", CKB_SOURCE_VIEW_OUTPUT));
        self.emit("sub t4, t0, t5");
        self.emit(format!("beqz t4, {}", output));
        self.emit(format!("li t5, {}", CKB_SOURCE_VIEW_GROUP_OUTPUT));
        self.emit("sub t4, t0, t5");
        self.emit(format!("beqz t4, {}", output));
        self.emit(format!("j {}", invalid));
        self.emit_label(&output);
        self.emit("addi a0, t1, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ret");
    }

    fn emit_runtime_cell_data_size_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_cell_data_size");
        self.emit_label("__ckb_cell_data_size");
        self.emit("# cellscript abi: CKB SourceView LOAD_CELL_DATA size probe");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("source_view_invalid");
        let done = self.fresh_label("cell_data_size_done");
        let failed = self.fresh_label("cell_data_size_failed");
        let status_ok = self.fresh_label("cell_data_size_status_ok");
        let abi = self.runtime_abi();
        self.emit("addi sp, sp, -48");
        self.emit("sd ra, 40(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 0");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", status_ok));
        self.emit(format!("beqz a0, {}", status_ok));
        self.emit(format!("j {}", failed));
        self.emit_label(&status_ok);
        self.emit("ld a0, 8(sp)");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit("addi sp, sp, 48");
        self.emit("ret");
    }

    fn emit_runtime_cell_data_hash_helper(&mut self, symbol: &str, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView LOAD_CELL_DATA and Blake2b ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=out32_ptr, a2=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const BUFFER_OFFSET: usize = 16;
        const OUT_PTR_OFFSET: usize = BUFFER_OFFSET + RUNTIME_CELL_BUFFER_SIZE;
        const SIZE_PTR_OFFSET: usize = OUT_PTR_OFFSET + 8;
        const RA_OFFSET: usize = SIZE_PTR_OFFSET + 8;
        const FRAME_SIZE: usize = RA_OFFSET + 8;

        let invalid = self.fresh_label("cell_data_hash_source_invalid");
        let bad_output = self.fresh_label("cell_data_hash_output_invalid");
        let failed = self.fresh_label("cell_data_hash_load_failed");
        let done = self.fresh_label("cell_data_hash_done");
        let abi = self.runtime_abi();

        self.emit(format!("addi sp, sp, -{}", FRAME_SIZE));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", OUT_PTR_OFFSET));
        self.emit(format!("sd a2, {}(sp)", SIZE_PTR_OFFSET));
        self.emit(format!("beqz a1, {}", bad_output));
        self.emit(format!("beqz a2, {}", bad_output));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", RUNTIME_CELL_BUFFER_SIZE));
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("addi a0, sp, {}", BUFFER_OFFSET));
        self.emit(format!("ld a1, {}(sp)", SIZE_OFFSET));
        self.emit(format!("ld a2, {}(sp)", OUT_PTR_OFFSET));
        self.emit("call __ckb_hash_blake2b_var");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("ld t6, {}(sp)", SIZE_PTR_OFFSET));
        self.emit("li t0, 32");
        self.emit("sd t0, 0(t6)");
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_output);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit(format!("addi sp, sp, {}", FRAME_SIZE));
        self.emit("ret");
    }

    fn emit_runtime_cell_data_hash_at_helper(&mut self, symbol: &str, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {}; a0=source_view, a1=offset, a2=out[32], a3=size_ptr", detail));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("cell_data_hash_at_source_invalid");
        let failed = self.fresh_label("cell_data_hash_at_failed");
        let loaded = self.fresh_label("cell_data_hash_at_loaded");
        let done = self.fresh_label("cell_data_hash_at_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -80");
        self.emit("sd ra, 72(sp)");
        self.emit("sd a1, 8(sp)");
        self.emit("sd a2, 16(sp)");
        self.emit("sd a3, 24(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("ld a0, 16(sp)");
        self.emit("ld a1, 24(sp)");
        self.emit("ld a2, 8(sp)");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("bnez t1, {}", failed));
        self.emit_label(&loaded);
        self.emit("# cellscript abi: normalize fixed 32-byte slice length after LOAD_CELL_DATA");
        self.emit("ld t0, 24(sp)");
        self.emit("li t1, 32");
        self.emit("sd t1, 0(t0)");
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&done);
        self.emit("ld ra, 72(sp)");
        self.emit("addi sp, sp, 80");
        self.emit("ret");
    }

    fn emit_runtime_cell_data_word_le_helper(&mut self, symbol: &str, detail: &str, width: usize, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} via LOAD_CELL_DATA offset argument", detail));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("source_view_invalid");
        let done = self.fresh_label("cell_data_u64_done");
        let failed = self.fresh_label("cell_data_u64_failed");
        let loaded = self.fresh_label("cell_data_u64_loaded");
        let ready = self.fresh_label("cell_data_u64_ready");
        let abi = self.runtime_abi();
        self.emit("addi sp, sp, -64");
        self.emit("sd ra, 56(sp)");
        self.emit("# cellscript abi: save requested data offset");
        self.emit("sd a1, 8(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", width));
        self.emit("sd t0, 16(sp)");
        self.emit("addi a0, sp, 24");
        self.emit("addi a1, sp, 16");
        self.emit("ld a2, 8(sp)");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("j {}", failed));
        self.emit_label(&loaded);
        self.emit_label(&ready);
        if width == 4 {
            self.emit("li a0, 0");
            for byte_index in 0..4 {
                self.emit(format!("lbu t0, {}(sp)", 24 + byte_index));
                if byte_index != 0 {
                    self.emit(format!("slli t0, t0, {}", byte_index * 8));
                }
                self.emit("or a0, a0, t0");
            }
        } else {
            self.emit("li a0, 0");
            for byte_index in 0..8 {
                self.emit(format!("lbu t0, {}(sp)", 24 + byte_index));
                if byte_index != 0 {
                    self.emit(format!("slli t0, t0, {}", byte_index * 8));
                }
                self.emit("or a0, a0, t0");
            }
        }
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 56(sp)");
        self.emit("addi sp, sp, 64");
        self.emit("ret");
    }

    fn emit_runtime_dao_accumulated_rate_helper(&mut self, enabled: bool) {
        self.emit_global("__dao_accumulated_rate");
        self.emit_label("__dao_accumulated_rate");
        self.emit(
            "# cellscript abi: DAO accumulated-rate HeaderDep SourceView helper via LOAD_HEADER at absolute header offset 160+8",
        );
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("dao_header_source_invalid");
        let done = self.fresh_label("dao_accumulated_rate_done");
        let failed = self.fresh_label("dao_accumulated_rate_failed");
        let loaded = self.fresh_label("dao_accumulated_rate_loaded");
        let abi = self.runtime_abi();
        self.emit("addi sp, sp, -48");
        self.emit("sd ra, 40(sp)");
        self.emit(format!("li t6, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("div t0, a0, t6");
        self.emit("rem t1, a0, t6");
        self.emit(format!("li t5, {}", CKB_SOURCE_VIEW_HEADER_DEP));
        self.emit("sub t4, t0, t5");
        self.emit(format!("bnez t4, {}", invalid));
        self.emit("li t0, 8");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_ACCUMULATED_RATE_ABSOLUTE_OFFSET));
        self.emit("addi a3, t1, 0");
        self.emit(format!("li a4, {}", abi.source_header_dep));
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("bnez t1, {}", failed));
        self.emit_label(&loaded);
        self.emit("ld t0, 8(sp)");
        self.emit("li t1, 8");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit("ld a0, 16(sp)");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit("addi sp, sp, 48");
        self.emit("ret");
    }

    fn emit_runtime_dao_input_accumulated_rate_helper(&mut self, enabled: bool) {
        self.emit_global("__dao_input_accumulated_rate");
        self.emit_label("__dao_input_accumulated_rate");
        self.emit(
            "# cellscript abi: DAO accumulated-rate from Input/GroupInput committed header via LOAD_HEADER at absolute header offset 160+8",
        );
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("dao_input_header_source_invalid");
        let done = self.fresh_label("dao_input_accumulated_rate_done");
        let failed = self.fresh_label("dao_input_accumulated_rate_failed");
        let loaded = self.fresh_label("dao_input_accumulated_rate_loaded");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -48");
        self.emit("sd ra, 40(sp)");
        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_ACCUMULATED_RATE_ABSOLUTE_OFFSET));
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("bnez t1, {}", failed));
        self.emit_label(&loaded);
        self.emit("ld t0, 8(sp)");
        self.emit("li t1, 8");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit("ld a0, 16(sp)");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit("addi sp, sp, 48");
        self.emit("ret");
    }

    fn emit_runtime_dao_type_classifier_helper(&mut self, enabled: bool) {
        self.emit_global("__dao_has_dao_type");
        self.emit_label("__dao_has_dao_type");
        self.emit("# cellscript abi: NervosDAO type-hash classifier");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("dao_type_source_invalid");
        let false_label = self.fresh_label("dao_type_false");
        let done = self.fresh_label("dao_type_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -64");
        self.emit("sd ra, 56(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 32");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", false_label));
        self.emit("ld t0, 8(sp)");
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", false_label));
        for (word_index, expected) in CKB_DAO_TYPE_HASH_WORDS_LE.iter().enumerate() {
            self.emit(format!("ld t0, {}(sp)", 16 + word_index * 8));
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", false_label));
        }
        self.emit("li a0, 1");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&false_label);
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&done);
        self.emit("ld ra, 56(sp)");
        self.emit("addi sp, sp, 64");
        self.emit("ret");
    }

    fn emit_runtime_dao_cell_data_classifier_helper(&mut self, symbol: &str, detail: &str, deposit: bool, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} via LOAD_CELL_DATA exact 8-byte DAO data", detail));
        self.emit("# cellscript abi: matches NervosDAO deposit/withdrawal-request 8-byte data convention");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("dao_data_source_invalid");
        let false_label = self.fresh_label("dao_data_false");
        let true_label = self.fresh_label("dao_data_true");
        let done = self.fresh_label("dao_data_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -48");
        self.emit("sd ra, 40(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", false_label));
        self.emit("ld t0, 8(sp)");
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", false_label));
        self.emit("ld t0, 16(sp)");
        if deposit {
            self.emit(format!("beqz t0, {}", true_label));
            self.emit(format!("j {}", false_label));
        } else {
            self.emit(format!("bnez t0, {}", true_label));
            self.emit(format!("j {}", false_label));
        }

        self.emit_label(&true_label);
        self.emit("li a0, 1");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&false_label);
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit("addi sp, sp, 48");
        self.emit("ret");
    }

    fn emit_runtime_dao_require_header_dep_for_input_helper(&mut self, enabled: bool) {
        self.emit_global("__dao_require_header_dep_for_input");
        self.emit_label("__dao_require_header_dep_for_input");
        self.emit("# cellscript abi: DAO input header to HeaderDep lineage requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=HeaderDep SourceView; compares full 32-byte DAO fields");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const INPUT_INDEX_OFFSET: usize = 16;
        const INPUT_SOURCE_OFFSET: usize = 24;
        const HEADER_INDEX_OFFSET: usize = 32;
        const INPUT_DAO_OFFSET: usize = 40;
        const HEADER_DAO_OFFSET: usize = 72;
        const HEADER_VIEW_OFFSET: usize = 104;

        let invalid_input = self.fresh_label("dao_lineage_input_source_invalid");
        let invalid_header = self.fresh_label("dao_lineage_header_source_invalid");
        let input_failed = self.fresh_label("dao_lineage_input_header_missing");
        let header_failed = self.fresh_label("dao_lineage_header_dep_missing");
        let malformed = self.fresh_label("dao_lineage_dao_field_malformed");
        let mismatch = self.fresh_label("dao_lineage_mismatch");
        let done = self.fresh_label("dao_lineage_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -128");
        self.emit("sd ra, 120(sp)");
        self.emit(format!("sd a1, {}(sp)", HEADER_VIEW_OFFSET));

        self.emit_decode_input_source_view_to_t1_t2(&invalid_input);
        self.emit(format!("sd t1, {}(sp)", INPUT_INDEX_OFFSET));
        self.emit(format!("sd t2, {}(sp)", INPUT_SOURCE_OFFSET));

        self.emit(format!("ld a0, {}(sp)", HEADER_VIEW_OFFSET));
        self.emit(format!("li t6, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("div t0, a0, t6");
        self.emit("rem t1, a0, t6");
        self.emit(format!("li t5, {}", CKB_SOURCE_VIEW_HEADER_DEP));
        self.emit("sub t4, t0, t5");
        self.emit(format!("bnez t4, {}", invalid_header));
        self.emit(format!("sd t1, {}(sp)", HEADER_INDEX_OFFSET));

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", INPUT_DAO_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_FIELD_ABSOLUTE_OFFSET));
        self.emit(format!("ld a3, {}(sp)", INPUT_INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", INPUT_SOURCE_OFFSET));
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", input_failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", HEADER_DAO_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_FIELD_ABSOLUTE_OFFSET));
        self.emit(format!("ld a3, {}(sp)", HEADER_INDEX_OFFSET));
        self.emit(format!("li a4, {}", CKB_SOURCE_HEADER_DEP));
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", header_failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit(format!("addi a0, sp, {}", INPUT_DAO_OFFSET));
        self.emit(format!("addi a1, sp, {}", HEADER_DAO_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid_input);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&invalid_header);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&input_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&header_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoHeaderLineageMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 120(sp)");
        self.emit("addi sp, sp, 128");
        self.emit("ret");
    }

    fn emit_runtime_dao_require_input_since_at_least_helper(&mut self, enabled: bool) {
        self.emit_global("__dao_require_input_since_at_least");
        self.emit_label("__dao_require_input_since_at_least");
        self.emit("# cellscript abi: DAO input since lower-bound requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=required_since; enforces loaded_since >= required_since");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const REQUIRED_SINCE_OFFSET: usize = 16;
        const SINCE_OFFSET: usize = 24;

        let invalid = self.fresh_label("dao_since_input_source_invalid");
        let failed = self.fresh_label("dao_since_load_failed");
        let malformed = self.fresh_label("dao_since_field_malformed");
        let immature = self.fresh_label("dao_since_immature");
        let done = self.fresh_label("dao_since_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -48");
        self.emit("sd ra, 40(sp)");
        self.emit(format!("sd a1, {}(sp)", REQUIRED_SINCE_OFFSET));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SINCE_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_SINCE));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit(format!("ld t0, {}(sp)", SINCE_OFFSET));
        self.emit(format!("ld t1, {}(sp)", REQUIRED_SINCE_OFFSET));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", immature));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&immature);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoMaturityViolation.code()));
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit("addi sp, sp, 48");
        self.emit("ret");
    }

    fn emit_runtime_dao_require_input_relative_epoch_since_at_least_helper(&mut self, enabled: bool) {
        self.emit_global("__dao_require_input_relative_epoch_since_at_least");
        self.emit_label("__dao_require_input_relative_epoch_since_at_least");
        self.emit("# cellscript abi: DAO relative epoch since maturity requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=epoch_number, a2=epoch_index, a3=epoch_length");
        self.emit("# cellscript abi: loads input since, requires RFC0017 relative epoch flags, and compares epoch fractions");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const REQUIRED_NUMBER_OFFSET: usize = 16;
        const REQUIRED_INDEX_OFFSET: usize = 24;
        const REQUIRED_LENGTH_OFFSET: usize = 32;
        const SINCE_OFFSET: usize = 40;
        const LOADED_NUMBER_OFFSET: usize = 48;
        const LOADED_INDEX_OFFSET: usize = 56;
        const LOADED_LENGTH_OFFSET: usize = 64;

        let invalid = self.fresh_label("dao_epoch_since_input_source_invalid");
        let failed = self.fresh_label("dao_epoch_since_load_failed");
        let malformed = self.fresh_label("dao_epoch_since_malformed");
        let immature = self.fresh_label("dao_epoch_since_immature");
        let success = self.fresh_label("dao_epoch_since_success");
        let done = self.fresh_label("dao_epoch_since_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -80");
        self.emit("sd ra, 72(sp)");
        self.emit(format!("sd a1, {}(sp)", REQUIRED_NUMBER_OFFSET));
        self.emit(format!("sd a2, {}(sp)", REQUIRED_INDEX_OFFSET));
        self.emit(format!("sd a3, {}(sp)", REQUIRED_LENGTH_OFFSET));

        self.emit(format!("li t0, {}", CKB_EPOCH_NUMBER_BOUND));
        self.emit("sltu t1, a1, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("li t0, {}", CKB_EPOCH_FRACTION_BOUND));
        self.emit("sltu t1, a2, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit("sltu t1, a3, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("beqz a3, {}", malformed));
        self.emit("sltu t1, a2, a3");
        self.emit(format!("beqz t1, {}", malformed));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SINCE_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_SINCE));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit(format!("ld t0, {}(sp)", SINCE_OFFSET));
        self.emit("li t1, 1");
        self.emit("slli t1, t1, 63");
        self.emit("and t2, t0, t1");
        self.emit(format!("beqz t2, {}", malformed));
        self.emit(format!("li t1, {}", CKB_SINCE_REMAIN_FLAGS_BITS));
        self.emit("and t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit(format!("li t1, {}", CKB_SINCE_METRIC_TYPE_FLAG_MASK));
        self.emit("and t2, t0, t1");
        self.emit(format!("li t3, {}", CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG));
        self.emit("sub t4, t2, t3");
        self.emit(format!("bnez t4, {}", malformed));

        self.emit(format!("li t1, {}", CKB_SINCE_VALUE_MASK));
        self.emit("and t0, t0, t1");
        self.emit(format!("li t1, {}", CKB_EPOCH_NUMBER_MASK));
        self.emit("and t2, t0, t1");
        self.emit("srai t3, t0, 24");
        self.emit(format!("li t1, {}", CKB_EPOCH_FRACTION_MASK));
        self.emit("and t3, t3, t1");
        self.emit("srai t4, t0, 40");
        self.emit("and t4, t4, t1");
        self.emit(format!("beqz t4, {}", malformed));
        self.emit("sltu t5, t3, t4");
        self.emit(format!("beqz t5, {}", malformed));
        self.emit(format!("sd t2, {}(sp)", LOADED_NUMBER_OFFSET));
        self.emit(format!("sd t3, {}(sp)", LOADED_INDEX_OFFSET));
        self.emit(format!("sd t4, {}(sp)", LOADED_LENGTH_OFFSET));

        self.emit(format!("ld t0, {}(sp)", REQUIRED_NUMBER_OFFSET));
        self.emit("sltu t1, t0, t2");
        self.emit(format!("bnez t1, {}", success));
        self.emit("sltu t1, t2, t0");
        self.emit(format!("bnez t1, {}", immature));
        self.emit(format!("ld t0, {}(sp)", LOADED_INDEX_OFFSET));
        self.emit(format!("ld t1, {}(sp)", REQUIRED_LENGTH_OFFSET));
        self.emit("mul t2, t0, t1");
        self.emit(format!("ld t0, {}(sp)", REQUIRED_INDEX_OFFSET));
        self.emit(format!("ld t1, {}(sp)", LOADED_LENGTH_OFFSET));
        self.emit("mul t3, t0, t1");
        self.emit("sltu t4, t2, t3");
        self.emit(format!("bnez t4, {}", immature));

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&immature);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoMaturityViolation.code()));
        self.emit_label(&done);
        self.emit("ld ra, 72(sp)");
        self.emit("addi sp, sp, 80");
        self.emit("ret");
    }

    fn emit_runtime_xudt_amount_word_helper(&mut self, symbol: &str, detail: &str, offset: u64, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} via LOAD_CELL_DATA offset={}", detail, offset));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("source_view_invalid");
        let done = self.fresh_label("xudt_amount_done");
        let failed = self.fresh_label("xudt_amount_failed");
        let abi = self.runtime_abi();
        self.emit("addi sp, sp, -48");
        self.emit("sd ra, 40(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit(format!("li a2, {}", offset));
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit("ld a0, 16(sp)");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit("addi sp, sp, 48");
        self.emit("ret");
    }

    fn emit_runtime_xudt_require_owner_mode_input_type_helper(&mut self, enabled: bool) {
        self.emit_runtime_cell_hash_requirement_helper(
            "__xudt_require_owner_mode_input_type",
            "xUDT owner-mode input-type full 32-byte binding check",
            CKB_CELL_FIELD_TYPE_HASH,
            CellScriptRuntimeError::XudtBindingMismatch,
            enabled,
        );
    }

    fn emit_stack_u32_le_to(&mut self, dest: &str, stack_offset: usize) {
        self.emit(format!("lbu {}, {}(sp)", dest, stack_offset));
        self.emit(format!("lbu t4, {}(sp)", stack_offset + 1));
        self.emit("slli t4, t4, 8");
        self.emit(format!("or {}, {}, t4", dest, dest));
        self.emit(format!("lbu t4, {}(sp)", stack_offset + 2));
        self.emit("slli t4, t4, 16");
        self.emit(format!("or {}, {}, t4", dest, dest));
        self.emit(format!("lbu t4, {}(sp)", stack_offset + 3));
        self.emit("slli t4, t4, 24");
        self.emit(format!("or {}, {}, t4", dest, dest));
    }

    fn emit_u32_le_from_base_to(&mut self, dest: &str, base: &str, offset: usize, scratch: &str) {
        self.emit(format!("lbu {}, {}({})", dest, offset, base));
        self.emit(format!("lbu {}, {}({})", scratch, offset + 1, base));
        self.emit(format!("slli {}, {}, 8", scratch, scratch));
        self.emit(format!("or {}, {}, {}", dest, dest, scratch));
        self.emit(format!("lbu {}, {}({})", scratch, offset + 2, base));
        self.emit(format!("slli {}, {}, 16", scratch, scratch));
        self.emit(format!("or {}, {}, {}", dest, dest, scratch));
        self.emit(format!("lbu {}, {}({})", scratch, offset + 3, base));
        self.emit(format!("slli {}, {}, 24", scratch, scratch));
        self.emit(format!("or {}, {}, {}", dest, dest, scratch));
    }

    fn emit_runtime_xudt_require_owner_mode_type_args_helper(&mut self, enabled: bool) {
        self.emit_global("__xudt_require_owner_mode_type_args");
        self.emit_label("__xudt_require_owner_mode_type_args");
        self.emit("# cellscript abi: xUDT owner-mode Type Script args requirement");
        self.emit("# cellscript abi: args a0=SourceView, a1=owner_hash_ptr, a2=owner_hash_len, a3=flags_u32");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_BUFFER_OFFSET: usize = 16;
        const SCRIPT_SIZE_OFFSET: usize = 8;
        const OWNER_ARGS_OFFSET: usize = SCRIPT_BUFFER_OFFSET + 53;
        const FLAGS_ARGS_OFFSET: usize = OWNER_ARGS_OFFSET + 32;

        let invalid = self.fresh_label("xudt_args_source_invalid");
        let bad_expected = self.fresh_label("xudt_args_expected_invalid");
        let malformed = self.fresh_label("xudt_script_malformed");
        let failed = self.fresh_label("xudt_script_load_failed");
        let mismatch = self.fresh_label("xudt_args_mismatch");
        let done = self.fresh_label("xudt_args_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -192");
        self.emit("sd ra, 184(sp)");
        self.emit("sd a1, 176(sp)");
        self.emit("sd a2, 168(sp)");
        self.emit("sd a3, 160(sp)");

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));
        self.emit("li t0, 4294967296");
        self.emit("sltu t1, a3, t0");
        self.emit(format!("beqz t1, {}", mismatch));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 128");
        self.emit(format!("sd t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));

        self.emit(format!("ld t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit("li t1, 89");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        for (offset, expected) in [(0usize, 89u64), (4, 16), (8, 48), (12, 49), (49, 36)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit(format!("addi a0, sp, {}", OWNER_ARGS_OFFSET));
        self.emit("ld a1, 176(sp)");
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));

        self.emit_stack_u32_le_to("t0", FLAGS_ARGS_OFFSET);
        self.emit("ld t1, 160(sp)");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 184(sp)");
        self.emit("addi sp, sp, 192");
        self.emit("ret");
    }

    fn emit_runtime_xudt_require_owner_mode_type_args_current_script_helper(&mut self, enabled: bool) {
        self.emit_global("__xudt_require_owner_mode_type_args_current_script");
        self.emit_label("__xudt_require_owner_mode_type_args_current_script");
        self.emit("# cellscript abi: xUDT owner-mode Type Script args requirement bound to current script hash");
        self.emit("# cellscript abi: args a0=SourceView, a1=flags_u32; owner hash is LOAD_SCRIPT_HASH(current script)");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const SOURCE_VIEW_OFFSET: usize = 16;
        const FLAGS_OFFSET: usize = 24;
        const SCRIPT_HASH_OFFSET: usize = 32;

        let hash_failed = self.fresh_label("xudt_current_script_hash_load_failed");
        let hash_malformed = self.fresh_label("xudt_current_script_hash_malformed");
        let done = self.fresh_label("xudt_current_script_args_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -80");
        self.emit("sd ra, 72(sp)");
        self.emit(format!("sd a0, {}(sp)", SOURCE_VIEW_OFFSET));
        self.emit(format!("sd a1, {}(sp)", FLAGS_OFFSET));

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", hash_failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", hash_malformed));

        self.emit(format!("ld a0, {}(sp)", SOURCE_VIEW_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit("li a2, 32");
        self.emit(format!("ld a3, {}(sp)", FLAGS_OFFSET));
        self.emit("call __xudt_require_owner_mode_type_args");
        self.emit(format!("j {}", done));

        self.emit_label(&hash_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&hash_malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit_label(&done);
        self.emit("ld ra, 72(sp)");
        self.emit("addi sp, sp, 80");
        self.emit("ret");
    }

    fn emit_runtime_xudt_require_group_amount_conserved_helper(&mut self, enabled: bool) {
        self.emit_global("__xudt_require_group_amount_conserved");
        self.emit_label("__xudt_require_group_amount_conserved");
        self.emit("# cellscript abi: scans current xUDT type group and requires sum(inputs.amount) == sum(outputs.amount)");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const BUFFER_OFFSET: usize = 16;
        const INPUT_LOW_OFFSET: usize = 32;
        const INPUT_HIGH_OFFSET: usize = 40;
        const OUTPUT_LOW_OFFSET: usize = 48;
        const OUTPUT_HIGH_OFFSET: usize = 56;
        const INDEX_OFFSET: usize = 64;
        const SOURCE_OFFSET: usize = 72;
        const SUM_LOW_OFFSET: usize = 80;
        const SUM_HIGH_OFFSET: usize = 88;

        let scan_source = self.fresh_label("xudt_group_scan_source");
        let scan_loop = self.fresh_label("xudt_group_scan_loop");
        let scan_done = self.fresh_label("xudt_group_scan_done");
        let scan_failed = self.fresh_label("xudt_group_scan_failed");
        let scan_malformed = self.fresh_label("xudt_group_scan_malformed");
        let overflow = self.fresh_label("xudt_group_sum_overflow");
        let output_phase = self.fresh_label("xudt_group_output_phase");
        let compare = self.fresh_label("xudt_group_compare");
        let mismatch = self.fresh_label("xudt_group_mismatch");
        let done = self.fresh_label("xudt_group_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -112");
        self.emit("sd ra, 104(sp)");
        for offset in [INPUT_LOW_OFFSET, INPUT_HIGH_OFFSET, OUTPUT_LOW_OFFSET, OUTPUT_HIGH_OFFSET] {
            self.emit(format!("sd zero, {}(sp)", offset));
        }

        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("sd t0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("addi t0, sp, {}", INPUT_LOW_OFFSET));
        self.emit(format!("addi t1, sp, {}", INPUT_HIGH_OFFSET));
        self.emit(format!("j {}", scan_source));

        self.emit_label(&output_phase);
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit(format!("sd t0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("addi t0, sp, {}", OUTPUT_LOW_OFFSET));
        self.emit(format!("addi t1, sp, {}", OUTPUT_HIGH_OFFSET));

        self.emit_label(&scan_source);
        self.emit(format!("sd t0, {}(sp)", SUM_LOW_OFFSET));
        self.emit(format!("sd t1, {}(sp)", SUM_HIGH_OFFSET));
        self.emit(format!("sd zero, {}(sp)", INDEX_OFFSET));

        self.emit_label(&scan_loop);
        self.emit("li t0, 16");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", scan_done));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", compare));
        self.emit(format!("j {}", scan_failed));

        self.emit_label(&scan_done);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 16");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", scan_malformed));
        self.emit(format!("ld t0, {}(sp)", SUM_LOW_OFFSET));
        self.emit(format!("ld t1, {}(sp)", SUM_HIGH_OFFSET));
        self.emit("ld t2, 16(sp)");
        self.emit("ld t3, 24(sp)");
        self.emit("ld t4, 0(t0)");
        self.emit("ld t5, 0(t1)");
        self.emit("add t6, t4, t2");
        self.emit("sltu t4, t6, t4");
        self.emit("add t5, t5, t3");
        self.emit("sltu t3, t5, t3");
        self.emit(format!("bnez t3, {}", overflow));
        self.emit("add t5, t5, t4");
        self.emit("sltu t4, t5, t4");
        self.emit(format!("bnez t4, {}", overflow));
        self.emit("sd t6, 0(t0)");
        self.emit("sd t5, 0(t1)");
        self.emit(format!("ld t0, {}(sp)", INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", INDEX_OFFSET));
        self.emit(format!("j {}", scan_loop));

        self.emit_label(&compare);
        self.emit(format!("ld t0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li t1, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_phase));
        self.emit(format!("ld t0, {}(sp)", INPUT_LOW_OFFSET));
        self.emit(format!("ld t1, {}(sp)", OUTPUT_LOW_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit(format!("ld t0, {}(sp)", INPUT_HIGH_OFFSET));
        self.emit(format!("ld t1, {}(sp)", OUTPUT_HIGH_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&scan_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&scan_malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&overflow);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 104(sp)");
        self.emit("addi sp, sp, 112");
        self.emit("ret");
    }

    fn emit_runtime_xudt_require_group_amount_delta_helper(&mut self, symbol: &str, minted: bool, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if minted {
            self.emit(
                "# cellscript abi: scans current xUDT type group and requires sum(outputs.amount) == sum(inputs.amount) + delta",
            );
        } else {
            self.emit(
                "# cellscript abi: scans current xUDT type group and requires sum(inputs.amount) == sum(outputs.amount) + delta",
            );
        }
        self.emit("# cellscript abi: args a0=delta_u128_le_ptr");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const BUFFER_OFFSET: usize = 16;
        const INPUT_LOW_OFFSET: usize = 32;
        const INPUT_HIGH_OFFSET: usize = 40;
        const OUTPUT_LOW_OFFSET: usize = 48;
        const OUTPUT_HIGH_OFFSET: usize = 56;
        const INDEX_OFFSET: usize = 64;
        const SOURCE_OFFSET: usize = 72;
        const SUM_LOW_OFFSET: usize = 80;
        const SUM_HIGH_OFFSET: usize = 88;
        const DELTA_PTR_OFFSET: usize = 96;

        let bad_delta = self.fresh_label("xudt_group_delta_bad");
        let scan_source = self.fresh_label("xudt_group_delta_scan_source");
        let scan_loop = self.fresh_label("xudt_group_delta_scan_loop");
        let scan_done = self.fresh_label("xudt_group_delta_scan_done");
        let scan_failed = self.fresh_label("xudt_group_delta_scan_failed");
        let scan_malformed = self.fresh_label("xudt_group_delta_scan_malformed");
        let overflow = self.fresh_label("xudt_group_delta_overflow");
        let output_phase = self.fresh_label("xudt_group_delta_output_phase");
        let compare = self.fresh_label("xudt_group_delta_compare");
        let mismatch = self.fresh_label("xudt_group_delta_mismatch");
        let done = self.fresh_label("xudt_group_delta_done");
        let abi = self.runtime_abi();

        self.emit("addi sp, sp, -128");
        self.emit("sd ra, 120(sp)");
        self.emit(format!("beqz a0, {}", bad_delta));
        self.emit(format!("sd a0, {}(sp)", DELTA_PTR_OFFSET));
        for offset in [INPUT_LOW_OFFSET, INPUT_HIGH_OFFSET, OUTPUT_LOW_OFFSET, OUTPUT_HIGH_OFFSET] {
            self.emit(format!("sd zero, {}(sp)", offset));
        }

        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("sd t0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("addi t0, sp, {}", INPUT_LOW_OFFSET));
        self.emit(format!("addi t1, sp, {}", INPUT_HIGH_OFFSET));
        self.emit(format!("j {}", scan_source));

        self.emit_label(&output_phase);
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit(format!("sd t0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("addi t0, sp, {}", OUTPUT_LOW_OFFSET));
        self.emit(format!("addi t1, sp, {}", OUTPUT_HIGH_OFFSET));

        self.emit_label(&scan_source);
        self.emit(format!("sd t0, {}(sp)", SUM_LOW_OFFSET));
        self.emit(format!("sd t1, {}(sp)", SUM_HIGH_OFFSET));
        self.emit(format!("sd zero, {}(sp)", INDEX_OFFSET));

        self.emit_label(&scan_loop);
        self.emit("li t0, 16");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", scan_done));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", compare));
        self.emit(format!("j {}", scan_failed));

        self.emit_label(&scan_done);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 16");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", scan_malformed));
        self.emit(format!("ld t0, {}(sp)", SUM_LOW_OFFSET));
        self.emit(format!("ld t1, {}(sp)", SUM_HIGH_OFFSET));
        self.emit("ld t2, 16(sp)");
        self.emit("ld t3, 24(sp)");
        self.emit("ld t4, 0(t0)");
        self.emit("ld t5, 0(t1)");
        self.emit("add t6, t4, t2");
        self.emit("sltu t4, t6, t4");
        self.emit("add t5, t5, t3");
        self.emit("sltu t3, t5, t3");
        self.emit(format!("bnez t3, {}", overflow));
        self.emit("add t5, t5, t4");
        self.emit("sltu t4, t5, t4");
        self.emit(format!("bnez t4, {}", overflow));
        self.emit("sd t6, 0(t0)");
        self.emit("sd t5, 0(t1)");
        self.emit(format!("ld t0, {}(sp)", INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", INDEX_OFFSET));
        self.emit(format!("j {}", scan_loop));

        self.emit_label(&compare);
        self.emit(format!("ld t0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li t1, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_phase));

        self.emit(format!("ld a0, {}(sp)", DELTA_PTR_OFFSET));
        self.emit("ld t2, 0(a0)");
        self.emit("ld t3, 8(a0)");
        if minted {
            self.emit(format!("ld t0, {}(sp)", INPUT_LOW_OFFSET));
            self.emit(format!("ld t1, {}(sp)", INPUT_HIGH_OFFSET));
            self.emit(format!("ld t4, {}(sp)", OUTPUT_LOW_OFFSET));
            self.emit(format!("ld t5, {}(sp)", OUTPUT_HIGH_OFFSET));
        } else {
            self.emit(format!("ld t0, {}(sp)", OUTPUT_LOW_OFFSET));
            self.emit(format!("ld t1, {}(sp)", OUTPUT_HIGH_OFFSET));
            self.emit(format!("ld t4, {}(sp)", INPUT_LOW_OFFSET));
            self.emit(format!("ld t5, {}(sp)", INPUT_HIGH_OFFSET));
        }
        self.emit("add t6, t0, t2");
        self.emit("sltu t0, t6, t0");
        self.emit("add t1, t1, t3");
        self.emit("sltu t3, t1, t3");
        self.emit(format!("bnez t3, {}", overflow));
        self.emit("add t1, t1, t0");
        self.emit("sltu t0, t1, t0");
        self.emit(format!("bnez t0, {}", overflow));
        self.emit("sub t0, t6, t4");
        self.emit(format!("bnez t0, {}", mismatch));
        self.emit("sub t0, t1, t5");
        self.emit(format!("bnez t0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&bad_delta);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&scan_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&scan_malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&overflow);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 120(sp)");
        self.emit("addi sp, sp, 128");
        self.emit("ret");
    }

    fn emit_runtime_memcmp_fixed(&mut self) {
        self.emit_global("__cellscript_memcmp_fixed");
        self.emit_label("__cellscript_memcmp_fixed");
        self.emit("# cellscript abi: fixed-byte helper compares a0/a1 for a2 bytes; returns a0=0 when equal");
        let loop_label = ".L__cellscript_memcmp_fixed_loop";
        let mismatch_label = ".L__cellscript_memcmp_fixed_mismatch";
        let equal_label = ".L__cellscript_memcmp_fixed_equal";
        self.emit(format!("beqz a2, {}", equal_label));
        self.emit_label(loop_label);
        self.emit("lbu t0, 0(a0)");
        self.emit("lbu t1, 0(a1)");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch_label));
        self.emit("addi a0, a0, 1");
        self.emit("addi a1, a1, 1");
        self.emit("addi a2, a2, -1");
        self.emit(format!("bnez a2, {}", loop_label));
        self.emit_label(equal_label);
        self.emit("li a0, 0");
        self.emit("ret");
        self.emit_label(mismatch_label);
        self.emit("li a0, 1");
        self.emit("ret");
    }

    fn emit_runtime_memzero_fixed(&mut self) {
        self.emit_global("__cellscript_memzero_fixed");
        self.emit_label("__cellscript_memzero_fixed");
        self.emit("# cellscript abi: fixed-byte helper checks a0 for a1 zero bytes; returns a0=0 when all zero");
        let loop_label = ".L__cellscript_memzero_fixed_loop";
        let mismatch_label = ".L__cellscript_memzero_fixed_mismatch";
        let equal_label = ".L__cellscript_memzero_fixed_equal";
        self.emit(format!("beqz a1, {}", equal_label));
        self.emit_label(loop_label);
        self.emit("lbu t0, 0(a0)");
        self.emit(format!("bnez t0, {}", mismatch_label));
        self.emit("addi a0, a0, 1");
        self.emit("addi a1, a1, -1");
        self.emit(format!("bnez a1, {}", loop_label));
        self.emit_label(equal_label);
        self.emit("li a0, 0");
        self.emit("ret");
        self.emit_label(mismatch_label);
        self.emit("li a0, 1");
        self.emit("ret");
    }

    fn emit_runtime_memcpy_fixed(&mut self) {
        self.emit_global("__cellscript_memcpy_fixed");
        self.emit_label("__cellscript_memcpy_fixed");
        self.emit("# cellscript abi: fixed-byte helper copies a0 to a1 for a2 bytes; returns a0=0");
        let loop_label = ".L__cellscript_memcpy_fixed_loop";
        let done_label = ".L__cellscript_memcpy_fixed_done";
        self.emit(format!("beqz a2, {}", done_label));
        self.emit_label(loop_label);
        self.emit("lbu t0, 0(a0)");
        self.emit("sb t0, 0(a1)");
        self.emit("addi a0, a0, 1");
        self.emit("addi a1, a1, 1");
        self.emit("addi a2, a2, -1");
        self.emit(format!("bnez a2, {}", loop_label));
        self.emit_label(done_label);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    fn emit_runtime_size_guards(&mut self) {
        self.emit_global("__cellscript_require_min_size");
        self.emit_label("__cellscript_require_min_size");
        self.emit("# cellscript abi: returns a0=0 when actual size a0 is at least required size a1");
        self.emit("slt a0, a0, a1");
        self.emit("ret");

        self.emit_global("__cellscript_require_exact_size");
        self.emit_label("__cellscript_require_exact_size");
        self.emit("# cellscript abi: returns a0=0 when actual size a0 equals expected size a1");
        self.emit("sub a0, a0, a1");
        self.emit("ret");
    }

    fn emit_runtime_header_field_u64(&mut self, symbol: &str, field_name: &str, field_id: u64, enabled: bool, disabled_reason: &str) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_runtime_error_comment(CellScriptRuntimeError::ConsumeInvalidOperand);
            self.emit(format!("li a0, {}", CellScriptRuntimeError::ConsumeInvalidOperand.code()));
            self.emit("ret");
            return;
        }

        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("ra", 24);
        self.emit(format!("# cellscript abi: LOAD_HEADER_BY_FIELD field={} source=HeaderDep index=0", field_name));
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", abi.source_header_dep));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_header_by_field));
        self.emit("ecall");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }

    fn emit_runtime_input_field_u64(&mut self, symbol: &str, field_name: &str, field_id: u64, enabled: bool, disabled_reason: &str) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_runtime_error_comment(CellScriptRuntimeError::ConsumeInvalidOperand);
            self.emit(format!("li a0, {}", CellScriptRuntimeError::ConsumeInvalidOperand.code()));
            self.emit("ret");
            return;
        }

        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("ra", 24);
        self.emit(format!("# cellscript abi: LOAD_INPUT_BY_FIELD field={} source=GroupInput index=0", field_name));
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", abi.source_group_input));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }

    fn assemble(&self, format: ArtifactFormat) -> Result<Vec<u8>> {
        let assembly_text = self.assembly.join("\n");
        match format {
            ArtifactFormat::RiscvAssembly => Ok(assembly_text.into_bytes()),
            ArtifactFormat::RiscvElf => {
                // All former non-executable runtime paths now have real RISC-V
                // lowerings or fail-closed traps with specific error codes.
                // ELF emission is always permitted.
                assemble_elf(&self.assembly)
            }
        }
    }
}

pub fn generate(ir: &IrModule, options: &CodegenOptions, format: ArtifactFormat) -> Result<Vec<u8>> {
    let generator = CodeGenerator::new(options.clone());
    generator.generate(ir, format)
}

pub fn analyze_backend_shape(assembly: &str) -> Result<BackendShapeMetrics> {
    let lines = assembly.lines().map(str::to_string).collect::<Vec<_>>();
    MachineLayoutPlan::build(&lines).map(|plan| plan.metrics.into())
}

fn first_entrypoint(ir: &IrModule) -> Option<(&str, &[IrParam])> {
    for item in &ir.items {
        if let IrItem::Action(action) = item {
            if action.name == "main" {
                return Some((&action.name, &action.params));
            }
        }
    }
    for item in &ir.items {
        if let IrItem::Action(action) = item {
            if action.params.is_empty() {
                return Some((&action.name, &action.params));
            }
        }
    }
    for item in &ir.items {
        if let IrItem::Action(action) = item {
            return Some((&action.name, &action.params));
        }
    }
    for item in &ir.items {
        if let IrItem::Lock(lock) = item {
            return Some((&lock.name, &lock.params));
        }
    }
    None
}

fn entry_witness_payload_layout(params: &[IrParam], runtime_bound_param_indices: &BTreeSet<usize>) -> Vec<EntryWitnessPayloadArg> {
    params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            if !entry_param_consumes_witness_payload(param, index, runtime_bound_param_indices) {
                EntryWitnessPayloadArg { width: 0, schema_dynamic: false, unsupported: false }
            } else if entry_witness_dynamic_schema_param(&param.ty) {
                EntryWitnessPayloadArg { width: 4, schema_dynamic: true, unsupported: false }
            } else if let Some(width) =
                fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty))
            {
                EntryWitnessPayloadArg { width, schema_dynamic: false, unsupported: false }
            } else if let Some(width) = entry_witness_register_param_width(&param.ty) {
                EntryWitnessPayloadArg { width, schema_dynamic: false, unsupported: false }
            } else {
                EntryWitnessPayloadArg { width: 0, schema_dynamic: false, unsupported: true }
            }
        })
        .collect()
}

fn entry_param_consumes_witness_payload(param: &IrParam, index: usize, runtime_bound_param_indices: &BTreeSet<usize>) -> bool {
    param.source != ParamSource::LockArgs
        && !runtime_bound_param_indices.contains(&index)
        && !matches!(param.ty, IrType::Ref(_) | IrType::MutRef(_))
}

fn entry_witness_dynamic_schema_param(ty: &IrType) -> bool {
    fixed_byte_pointer_param_width(ty).is_none()
        && fixed_aggregate_pointer_param_width(ty).is_none()
        && entry_witness_register_param_width(ty).is_none()
}

fn entry_witness_register_param_width(ty: &IrType) -> Option<usize> {
    fixed_register_width(ty, type_static_length(ty)).or_else(|| match ty {
        IrType::Array(_, _) | IrType::Tuple(_) => type_static_length(ty).filter(|width| (1..=8).contains(width)),
        IrType::Unit => Some(0),
        _ => None,
    })
}

fn named_type_name(ty: &IrType) -> Option<&str> {
    match ty {
        IrType::Named(name) => Some(name.as_str()),
        IrType::Ref(inner) | IrType::MutRef(inner) => named_type_name(inner),
        _ => None,
    }
}

fn consumed_operand_var(instruction: &IrInstruction) -> Option<&IrVar> {
    let operand = match instruction {
        IrInstruction::Consume { operand }
        | IrInstruction::Transfer { operand, .. }
        | IrInstruction::Destroy { operand, .. }
        | IrInstruction::Settle { operand, .. }
        | IrInstruction::ReplaceUnique { operand, .. } => operand,
        IrInstruction::Claim { receipt, .. } => receipt,
        _ => return None,
    };
    match operand {
        IrOperand::Var(var) if named_type_name(&var.ty).is_some() => Some(var),
        _ => None,
    }
}

const ELF_HEADER_SIZE: usize = 64;
const ELF_PROGRAM_HEADER_SIZE: usize = 56;
const ELF_SEGMENT_ALIGN: usize = 0x1000;
const ELF_PF_X: u32 = 1;
#[cfg(test)]
const ELF_PF_W: u32 = 2;
const ELF_PF_R: u32 = 4;
const ELF_BASE_ADDR: u64 = 0x10000;
const START_TRAMPOLINE_SIZE: usize = 20;
const EXIT_SYSCALL_NUMBER: i64 = 93;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SectionKind {
    Text,
    Rodata,
}

#[derive(Debug, Clone)]
enum AsmOp {
    Label(String),
    Instruction(Instruction),
    Word(u32),
    Byte(u8),
    Ascii(Vec<u8>),
    Align(usize),
}

#[derive(Debug, Clone, Copy)]
struct SymbolDef {
    section: SectionKind,
    offset: usize,
}

#[derive(Debug, Clone, Copy)]
struct SectionLayout {
    text_base: u64,
    text_user_base: u64,
    rodata_base: u64,
}

impl SectionLayout {
    fn for_text_user_size(text_user_size: usize) -> Self {
        let rodata_offset = align_up(START_TRAMPOLINE_SIZE + text_user_size, 8);
        Self {
            text_base: ELF_BASE_ADDR,
            text_user_base: ELF_BASE_ADDR + START_TRAMPOLINE_SIZE as u64,
            rodata_base: ELF_BASE_ADDR + rodata_offset as u64,
        }
    }

    fn rodata_offset(&self) -> Result<usize> {
        usize::try_from(self.rodata_base - self.text_base)
            .map_err(|_| CompileError::new("ELF rodata offset does not fit usize", crate::error::Span::default()))
    }
}

#[derive(Debug)]
struct MachineLayoutPlan {
    parsed: ParsedAssembly,
    layout: SectionLayout,
    cfg: MachineCfg,
    order: MachineLayoutOrder,
    metrics: BackendLayoutMetrics,
}

#[derive(Debug, Clone, Copy, Default)]
struct BackendLayoutMetrics {
    text_size: usize,
    rodata_size: usize,
    executable_text_op_count: usize,
    covered_text_op_count: usize,
    relaxed_branch_count: usize,
    max_cond_branch_abs_distance: u64,
    machine_block_count: usize,
    max_machine_block_size: usize,
    conditional_branch_block_count: usize,
    labeled_machine_block_count: usize,
    machine_cfg_edge_count: usize,
    machine_call_edge_count: usize,
    unreachable_machine_block_count: usize,
    layout_order_block_count: usize,
    layout_order_text_size: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct BackendShapeMetrics {
    pub text_size: usize,
    pub rodata_size: usize,
    pub executable_text_op_count: usize,
    pub covered_text_op_count: usize,
    pub relaxed_branch_count: usize,
    pub max_cond_branch_abs_distance: u64,
    pub machine_block_count: usize,
    pub max_machine_block_size: usize,
    pub conditional_branch_block_count: usize,
    pub labeled_machine_block_count: usize,
    pub machine_cfg_edge_count: usize,
    pub machine_call_edge_count: usize,
    pub unreachable_machine_block_count: usize,
    pub layout_order_block_count: usize,
    pub layout_order_text_size: usize,
}

impl From<BackendLayoutMetrics> for BackendShapeMetrics {
    fn from(metrics: BackendLayoutMetrics) -> Self {
        Self {
            text_size: metrics.text_size,
            rodata_size: metrics.rodata_size,
            executable_text_op_count: metrics.executable_text_op_count,
            covered_text_op_count: metrics.covered_text_op_count,
            relaxed_branch_count: metrics.relaxed_branch_count,
            max_cond_branch_abs_distance: metrics.max_cond_branch_abs_distance,
            machine_block_count: metrics.machine_block_count,
            max_machine_block_size: metrics.max_machine_block_size,
            conditional_branch_block_count: metrics.conditional_branch_block_count,
            labeled_machine_block_count: metrics.labeled_machine_block_count,
            machine_cfg_edge_count: metrics.machine_cfg_edge_count,
            machine_call_edge_count: metrics.machine_call_edge_count,
            unreachable_machine_block_count: metrics.unreachable_machine_block_count,
            layout_order_block_count: metrics.layout_order_block_count,
            layout_order_text_size: metrics.layout_order_text_size,
        }
    }
}

#[derive(Debug, Clone)]
enum Instruction {
    Addi { rd: u8, rs1: u8, imm: i64 },
    Add { rd: u8, rs1: u8, rs2: u8 },
    Sub { rd: u8, rs1: u8, rs2: u8 },
    And { rd: u8, rs1: u8, rs2: u8 },
    Or { rd: u8, rs1: u8, rs2: u8 },
    Xor { rd: u8, rs1: u8, rs2: u8 },
    Mul { rd: u8, rs1: u8, rs2: u8 },
    Mulhu { rd: u8, rs1: u8, rs2: u8 },
    Div { rd: u8, rs1: u8, rs2: u8 },
    Divu { rd: u8, rs1: u8, rs2: u8 },
    Rem { rd: u8, rs1: u8, rs2: u8 },
    Remu { rd: u8, rs1: u8, rs2: u8 },
    Slt { rd: u8, rs1: u8, rs2: u8 },
    Sltu { rd: u8, rs1: u8, rs2: u8 },
    Sgt { rd: u8, rs1: u8, rs2: u8 },
    Xori { rd: u8, rs1: u8, imm: i64 },
    Seqz { rd: u8, rs: u8 },
    Snez { rd: u8, rs: u8 },
    Neg { rd: u8, rs: u8 },
    Ld { rd: u8, rs1: u8, imm: i64 },
    Lbu { rd: u8, rs1: u8, imm: i64 },
    Sb { rs2: u8, rs1: u8, imm: i64 },
    Sh { rs2: u8, rs1: u8, imm: i64 },
    Sw { rs2: u8, rs1: u8, imm: i64 },
    Sd { rs2: u8, rs1: u8, imm: i64 },
    Slli { rd: u8, rs1: u8, shamt: i64 },
    Srai { rd: u8, rs1: u8, shamt: i64 },
    Srli { rd: u8, rs1: u8, shamt: i64 },
    Li { rd: u8, imm: i128 },
    La { rd: u8, label: String },
    Call { label: String },
    Jump { label: String },
    Beq { rs1: u8, rs2: u8, label: String },
    Bne { rs1: u8, rs2: u8, label: String },
    Blt { rs1: u8, rs2: u8, label: String },
    Bge { rs1: u8, rs2: u8, label: String },
    Bltu { rs1: u8, rs2: u8, label: String },
    Bgeu { rs1: u8, rs2: u8, label: String },
    Beqz { rs: u8, label: String },
    Bnez { rs: u8, label: String },
    Ret,
    Ecall,
}

fn assemble_elf(lines: &[String]) -> Result<Vec<u8>> {
    reject_unresolved_calls(lines)?;
    if let Some(external) = try_external_elf_toolchain(lines)? {
        return Ok(external);
    }
    assemble_elf_internal(lines)
}

fn reject_unresolved_calls(lines: &[String]) -> Result<()> {
    let mut labels = BTreeSet::new();
    let mut calls = BTreeSet::new();

    for line in lines {
        let Some(clean) = strip_comment(line) else {
            continue;
        };
        if let Some(label) = clean.strip_suffix(':') {
            labels.insert(label.trim().to_string());
            continue;
        }
        if let Some(target) = clean.strip_prefix("call ") {
            let target = target.trim();
            if !target.is_empty() {
                calls.insert(target.to_string());
            }
        }
    }

    let missing = calls.difference(&labels).cloned().collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }

    Err(CompileError::without_span(format!(
        "unresolved call target(s) in generated assembly: {}; production ELF emission requires all call targets to be lowered",
        missing.join(", ")
    )))
}

fn assemble_elf_internal(lines: &[String]) -> Result<Vec<u8>> {
    let plan = MachineLayoutPlan::build(lines)?;
    let parsed = &plan.parsed;
    let layout = plan.layout;
    let _layout_control_metrics = (
        plan.metrics.executable_text_op_count,
        plan.metrics.covered_text_op_count,
        plan.metrics.relaxed_branch_count,
        plan.metrics.max_cond_branch_abs_distance,
        plan.metrics.machine_block_count,
        plan.metrics.max_machine_block_size,
        plan.metrics.conditional_branch_block_count,
        plan.metrics.labeled_machine_block_count,
        plan.metrics.machine_cfg_edge_count,
        plan.metrics.machine_call_edge_count,
        plan.metrics.unreachable_machine_block_count,
        plan.metrics.layout_order_block_count,
        plan.metrics.layout_order_text_size,
        plan.cfg.blocks.len(),
        plan.cfg.edges.len(),
        plan.order.block_order.len(),
        plan.order.placed_blocks.len(),
        plan.order.text_size,
    );
    let entry_label = parsed.entry_label.as_deref().ok_or_else(|| {
        CompileError::new("ELF target requires at least one action or lock entry point", crate::error::Span::default())
    })?;
    let text_user_size = plan.metrics.text_size;
    let rodata_size = plan.metrics.rodata_size;
    let rodata_offset = layout.rodata_offset()?;
    let mut text_bytes = Vec::with_capacity(START_TRAMPOLINE_SIZE + text_user_size);
    if entry_requires_explicit_parameter_abi(lines, entry_label) {
        encode_li_sequence(&mut text_bytes, 10, 25)?;
    } else {
        let entry_addr = parsed.symbol_address(entry_label, &layout)?;
        encode_call_sequence(&mut text_bytes, layout.text_base, entry_addr)?;
    }
    encode_li_sequence(&mut text_bytes, 17, i128::from(EXIT_SYSCALL_NUMBER))?;
    text_bytes.extend_from_slice(&encode_ecall().to_le_bytes());
    debug_assert_eq!(text_bytes.len(), START_TRAMPOLINE_SIZE);
    parsed.encode_section(SectionKind::Text, &mut text_bytes, &layout, START_TRAMPOLINE_SIZE)?;

    let mut rodata_bytes = Vec::with_capacity(rodata_size);
    parsed.encode_section(SectionKind::Rodata, &mut rodata_bytes, &layout, 0)?;

    let segment_file_payload_size = rodata_offset + rodata_bytes.len();
    let segment_file_offset = align_up(ELF_HEADER_SIZE + ELF_PROGRAM_HEADER_SIZE, ELF_SEGMENT_ALIGN);
    let load_segment_offset = 0u64;
    let load_segment_vaddr = layout.text_base.checked_sub(segment_file_offset as u64).ok_or_else(|| {
        CompileError::new("ELF text base is smaller than the load segment file offset", crate::error::Span::default())
    })?;
    let load_segment_file_size = segment_file_offset + segment_file_payload_size;
    let mut elf = vec![0u8; load_segment_file_size];
    write_elf_header(&mut elf[..ELF_HEADER_SIZE], layout.text_base, 1)?;
    write_program_header(
        &mut elf[ELF_HEADER_SIZE..ELF_HEADER_SIZE + ELF_PROGRAM_HEADER_SIZE],
        ELF_PF_R | ELF_PF_X,
        load_segment_offset,
        load_segment_vaddr,
        load_segment_file_size as u64,
        load_segment_file_size as u64,
    )?;

    let segment = &mut elf[segment_file_offset..segment_file_offset + segment_file_payload_size];
    segment[..text_bytes.len()].copy_from_slice(&text_bytes);
    segment[rodata_offset..rodata_offset + rodata_bytes.len()].copy_from_slice(&rodata_bytes);
    Ok(elf)
}

fn try_external_elf_toolchain(lines: &[String]) -> Result<Option<Vec<u8>>> {
    let Some(toolchain) = discover_external_toolchain()? else {
        return Ok(None);
    };
    let parsed = ParsedAssembly::from_lines(lines)?;
    let entry_label = parsed.entry_label.as_deref().ok_or_else(|| {
        CompileError::new("ELF target requires at least one action or lock entry point", crate::error::Span::default())
    })?;

    let temp_dir = make_external_toolchain_temp_dir()?;
    let _temp_dir_cleanup = TempDirCleanup(temp_dir.clone());
    let asm_path = temp_dir.join("module.s");
    let elf_path = temp_dir.join("module.elf");
    let obj_path = temp_dir.join("module.o");
    fs::write(&asm_path, render_external_assembly(lines, entry_label)).map_err(|err| {
        CompileError::new(
            format!("failed to write temporary assembly file '{}': {}", asm_path.display(), err),
            crate::error::Span::default(),
        )
    })?;

    let external_result = match &toolchain.mode {
        ExternalToolchainMode::Compiler(compiler) => run_external_command(
            Command::new(compiler)
                .arg("-nostdlib")
                .arg("-march=rv64imac")
                .arg("-mabi=lp64")
                .arg("-Wl,--strip-all")
                .arg("-Wl,-e,_start")
                .arg("-Wl,-Ttext=0x10000")
                .arg("-o")
                .arg(&elf_path)
                .arg(&asm_path),
            "RISC-V compiler",
        ),
        ExternalToolchainMode::AssemblerLinker { assembler, linker } => run_external_command(
            Command::new(assembler).arg("-march=rv64imac").arg("-mabi=lp64").arg(&asm_path).arg("-o").arg(&obj_path),
            "RISC-V assembler",
        )
        .and_then(|_| {
            run_external_command(
                Command::new(linker)
                    .arg("-m")
                    .arg("elf64lriscv")
                    .arg("--strip-all")
                    .arg("-e")
                    .arg("_start")
                    .arg("-Ttext")
                    .arg("0x10000")
                    .arg("-o")
                    .arg(&elf_path)
                    .arg(&obj_path),
                "RISC-V linker",
            )
        }),
    };

    let elf = match external_result {
        Ok(()) => fs::read(&elf_path).map_err(|err| {
            CompileError::new(
                format!("failed to read external ELF output '{}': {}", elf_path.display(), err),
                crate::error::Span::default(),
            )
        }),
        Err(err) => Err(err),
    };

    let elf = elf.and_then(|bytes| {
        if bytes.starts_with(b"\x7fELF") {
            Ok(bytes)
        } else {
            Err(CompileError::new(
                format!("external toolchain output '{}' is not an ELF file", elf_path.display()),
                crate::error::Span::default(),
            ))
        }
    })?;

    Ok(Some(elf))
}

struct TempDirCleanup(PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn render_external_assembly(lines: &[String], entry_label: &str) -> String {
    let mut rendered =
        vec![".section .text".to_string(), ".global _start".to_string(), ".type _start, @function".to_string(), "_start:".to_string()];
    if entry_requires_explicit_parameter_abi(lines, entry_label) {
        let error = CellScriptRuntimeError::EntryWitnessAbiInvalid;
        rendered.push(format!("    # cellscript runtime error {} {}", error.code(), error.name()));
        rendered.push(format!("    li a0, {}", error.code()));
    } else {
        rendered.push(format!("    call {}", entry_label));
    }
    rendered.push(format!("    li a7, {}", EXIT_SYSCALL_NUMBER));
    rendered.push("    ecall".to_string());
    rendered.extend(lines.iter().filter(|line| !line.trim_start().starts_with(".option arch,")).cloned());
    let mut rendered = rendered.join("\n");
    rendered.push('\n');
    rendered
}

fn entry_requires_explicit_parameter_abi(lines: &[String], entry_label: &str) -> bool {
    let marker = format!("# cellscript entry abi: {} requires-explicit-parameter-abi", entry_label);
    lines.iter().any(|line| line.trim() == marker)
}

#[derive(Debug, Clone)]
struct ExternalToolchain {
    mode: ExternalToolchainMode,
}

#[derive(Debug, Clone)]
enum ExternalToolchainMode {
    Compiler(PathBuf),
    AssemblerLinker { assembler: PathBuf, linker: PathBuf },
}

fn discover_external_toolchain() -> Result<Option<ExternalToolchain>> {
    let explicit_compiler = explicit_toolchain_path("CELLSCRIPT_RISCV_CC")?;
    let explicit_assembler = explicit_toolchain_path("CELLSCRIPT_RISCV_AS")?;
    let explicit_linker = explicit_toolchain_path("CELLSCRIPT_RISCV_LD")?;

    if let Some(compiler) = explicit_compiler {
        if explicit_assembler.is_some() || explicit_linker.is_some() {
            return Err(CompileError::new(
                "set either CELLSCRIPT_RISCV_CC or CELLSCRIPT_RISCV_AS/CELLSCRIPT_RISCV_LD, not both",
                crate::error::Span::default(),
            ));
        }
        return Ok(Some(ExternalToolchain { mode: ExternalToolchainMode::Compiler(compiler) }));
    }

    match (explicit_assembler, explicit_linker) {
        (Some(assembler), Some(linker)) => {
            return Ok(Some(ExternalToolchain { mode: ExternalToolchainMode::AssemblerLinker { assembler, linker } }));
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(CompileError::new(
                "CELLSCRIPT_RISCV_AS and CELLSCRIPT_RISCV_LD must be set together",
                crate::error::Span::default(),
            ));
        }
        (None, None) => {}
    }

    Ok(None)
}

fn explicit_toolchain_path(var: &str) -> Result<Option<PathBuf>> {
    env::var_os(var).map(PathBuf::from).map(|path| validate_explicit_toolchain_path(var, path)).transpose()
}

fn validate_explicit_toolchain_path(var: &str, path: PathBuf) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(CompileError::new(
            format!("{} must be an absolute path, got '{}'", var, path.display()),
            crate::error::Span::default(),
        ));
    }

    let metadata = fs::metadata(&path).map_err(|err| {
        CompileError::new(
            format!("{} points to unreadable toolchain path '{}': {}", var, path.display(), err),
            crate::error::Span::default(),
        )
    })?;
    if !metadata.is_file() {
        return Err(CompileError::new(
            format!("{} must point to an executable file, got '{}'", var, path.display()),
            crate::error::Span::default(),
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(CompileError::new(
                format!("{} path '{}' is not executable", var, path.display()),
                crate::error::Span::default(),
            ));
        }
    }

    Ok(path)
}

fn run_external_command(command: &mut Command, label: &str) -> Result<()> {
    let rendered = render_command(command);
    let output = command.output().map_err(|err| {
        CompileError::new(format!("failed to launch {} ({}): {}", label, rendered, err), crate::error::Span::default())
    })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = format!("{} failed ({}): {}", label, rendered, stderr.trim());
    Err(CompileError::new(message, crate::error::Span::default()))
}

fn render_command(command: &Command) -> String {
    let program = command.get_program().to_string_lossy();
    let args = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
    if args.is_empty() {
        program.into_owned()
    } else {
        format!("{} {}", program, args)
    }
}

fn make_external_toolchain_temp_dir() -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or_default();
    let dir = env::temp_dir().join(format!("cellscript-riscv-{}-{}", std::process::id(), stamp));
    fs::create_dir_all(&dir).map_err(|err| {
        CompileError::new(
            format!("failed to create temporary toolchain directory '{}': {}", dir.display(), err),
            crate::error::Span::default(),
        )
    })?;
    Ok(dir)
}

#[derive(Debug, Default)]
struct ParsedAssembly {
    text_ops: Vec<AsmOp>,
    rodata_ops: Vec<AsmOp>,
    text_size: usize,
    rodata_size: usize,
    symbols: HashMap<String, SymbolDef>,
    entry_label: Option<String>,
    relaxed_text_branches: BTreeSet<usize>,
}

impl ParsedAssembly {
    fn from_lines(lines: &[String]) -> Result<Self> {
        Self::from_lines_with_branch_mode(lines, BranchSizeMode::Exact(&BTreeSet::new()))
    }

    fn from_lines_relaxed(lines: &[String], layout: &SectionLayout) -> Result<Self> {
        let conservative = Self::from_lines_with_branch_mode(lines, BranchSizeMode::Conservative)?;
        let relaxed_text_branches = conservative.relaxed_branch_indices(layout)?;
        Self::from_lines_with_branch_mode(lines, BranchSizeMode::Exact(&relaxed_text_branches))
    }

    fn from_lines_with_branch_mode(lines: &[String], branch_size_mode: BranchSizeMode<'_>) -> Result<Self> {
        let mut current_section = SectionKind::Text;
        let mut text_size = 0usize;
        let mut rodata_size = 0usize;
        let mut text_ops = Vec::new();
        let mut rodata_ops = Vec::new();
        let mut symbols = HashMap::new();
        let mut globals = BTreeSet::new();
        let mut entry_label = None;
        let mut fallback_entry = None;

        for line in lines {
            let Some(clean) = strip_comment(line) else {
                continue;
            };
            if clean.is_empty() {
                continue;
            }

            if let Some(section) = parse_section_directive(clean)? {
                current_section = section;
                continue;
            }
            if clean.starts_with(".option ") || clean.starts_with(".type ") {
                continue;
            }
            if let Some(symbol) = clean.strip_prefix(".global ") {
                globals.insert(symbol.trim().to_string());
                continue;
            }

            let (ops, offset) = match current_section {
                SectionKind::Text => (&mut text_ops, &mut text_size),
                SectionKind::Rodata => (&mut rodata_ops, &mut rodata_size),
            };
            let op_index = ops.len();

            if let Some(label) = clean.strip_suffix(':') {
                let label = label.trim().to_string();
                let symbol = SymbolDef { section: current_section, offset: *offset };
                if symbols.insert(label.clone(), symbol).is_some() {
                    return Err(CompileError::new(format!("duplicate assembly label '{}'", label), crate::error::Span::default()));
                }
                if current_section == SectionKind::Text && globals.contains(&label) {
                    if fallback_entry.is_none() {
                        fallback_entry = Some(label.clone());
                    }
                    if !label.starts_with("__") && entry_label.is_none() {
                        entry_label = Some(label.clone());
                    }
                }
                ops.push(AsmOp::Label(label));
                continue;
            }

            let op = parse_asm_op(clean)?;
            *offset += op_size(&op, *offset, current_section, op_index, branch_size_mode);
            ops.push(op);
        }

        Ok(Self {
            text_ops,
            rodata_ops,
            text_size,
            rodata_size,
            symbols,
            entry_label: entry_label.or(fallback_entry),
            relaxed_text_branches: branch_size_mode.relaxed_text_branches().cloned().unwrap_or_default(),
        })
    }

    fn relaxed_branch_indices(&self, layout: &SectionLayout) -> Result<BTreeSet<usize>> {
        let mut relaxed = BTreeSet::new();
        let mut offset = 0usize;
        for (index, op) in self.text_ops.iter().enumerate() {
            if let AsmOp::Instruction(inst) = op {
                if conditional_branch_parts(inst).is_some() {
                    let pc = layout.text_user_base + offset as u64;
                    let target = branch_target(inst, self, layout)?;
                    if !signed_bits_fit(relative_offset(pc, target)?, 13) {
                        relaxed.insert(index);
                    }
                }
            }
            offset += op_size(op, offset, SectionKind::Text, index, BranchSizeMode::Conservative);
        }
        Ok(relaxed)
    }

    fn section_size(&self, section: SectionKind) -> usize {
        match section {
            SectionKind::Text => self.text_size,
            SectionKind::Rodata => self.rodata_size,
        }
    }

    fn symbol_address(&self, label: &str, layout: &SectionLayout) -> Result<u64> {
        let symbol = self
            .symbols
            .get(label)
            .ok_or_else(|| CompileError::new(format!("unknown assembly label '{}'", label), crate::error::Span::default()))?;
        Ok(match symbol.section {
            SectionKind::Text => layout.text_user_base + symbol.offset as u64,
            SectionKind::Rodata => layout.rodata_base + symbol.offset as u64,
        })
    }

    fn encode_section(&self, section: SectionKind, out: &mut Vec<u8>, layout: &SectionLayout, base_bias: usize) -> Result<()> {
        let ops = match section {
            SectionKind::Text => &self.text_ops,
            SectionKind::Rodata => &self.rodata_ops,
        };
        let section_base = match section {
            SectionKind::Text => layout.text_user_base,
            SectionKind::Rodata => layout.rodata_base,
        };

        for (op_index, op) in ops.iter().enumerate() {
            match op {
                AsmOp::Label(_) => {}
                AsmOp::Word(word) => out.extend_from_slice(&word.to_le_bytes()),
                AsmOp::Byte(byte) => out.push(*byte),
                AsmOp::Ascii(bytes) => out.extend_from_slice(bytes),
                AsmOp::Align(bytes) => pad_to_alignment(out, *bytes),
                AsmOp::Instruction(inst) => {
                    let section_offset = out.len().checked_sub(base_bias).ok_or_else(|| {
                        CompileError::new("assembly output offset is smaller than section base bias", crate::error::Span::default())
                    })?;
                    let pc = section_base + section_offset as u64;
                    encode_instruction(
                        out,
                        inst,
                        pc,
                        self,
                        layout,
                        section == SectionKind::Text && self.relaxed_text_branches.contains(&op_index),
                    )?;
                }
            }
        }

        Ok(())
    }
}

impl MachineLayoutPlan {
    fn build(lines: &[String]) -> Result<Self> {
        let preliminary = ParsedAssembly::from_lines_with_branch_mode(lines, BranchSizeMode::Conservative)?;
        let preliminary_layout = SectionLayout::for_text_user_size(preliminary.section_size(SectionKind::Text));
        let parsed = ParsedAssembly::from_lines_relaxed(lines, &preliminary_layout)?;
        let layout = SectionLayout::for_text_user_size(parsed.section_size(SectionKind::Text));
        let cfg = machine_cfg(&parsed)?;
        let coverage = validate_machine_block_coverage(&parsed, &cfg)?;
        let order = machine_layout_order(&cfg)?;
        let metrics = parsed.layout_metrics(&layout, &cfg, &order, coverage)?;
        Ok(Self { parsed, layout, cfg, order, metrics })
    }
}

#[derive(Debug, Clone, Copy)]
struct TextOpLayout {
    op_index: usize,
    offset: usize,
    size: usize,
}

#[derive(Debug, Clone)]
struct MachineBlock {
    label: Option<String>,
    op_start: usize,
    op_end: usize,
    byte_start: usize,
    byte_size: usize,
    terminator: MachineTerminator,
}

#[derive(Debug, Clone)]
struct MachineCfg {
    blocks: Vec<MachineBlock>,
    edges: Vec<MachineCfgEdge>,
}

#[derive(Debug, Clone, Copy, Default)]
struct MachineBlockCoverage {
    executable_text_op_count: usize,
    covered_text_op_count: usize,
}

#[derive(Debug, Clone)]
struct MachineLayoutOrder {
    block_order: Vec<usize>,
    placed_blocks: Vec<MachinePlacedBlock>,
    text_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MachinePlacedBlock {
    block_index: usize,
    byte_start: usize,
    byte_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MachineCfgEdge {
    from: usize,
    to: usize,
    kind: MachineCfgEdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MachineCfgEdgeKind {
    Fallthrough,
    Jump,
    ConditionalTaken,
    ConditionalFallthrough,
    Call,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MachineTerminator {
    Fallthrough,
    Jump { target: String },
    ConditionalBranch { target: String },
    Return,
}

fn text_op_layouts(parsed: &ParsedAssembly) -> Vec<TextOpLayout> {
    let mut offset = 0usize;
    let mut layouts = Vec::with_capacity(parsed.text_ops.len());
    for (op_index, op) in parsed.text_ops.iter().enumerate() {
        let size = op_size(op, offset, SectionKind::Text, op_index, BranchSizeMode::Exact(&parsed.relaxed_text_branches));
        layouts.push(TextOpLayout { op_index, offset, size });
        offset += size;
    }
    layouts
}

fn machine_blocks(parsed: &ParsedAssembly) -> Vec<MachineBlock> {
    let layouts = text_op_layouts(parsed);
    let mut blocks = Vec::new();
    let mut block_start = 0usize;
    let mut block_label = None;

    for (op_index, op) in parsed.text_ops.iter().enumerate() {
        if let AsmOp::Label(label) = op {
            if block_has_executable_ops(&parsed.text_ops[block_start..op_index]) {
                blocks.push(build_machine_block(parsed, &layouts, block_start, op_index, block_label.take()));
                block_start = op_index;
            }
            if block_label.is_none() {
                block_label = Some(label.clone());
            }
            continue;
        }

        if instruction_terminator(op).is_some() {
            blocks.push(build_machine_block(parsed, &layouts, block_start, op_index + 1, block_label.take()));
            block_start = op_index + 1;
        }
    }

    if block_start < parsed.text_ops.len() && block_has_executable_ops(&parsed.text_ops[block_start..]) {
        blocks.push(build_machine_block(parsed, &layouts, block_start, parsed.text_ops.len(), block_label));
    }

    blocks
}

fn machine_cfg(parsed: &ParsedAssembly) -> Result<MachineCfg> {
    let blocks = machine_blocks(parsed);
    let label_to_block = machine_label_to_block(parsed, &blocks);
    let mut edges = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        for target in machine_block_call_targets(parsed, block) {
            if let Some(&target_block) = label_to_block.get(&target) {
                edges.push(MachineCfgEdge { from: index, to: target_block, kind: MachineCfgEdgeKind::Call });
            }
        }
        match &block.terminator {
            MachineTerminator::Fallthrough => {
                if index + 1 < blocks.len() {
                    edges.push(MachineCfgEdge { from: index, to: index + 1, kind: MachineCfgEdgeKind::Fallthrough });
                }
            }
            MachineTerminator::Jump { target } => {
                edges.push(MachineCfgEdge {
                    from: index,
                    to: machine_cfg_target_block(target, &label_to_block)?,
                    kind: MachineCfgEdgeKind::Jump,
                });
            }
            MachineTerminator::ConditionalBranch { target } => {
                edges.push(MachineCfgEdge {
                    from: index,
                    to: machine_cfg_target_block(target, &label_to_block)?,
                    kind: MachineCfgEdgeKind::ConditionalTaken,
                });
                if index + 1 < blocks.len() {
                    edges.push(MachineCfgEdge { from: index, to: index + 1, kind: MachineCfgEdgeKind::ConditionalFallthrough });
                }
            }
            MachineTerminator::Return => {}
        }
    }

    Ok(MachineCfg { blocks, edges })
}

fn validate_machine_block_coverage(parsed: &ParsedAssembly, cfg: &MachineCfg) -> Result<MachineBlockCoverage> {
    let executable_text_op_count = parsed.text_ops.iter().filter(|op| !matches!(op, AsmOp::Label(_))).count();
    let mut covered = BTreeSet::new();

    for block in &cfg.blocks {
        if block.op_start >= block.op_end || block.op_end > parsed.text_ops.len() {
            return Err(CompileError::new(
                format!("machine block has invalid op range {}..{}", block.op_start, block.op_end),
                crate::error::Span::default(),
            ));
        }
        if !block_has_executable_ops(&parsed.text_ops[block.op_start..block.op_end]) {
            return Err(CompileError::new("machine block contains no executable instructions", crate::error::Span::default()));
        }
        for op_index in block.op_start..block.op_end {
            if matches!(parsed.text_ops[op_index], AsmOp::Label(_)) {
                continue;
            }
            if !covered.insert(op_index) {
                return Err(CompileError::new(
                    format!("machine block coverage overlaps text op {}", op_index),
                    crate::error::Span::default(),
                ));
            }
        }
    }

    if covered.len() != executable_text_op_count {
        return Err(CompileError::new(
            format!("machine blocks cover {} executable text ops but assembly contains {}", covered.len(), executable_text_op_count),
            crate::error::Span::default(),
        ));
    }

    Ok(MachineBlockCoverage { executable_text_op_count, covered_text_op_count: covered.len() })
}

fn machine_layout_order(cfg: &MachineCfg) -> Result<MachineLayoutOrder> {
    let block_order = (0..cfg.blocks.len()).collect::<Vec<_>>();
    build_machine_layout_order(cfg, block_order)
}

fn build_machine_layout_order(cfg: &MachineCfg, block_order: Vec<usize>) -> Result<MachineLayoutOrder> {
    validate_machine_layout_order(cfg, &block_order)?;
    let mut byte_start = 0usize;
    let mut placed_blocks = Vec::with_capacity(block_order.len());
    for &block_index in &block_order {
        let block = &cfg.blocks[block_index];
        placed_blocks.push(MachinePlacedBlock { block_index, byte_start, byte_size: block.byte_size });
        byte_start += block.byte_size;
    }
    Ok(MachineLayoutOrder { block_order, placed_blocks, text_size: byte_start })
}

fn validate_machine_layout_order(cfg: &MachineCfg, block_order: &[usize]) -> Result<()> {
    if block_order.len() != cfg.blocks.len() {
        return Err(CompileError::new(
            format!("machine layout order contains {} blocks but CFG contains {}", block_order.len(), cfg.blocks.len()),
            crate::error::Span::default(),
        ));
    }

    let mut seen = BTreeSet::new();
    for &block_index in block_order {
        if block_index >= cfg.blocks.len() {
            return Err(CompileError::new(
                format!("machine layout order references missing block {}", block_index),
                crate::error::Span::default(),
            ));
        }
        if !seen.insert(block_index) {
            return Err(CompileError::new(
                format!("machine layout order repeats block {}", block_index),
                crate::error::Span::default(),
            ));
        }
    }

    Ok(())
}

fn machine_label_to_block(parsed: &ParsedAssembly, blocks: &[MachineBlock]) -> HashMap<String, usize> {
    let mut label_to_block = HashMap::new();
    for (label, symbol) in &parsed.symbols {
        if symbol.section != SectionKind::Text {
            continue;
        }
        if let Some((block_index, _)) = blocks.iter().enumerate().find(|(_, block)| block.byte_start == symbol.offset) {
            label_to_block.insert(label.clone(), block_index);
        }
    }
    label_to_block
}

fn machine_cfg_target_block(target: &str, label_to_block: &HashMap<String, usize>) -> Result<usize> {
    label_to_block.get(target).copied().ok_or_else(|| {
        CompileError::new(format!("assembly branch target '{}' does not start a machine block", target), crate::error::Span::default())
    })
}

fn machine_block_call_targets(parsed: &ParsedAssembly, block: &MachineBlock) -> Vec<String> {
    parsed.text_ops[block.op_start..block.op_end]
        .iter()
        .filter_map(|op| match op {
            AsmOp::Instruction(Instruction::Call { label }) => Some(label.clone()),
            _ => None,
        })
        .collect()
}

fn unreachable_machine_block_count(parsed: &ParsedAssembly, cfg: &MachineCfg) -> usize {
    if cfg.blocks.is_empty() {
        return 0;
    }
    let label_to_block = machine_label_to_block(parsed, &cfg.blocks);
    let mut roots = parsed.entry_label.as_ref().and_then(|label| label_to_block.get(label).copied()).into_iter().collect::<Vec<_>>();
    if roots.is_empty() {
        roots.push(0);
    }
    let mut reachable = BTreeSet::new();
    let mut stack = roots;
    while let Some(block) = stack.pop() {
        if !reachable.insert(block) {
            continue;
        }
        for edge in cfg.edges.iter().filter(|edge| edge.from == block) {
            stack.push(edge.to);
        }
    }
    cfg.blocks.len().saturating_sub(reachable.len())
}

fn block_has_executable_ops(ops: &[AsmOp]) -> bool {
    ops.iter().any(|op| !matches!(op, AsmOp::Label(_)))
}

fn build_machine_block(
    parsed: &ParsedAssembly,
    layouts: &[TextOpLayout],
    op_start: usize,
    op_end: usize,
    label: Option<String>,
) -> MachineBlock {
    let byte_start = layouts.get(op_start).map(|layout| layout.offset).unwrap_or(0);
    let byte_end =
        op_end.checked_sub(1).and_then(|last| layouts.get(last).map(|layout| layout.offset + layout.size)).unwrap_or(byte_start);
    let terminator =
        parsed.text_ops[op_start..op_end].iter().rev().find_map(instruction_terminator).unwrap_or(MachineTerminator::Fallthrough);
    MachineBlock { label, op_start, op_end, byte_start, byte_size: byte_end.saturating_sub(byte_start), terminator }
}

fn instruction_terminator(op: &AsmOp) -> Option<MachineTerminator> {
    match op {
        AsmOp::Instruction(Instruction::Jump { label }) => Some(MachineTerminator::Jump { target: label.clone() }),
        AsmOp::Instruction(Instruction::Ret) => Some(MachineTerminator::Return),
        AsmOp::Instruction(inst) => {
            conditional_branch_parts(inst).map(|(_, _, label, _)| MachineTerminator::ConditionalBranch { target: label.to_string() })
        }
        _ => None,
    }
}

impl ParsedAssembly {
    fn layout_metrics(
        &self,
        layout: &SectionLayout,
        machine_cfg: &MachineCfg,
        machine_order: &MachineLayoutOrder,
        coverage: MachineBlockCoverage,
    ) -> Result<BackendLayoutMetrics> {
        let text_op_layouts = text_op_layouts(self);
        let text_size = text_op_layouts.iter().map(|op| op.size).sum();
        let mut max_cond_branch_abs_distance = 0u64;
        for op_layout in text_op_layouts {
            let AsmOp::Instruction(inst) = &self.text_ops[op_layout.op_index] else {
                continue;
            };
            if conditional_branch_parts(inst).is_none() {
                continue;
            };
            let pc = layout.text_user_base + op_layout.offset as u64;
            let target = branch_target(inst, self, layout)?;
            let distance = relative_offset(pc, target)?.unsigned_abs();
            max_cond_branch_abs_distance = max_cond_branch_abs_distance.max(distance);
        }
        let machine_block_count = machine_cfg.blocks.len();
        let max_machine_block_size = machine_cfg.blocks.iter().map(|block| block.byte_size).max().unwrap_or_default();
        let conditional_branch_block_count =
            machine_cfg.blocks.iter().filter(|block| matches!(block.terminator, MachineTerminator::ConditionalBranch { .. })).count();
        let labeled_machine_block_count = machine_cfg.blocks.iter().filter(|block| block.label.is_some()).count();
        let machine_cfg_edge_count = machine_cfg.edges.len();
        let machine_call_edge_count = machine_cfg.edges.iter().filter(|edge| edge.kind == MachineCfgEdgeKind::Call).count();
        let unreachable_machine_block_count = unreachable_machine_block_count(self, machine_cfg);
        let layout_order_block_count = machine_order.block_order.len();
        let layout_order_text_size = machine_order.text_size;
        let _covered_text_ops = machine_cfg.blocks.iter().map(|block| block.op_end.saturating_sub(block.op_start)).sum::<usize>();
        let _first_block_byte_start = machine_cfg.blocks.first().map(|block| block.byte_start).unwrap_or_default();
        Ok(BackendLayoutMetrics {
            text_size,
            rodata_size: self.section_size(SectionKind::Rodata),
            executable_text_op_count: coverage.executable_text_op_count,
            covered_text_op_count: coverage.covered_text_op_count,
            relaxed_branch_count: self.relaxed_text_branches.len(),
            max_cond_branch_abs_distance,
            machine_block_count,
            max_machine_block_size,
            conditional_branch_block_count,
            labeled_machine_block_count,
            machine_cfg_edge_count,
            machine_call_edge_count,
            unreachable_machine_block_count,
            layout_order_block_count,
            layout_order_text_size,
        })
    }
}

fn parse_section_directive(line: &str) -> Result<Option<SectionKind>> {
    if let Some(section) = line.strip_prefix(".section ") {
        return match section.trim() {
            ".text" => Ok(Some(SectionKind::Text)),
            ".rodata" => Ok(Some(SectionKind::Rodata)),
            other => Err(CompileError::new(format!("unsupported assembly section '{}'", other), crate::error::Span::default())),
        };
    }
    Ok(None)
}

fn parse_asm_op(line: &str) -> Result<AsmOp> {
    if let Some(value) = line.strip_prefix(".word ") {
        let value = parse_immediate(value.trim())?;
        return Ok(AsmOp::Word(
            u32::try_from(value).map_err(|_| {
                CompileError::new(format!("'.word' value '{}' does not fit u32", value), crate::error::Span::default())
            })?,
        ));
    }
    if let Some(value) = line.strip_prefix(".byte ") {
        let value = parse_immediate(value.trim())?;
        return Ok(AsmOp::Byte(
            u8::try_from(value)
                .map_err(|_| CompileError::new(format!("'.byte' value '{}' does not fit u8", value), crate::error::Span::default()))?,
        ));
    }
    if let Some(value) = line.strip_prefix(".ascii ") {
        return Ok(AsmOp::Ascii(parse_ascii_literal(value.trim())?));
    }
    if let Some(value) = line.strip_prefix(".align ") {
        let align_pow = parse_immediate(value.trim())?;
        if !(0..=16).contains(&align_pow) {
            return Err(CompileError::new(format!("unsupported .align value '{}'", align_pow), crate::error::Span::default()));
        }
        return Ok(AsmOp::Align(1usize << (align_pow as usize)));
    }
    Ok(AsmOp::Instruction(parse_instruction(line)?))
}

fn parse_instruction(line: &str) -> Result<Instruction> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let opcode = parts.next().unwrap().trim();
    let args = parts.next().unwrap_or("").trim();
    let args = if args.is_empty() { Vec::new() } else { args.split(',').map(|arg| arg.trim().to_string()).collect() };

    match opcode {
        "addi" => Ok(Instruction::Addi {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            imm: parse_immediate(arg(&args, 2)?)?,
        }),
        "add" => Ok(Instruction::Add {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "sub" => Ok(Instruction::Sub {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "and" => Ok(Instruction::And {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "or" => Ok(Instruction::Or {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "xor" => Ok(Instruction::Xor {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "mul" => Ok(Instruction::Mul {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "mulhu" => Ok(Instruction::Mulhu {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "div" => Ok(Instruction::Div {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "divu" => Ok(Instruction::Divu {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "rem" => Ok(Instruction::Rem {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "remu" => Ok(Instruction::Remu {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "slt" => Ok(Instruction::Slt {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "sltu" => Ok(Instruction::Sltu {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "sgt" => Ok(Instruction::Sgt {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "xori" => Ok(Instruction::Xori {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            imm: parse_immediate(arg(&args, 2)?)?,
        }),
        "seqz" => Ok(Instruction::Seqz { rd: parse_register(arg(&args, 0)?)?, rs: parse_register(arg(&args, 1)?)? }),
        "snez" => Ok(Instruction::Snez { rd: parse_register(arg(&args, 0)?)?, rs: parse_register(arg(&args, 1)?)? }),
        "neg" => Ok(Instruction::Neg { rd: parse_register(arg(&args, 0)?)?, rs: parse_register(arg(&args, 1)?)? }),
        "ld" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Ld { rd: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "lbu" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Lbu { rd: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "sb" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Sb { rs2: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "sh" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Sh { rs2: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "sw" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Sw { rs2: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "sd" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Sd { rs2: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "slli" => Ok(Instruction::Slli {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            shamt: parse_immediate(arg(&args, 2)?)?,
        }),
        "srai" => Ok(Instruction::Srai {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            shamt: parse_immediate(arg(&args, 2)?)?,
        }),
        "srli" => Ok(Instruction::Srli {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            shamt: parse_immediate(arg(&args, 2)?)?,
        }),
        "li" => Ok(Instruction::Li { rd: parse_register(arg(&args, 0)?)?, imm: parse_li_immediate(arg(&args, 1)?)? }),
        "mv" => Ok(Instruction::Addi { rd: parse_register(arg(&args, 0)?)?, rs1: parse_register(arg(&args, 1)?)?, imm: 0 }),
        "la" => Ok(Instruction::La { rd: parse_register(arg(&args, 0)?)?, label: arg(&args, 1)?.to_string() }),
        "call" => Ok(Instruction::Call { label: arg(&args, 0)?.to_string() }),
        "j" => Ok(Instruction::Jump { label: arg(&args, 0)?.to_string() }),
        "bgt" => Ok(Instruction::Blt {
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 0)?)?,
            label: arg(&args, 2)?.to_string(),
        }),
        "bgez" => Ok(Instruction::Bge { rs1: parse_register(arg(&args, 0)?)?, rs2: 0, label: arg(&args, 1)?.to_string() }),
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" => {
            let rs1 = parse_register(arg(&args, 0)?)?;
            let rs2 = parse_register(arg(&args, 1)?)?;
            let label = arg(&args, 2)?.to_string();
            match opcode {
                "beq" => Ok(Instruction::Beq { rs1, rs2, label }),
                "bne" => Ok(Instruction::Bne { rs1, rs2, label }),
                "blt" => Ok(Instruction::Blt { rs1, rs2, label }),
                "bge" => Ok(Instruction::Bge { rs1, rs2, label }),
                "bltu" => Ok(Instruction::Bltu { rs1, rs2, label }),
                "bgeu" => Ok(Instruction::Bgeu { rs1, rs2, label }),
                _ => unreachable!("branch opcode matched above"),
            }
        }
        "beqz" => Ok(Instruction::Beqz { rs: parse_register(arg(&args, 0)?)?, label: arg(&args, 1)?.to_string() }),
        "bnez" => Ok(Instruction::Bnez { rs: parse_register(arg(&args, 0)?)?, label: arg(&args, 1)?.to_string() }),
        "ret" => Ok(Instruction::Ret),
        "ecall" => Ok(Instruction::Ecall),
        other => Err(CompileError::new(format!("unsupported assembly instruction '{}'", other), crate::error::Span::default())),
    }
}

#[derive(Debug, Clone, Copy)]
enum BranchSizeMode<'a> {
    Conservative,
    Exact(&'a BTreeSet<usize>),
}

impl<'a> BranchSizeMode<'a> {
    fn relaxed_text_branches(self) -> Option<&'a BTreeSet<usize>> {
        match self {
            Self::Conservative => None,
            Self::Exact(branches) => Some(branches),
        }
    }
}

fn branch_target(inst: &Instruction, parsed: &ParsedAssembly, layout: &SectionLayout) -> Result<u64> {
    if let Some((_, _, label, _)) = conditional_branch_parts(inst) {
        parsed.symbol_address(label, layout)
    } else {
        Err(CompileError::new("instruction is not a conditional branch", crate::error::Span::default()))
    }
}

fn conditional_branch_parts(inst: &Instruction) -> Option<(u8, u8, &str, u32)> {
    match inst {
        Instruction::Beq { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b000)),
        Instruction::Bne { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b001)),
        Instruction::Blt { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b100)),
        Instruction::Bge { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b101)),
        Instruction::Bltu { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b110)),
        Instruction::Bgeu { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b111)),
        Instruction::Beqz { rs, label } => Some((*rs, 0, label.as_str(), 0b000)),
        Instruction::Bnez { rs, label } => Some((*rs, 0, label.as_str(), 0b001)),
        _ => None,
    }
}

fn inverse_branch_funct3(funct3: u32) -> u32 {
    match funct3 {
        0b000 => 0b001,
        0b001 => 0b000,
        0b100 => 0b101,
        0b101 => 0b100,
        0b110 => 0b111,
        0b111 => 0b110,
        _ => unreachable!("unsupported branch funct3"),
    }
}

fn encode_instruction(
    out: &mut Vec<u8>,
    inst: &Instruction,
    pc: u64,
    parsed: &ParsedAssembly,
    layout: &SectionLayout,
    relaxed_branch: bool,
) -> Result<()> {
    match inst {
        Instruction::Addi { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b000, *rs1, *imm)?.to_le_bytes()),
        Instruction::Add { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Sub { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, *rs1, *rs2, 0b0100000).to_le_bytes())
        }
        Instruction::And { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b111, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Or { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b110, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Xor { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b100, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Mul { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Mulhu { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b011, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Div { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b100, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Divu { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b101, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Rem { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b110, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Remu { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b111, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Slt { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b010, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Sltu { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b011, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Sgt { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b010, *rs2, *rs1, 0b0000000).to_le_bytes())
        }
        Instruction::Xori { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b100, *rs1, *imm)?.to_le_bytes()),
        Instruction::Seqz { rd, rs } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b011, *rs, 1)?.to_le_bytes()),
        Instruction::Snez { rd, rs } => out.extend_from_slice(&encode_r_type(0x33, *rd, 0b011, 0, *rs, 0b0000000).to_le_bytes()),
        Instruction::Neg { rd, rs } => out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, 0, *rs, 0b0100000).to_le_bytes()),
        Instruction::Ld { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x03, *rd, 0b011, *rs1, *imm)?.to_le_bytes()),
        Instruction::Lbu { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x03, *rd, 0b100, *rs1, *imm)?.to_le_bytes()),
        Instruction::Sb { rs2, rs1, imm } => out.extend_from_slice(&encode_s_type(0x23, 0b000, *rs1, *rs2, *imm)?.to_le_bytes()),
        Instruction::Sh { rs2, rs1, imm } => out.extend_from_slice(&encode_s_type(0x23, 0b001, *rs1, *rs2, *imm)?.to_le_bytes()),
        Instruction::Sw { rs2, rs1, imm } => out.extend_from_slice(&encode_s_type(0x23, 0b010, *rs1, *rs2, *imm)?.to_le_bytes()),
        Instruction::Sd { rs2, rs1, imm } => out.extend_from_slice(&encode_s_type(0x23, 0b011, *rs1, *rs2, *imm)?.to_le_bytes()),
        Instruction::Slli { rd, rs1, shamt } => {
            if !(0..=63).contains(shamt) {
                return Err(CompileError::new("slli shift amount must be in 0..=63", crate::error::Span::default()));
            }
            out.extend_from_slice(&encode_i_type(0x13, *rd, 0b001, *rs1, *shamt)?.to_le_bytes());
        }
        Instruction::Srai { rd, rs1, shamt } => {
            if !(0..=63).contains(shamt) {
                return Err(CompileError::new("srai shift amount must be in 0..=63", crate::error::Span::default()));
            }
            let imm = (0b0100000_i64 << 5) | *shamt;
            out.extend_from_slice(&encode_i_type(0x13, *rd, 0b101, *rs1, imm)?.to_le_bytes());
        }
        Instruction::Srli { rd, rs1, shamt } => {
            if !(0..=63).contains(shamt) {
                return Err(CompileError::new("srli shift amount must be in 0..=63", crate::error::Span::default()));
            }
            out.extend_from_slice(&encode_i_type(0x13, *rd, 0b101, *rs1, *shamt)?.to_le_bytes());
        }
        Instruction::Li { rd, imm } => encode_li_sequence(out, *rd, *imm)?,
        Instruction::La { rd, label } => encode_address_sequence(out, *rd, pc, parsed.symbol_address(label, layout)?)?,
        Instruction::Call { label } => {
            let target = parsed.symbol_address(label, layout)?;
            encode_call_sequence(out, pc, target)?;
        }
        Instruction::Jump { label } => {
            let target = parsed.symbol_address(label, layout)?;
            out.extend_from_slice(&encode_j_type(0x6f, 0, relative_offset(pc, target)?)?.to_le_bytes());
        }
        Instruction::Beq { .. }
        | Instruction::Bne { .. }
        | Instruction::Blt { .. }
        | Instruction::Bge { .. }
        | Instruction::Bltu { .. }
        | Instruction::Bgeu { .. }
        | Instruction::Beqz { .. }
        | Instruction::Bnez { .. } => {
            let (rs1, rs2, label, funct3) = conditional_branch_parts(inst).expect("conditional branch parts");
            let target = parsed.symbol_address(label, layout)?;
            if relaxed_branch {
                out.extend_from_slice(&encode_b_type(0x63, inverse_branch_funct3(funct3), rs1, rs2, 8)?.to_le_bytes());
                out.extend_from_slice(&encode_j_type(0x6f, 0, relative_offset(pc + 4, target)?)?.to_le_bytes());
            } else {
                out.extend_from_slice(&encode_b_type(0x63, funct3, rs1, rs2, relative_offset(pc, target)?)?.to_le_bytes());
            }
        }
        Instruction::Ret => out.extend_from_slice(&encode_i_type(0x67, 0, 0b000, 1, 0)?.to_le_bytes()),
        Instruction::Ecall => out.extend_from_slice(&encode_ecall().to_le_bytes()),
    }
    Ok(())
}

fn encode_li_sequence(out: &mut Vec<u8>, rd: u8, imm: i128) -> Result<()> {
    if let Some(signed) = li_signed_i64(imm) {
        if li_fits_lui_addi_rv64(signed) {
            let (hi, lo) = split_hi_lo(signed)?;
            out.extend_from_slice(&encode_u_type(0x37, rd, hi).to_le_bytes());
            out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, rd, lo)?.to_le_bytes());
            return Ok(());
        }
    }
    encode_large_li_sequence(out, rd, li_bits(imm)?)
}

fn encode_large_li_sequence(out: &mut Vec<u8>, rd: u8, bits: u64) -> Result<()> {
    let bytes = bits.to_be_bytes();
    out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, 0, i64::from(bytes[0]))?.to_le_bytes());
    for byte in bytes.iter().skip(1) {
        out.extend_from_slice(&encode_i_type(0x13, rd, 0b001, rd, 8)?.to_le_bytes());
        out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, rd, i64::from(*byte))?.to_le_bytes());
    }
    Ok(())
}

fn li_signed_i64(imm: i128) -> Option<i64> {
    i64::try_from(imm).ok()
}

fn li_bits(imm: i128) -> Result<u64> {
    if imm < i128::from(i64::MIN) || imm > i128::from(u64::MAX) {
        return Err(CompileError::new(format!("li immediate '{}' does not fit 64 bits", imm), crate::error::Span::default()));
    }
    if imm < 0 {
        Ok((imm as i64) as u64)
    } else {
        Ok(imm as u64)
    }
}

fn encode_address_sequence(out: &mut Vec<u8>, rd: u8, pc: u64, target: u64) -> Result<()> {
    let (hi, lo) = split_hi_lo(relative_offset(pc, target)?)?;
    out.extend_from_slice(&encode_u_type(0x17, rd, hi).to_le_bytes());
    out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, rd, lo)?.to_le_bytes());
    Ok(())
}

fn encode_call_sequence(out: &mut Vec<u8>, pc: u64, target: u64) -> Result<()> {
    let (hi, lo) = split_hi_lo(relative_offset(pc, target)?)?;
    out.extend_from_slice(&encode_u_type(0x17, 1, hi).to_le_bytes());
    out.extend_from_slice(&encode_i_type(0x67, 1, 0b000, 1, lo)?.to_le_bytes());
    Ok(())
}

fn op_size(op: &AsmOp, current_offset: usize, section: SectionKind, op_index: usize, branch_size_mode: BranchSizeMode<'_>) -> usize {
    match op {
        AsmOp::Label(_) => 0,
        AsmOp::Instruction(Instruction::Li { imm, .. }) => li_sequence_size(*imm),
        AsmOp::Instruction(Instruction::La { .. }) => 8,
        AsmOp::Instruction(Instruction::Call { .. }) => 8,
        AsmOp::Instruction(
            Instruction::Beq { .. }
            | Instruction::Bne { .. }
            | Instruction::Blt { .. }
            | Instruction::Bge { .. }
            | Instruction::Bltu { .. }
            | Instruction::Bgeu { .. }
            | Instruction::Beqz { .. }
            | Instruction::Bnez { .. },
        ) => match branch_size_mode {
            BranchSizeMode::Conservative => 8,
            BranchSizeMode::Exact(relaxed) if section == SectionKind::Text && relaxed.contains(&op_index) => 8,
            BranchSizeMode::Exact(_) => 4,
        },
        AsmOp::Instruction(_) => 4,
        AsmOp::Word(_) => 4,
        AsmOp::Byte(_) => 1,
        AsmOp::Ascii(bytes) => bytes.len(),
        AsmOp::Align(bytes) => padding_for(current_offset, *bytes),
    }
}

fn li_sequence_size(imm: i128) -> usize {
    if li_signed_i64(imm).is_some_and(li_fits_lui_addi_rv64) {
        8
    } else {
        60
    }
}

fn write_elf_header(out: &mut [u8], entry: u64, program_header_count: u16) -> Result<()> {
    if out.len() != ELF_HEADER_SIZE {
        return Err(CompileError::new("invalid ELF header buffer size", crate::error::Span::default()));
    }
    out.fill(0);
    out[0..4].copy_from_slice(b"\x7fELF");
    out[4] = 2;
    out[5] = 1;
    out[6] = 1;
    out[16..18].copy_from_slice(&2u16.to_le_bytes());
    out[18..20].copy_from_slice(&243u16.to_le_bytes());
    out[20..24].copy_from_slice(&1u32.to_le_bytes());
    out[24..32].copy_from_slice(&entry.to_le_bytes());
    out[32..40].copy_from_slice(&(ELF_HEADER_SIZE as u64).to_le_bytes());
    out[40..48].copy_from_slice(&0u64.to_le_bytes());
    out[48..52].copy_from_slice(&0u32.to_le_bytes());
    out[52..54].copy_from_slice(&(ELF_HEADER_SIZE as u16).to_le_bytes());
    out[54..56].copy_from_slice(&(ELF_PROGRAM_HEADER_SIZE as u16).to_le_bytes());
    out[56..58].copy_from_slice(&program_header_count.to_le_bytes());
    Ok(())
}

fn write_program_header(out: &mut [u8], flags: u32, offset: u64, vaddr: u64, file_size: u64, memory_size: u64) -> Result<()> {
    if out.len() != ELF_PROGRAM_HEADER_SIZE {
        return Err(CompileError::new("invalid ELF program header buffer size", crate::error::Span::default()));
    }
    out.fill(0);
    out[0..4].copy_from_slice(&1u32.to_le_bytes());
    out[4..8].copy_from_slice(&flags.to_le_bytes());
    out[8..16].copy_from_slice(&offset.to_le_bytes());
    out[16..24].copy_from_slice(&vaddr.to_le_bytes());
    out[24..32].copy_from_slice(&vaddr.to_le_bytes());
    out[32..40].copy_from_slice(&file_size.to_le_bytes());
    out[40..48].copy_from_slice(&memory_size.to_le_bytes());
    out[48..56].copy_from_slice(&(ELF_SEGMENT_ALIGN as u64).to_le_bytes());
    Ok(())
}

fn strip_comment(line: &str) -> Option<&str> {
    let mut in_string = false;
    let mut escape = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '"' if !escape => in_string = !in_string,
            '#' if !in_string => return Some(line[..idx].trim()),
            '\\' if in_string => {
                escape = !escape;
                continue;
            }
            _ => {}
        }
        escape = false;
    }
    let trimmed = line.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn parse_ascii_literal(value: &str) -> Result<Vec<u8>> {
    let Some(inner) = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return Err(CompileError::new(format!("invalid .ascii literal '{}'", value), crate::error::Span::default()));
    };

    let mut out = Vec::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.extend_from_slice(ch.to_string().as_bytes());
            continue;
        }

        let escaped = chars
            .next()
            .ok_or_else(|| CompileError::new("unterminated escape sequence in .ascii literal", crate::error::Span::default()))?;
        match escaped {
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            't' => out.push(b'\t'),
            '\\' => out.push(b'\\'),
            '"' => out.push(b'"'),
            'x' => {
                let hi = chars
                    .next()
                    .ok_or_else(|| CompileError::new("incomplete hex escape in .ascii literal", crate::error::Span::default()))?;
                let lo = chars
                    .next()
                    .ok_or_else(|| CompileError::new("incomplete hex escape in .ascii literal", crate::error::Span::default()))?;
                let hex = format!("{}{}", hi, lo);
                let byte = u8::from_str_radix(&hex, 16)
                    .map_err(|_| CompileError::new(format!("invalid hex escape '\\x{}'", hex), crate::error::Span::default()))?;
                out.push(byte);
            }
            other => {
                return Err(CompileError::new(
                    format!("unsupported escape sequence '\\{}' in .ascii literal", other),
                    crate::error::Span::default(),
                ));
            }
        }
    }

    Ok(out)
}

fn parse_memory_operand(value: &str) -> Result<(i64, u8)> {
    let open = value
        .find('(')
        .ok_or_else(|| CompileError::new(format!("invalid memory operand '{}'", value), crate::error::Span::default()))?;
    let close = value
        .rfind(')')
        .ok_or_else(|| CompileError::new(format!("invalid memory operand '{}'", value), crate::error::Span::default()))?;
    let imm = parse_immediate(value[..open].trim())?;
    let rs1 = parse_register(value[open + 1..close].trim())?;
    Ok((imm, rs1))
}

fn memory_operand_offset_and_base(value: &str) -> Option<(i64, &str)> {
    let open = value.find('(')?;
    let close = value.rfind(')')?;
    let offset = parse_immediate(value[..open].trim()).ok()?;
    let base = value[open + 1..close].trim();
    (!base.is_empty()).then_some((offset, base))
}

fn small_signed_immediate(value: i64) -> bool {
    (-2048..=2047).contains(&value)
}

fn scratch_register_avoiding(registers: &[&str]) -> &'static str {
    for candidate in ["t6", "t5", "t3", "t2", "t1", "t0"] {
        let candidate_id = parse_register(candidate).expect("scratch register name should be valid");
        if registers.iter().all(|register| parse_register(register).ok() != Some(candidate_id)) {
            return candidate;
        }
    }
    "t6"
}

fn parse_register(name: &str) -> Result<u8> {
    let reg = match name {
        "zero" | "x0" => 0,
        "ra" | "x1" => 1,
        "sp" | "x2" => 2,
        "gp" | "x3" => 3,
        "tp" | "x4" => 4,
        "t0" | "x5" => 5,
        "t1" | "x6" => 6,
        "t2" | "x7" => 7,
        "s0" | "fp" | "x8" => 8,
        "s1" | "x9" => 9,
        "a0" | "x10" => 10,
        "a1" | "x11" => 11,
        "a2" | "x12" => 12,
        "a3" | "x13" => 13,
        "a4" | "x14" => 14,
        "a5" | "x15" => 15,
        "a6" | "x16" => 16,
        "a7" | "x17" => 17,
        "s2" | "x18" => 18,
        "s3" | "x19" => 19,
        "s4" | "x20" => 20,
        "s5" | "x21" => 21,
        "s6" | "x22" => 22,
        "s7" | "x23" => 23,
        "s8" | "x24" => 24,
        "s9" | "x25" => 25,
        "s10" | "x26" => 26,
        "s11" | "x27" => 27,
        "t3" | "x28" => 28,
        "t4" | "x29" => 29,
        "t5" | "x30" => 30,
        "t6" | "x31" => 31,
        other => return Err(CompileError::new(format!("unknown register '{}'", other), crate::error::Span::default())),
    };
    Ok(reg)
}

fn parse_immediate(value: &str) -> Result<i64> {
    if let Some(hex) = value.strip_prefix("-0x") {
        return i64::from_str_radix(hex, 16)
            .map(|value| -value)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()));
    }
    if let Some(hex) = value.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()));
    }
    value.parse::<i64>().map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()))
}

fn parse_li_immediate(value: &str) -> Result<i128> {
    if let Some(hex) = value.strip_prefix("-0x") {
        return i128::from_str_radix(hex, 16)
            .map(|value| -value)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()));
    }
    if let Some(hex) = value.strip_prefix("0x") {
        let parsed = u128::from_str_radix(hex, 16)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()))?;
        if parsed <= u128::from(u64::MAX) {
            return Ok(parsed as i128);
        }
        return Err(CompileError::new(format!("li immediate '{}' does not fit 64 bits", value), crate::error::Span::default()));
    }
    if value.starts_with('-') {
        value.parse::<i128>().map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()))
    } else {
        value
            .parse::<u128>()
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()))
            .and_then(|parsed| {
                if parsed <= u128::from(u64::MAX) {
                    Ok(parsed as i128)
                } else {
                    Err(CompileError::new(format!("li immediate '{}' does not fit 64 bits", value), crate::error::Span::default()))
                }
            })
    }
}

fn arg(args: &[String], index: usize) -> Result<&str> {
    args.get(index)
        .map(|value| value.as_str())
        .ok_or_else(|| CompileError::new("malformed assembly instruction", crate::error::Span::default()))
}

fn encode_r_type(opcode: u32, rd: u8, funct3: u32, rs1: u8, rs2: u8, funct7: u32) -> u32 {
    (funct7 << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) | (funct3 << 12) | ((rd as u32) << 7) | opcode
}

fn encode_i_type(opcode: u32, rd: u8, funct3: u32, rs1: u8, imm: i64) -> Result<u32> {
    let imm = encode_signed_bits(imm, 12)?;
    Ok((imm << 20) | ((rs1 as u32) << 15) | (funct3 << 12) | ((rd as u32) << 7) | opcode)
}

fn encode_s_type(opcode: u32, funct3: u32, rs1: u8, rs2: u8, imm: i64) -> Result<u32> {
    let imm = encode_signed_bits(imm, 12)?;
    let imm_lo = imm & 0x1f;
    let imm_hi = (imm >> 5) & 0x7f;
    Ok((imm_hi << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) | (funct3 << 12) | (imm_lo << 7) | opcode)
}

fn encode_b_type(opcode: u32, funct3: u32, rs1: u8, rs2: u8, imm: i64) -> Result<u32> {
    if imm % 2 != 0 {
        return Err(CompileError::new("branch target is not 2-byte aligned", crate::error::Span::default()));
    }
    let imm = encode_signed_bits(imm, 13)?;
    let bit12 = (imm >> 12) & 0x1;
    let bits10_5 = (imm >> 5) & 0x3f;
    let bits4_1 = (imm >> 1) & 0xf;
    let bit11 = (imm >> 11) & 0x1;
    Ok((bit12 << 31)
        | (bits10_5 << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | (bits4_1 << 8)
        | (bit11 << 7)
        | opcode)
}

fn encode_u_type(opcode: u32, rd: u8, imm: i64) -> u32 {
    (((imm as i32 as u32) & 0x000f_ffff) << 12) | ((rd as u32) << 7) | opcode
}

fn encode_j_type(opcode: u32, rd: u8, imm: i64) -> Result<u32> {
    if imm % 2 != 0 {
        return Err(CompileError::new("jump target is not 2-byte aligned", crate::error::Span::default()));
    }
    let imm = encode_signed_bits(imm, 21)?;
    let bit20 = (imm >> 20) & 0x1;
    let bits10_1 = (imm >> 1) & 0x3ff;
    let bit11 = (imm >> 11) & 0x1;
    let bits19_12 = (imm >> 12) & 0xff;
    Ok((bit20 << 31) | (bits10_1 << 21) | (bit11 << 20) | (bits19_12 << 12) | ((rd as u32) << 7) | opcode)
}

fn encode_ecall() -> u32 {
    0x0000_0073
}

fn encode_signed_bits(value: i64, bits: u32) -> Result<u32> {
    if !signed_bits_fit(value, bits) {
        return Err(CompileError::new(
            format!("immediate '{}' does not fit {}-bit signed field", value, bits),
            crate::error::Span::default(),
        ));
    }
    Ok((value as i32 as u32) & ((1u32 << bits) - 1))
}

fn signed_bits_fit(value: i64, bits: u32) -> bool {
    let min = -(1i64 << (bits - 1));
    let max = (1i64 << (bits - 1)) - 1;
    value >= min && value <= max
}

fn split_hi_lo(value: i64) -> Result<(i64, i64)> {
    if !li_fits_lui_addi_rv64(value) {
        return Err(CompileError::new(
            format!("value '{}' is outside the supported RV64 LUI/ADDI immediate range", value),
            crate::error::Span::default(),
        ));
    }
    let hi = (value + 0x800) >> 12;
    let lo = value - (hi << 12);
    if !(-2048..=2047).contains(&lo) {
        return Err(CompileError::new(format!("low immediate '{}' is out of range after split", lo), crate::error::Span::default()));
    }
    Ok((hi, lo))
}

fn li_fits_lui_addi_rv64(value: i64) -> bool {
    if !(i32::MIN as i64..=i32::MAX as i64).contains(&value) {
        return false;
    }
    let hi = (value + 0x800) >> 12;
    (-0x80000..=0x7ffff).contains(&hi)
}

fn relative_offset(pc: u64, target: u64) -> Result<i64> {
    i64::try_from(target as i128 - pc as i128)
        .map_err(|_| CompileError::new("relative offset overflowed i64", crate::error::Span::default()))
}

fn align_up(value: usize, align: usize) -> usize {
    if align <= 1 {
        return value;
    }
    (value + align - 1) & !(align - 1)
}

fn align_frame(value: usize) -> usize {
    align_up(value.max(16), 16)
}

fn is_min_call(func: &str) -> bool {
    matches!(func, "min" | "math_min" | "__math_min")
}

fn is_void_runtime_requirement_call(func: &str) -> bool {
    matches!(
        func,
        "__ckb_require_maturity"
            | "__ckb_require_time"
            | "__ckb_require_epoch_after"
            | "__ckb_require_epoch_relative"
            | "__ckb_require_cell_lock_hash"
            | "__ckb_require_cell_type_hash"
            | "__ckb_require_current_script_args_empty"
            | "__ckb_require_cell_lock_args_empty"
            | "__ckb_require_cell_type_args_empty"
            | "__ckb_require_cell_lock_args_hash"
            | "__ckb_require_cell_type_args_hash"
            | "__ckb_require_cell_lock_args_prefix_hash"
            | "__ckb_require_cell_type_args_prefix_hash"
            | "__ckb_require_cell_lock_args_suffix_hash"
            | "__ckb_require_cell_type_args_suffix_hash"
            | "__ckb_require_cell_lock_script_hash_type"
            | "__ckb_require_cell_type_script_hash_type"
            | "__ckb_require_input_out_point_tx_hash"
            | "__ckb_require_input_out_point"
            | "__ckb_require_metapoint_relative"
            | "__ckb_require_lock_type_metapoint_pairs"
            | "__ckb_require_type_lock_metapoint_pairs"
            | "__ckb_require_lock_type_metapoint_pairs_from_i32_data"
            | "__ckb_require_type_lock_metapoint_pairs_from_i32_data"
            | "__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered"
            | "__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered"
            | "__ckb_require_lock_match_master_out_point_pairs_from_data"
            | "__dao_require_header_dep_for_input"
            | "__dao_require_input_since_at_least"
            | "__dao_require_input_relative_epoch_since_at_least"
            | "__xudt_require_owner_mode_input_type"
            | "__xudt_require_owner_mode_type_args"
            | "__xudt_require_owner_mode_type_args_current_script"
            | "__xudt_require_group_amount_conserved"
            | "__xudt_require_group_amount_minted"
            | "__xudt_require_group_amount_burned"
            | "__c256_require_u128_product_lte"
            | "__c256_require_u128_product_eq"
            | "__c256_require_u128_sum2_products_lte"
            | "__c256_require_u128_sum2_products_eq"
            | "__ckb_require_witness_size_at_least"
    )
}

fn is_runtime_scalar_failclosed_call(func: &str) -> bool {
    matches!(
        func,
        "__ckb_source_input"
            | "__ckb_source_output"
            | "__ckb_source_cell_dep"
            | "__ckb_source_header_dep"
            | "__ckb_source_group_input"
            | "__ckb_source_group_output"
            | "__ckb_since_epoch_absolute"
            | "__ckb_since_epoch_relative"
            | "__ckb_current_role"
            | "__ckb_cell_capacity"
            | "__ckb_cell_occupied_capacity"
            | "__ckb_cell_unoccupied_capacity"
            | "__ckb_cell_output_index"
            | "__ckb_cell_data_size"
            | "__ckb_cell_data_u32_le"
            | "__ckb_cell_data_u64_le"
            | "__ckb_cell_lock_hash_type"
            | "__ckb_cell_type_hash_type"
            | "__ckb_cell_lock_args_empty"
            | "__ckb_cell_type_args_empty"
            | "__dao_accumulated_rate"
            | "__dao_input_accumulated_rate"
            | "__dao_has_dao_type"
            | "__dao_is_deposit_data"
            | "__dao_is_withdrawal_request_data"
            | "__xudt_amount_low"
            | "__xudt_amount_high"
            | "__xudt_owner_mode_input_type_hash"
            | "__ckb_witness_size"
    )
}

fn is_runtime_header_u64_call(func: &str) -> bool {
    matches!(
        func,
        "__env_current_timepoint"
            | "__ckb_header_epoch_number"
            | "__ckb_header_epoch_start_block_number"
            | "__ckb_header_epoch_length"
            | "__ckb_input_since"
    )
}

fn ckb_source_name(source: u64) -> &'static str {
    match source {
        CKB_SOURCE_INPUT => "Input",
        CKB_SOURCE_OUTPUT => "Output",
        CKB_SOURCE_CELL_DEP => "CellDep",
        CKB_SOURCE_HEADER_DEP => "HeaderDep",
        CKB_SOURCE_GROUP_INPUT => "GroupInput",
        CKB_SOURCE_GROUP_OUTPUT => "GroupOutput",
        source if source == (CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT) => "GroupInput",
        source if source == (CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT) => "GroupOutput",
        source if source == (CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_CELL_DEP) => "GroupCellDep",
        source if source == (CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_HEADER_DEP) => "GroupHeaderDep",
        _ => "Unknown",
    }
}

fn padding_for(offset: usize, align: usize) -> usize {
    align_up(offset, align) - offset
}

fn pad_to_alignment(out: &mut Vec<u8>, align: usize) {
    let pad = padding_for(out.len(), align);
    out.resize(out.len() + pad, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS: &[(&str, &str)] = &[
        ("add", "add t0, a0, a1"),
        ("addi", "addi t0, t0, -1"),
        ("and", "and t2, a0, a1"),
        ("beq", "beq a0, a1, branch_target"),
        ("bge", "bge a0, a1, branch_target"),
        ("bgeu", "bgeu a0, a1, branch_target"),
        ("bgez", "bgez a0, branch_target"),
        ("bgt", "bgt a0, a1, branch_target"),
        ("blt", "blt a1, a0, branch_target"),
        ("bltu", "bltu a1, a0, branch_target"),
        ("bne", "bne a0, a1, branch_target"),
        ("bnez", "bnez a0, branch_target"),
        ("beqz", "beqz a0, branch_target"),
        ("call", "call helper"),
        ("div", "div t5, a0, a1"),
        ("divu", "divu t5, a0, a1"),
        ("ecall", "ecall"),
        ("j", "j done"),
        ("la", "la t3, data_label"),
        ("lbu", "lbu t2, 8(sp)"),
        ("ld", "ld t1, 0(sp)"),
        ("li", "li a0, 8"),
        ("mul", "mul t4, a0, a1"),
        ("mv", "mv s9, a0"),
        ("neg", "neg s6, a0"),
        ("or", "or t3, a0, a1"),
        ("rem", "rem t6, a0, a1"),
        ("remu", "remu t6, a0, a1"),
        ("ret", "ret"),
        ("sb", "sb t1, 8(sp)"),
        ("sd", "sd t0, 0(sp)"),
        ("seqz", "seqz s4, a0"),
        ("sgt", "sgt s2, a0, a1"),
        ("sh", "sh t1, 10(sp)"),
        ("slli", "slli s7, a0, 3"),
        ("slt", "slt s0, a1, a0"),
        ("sltu", "sltu s1, a1, a0"),
        ("snez", "snez s5, a0"),
        ("srai", "srai a0, a0, 1"),
        ("srli", "srli s8, a0, 1"),
        ("sub", "sub t1, a0, a1"),
        ("sw", "sw t1, 12(sp)"),
        ("xor", "xor a0, a0, a1"),
        ("xori", "xori s3, a0, 1"),
    ];

    const INTENTIONALLY_UNSUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS: &[(&str, &str)] = &[
        ("addiw", "addiw a0, a0, 1"),
        ("addw", "addw a0, a0, a1"),
        ("andi", "andi a0, a0, 1"),
        ("amoadd.w", "amoadd.w a0, a1, (a2)"),
        ("auipc", "auipc a0, 0"),
        ("ble", "ble a0, a1, target"),
        ("bleu", "bleu a0, a1, target"),
        ("blez", "blez a0, target"),
        ("bgtu", "bgtu a0, a1, target"),
        ("bgtz", "bgtz a0, target"),
        ("bltz", "bltz a0, target"),
        ("c.nop", "c.nop"),
        ("csrr", "csrr a0, cycle"),
        ("fence", "fence"),
        ("flw", "flw fa0, 0(sp)"),
        ("jal", "jal ra, target"),
        ("jalr", "jalr zero, 0(ra)"),
        ("jr", "jr ra"),
        ("lb", "lb a0, 0(sp)"),
        ("lh", "lh a0, 0(sp)"),
        ("lhu", "lhu a0, 0(sp)"),
        ("lui", "lui a0, 1"),
        ("lw", "lw a0, 0(sp)"),
        ("lwu", "lwu a0, 0(sp)"),
        ("nop", "nop"),
        ("not", "not a0, a1"),
        ("ori", "ori a0, a0, 1"),
        ("sll", "sll a0, a0, a1"),
        ("slti", "slti a0, a0, 1"),
        ("sltiu", "sltiu a0, a0, 1"),
        ("sra", "sra a0, a0, a1"),
        ("srl", "srl a0, a0, a1"),
        ("subw", "subw a0, a0, a1"),
        ("tail", "tail target"),
    ];

    #[derive(Debug)]
    struct TestProgramHeader {
        p_type: u32,
        flags: u32,
        offset: u64,
        vaddr: u64,
        file_size: u64,
        memory_size: u64,
    }

    fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
        let mut raw = [0u8; 2];
        raw.copy_from_slice(&bytes[offset..offset + 2]);
        u16::from_le_bytes(raw)
    }

    fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
        let mut raw = [0u8; 4];
        raw.copy_from_slice(&bytes[offset..offset + 4]);
        u32::from_le_bytes(raw)
    }

    fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(&bytes[offset..offset + 8]);
        u64::from_le_bytes(raw)
    }

    fn elf_program_headers(elf: &[u8]) -> Vec<TestProgramHeader> {
        assert!(elf.starts_with(b"\x7fELF"), "expected ELF magic");
        let phoff = usize::try_from(read_u64_le(elf, 32)).expect("program header offset should fit usize");
        let phentsize = usize::from(read_u16_le(elf, 54));
        let phnum = usize::from(read_u16_le(elf, 56));
        assert_eq!(phentsize, ELF_PROGRAM_HEADER_SIZE);

        (0..phnum)
            .map(|index| {
                let offset = phoff + index * phentsize;
                TestProgramHeader {
                    p_type: read_u32_le(elf, offset),
                    flags: read_u32_le(elf, offset + 4),
                    offset: read_u64_le(elf, offset + 8),
                    vaddr: read_u64_le(elf, offset + 16),
                    file_size: read_u64_le(elf, offset + 32),
                    memory_size: read_u64_le(elf, offset + 40),
                }
            })
            .collect()
    }

    fn elf_text_file_offset(elf: &[u8]) -> usize {
        let header = elf_program_headers(elf)
            .into_iter()
            .find(|header| header.p_type == 1 && header.flags & ELF_PF_X != 0)
            .expect("ELF should contain an executable load segment");
        let offset_into_segment = ELF_BASE_ADDR.checked_sub(header.vaddr).expect("text base should be inside load segment");
        usize::try_from(header.offset + offset_into_segment).expect("text file offset should fit usize")
    }

    #[test]
    fn strict_audit_internal_elf_entry_preserves_ckb_stack_pointer() {
        let lines = vec![".section .text".to_string(), ".global entry".to_string(), "entry:".to_string(), "ret".to_string()];

        let elf = assemble_elf_internal(&lines).expect("internal assembler should emit a CKB-loadable ELF");
        let headers = elf_program_headers(&elf);
        assert_eq!(headers.len(), 1, "internal CKB ELF should expose one load segment");
        assert_eq!(headers[0].flags, ELF_PF_R | ELF_PF_X, "code segment should be readable and executable only");
        assert_eq!(headers[0].flags & ELF_PF_W, 0, "code segment must not be writable");
        assert_eq!(headers[0].file_size, headers[0].memory_size, "code segment should not fake stack memory in PT_LOAD");

        let text_offset = elf_text_file_offset(&elf);
        let first_instruction = read_u32_le(&elf, text_offset);
        assert_eq!(first_instruction & 0x7f, 0x17, "trampoline should call the entrypoint, not load sp");
        assert_eq!((first_instruction >> 7) & 0x1f, 1, "trampoline call should target ra");

        let entry_instruction = read_u32_le(&elf, text_offset + START_TRAMPOLINE_SIZE);
        assert_eq!(entry_instruction, 0x0000_8067, "entry body should start after the 20-byte trampoline");
    }

    #[test]
    fn strict_audit_external_assembly_entry_preserves_ckb_stack_pointer() {
        let lines = vec![".section .text".to_string(), ".global entry".to_string(), "entry:".to_string(), "ret".to_string()];

        let rendered = render_external_assembly(&lines, "entry");
        assert!(
            !rendered.lines().any(|line| line.trim_start().starts_with("li sp,")),
            "external assembly trampoline must not overwrite the CKB VM stack pointer:\n{rendered}"
        );
        assert!(rendered.contains("\n    call entry\n"), "external assembly should call the entrypoint:\n{rendered}");
    }

    #[test]
    fn internal_assembler_relaxes_out_of_range_conditional_branch() {
        let mut lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, far_target".to_string(),
        ];
        for _ in 0..1500 {
            lines.push("addi t0, t0, 0".to_string());
        }
        lines.push("far_target:".to_string());
        lines.push("ret".to_string());

        let elf = assemble_elf_internal(&lines).expect("internal assembler should relax long conditional branches");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn internal_assembler_encodes_register_conditional_branches() {
        for mnemonic in ["beq", "bne", "blt", "bge", "bltu", "bgeu"] {
            let lines = vec![
                ".section .text".to_string(),
                ".global entry".to_string(),
                "entry:".to_string(),
                "li a0, 1".to_string(),
                "li a1, 1".to_string(),
                format!("{} a0, a1, target", mnemonic),
                "li a0, 2".to_string(),
                "target:".to_string(),
                "ret".to_string(),
            ];

            let elf = assemble_elf_internal(&lines).unwrap_or_else(|err| panic!("internal assembler should encode {mnemonic}: {err}"));
            assert!(elf.starts_with(b"\x7fELF"), "expected ELF output for {mnemonic}");
        }
    }

    #[test]
    fn internal_assembler_encodes_emitted_instruction_surface() {
        let lines = supported_instruction_surface_lines();

        let elf = assemble_elf_internal(&lines).expect("internal assembler should encode the emitted instruction surface");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn internal_assembler_rejects_intentionally_unsupported_mnemonics() {
        for (mnemonic, instruction) in INTENTIONALLY_UNSUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS {
            let lines = vec![
                ".section .text".to_string(),
                ".global entry".to_string(),
                "entry:".to_string(),
                (*instruction).to_string(),
                "target:".to_string(),
                "ret".to_string(),
            ];
            let err = match assemble_elf_internal(&lines) {
                Ok(_) => panic!("internal assembler unexpectedly accepted unsupported mnemonic {mnemonic}"),
                Err(err) => err,
            };
            assert!(
                err.message.contains("unsupported assembly instruction"),
                "unexpected error for unsupported mnemonic {mnemonic}: {err}"
            );
        }
    }

    #[test]
    fn generated_public_assembly_mnemonics_are_declared() {
        let surfaces = [
            ("stdlib", crate::stdlib::StdLib::generate_assembly()),
            ("collections", crate::stdlib::collections::Collections::generate_assembly()),
        ];
        let supported = SUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS.iter().map(|(mnemonic, _)| *mnemonic).collect::<BTreeSet<_>>();
        let mut undeclared = Vec::new();

        for (surface, assembly) in surfaces {
            for (line_number, mnemonic) in emitted_mnemonics(&assembly).into_iter() {
                if !supported.contains(mnemonic.as_str()) {
                    undeclared.push(format!("{surface}:{line_number}: {mnemonic}"));
                }
            }
        }

        assert!(
            undeclared.is_empty(),
            "generated public assembly used mnemonics outside the declared internal assembler surface:\n{}",
            undeclared.join("\n")
        );
    }

    #[test]
    fn bundled_example_codegen_mnemonics_are_declared() {
        let examples = ["amm_pool.cell", "launch.cell", "multisig.cell", "nft.cell", "timelock.cell", "token.cell", "vesting.cell"];
        let supported = SUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS.iter().map(|(mnemonic, _)| *mnemonic).collect::<BTreeSet<_>>();
        let mut undeclared = Vec::new();

        for example in examples {
            let path = camino::Utf8PathBuf::from(format!("{}/examples/{}", env!("CARGO_MANIFEST_DIR"), example));
            let result = crate::compile_file(
                path,
                crate::CompileOptions { target: Some("riscv64-asm".to_string()), ..crate::CompileOptions::default() },
            )
            .unwrap_or_else(|err| panic!("{example} should compile to assembly: {}", err.message));
            let assembly = std::str::from_utf8(&result.artifact_bytes)
                .unwrap_or_else(|err| panic!("{example} emitted invalid utf-8 assembly: {err}"));

            for (line_number, mnemonic) in emitted_mnemonics(assembly).into_iter() {
                if !supported.contains(mnemonic.as_str()) {
                    undeclared.push(format!("{example}:{line_number}: {mnemonic}"));
                }
            }
        }

        assert!(
            undeclared.is_empty(),
            "bundled examples used mnemonics outside the declared internal assembler surface:\n{}",
            undeclared.join("\n")
        );
    }

    fn supported_instruction_surface_lines() -> Vec<String> {
        let mut lines = vec![".section .text".to_string(), ".global entry".to_string(), "entry:".to_string(), "li a1, 4".to_string()];
        for (mnemonic, instruction) in SUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS {
            if !matches!(*mnemonic, "ecall" | "ret") {
                lines.push((*instruction).to_string());
            }
        }
        lines.extend([
            "branch_target:".to_string(),
            "ecall".to_string(),
            "helper:".to_string(),
            "ret".to_string(),
            "done:".to_string(),
            "ret".to_string(),
            ".section .rodata".to_string(),
            "data_label:".to_string(),
            ".word 7".to_string(),
            ".byte 1".to_string(),
            ".ascii \"x\"".to_string(),
            ".align 3".to_string(),
        ]);
        lines
    }

    fn emitted_mnemonics(assembly: &str) -> Vec<(usize, String)> {
        assembly
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let clean = strip_comment(line)?;
                if clean.is_empty() || clean.starts_with('.') || clean.ends_with(':') {
                    return None;
                }
                let mnemonic = clean.split_whitespace().next()?.trim_end_matches(',');
                Some((index + 1, mnemonic.to_string()))
            })
            .collect()
    }

    #[test]
    fn internal_assembler_encodes_full_width_li_literals() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 9223372036854775808".to_string(),
            "li a1, 18446744073709551615".to_string(),
            "ret".to_string(),
        ];

        let elf = assemble_elf_internal(&lines).expect("internal assembler should encode u64-width li literals");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn rv64_li_boundary_values_materialize_correct_bits() {
        let cases = [(0x7fff_f7ffi128, 8usize), (0x7fff_f800i128, 60usize), (0x7fff_ffffi128, 60usize), (0x8000_0000i128, 60usize)];

        for (value, expected_size) in cases {
            let mut bytes = Vec::new();
            encode_li_sequence(&mut bytes, 10, value).expect("li should encode");
            assert_eq!(bytes.len(), expected_size, "unexpected li size for {value:#x}");
            assert_eq!(simulate_li_sequence(&bytes, 10), value as u64, "li materialized wrong bits for {value:#x}");
        }
    }

    fn simulate_li_sequence(bytes: &[u8], register: usize) -> u64 {
        let mut regs = [0u64; 32];
        for chunk in bytes.chunks_exact(4) {
            let inst = u32::from_le_bytes(chunk.try_into().expect("instruction chunk should be four bytes"));
            let opcode = inst & 0x7f;
            let rd = ((inst >> 7) & 0x1f) as usize;
            let funct3 = (inst >> 12) & 0x7;
            let rs1 = ((inst >> 15) & 0x1f) as usize;
            match (opcode, funct3) {
                (0x37, _) => {
                    regs[rd] = ((inst & 0xffff_f000) as i32 as i64) as u64;
                }
                (0x13, 0b000) => {
                    let imm = sign_extend(inst >> 20, 12);
                    regs[rd] = regs[rs1].wrapping_add(imm as u64);
                }
                (0x13, 0b001) => {
                    let shamt = (inst >> 20) & 0x3f;
                    regs[rd] = regs[rs1] << shamt;
                }
                _ => panic!("unexpected instruction in li sequence: 0x{inst:08x}"),
            }
            regs[0] = 0;
        }
        regs[register]
    }

    fn sign_extend(value: u32, bits: u32) -> i64 {
        let shift = 64 - bits;
        ((u64::from(value) << shift) as i64) >> shift
    }

    #[test]
    fn stack_pointer_offsets_are_emitted_through_helpers() {
        let implementation = include_str!("mod.rs")
            .split("\n    fn emit_runtime_ckb_v014_surface_helpers")
            .next()
            .expect("source should contain runtime helper boundary");
        let offenders = implementation
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let emits_stack_memory =
                    (line.contains("self.emit(format!(") || line.contains("self.emit(\"")) && line.contains("(sp)");
                let emits_stack_addi =
                    (line.contains("self.emit(\"addi ") || line.contains("self.emit(format!(\"addi ")) && line.contains(", sp,");
                let allowed_stack_memory = line.contains("self.emit(format!(\"{} {}, {}(sp)\", opcode, register, offset))");
                let allowed_outgoing_stack_memory = line.contains("self.emit(format!(\"sd {}, {}(sp)\", register, offset))");
                let allowed_stack_addi = line.contains("self.emit(format!(\"addi {}, sp, {}\", rd, offset))");
                ((emits_stack_memory && !allowed_stack_memory && !allowed_outgoing_stack_memory)
                    || (emits_stack_addi && !allowed_stack_addi))
                    .then(|| format!("{}: {}", index + 1, line.trim()))
            })
            .collect::<Vec<_>>();

        assert!(offenders.is_empty(), "stack pointer accesses must go through stack helpers:\n{}", offenders.join("\n"));
    }

    #[test]
    fn large_addi_avoids_clobbering_source_register() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.emit_large_addi("t0", "t6", 2048);
        generator.emit_large_addi("t6", "t6", 4096);

        assert_eq!(generator.assembly, vec!["    li t5, 2048", "    add t0, t6, t5", "    li t5, 4096", "    add t6, t6, t5",]);
    }

    #[test]
    fn sp_addi_large_offsets_clobber_only_destination_register() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.emit_sp_addi("t4", 4096);
        generator.emit_sp_addi("t6", 8192);

        assert_eq!(generator.assembly, vec!["    li t4, 4096", "    add t4, sp, t4", "    li t6, 8192", "    add t6, sp, t6",]);
    }

    #[test]
    fn state_transition_edges_use_explicit_consumed_binding() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.consume_order = vec![1, 2];
        generator.consume_type_names.insert(1, "Offer".to_string());
        generator.consume_type_names.insert(2, "Offer".to_string());
        generator.consume_binding_ids.insert("left".to_string(), 1);
        generator.consume_binding_ids.insert("right".to_string(), 2);

        let state_edge = IrStateTransitionEdge {
            input_binding: Some("right".to_string()),
            output_binding: None,
            type_name: "Offer".to_string(),
            field_name: "state".to_string(),
            from: "Live".to_string(),
            to: "Filled".to_string(),
            from_index: 1,
            to_index: 2,
        };

        assert_eq!(generator.consumed_var_for_state_transition("Offer", &[state_edge]), Some(2));
    }

    #[test]
    fn consumed_schema_params_use_loaded_cell_size_for_field_checks() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        let binding = IrVar { id: 0, name: "auth".to_string(), ty: IrType::Named("MintAuthority".to_string()) };
        let params = vec![IrParam {
            name: "auth".to_string(),
            ty: binding.ty.clone(),
            is_mut: false,
            is_ref: false,
            is_read_ref: false,
            source: ParamSource::Default,
            binding: binding.clone(),
        }];
        let body = IrBody {
            consume_set: vec![CellPattern {
                operation: "input".to_string(),
                type_hash: None,
                binding: "auth".to_string(),
                fields: Vec::new(),
            }],
            read_refs: Vec::new(),
            create_set: Vec::new(),
            mutate_set: Vec::new(),
            write_intents: Vec::new(),
            blocks: Vec::new(),
        };

        generator.prepare_function_layout(&body, &params);

        let loaded_size_offset =
            generator.cell_buffer_size_offsets.get(&binding.id).copied().expect("consumed input should have size slot");
        assert_eq!(generator.schema_pointer_size_offsets.get(&binding.id), Some(&loaded_size_offset));
    }

    #[test]
    fn unaligned_scalar_load_large_offsets_preserve_live_accumulator() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.emit_unaligned_scalar_load("t4", "t6", "t2", 2048, 2);

        assert_eq!(
            generator.assembly,
            vec![
                "    li t6, 0",
                "    li t5, 2048",
                "    add t5, t4, t5",
                "    lbu t2, 0(t5)",
                "    or t6, t6, t2",
                "    li t5, 2049",
                "    add t5, t4, t5",
                "    lbu t2, 0(t5)",
                "    slli t2, t2, 8",
                "    or t6, t6, t2",
            ]
        );
    }

    #[test]
    fn generated_large_offsets_are_normalized_before_assembly() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.emit("sd t0, 2048(sp)");
        generator.emit("ld t6, 2056(sp)");
        generator.emit("lbu t2, 2048(t4)");
        generator.emit("addi t0, t4, 2048");
        generator.emit("sb t0, 4096(t6)");

        assert_eq!(
            generator.assembly,
            vec![
                "    li t6, 2048",
                "    add t6, sp, t6",
                "    sd t0, 0(t6)",
                "    li t5, 2056",
                "    add t5, sp, t5",
                "    ld t6, 0(t5)",
                "    li t6, 2048",
                "    add t6, t4, t6",
                "    lbu t2, 0(t6)",
                "    li t6, 2048",
                "    add t0, t4, t6",
                "    li t5, 4096",
                "    add t5, t6, t5",
                "    sb t0, 0(t5)",
            ]
        );
    }

    #[test]
    fn read_ref_runtime_fallback_records_cell_buffer_state() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.frame_size = align_frame(RUNTIME_EXPR_TEMP_SIZE + RUNTIME_SCRATCH_SIZE + 16);
        let dest = IrVar { id: 42, name: "cfg".to_string(), ty: IrType::Named("Config".to_string()) };
        generator.read_ref_indices.insert(dest.id, 0);

        generator.emit_read_ref(&dest, "Config").expect("read_ref fallback should lower");

        let size_offset = generator.runtime_scratch_size_offset();
        let buffer_offset = generator.runtime_scratch_buffer_offset();
        assert_eq!(generator.schema_pointer_size_offsets.get(&dest.id), Some(&size_offset));
        assert_eq!(generator.cell_buffer_size_offsets.get(&dest.id), Some(&size_offset));
        assert_eq!(generator.cell_buffer_offsets.get(&dest.id), Some(&buffer_offset));
    }

    #[test]
    fn explicit_external_toolchain_paths_are_strict() {
        let err = validate_explicit_toolchain_path("CELLSCRIPT_RISCV_CC", PathBuf::from("riscv64-unknown-elf-gcc")).unwrap_err();
        assert!(err.message.contains("must be an absolute path"), "unexpected error: {}", err.message);

        let err = validate_explicit_toolchain_path("CELLSCRIPT_RISCV_CC", std::env::temp_dir()).unwrap_err();
        assert!(err.message.contains("must point to an executable file"), "unexpected error: {}", err.message);

        let current_exe = std::env::current_exe().expect("test executable path should be available");
        let validated =
            validate_explicit_toolchain_path("CELLSCRIPT_RISCV_CC", current_exe.clone()).expect("current test binary is executable");
        assert_eq!(validated, current_exe);
    }

    #[test]
    fn generated_stdlib_assembly_is_internal_assembler_clean() {
        let lines = crate::stdlib::StdLib::generate_assembly().lines().map(|line| line.to_string()).collect::<Vec<_>>();

        let elf = assemble_elf_internal(&lines).expect("generated stdlib assembly should assemble internally");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn generated_collection_assembly_is_internal_assembler_clean() {
        let lines =
            crate::stdlib::collections::Collections::generate_assembly().lines().map(|line| line.to_string()).collect::<Vec<_>>();

        let elf = assemble_elf_internal(&lines).expect("generated collection assembly should assemble internally");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn internal_assembler_rejects_unresolved_call_targets() {
        let lines = vec![".section .text".to_string(), ".global main".to_string(), "main:".to_string(), "call missing".to_string()];
        let err = assemble_elf_internal(&lines).unwrap_err();

        assert!(err.message.contains("unknown assembly label 'missing'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn internal_assembler_relaxes_out_of_range_register_conditional_branch() {
        for mnemonic in ["beq", "bne", "blt", "bge", "bltu", "bgeu"] {
            let mut lines = vec![
                ".section .text".to_string(),
                ".global entry".to_string(),
                "entry:".to_string(),
                "li a0, 0".to_string(),
                "li a1, 0".to_string(),
                format!("{} a0, a1, far_target", mnemonic),
            ];
            for _ in 0..1500 {
                lines.push("addi t0, t0, 0".to_string());
            }
            lines.push("far_target:".to_string());
            lines.push("ret".to_string());

            let plan = MachineLayoutPlan::build(&lines).unwrap_or_else(|err| panic!("machine layout should relax {mnemonic}: {err}"));
            assert_eq!(plan.metrics.relaxed_branch_count, 1, "expected one relaxed branch for {mnemonic}");
            let elf = assemble_elf_internal(&lines).unwrap_or_else(|err| panic!("internal assembler should relax {mnemonic}: {err}"));
            assert!(elf.starts_with(b"\x7fELF"), "expected ELF output for relaxed {mnemonic}");
        }
    }

    #[test]
    fn machine_layout_plan_reports_branch_relaxation_metrics() {
        let mut lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, far_target".to_string(),
        ];
        for _ in 0..1500 {
            lines.push("addi t0, t0, 0".to_string());
        }
        lines.push("far_target:".to_string());
        lines.push("ret".to_string());

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        assert_eq!(plan.metrics.relaxed_branch_count, 1);
        assert!(
            plan.metrics.max_cond_branch_abs_distance > 4096,
            "synthetic branch should exceed RV64 B-type range: {:?}",
            plan.metrics
        );
        assert_eq!(plan.metrics.text_size, plan.parsed.section_size(SectionKind::Text));
        assert_eq!(plan.metrics.covered_text_op_count, plan.metrics.executable_text_op_count);
        assert!(plan.metrics.executable_text_op_count > 1500, "synthetic text ops should be visible: {:?}", plan.metrics);
        assert_eq!(plan.metrics.layout_order_block_count, plan.metrics.machine_block_count);
        assert_eq!(plan.metrics.layout_order_text_size, plan.metrics.text_size);
        assert_eq!(plan.metrics.conditional_branch_block_count, 1);
        assert!(plan.metrics.machine_cfg_edge_count >= 2, "far branch CFG edges should be visible: {:?}", plan.metrics);
        assert_eq!(plan.metrics.machine_call_edge_count, 0);
        assert_eq!(plan.metrics.unreachable_machine_block_count, 0);
        assert!(plan.metrics.machine_block_count >= 2, "far branch should produce multiple machine blocks: {:?}", plan.metrics);
        assert!(
            plan.metrics.max_machine_block_size > 4096,
            "large fallthrough block should be visible in layout metrics: {:?}",
            plan.metrics
        );
    }

    #[test]
    fn machine_layout_plan_builds_explicit_machine_blocks() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, done".to_string(),
            "li a0, 1".to_string(),
            "j done".to_string(),
            "done:".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        let cfg = &plan.cfg;
        let blocks = &cfg.blocks;
        assert_eq!(blocks.len(), 3, "expected entry, fallthrough, and done blocks: {:?}", blocks);
        assert_eq!(blocks[0].label.as_deref(), Some("entry"));
        assert_eq!(blocks[0].terminator, MachineTerminator::ConditionalBranch { target: "done".to_string() });
        assert_eq!(blocks[1].terminator, MachineTerminator::Jump { target: "done".to_string() });
        assert_eq!(blocks[2].label.as_deref(), Some("done"));
        assert_eq!(blocks[2].terminator, MachineTerminator::Return);

        assert_eq!(cfg.blocks.len(), 3);
        assert_eq!(plan.order.block_order, vec![0, 1, 2]);
        assert_eq!(plan.order.placed_blocks.len(), 3);
        assert_eq!(
            plan.order.placed_blocks,
            vec![
                MachinePlacedBlock { block_index: 0, byte_start: 0, byte_size: cfg.blocks[0].byte_size },
                MachinePlacedBlock { block_index: 1, byte_start: cfg.blocks[0].byte_size, byte_size: cfg.blocks[1].byte_size },
                MachinePlacedBlock {
                    block_index: 2,
                    byte_start: cfg.blocks[0].byte_size + cfg.blocks[1].byte_size,
                    byte_size: cfg.blocks[2].byte_size
                },
            ]
        );
        assert_eq!(plan.order.text_size, plan.metrics.text_size);
        assert_eq!(plan.metrics.executable_text_op_count, 5);
        assert_eq!(plan.metrics.covered_text_op_count, 5);
        assert_eq!(plan.metrics.layout_order_block_count, 3);
        assert_eq!(
            cfg.edges,
            vec![
                MachineCfgEdge { from: 0, to: 2, kind: MachineCfgEdgeKind::ConditionalTaken },
                MachineCfgEdge { from: 0, to: 1, kind: MachineCfgEdgeKind::ConditionalFallthrough },
                MachineCfgEdge { from: 1, to: 2, kind: MachineCfgEdgeKind::Jump },
            ]
        );
        assert_eq!(unreachable_machine_block_count(&plan.parsed, cfg), 0);
    }

    #[test]
    fn machine_layout_plan_builds_register_conditional_branch_blocks() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "li a1, 0".to_string(),
            "bgeu a0, a1, done".to_string(),
            "li a0, 1".to_string(),
            "j done".to_string(),
            "done:".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        let cfg = &plan.cfg;
        assert_eq!(cfg.blocks.len(), 3, "expected entry, fallthrough, and done blocks: {:?}", cfg.blocks);
        assert_eq!(cfg.blocks[0].label.as_deref(), Some("entry"));
        assert_eq!(cfg.blocks[0].terminator, MachineTerminator::ConditionalBranch { target: "done".to_string() });
        assert_eq!(
            cfg.edges,
            vec![
                MachineCfgEdge { from: 0, to: 2, kind: MachineCfgEdgeKind::ConditionalTaken },
                MachineCfgEdge { from: 0, to: 1, kind: MachineCfgEdgeKind::ConditionalFallthrough },
                MachineCfgEdge { from: 1, to: 2, kind: MachineCfgEdgeKind::Jump },
            ]
        );
    }

    #[test]
    fn machine_cfg_tracks_call_edges_to_local_helpers() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "call local_helper".to_string(),
            "ret".to_string(),
            "local_helper:".to_string(),
            "li a0, 0".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        let cfg = &plan.cfg;
        assert_eq!(cfg.blocks.len(), 2, "expected entry and local helper blocks: {:?}", cfg.blocks);
        assert_eq!(cfg.blocks[0].label.as_deref(), Some("entry"));
        assert_eq!(cfg.blocks[1].label.as_deref(), Some("local_helper"));
        assert!(
            cfg.edges.contains(&MachineCfgEdge { from: 0, to: 1, kind: MachineCfgEdgeKind::Call }),
            "call edge to local helper should be explicit: {:?}",
            cfg.edges
        );
        assert_eq!(plan.metrics.machine_call_edge_count, 1);
        assert_eq!(unreachable_machine_block_count(&plan.parsed, cfg), 0);
    }

    #[test]
    fn machine_reachability_uses_entry_label_not_every_global() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "ret".to_string(),
            ".global unused_export".to_string(),
            "unused_export:".to_string(),
            "li a0, 1".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        assert_eq!(plan.parsed.entry_label.as_deref(), Some("entry"));
        assert_eq!(plan.cfg.blocks.len(), 2, "expected entry and unused export blocks: {:?}", plan.cfg.blocks);
        assert_eq!(plan.metrics.unreachable_machine_block_count, 1);
        assert_eq!(unreachable_machine_block_count(&plan.parsed, &plan.cfg), 1);
    }

    #[test]
    fn machine_layout_order_rejects_missing_duplicate_or_unknown_blocks() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, done".to_string(),
            "li a0, 1".to_string(),
            "j done".to_string(),
            "done:".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        assert!(validate_machine_layout_order(&plan.cfg, &[0, 1]).is_err());
        assert!(validate_machine_layout_order(&plan.cfg, &[0, 1, 1]).is_err());
        assert!(validate_machine_layout_order(&plan.cfg, &[0, 1, 3]).is_err());
        let permuted = build_machine_layout_order(&plan.cfg, vec![2, 0, 1]).expect("permuted layout order should be valid");
        assert_eq!(permuted.block_order, vec![2, 0, 1]);
        assert_eq!(permuted.placed_blocks[0].block_index, 2);
        assert_eq!(permuted.placed_blocks[0].byte_start, 0);
        assert_eq!(permuted.placed_blocks[1].byte_start, plan.cfg.blocks[2].byte_size);
        assert_eq!(permuted.text_size, plan.order.text_size);
    }

    #[test]
    fn machine_layout_plan_rejects_branch_target_outside_text() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, data_label".to_string(),
            "ret".to_string(),
            ".section .rodata".to_string(),
            "data_label:".to_string(),
            ".word 1".to_string(),
        ];

        let err = MachineLayoutPlan::build(&lines).expect_err("branch targets outside text blocks should be rejected");
        assert!(err.message.contains("does not start a machine block"), "unexpected error for invalid CFG target: {}", err.message);
    }

    #[test]
    fn generated_functions_use_shared_epilogue_tail() {
        let ir = IrModule {
            name: "shape_test".to_string(),
            items: vec![IrItem::Action(IrAction {
                name: "shape".to_string(),
                params: vec![],
                return_type: Some(IrType::U64),
                state_transition_edges: vec![],
                effect_class: EffectClass::Pure,
                scheduler_hints: SchedulerHints::default(),
                body: IrBody {
                    consume_set: vec![],
                    read_refs: vec![],
                    create_set: vec![],
                    mutate_set: vec![],
                    write_intents: vec![],
                    blocks: vec![IrBlock {
                        id: BlockId(0),
                        instructions: vec![],
                        terminator: IrTerminator::Return(Some(IrOperand::Const(IrConst::U64(7)))),
                    }],
                },
            })],
            external_type_defs: vec![],
            external_callable_abis: vec![],
            enum_fixed_sizes: HashMap::new(),
        };
        let assembly = CodeGenerator::new(CodegenOptions::default()).generate(&ir, ArtifactFormat::RiscvAssembly).unwrap();
        let assembly = String::from_utf8(assembly).unwrap();
        let shape_start = assembly.find("shape:\n").expect("shape function label");
        let runtime_start =
            assembly[shape_start..].find(".section .text").map(|offset| shape_start + offset).unwrap_or(assembly.len());
        let shape_assembly = &assembly[shape_start..runtime_start];

        assert!(shape_assembly.contains("j .Lshape_epilogue"), "return sites should jump to the shared epilogue:\n{}", shape_assembly);
        assert_eq!(
            shape_assembly.matches(".Lshape_epilogue:").count(),
            1,
            "a function should emit one shared epilogue label:\n{}",
            shape_assembly
        );
        assert_eq!(
            shape_assembly.matches("ret").count(),
            1,
            "a function should emit one physical return in its shared epilogue:\n{}",
            shape_assembly
        );
    }
}
