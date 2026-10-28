//! Removed source APIs that used a digest-shaped value as a byte buffer.
//!
//! These are migration diagnostics, not compatibility aliases. Keep the raw
//! bytes, exact-width reads and actual digest operations distinguishable.

pub(crate) fn replacement(name: &str) -> Option<&'static str> {
    match name {
        "witness::raw" => Some(
            "use witness::bounded_raw(view, maximum_bytes) for complete bounded bytes, or witness::bytes32(view, offset) for an exact 32-byte read",
        ),
        "witness::lock" => Some(
            "use witness::bounded_lock(view, maximum_bytes) for complete bounded bytes, or witness::lock_exact32(view) for an exactly 32-byte field",
        ),
        "witness::input_type" => Some(
            "use witness::bounded_entry(view, maximum_bytes), or witness::input_type_exact32(view) for an exactly 32-byte field",
        ),
        "witness::output_type" => Some(
            "use witness::bounded_output_type(view, maximum_bytes), or witness::output_type_exact32(view) for an exactly 32-byte field",
        ),
        "ckb::cell_data_hash_at" => Some("use ckb::cell_data_bytes32(view, offset), which returns raw [u8; 32] without hashing"),
        "ckb::cell_data_hash" => Some(
            "use ckb::cell_data_hash_field(view) for the consensus DATA_HASH, or ckb::cell_data_blake2b_span(view, offset, length) for a bounded raw-data digest; empty data has different results for these operations",
        ),
        "ckb::cell_lock_args_hash" => Some("use ckb::cell_lock_args32(view), which returns exactly 32 raw argument bytes"),
        "ckb::cell_type_args_hash" => Some("use ckb::cell_type_args32(view), which returns exactly 32 raw argument bytes"),
        "ckb::require_cell_lock_args_hash" => Some("use ckb::require_cell_lock_args_exact(view, expected_bytes)"),
        "ckb::require_cell_type_args_hash" => Some("use ckb::require_cell_type_args_exact(view, expected_bytes)"),
        "ckb::require_cell_lock_args_prefix_hash" => Some("use ckb::require_cell_lock_args_prefix32(view, expected_bytes)"),
        "ckb::require_cell_type_args_prefix_hash" => Some("use ckb::require_cell_type_args_prefix32(view, expected_bytes)"),
        "ckb::require_cell_lock_args_suffix_hash" => Some("use ckb::require_cell_lock_args_suffix32(view, expected_bytes)"),
        "ckb::require_cell_type_args_suffix_hash" => Some("use ckb::require_cell_type_args_suffix32(view, expected_bytes)"),
        _ => None,
    }
}

/// Old direct IR/stdlib calls must not reach the generic runtime fallback.
pub(crate) fn removed_runtime(name: &str) -> bool {
    matches!(
        name,
        "__ckb_cell_data_hash"
            | "__ckb_cell_data_hash_at"
            | "__ckb_witness_raw"
            | "__ckb_witness_lock"
            | "__ckb_witness_input_type"
            | "__ckb_witness_output_type"
            | "__ckb_cell_lock_args_hash"
            | "__ckb_cell_type_args_hash"
            | "__ckb_require_cell_lock_args_hash"
            | "__ckb_require_cell_type_args_hash"
            | "__ckb_require_cell_lock_args_prefix_hash"
            | "__ckb_require_cell_type_args_prefix_hash"
            | "__ckb_require_cell_lock_args_suffix_hash"
            | "__ckb_require_cell_type_args_suffix_hash"
    )
}
