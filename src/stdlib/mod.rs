//! Public standalone assembly compatibility surface. Ordinary CellScript lowering
//! owns its runtime helpers separately; this module is not linked into every ELF.
//!
//! Raw syscall descriptors use the CKB register ABI: caller-owned writable
//! buffer, pointer to a u64 in/out length, offset, and any index/source/field.
//! Load calls return raw status, current_cycles returns its value, and debug
//! has no result. This corrects the former descriptors that advertised implicit Hash/scalar results without
//! providing storage. Existing String-returning generator APIs are retained.
//! Checked scalar context helpers abort on syscall/size failure. The legacy
//! remaining-cycle estimate is retired because a syscall cannot know the caller's
//! cycle allowance. Collection assembly has its own explicit rejection boundary.

pub mod ckb_protocols;
pub mod collections;
pub mod signature;

use crate::{ckb_abi, ckb_blake2b256, ir::IrType, runtime_errors::CellScriptRuntimeError, TargetProfile};

/// A single register-ABI inventory drives both descriptors and raw bodies.
struct RawSyscall {
    name: &'static str,
    number: u64,
    parameters: &'static [&'static str],
    result: RawSyscallResult,
}

#[derive(Clone, Copy)]
enum RawSyscallResult {
    Status,
    Cycles,
    NoResult,
}

impl RawSyscallResult {
    fn ir_type(self) -> Option<IrType> {
        match self {
            Self::Status | Self::Cycles => Some(IrType::U64),
            Self::NoResult => None,
        }
    }
}

const RAW_SYSCALLS: &[RawSyscall] = &[
    RawSyscall {
        name: "syscall_load_tx_hash",
        number: ckb_abi::syscall::LOAD_TX_HASH,
        parameters: &["buffer", "length_pointer", "offset"],
        result: RawSyscallResult::Status,
    },
    RawSyscall {
        name: "syscall_load_script_hash",
        number: ckb_abi::syscall::LOAD_SCRIPT_HASH,
        parameters: &["buffer", "length_pointer", "offset"],
        result: RawSyscallResult::Status,
    },
    RawSyscall {
        name: "syscall_load_cell",
        number: ckb_abi::syscall::LOAD_CELL,
        parameters: &["buffer", "length_pointer", "offset", "index", "source"],
        result: RawSyscallResult::Status,
    },
    RawSyscall {
        name: "syscall_load_header",
        number: ckb_abi::syscall::LOAD_HEADER,
        parameters: &["buffer", "length_pointer", "offset", "index", "source"],
        result: RawSyscallResult::Status,
    },
    RawSyscall {
        name: "syscall_load_input",
        number: ckb_abi::syscall::LOAD_INPUT,
        parameters: &["buffer", "length_pointer", "offset", "index", "source"],
        result: RawSyscallResult::Status,
    },
    RawSyscall {
        name: "syscall_load_script",
        number: ckb_abi::syscall::LOAD_SCRIPT,
        parameters: &["buffer", "length_pointer", "offset"],
        result: RawSyscallResult::Status,
    },
    RawSyscall {
        name: "syscall_load_cell_by_field",
        number: ckb_abi::syscall::LOAD_CELL_BY_FIELD,
        parameters: &["buffer", "length_pointer", "offset", "index", "source", "field"],
        result: RawSyscallResult::Status,
    },
    RawSyscall {
        name: "syscall_load_cell_data",
        number: ckb_abi::syscall::LOAD_CELL_DATA,
        parameters: &["buffer", "length_pointer", "offset", "index", "source"],
        result: RawSyscallResult::Status,
    },
    RawSyscall {
        name: "syscall_load_witness",
        number: ckb_abi::syscall::LOAD_WITNESS,
        parameters: &["buffer", "length_pointer", "offset", "index", "source"],
        result: RawSyscallResult::Status,
    },
    RawSyscall {
        name: "syscall_current_cycles",
        number: ckb_abi::syscall::CURRENT_CYCLES,
        parameters: &[],
        result: RawSyscallResult::Cycles,
    },
    RawSyscall {
        name: "syscall_debug_print",
        number: ckb_abi::syscall::DEBUG,
        parameters: &["message_pointer"],
        result: RawSyscallResult::NoResult,
    },
];

/// Non-returning compatibility rejection, including a fallback if a custom VM
/// runner unexpectedly resumes EXIT. It never substitutes an error as a value.
fn push_abort(asm: &mut String, owner: &str, error: CellScriptRuntimeError) {
    asm.push_str(&format!(
        ".L{owner}_abort:\n    # cellscript runtime error {} {}\n    li a0, {}\n    li a7, {}\n    ecall\n    j .L{owner}_abort\n\n",
        error.code(),
        error.name(),
        error.code(),
        ckb_abi::syscall::EXIT
    ));
}

pub struct StdLib;

#[cfg(test)]
mod generated_tests;

impl StdLib {
    /// Legacy declaration catalog, not an assertion that every runtime family
    /// has a standalone assembly implementation. Raw syscall declarations here
    /// match this module's actual register ABI; ordinary DSL calls are admitted
    /// and emitted by the compiler's separate typed runtime contracts.
    pub fn functions() -> Vec<StdFunction> {
        let mut functions = RAW_SYSCALLS
            .iter()
            .map(|call| StdFunction {
                name: call.name.to_string(),
                params: call.parameters.iter().map(|name| ((*name).to_string(), IrType::U64)).collect(),
                return_type: call.result.ir_type(),
            })
            .collect::<Vec<_>>();
        functions.extend(vec![
            StdFunction {
                name: "math_min".to_string(),
                params: vec![("a".to_string(), IrType::U64), ("b".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "math_max".to_string(),
                params: vec![("a".to_string(), IrType::U64), ("b".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "math_isqrt".to_string(),
                params: vec![("n".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "math_abs_diff".to_string(),
                params: vec![("a".to_string(), IrType::U64), ("b".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "c256_require_product_lte".to_string(),
                params: vec![
                    ("left_amount".to_string(), IrType::U128),
                    ("left_multiplier".to_string(), IrType::U128),
                    ("right_amount".to_string(), IrType::U128),
                    ("right_multiplier".to_string(), IrType::U128),
                ],
                return_type: None,
            },
            StdFunction {
                name: "c256_require_product_eq".to_string(),
                params: vec![
                    ("left_amount".to_string(), IrType::U128),
                    ("left_multiplier".to_string(), IrType::U128),
                    ("right_amount".to_string(), IrType::U128),
                    ("right_multiplier".to_string(), IrType::U128),
                ],
                return_type: None,
            },
            StdFunction {
                name: "c256_require_sum2_products_lte".to_string(),
                params: vec![
                    ("left_amount_a".to_string(), IrType::U128),
                    ("left_multiplier_a".to_string(), IrType::U128),
                    ("left_amount_b".to_string(), IrType::U128),
                    ("left_multiplier_b".to_string(), IrType::U128),
                    ("right_amount_a".to_string(), IrType::U128),
                    ("right_multiplier_a".to_string(), IrType::U128),
                    ("right_amount_b".to_string(), IrType::U128),
                    ("right_multiplier_b".to_string(), IrType::U128),
                ],
                return_type: None,
            },
            StdFunction {
                name: "c256_require_sum2_products_eq".to_string(),
                params: vec![
                    ("left_amount_a".to_string(), IrType::U128),
                    ("left_multiplier_a".to_string(), IrType::U128),
                    ("left_amount_b".to_string(), IrType::U128),
                    ("left_multiplier_b".to_string(), IrType::U128),
                    ("right_amount_a".to_string(), IrType::U128),
                    ("right_multiplier_a".to_string(), IrType::U128),
                    ("right_amount_b".to_string(), IrType::U128),
                    ("right_multiplier_b".to_string(), IrType::U128),
                ],
                return_type: None,
            },
            StdFunction { name: "env_current_timepoint".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "ckb_header_epoch_number".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "ckb_header_epoch_start_block_number".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "ckb_header_epoch_length".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "ckb_input_since".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction {
                name: "ckb_since_epoch_absolute".to_string(),
                params: vec![
                    ("number".to_string(), IrType::U64),
                    ("index".to_string(), IrType::U64),
                    ("length".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_since_epoch_relative".to_string(),
                params: vec![
                    ("number".to_string(), IrType::U64),
                    ("index".to_string(), IrType::U64),
                    ("length".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_since_absolute_epoch".to_string(),
                params: vec![
                    ("number".to_string(), IrType::U64),
                    ("index".to_string(), IrType::U64),
                    ("length".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Named("AbsoluteEpochSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_relative_epoch".to_string(),
                params: vec![
                    ("number".to_string(), IrType::U64),
                    ("index".to_string(), IrType::U64),
                    ("length".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Named("RelativeEpochSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_absolute_block".to_string(),
                params: vec![("value".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("AbsoluteBlockSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_relative_block".to_string(),
                params: vec![("value".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("RelativeBlockSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_absolute_timestamp".to_string(),
                params: vec![("seconds".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("AbsoluteTimestampSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_relative_timestamp".to_string(),
                params: vec![("seconds".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("RelativeTimestampSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_decode".to_string(),
                params: vec![("encoded".to_string(), IrType::Named("EncodedSince".to_string()))],
                return_type: Some(IrType::Named("DecodedSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_from_raw_checked".to_string(),
                params: vec![("raw".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("DecodedSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_as_absolute_block".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::Named("AbsoluteBlockSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_as_relative_block".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::Named("RelativeBlockSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_as_absolute_epoch".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::Named("AbsoluteEpochSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_as_relative_epoch".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::Named("RelativeEpochSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_as_absolute_timestamp".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::Named("AbsoluteTimestampSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_as_relative_timestamp".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::Named("RelativeTimestampSince".to_string())),
            },
            StdFunction {
                name: "ckb_since_is_relative".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::Bool),
            },
            StdFunction {
                name: "ckb_since_is_disabled".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::Bool),
            },
            StdFunction {
                name: "ckb_since_metric".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_since_value".to_string(),
                params: vec![("decoded".to_string(), IrType::Named("DecodedSince".to_string()))],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_epoch_duration".to_string(),
                params: vec![("value".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("EpochDuration".to_string())),
            },
            StdFunction {
                name: "ckb_epoch_add".to_string(),
                params: vec![
                    ("epoch".to_string(), IrType::Named("EpochNumber".to_string())),
                    ("duration".to_string(), IrType::Named("EpochDuration".to_string())),
                ],
                return_type: Some(IrType::Named("EpochNumber".to_string())),
            },
            StdFunction {
                name: "ckb_epoch_sub".to_string(),
                params: vec![
                    ("epoch".to_string(), IrType::Named("EpochNumber".to_string())),
                    ("duration".to_string(), IrType::Named("EpochDuration".to_string())),
                ],
                return_type: Some(IrType::Named("EpochNumber".to_string())),
            },
            StdFunction {
                name: "ckb_epoch_duration_to_u64".to_string(),
                params: vec![("duration".to_string(), IrType::Named("EpochDuration".to_string()))],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_timestamp_millis_to_u64".to_string(),
                params: vec![("timestamp".to_string(), IrType::Named("TimestampMillis".to_string()))],
                return_type: Some(IrType::U64),
            },
            StdFunction { name: "ckb_current_role".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction {
                name: "ckb_cell_capacity".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_cell_occupied_capacity".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_cell_unoccupied_capacity".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_cell_data_u64_le".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("offset".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_cell_data_u32_le".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("offset".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction { name: "ckb_current_script_hash".to_string(), params: vec![], return_type: Some(IrType::Hash) },
            StdFunction {
                name: "ckb_cell_lock_hash_low".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_cell_type_hash_low".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_cell_lock_hash".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Hash),
            },
            StdFunction {
                name: "ckb_cell_type_hash".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Hash),
            },
            StdFunction {
                name: "ckb_cell_lock_code_hash".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Hash),
            },
            StdFunction {
                name: "ckb_cell_type_code_hash".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Hash),
            },
            StdFunction {
                name: "ckb_cell_lock_hash_type".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_cell_type_hash_type".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_cell_lock_args_empty".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Bool),
            },
            StdFunction {
                name: "ckb_cell_type_args_empty".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Bool),
            },
            StdFunction {
                name: "ckb_cell_lock_args32".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Array(Box::new(IrType::U8), 32)),
            },
            StdFunction {
                name: "ckb_cell_type_args32".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Array(Box::new(IrType::U8), 32)),
            },
            StdFunction {
                name: "ckb_input_out_point_index".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U32),
            },
            StdFunction {
                name: "ckb_input_out_point_tx_hash_low".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "ckb_input_out_point_tx_hash".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Hash),
            },
            StdFunction {
                name: "ckb_require_cell_lock_hash".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("expected_hash".to_string(), IrType::Hash)],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_type_hash".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("expected_hash".to_string(), IrType::Hash)],
                return_type: None,
            },
            StdFunction { name: "ckb_require_current_script_args_empty".to_string(), params: vec![], return_type: None },
            StdFunction {
                name: "ckb_require_cell_lock_args_empty".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_type_args_empty".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_lock_args32".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("expected_bytes".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_type_args32".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("expected_bytes".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_lock_args_prefix32".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("expected_bytes".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_type_args_prefix32".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("expected_bytes".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_lock_args_suffix32".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("expected_bytes".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_type_args_suffix32".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("expected_bytes".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_lock_script_hash_type".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("expected_code_hash".to_string(), IrType::Hash),
                    ("expected_hash_type".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_cell_type_script_hash_type".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("expected_code_hash".to_string(), IrType::Hash),
                    ("expected_hash_type".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_input_out_point_tx_hash".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("expected_hash".to_string(), IrType::Hash)],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_input_out_point".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("expected_hash".to_string(), IrType::Hash),
                    ("expected_index".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_metapoint_relative".to_string(),
                params: vec![
                    ("base_view".to_string(), IrType::U64),
                    ("related_view".to_string(), IrType::U64),
                    ("relative_distance".to_string(), IrType::I32),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_lock_type_metapoint_pairs".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("relative_distance".to_string(), IrType::I32)],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_type_lock_metapoint_pairs".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("relative_distance".to_string(), IrType::I32)],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_lock_type_metapoint_pairs_from_i32_data".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("distance_offset".to_string(), IrType::U64)],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_type_lock_metapoint_pairs_from_i32_data".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("distance_offset".to_string(), IrType::U64)],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("distance_offset".to_string(), IrType::U64),
                    ("expected_related_type_hash".to_string(), IrType::Hash),
                    ("related_data_rule".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("distance_offset".to_string(), IrType::U64),
                    ("expected_related_type_hash".to_string(), IrType::Hash),
                    ("related_data_rule".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            StdFunction {
                name: "ckb_require_lock_match_master_out_point_pairs_from_data".to_string(),
                params: vec![
                    ("input_source_view".to_string(), IrType::U64),
                    ("output_source_view".to_string(), IrType::U64),
                    ("action_offset".to_string(), IrType::U64),
                    ("tx_hash_offset".to_string(), IrType::U64),
                    ("index_offset".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            StdFunction {
                name: "dao_accumulated_rate".to_string(),
                params: vec![("header_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "dao_input_accumulated_rate".to_string(),
                params: vec![("input_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "dao_has_dao_type".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Bool),
            },
            StdFunction {
                name: "dao_is_deposit_data".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Bool),
            },
            StdFunction {
                name: "dao_is_withdrawal_request_data".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::Bool),
            },
            StdFunction {
                name: "dao_require_header_dep_for_input".to_string(),
                params: vec![("input_view".to_string(), IrType::U64), ("header_view".to_string(), IrType::U64)],
                return_type: None,
            },
            StdFunction {
                name: "dao_require_input_since_at_least".to_string(),
                params: vec![("input_view".to_string(), IrType::U64), ("required_since".to_string(), IrType::U64)],
                return_type: None,
            },
            StdFunction {
                name: "dao_require_input_relative_epoch_since_at_least".to_string(),
                params: vec![
                    ("input_view".to_string(), IrType::U64),
                    ("number".to_string(), IrType::U64),
                    ("index".to_string(), IrType::U64),
                    ("length".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            StdFunction {
                name: "xudt_amount_low".to_string(),
                params: vec![("source_view".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "xudt_require_owner_mode_type_args".to_string(),
                params: vec![
                    ("source_view".to_string(), IrType::U64),
                    ("owner_hash".to_string(), IrType::Hash),
                    ("flags".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            StdFunction {
                name: "xudt_require_owner_mode_type_args_current_script".to_string(),
                params: vec![("source_view".to_string(), IrType::U64), ("flags".to_string(), IrType::U64)],
                return_type: None,
            },
            StdFunction { name: "xudt_require_group_amount_conserved".to_string(), params: vec![], return_type: None },
            StdFunction {
                name: "xudt_require_group_amount_minted".to_string(),
                params: vec![("delta".to_string(), IrType::U128)],
                return_type: None,
            },
            StdFunction {
                name: "xudt_require_group_amount_burned".to_string(),
                params: vec![("delta".to_string(), IrType::U128)],
                return_type: None,
            },
            StdFunction { name: "env_remaining_cycles".to_string(), params: vec![], return_type: Some(IrType::U64) },
        ]);
        functions
    }

    pub fn is_std_function(name: &str) -> bool {
        Self::functions().iter().any(|f| f.name == name)
    }

    pub fn get_function(name: &str) -> Option<StdFunction> {
        Self::functions().into_iter().find(|f| f.name == name)
    }

    /// Generate the standalone raw-syscall, unsigned-math and checked-context
    /// compatibility surface. Collection allocation is not linked implicitly.
    pub fn generate_assembly() -> String {
        Self::generate_assembly_for_target_profile(TargetProfile::Ckb)
    }

    pub fn generate_assembly_for_target_profile(target_profile: TargetProfile) -> String {
        let mut asm = String::new();

        asm.push_str("# CellScript Standard Library\n\n");
        asm.push_str(".section .text\n\n");

        asm.push_str(&Self::generate_syscalls(target_profile));

        asm.push_str(&Self::generate_math());

        asm.push_str(&Self::generate_env(target_profile));

        asm
    }

    fn generate_syscalls(_target_profile: TargetProfile) -> String {
        let mut asm = String::new();
        for call in RAW_SYSCALLS {
            asm.push_str(&format!(
                "# Syscall: {} ({})\n# Raw CKB argument/status ABI\n.global __{}\n__{}:\n",
                call.name.strip_prefix("syscall_").unwrap_or(call.name),
                call.number,
                call.name,
                call.name
            ));
            for (index, parameter) in call.parameters.iter().enumerate() {
                asm.push_str(&format!("    # a{index} = {parameter}\n"));
            }
            asm.push_str(&format!("    li a7, {}\n    ecall\n    ret\n\n", call.number));
        }
        asm
    }

    fn generate_math() -> String {
        let mut asm = String::new();

        // math_min
        asm.push_str("# Math: min\n");
        asm.push_str(".global __math_min\n");
        asm.push_str("__math_min:\n");
        asm.push_str("    # a0 = a, a1 = b\n");
        asm.push_str("    bltu a0, a1, .Lmin_ret_a\n");
        asm.push_str("    mv a0, a1\n");
        asm.push_str(".Lmin_ret_a:\n");
        asm.push_str("    ret\n\n");

        // math_max
        asm.push_str("# Math: max\n");
        asm.push_str(".global __math_max\n");
        asm.push_str("__math_max:\n");
        asm.push_str("    # a0 = a, a1 = b\n");
        asm.push_str("    bltu a1, a0, .Lmax_ret_a\n");
        asm.push_str("    mv a0, a1\n");
        asm.push_str(".Lmax_ret_a:\n");
        asm.push_str("    ret\n\n");

        asm.push_str("# Math: isqrt (integer square root)\n");
        asm.push_str(".global __math_isqrt\n");
        asm.push_str("__math_isqrt:\n");
        asm.push_str("    addi sp, sp, -32\n");
        asm.push_str("    sd ra, 24(sp)\n");
        asm.push_str("    sd s0, 16(sp)\n");
        asm.push_str("    sd s1, 8(sp)\n");
        asm.push_str("    # a0 = n\n");
        asm.push_str("    mv s0, a0          # x = n, including zero\n");
        asm.push_str("    beqz a0, .Lisqrt_ret\n");
        asm.push_str("    srli s1, a0, 1\n");
        asm.push_str("    andi t0, a0, 1\n");
        asm.push_str("    add s1, s1, t0    # y = ceil(n/2), without overflowing n+1\n");
        asm.push_str(".Lisqrt_loop:\n");
        asm.push_str("    bgeu s1, s0, .Lisqrt_ret\n");
        asm.push_str("    mv s0, s1          # x = y\n");
        asm.push_str("    divu t0, a0, s0\n");
        asm.push_str("    add s1, s0, t0\n");
        asm.push_str("    srli s1, s1, 1     # y = (x + n/x) / 2\n");
        asm.push_str("    j .Lisqrt_loop\n");
        asm.push_str(".Lisqrt_ret:\n");
        asm.push_str("    mv a0, s0\n");
        asm.push_str("    ld ra, 24(sp)\n");
        asm.push_str("    ld s0, 16(sp)\n");
        asm.push_str("    ld s1, 8(sp)\n");
        asm.push_str("    addi sp, sp, 32\n");
        asm.push_str("    ret\n\n");

        // math_abs_diff
        asm.push_str("# Math: abs_diff\n");
        asm.push_str(".global __math_abs_diff\n");
        asm.push_str("__math_abs_diff:\n");
        asm.push_str("    # a0 = a, a1 = b\n");
        asm.push_str("    bltu a0, a1, .Labs_diff_reverse\n");
        asm.push_str("    sub a0, a0, a1\n");
        asm.push_str("    ret\n");
        asm.push_str(".Labs_diff_reverse:\n");
        asm.push_str("    sub a0, a1, a0\n");
        asm.push_str("    ret\n\n");

        asm
    }

    fn generate_env(target_profile: TargetProfile) -> String {
        let mut asm = String::new();

        Self::push_ckb_header_epoch_helper(
            &mut asm,
            "__env_current_timepoint",
            "current_timepoint (CKB epoch number, not Unix timestamp)",
            ckb_abi::header_field::EPOCH_NUMBER,
            target_profile.is_ckb(),
        );

        Self::push_ckb_header_epoch_helper(
            &mut asm,
            "__ckb_header_epoch_number",
            "ckb_epoch_number",
            ckb_abi::header_field::EPOCH_NUMBER,
            target_profile.is_ckb(),
        );
        Self::push_ckb_header_epoch_helper(
            &mut asm,
            "__ckb_header_epoch_start_block_number",
            "ckb_epoch_start_block_number",
            ckb_abi::header_field::EPOCH_START_BLOCK_NUMBER,
            target_profile.is_ckb(),
        );
        Self::push_ckb_header_epoch_helper(
            &mut asm,
            "__ckb_header_epoch_length",
            "ckb_epoch_length",
            ckb_abi::header_field::EPOCH_LENGTH,
            target_profile.is_ckb(),
        );
        Self::push_ckb_input_since_helper(&mut asm, target_profile.is_ckb());

        asm.push_str("# Retired: current cycles cannot reveal the caller's remaining cycle allowance\n");
        asm.push_str(".global __env_remaining_cycles\n__env_remaining_cycles:\n");
        push_abort(&mut asm, "__env_remaining_cycles", CellScriptRuntimeError::SyscallFailed);

        asm
    }

    fn push_ckb_header_epoch_helper(asm: &mut String, symbol: &str, field_name: &str, field_id: u64, enabled: bool) {
        Self::push_checked_u64_load(
            asm,
            symbol,
            field_name,
            (ckb_abi::syscall::LOAD_HEADER_BY_FIELD, "LOAD_HEADER_BY_FIELD"),
            ckb_abi::source::HEADER_DEP,
            field_id,
            enabled,
        );
    }

    fn push_ckb_input_since_helper(asm: &mut String, enabled: bool) {
        Self::push_checked_u64_load(
            asm,
            "__ckb_input_since",
            "input_since",
            (ckb_abi::syscall::LOAD_INPUT_BY_FIELD, "LOAD_INPUT_BY_FIELD"),
            ckb_abi::source::GROUP_INPUT,
            ckb_abi::input_field::SINCE,
            enabled,
        );
    }

    /// Scalar values own local storage and never return an unchecked status or
    /// uninitialized buffer as a value. Raw status-returning calls stay separate.
    fn push_checked_u64_load(
        asm: &mut String,
        symbol: &str,
        detail: &str,
        syscall: (u64, &str),
        source: u64,
        field: u64,
        enabled: bool,
    ) {
        let (syscall, syscall_name) = syscall;
        asm.push_str(&format!("# Checked CKB scalar: {detail}\n.global {symbol}\n{symbol}:\n"));
        if !enabled {
            push_abort(asm, symbol, CellScriptRuntimeError::ConsumeInvalidOperand);
            return;
        }
        asm.push_str("    addi sp, sp, -32\n    sd ra, 24(sp)\n    li t0, 8\n    sd t0, 8(sp)\n    sd zero, 16(sp)\n");
        asm.push_str("    addi a0, sp, 16\n    addi a1, sp, 8\n    li a2, 0\n    li a3, 0\n");
        let source_name = if source == ckb_abi::source::GROUP_INPUT { "GroupInput" } else { "HeaderDep" };
        asm.push_str(&format!(
            "    li a4, {source}  # Source::{source_name}\n    li a5, {field}\n    li a7, {syscall}  # {syscall_name}\n    ecall\n"
        ));
        asm.push_str(&format!(
            "    bnez a0, .L{symbol}_syscall_failed\n    ld t0, 8(sp)\n    li t1, 8\n    bne t0, t1, .L{symbol}_size_failed\n"
        ));
        asm.push_str("    ld a0, 16(sp)\n    ld ra, 24(sp)\n    addi sp, sp, 32\n    ret\n");
        asm.push_str(&format!(".L{symbol}_syscall_failed:\n"));
        push_abort(asm, &format!("{symbol}_syscall_failed"), CellScriptRuntimeError::SyscallFailed);
        asm.push_str(&format!(".L{symbol}_size_failed:\n"));
        push_abort(asm, &format!("{symbol}_size_failed"), CellScriptRuntimeError::ExactSizeMismatch);
    }
}

#[derive(Debug, Clone)]
pub struct StdFunction {
    pub name: String,
    pub params: Vec<(String, IrType)>,
    pub return_type: Option<IrType>,
}

pub struct SchedulerMetadata;

/// Scheduler-visible CKB runtime access summary.
#[derive(Debug, Clone)]
pub struct SchedulerAccess {
    pub operation: String,
    pub source: String,
    pub index: u32,
    pub binding: String,
}

impl SchedulerMetadata {
    pub fn generate(
        effects: &crate::ir::EffectFacts,
        parallelizable: bool,
        touches_shared: Vec<[u8; 32]>,
        estimated_cycles: u64,
        accesses: Vec<SchedulerAccess>,
    ) -> Vec<u8> {
        Self::generate_molecule(effects, parallelizable, touches_shared, estimated_cycles, accesses)
    }

    /// Only the current fact-bearing Molecule table is admitted. No category
    /// byte, old version, alias field or format detection participates.
    pub(crate) fn validate_current(bytes: &[u8], expected: &crate::ir::EffectFacts) -> crate::error::Result<()> {
        use crate::error::CompileError;
        let invalid = || CompileError::without_span("invalid current scheduler fact witness");
        let word = |slice: &[u8]| -> crate::error::Result<usize> {
            let value: [u8; 4] = slice.try_into().map_err(|_| invalid())?;
            Ok(u32::from_le_bytes(value) as usize)
        };
        if bytes.len() < 40 || word(&bytes[..4])? != bytes.len() {
            return Err(invalid());
        }
        let mut offsets = bytes[4..40].chunks_exact(4).map(word).collect::<crate::error::Result<Vec<_>>>()?;
        if offsets.first() != Some(&40)
            || offsets.windows(2).any(|pair| pair[0] > pair[1])
            || offsets.last().is_none_or(|last| *last > bytes.len())
        {
            return Err(invalid());
        }
        offsets.push(bytes.len());
        let fields = offsets.windows(2).map(|pair| &bytes[pair[0]..pair[1]]).collect::<Vec<_>>();
        if fields[0] != 0xCE11u16.to_le_bytes() || fields[1] != [2] || fields[2].len() < 4 {
            return Err(invalid());
        }
        if word(&fields[2][..4])? != fields[2].len() - 4 {
            return Err(invalid());
        }
        let facts: crate::ir::EffectFacts = serde_json::from_slice(&fields[2][4..]).map_err(|_| invalid())?;
        if &facts != expected || serde_json::to_vec(&facts).map_err(|_| invalid())? != fields[2][4..] {
            return Err(invalid());
        }
        if !matches!(fields[3], [0] | [1])
            || fields[4].len() != 4
            || fields[5].len() < 4
            || fields[6].len() != 8
            || fields[7].len() != 4
            || fields[8].len() < 4
        {
            return Err(invalid());
        }
        for (count, data, width) in [(fields[4], fields[5], 32usize), (fields[7], fields[8], 38usize)] {
            let count = word(count)?;
            if count != word(&data[..4])? || count.checked_mul(width).and_then(|length| length.checked_add(4)) != Some(data.len()) {
                return Err(invalid());
            }
        }
        Ok(())
    }

    pub fn generate_molecule(
        effects: &crate::ir::EffectFacts,
        parallelizable: bool,
        touches_shared: Vec<[u8; 32]>,
        estimated_cycles: u64,
        accesses: Vec<SchedulerAccess>,
    ) -> Vec<u8> {
        // The closed fact type has no fallible/custom leaf serializer. All
        // facts were validated before this current-only scheduler encoding.
        let effect_bytes = serde_json::to_vec(effects).expect("closed effect facts serialize to JSON");
        let accesses = accesses
            .into_iter()
            .map(|access| {
                let mut out = Vec::with_capacity(38);
                out.push(scheduler_operation_id(&access.operation));
                out.push(scheduler_source_id(&access.source));
                out.extend_from_slice(&access.index.to_le_bytes());
                out.extend_from_slice(&ckb_blake2b256(access.binding.as_bytes()));
                out
            })
            .collect::<Vec<_>>();

        scheduler_molecule_encode_table(&[
            0xCE11u16.to_le_bytes().to_vec(),
            vec![2],
            scheduler_molecule_encode_bytes(&effect_bytes),
            vec![u8::from(parallelizable)],
            (touches_shared.len() as u32).to_le_bytes().to_vec(),
            scheduler_molecule_encode_fixvec_byte32(&touches_shared),
            estimated_cycles.to_le_bytes().to_vec(),
            (accesses.len() as u32).to_le_bytes().to_vec(),
            scheduler_molecule_encode_fixvec_access(&accesses),
        ])
    }
}

fn scheduler_operation_id(operation: &str) -> u8 {
    match operation {
        "consume" => 1,
        "transfer" => 2,
        "destroy" => 3,
        "claim" => 4,
        "settle" => 5,
        "read_ref" => 6,
        "create" => 7,
        "mutate-input" => 8,
        "mutate-output" => 9,
        _ => 0,
    }
}

fn scheduler_source_id(source: &str) -> u8 {
    match source {
        "Input" => 1,
        "CellDep" => 2,
        "Output" => 3,
        _ => 0,
    }
}

fn scheduler_molecule_pack_number(value: usize) -> [u8; 4] {
    (value as u32).to_le_bytes()
}

fn scheduler_molecule_encode_table(fields: &[Vec<u8>]) -> Vec<u8> {
    let header_size = 4 * (fields.len() + 1);
    let total_size = header_size + fields.iter().map(Vec::len).sum::<usize>();
    let mut out = Vec::with_capacity(total_size);
    out.extend_from_slice(&scheduler_molecule_pack_number(total_size));

    let mut offset = header_size;
    for field in fields {
        out.extend_from_slice(&scheduler_molecule_pack_number(offset));
        offset += field.len();
    }
    for field in fields {
        out.extend_from_slice(field);
    }
    out
}

fn scheduler_molecule_encode_fixvec_byte32(values: &[[u8; 32]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + values.len() * 32);
    out.extend_from_slice(&scheduler_molecule_pack_number(values.len()));
    for value in values {
        out.extend_from_slice(value);
    }
    out
}

fn scheduler_molecule_encode_fixvec_access(accesses: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + accesses.iter().map(Vec::len).sum::<usize>());
    out.extend_from_slice(&scheduler_molecule_pack_number(accesses.len()));
    for access in accesses {
        out.extend_from_slice(access);
    }
    out
}

fn scheduler_molecule_encode_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(4 + bytes.len());
    result.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    result.extend_from_slice(bytes);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_std_functions() {
        let funcs = StdLib::functions();
        assert!(!funcs.is_empty());
        assert!(StdLib::is_std_function("syscall_load_cell"));
        assert!(StdLib::is_std_function("syscall_load_script"));
        assert!(StdLib::is_std_function("syscall_load_cell_by_field"));
        assert!(StdLib::is_std_function("syscall_load_cell_data"));
        assert!(StdLib::is_std_function("math_isqrt"));
    }

    #[test]
    fn test_get_function() {
        let func = StdLib::get_function("math_min");
        assert!(func.is_some());
        let func = func.unwrap();
        assert_eq!(func.params.len(), 2);
    }

    #[test]
    fn test_generate_assembly() {
        let asm = StdLib::generate_assembly();
        assert!(asm.contains("__syscall_load_cell"));
        assert!(asm.contains("__syscall_load_script:\n"));
        assert!(asm.contains("__syscall_load_cell_by_field:\n"));
        assert!(asm.contains("__syscall_load_cell_data:\n"));
        // Default is now CKB profile which uses 2052 for load_script
        assert!(asm.contains("li a7, 2052"));
        assert!(asm.contains("li a7, 2081"));
        assert!(asm.contains("li a7, 2092"));
        assert!(!asm.contains("__hash"));
        assert!(!asm.contains("3001"));
        assert!(!asm.contains("li a7, 2100"));
        assert!(asm.contains("__math_isqrt"));
    }

    #[test]
    fn test_generate_ckb_assembly_uses_ckb_load_script_syscall() {
        let asm = StdLib::generate_assembly_for_target_profile(TargetProfile::Ckb);
        assert!(asm.contains("# Syscall: load_script (2052)"));
        assert!(asm.contains("li a7, 2052"));
        assert!(!asm.contains("li a7, 2075"));
        assert!(asm.contains("current_timepoint"));
        assert!(asm.contains("__ckb_input_since"));
        assert!(asm.contains("li a7, 2083  # LOAD_INPUT_BY_FIELD"));
        assert!(asm.contains("Source::GroupInput"));
    }

    #[test]
    fn test_scheduler_metadata_generate_molecule_uses_table_layout() {
        let bytes = SchedulerMetadata::generate(
            &crate::ir::EffectFacts::lifecycle(cellscript_artifact_checker::effect_facts::LifecycleOperation::Create),
            false,
            vec![[0x42; 32]],
            64,
            vec![SchedulerAccess {
                operation: "create".to_string(),
                source: "Output".to_string(),
                index: 0,
                binding: "create:Output#0".to_string(),
            }],
        );

        assert!(!bytes.starts_with(&[0x11, 0xCE, 1]));
        assert_eq!(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize, bytes.len());
        assert_eq!(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 40);
        assert_eq!(&bytes[40..42], &[0x11, 0xCE]);
    }
}
