mod bindings;
mod checker;
pub mod code_origin;
mod elf;
pub mod entry_codec;
pub mod external_codec;
mod failure;
pub mod fixed_cell_fields;
pub mod fixed_cell_reads;
pub mod fixed_cell_storage;
mod fixed_results;
mod generic_projection;
pub mod interface;
pub mod open_handle_policy;
mod policy;
mod policy_machine;
mod schema;
mod value_abilities;
pub mod zk;
pub mod zk_profile;

pub use checker::{
    canonical_bytes, canonical_hash, check_bundle, check_bundle_values, domain_hash_bytes, parse_lowering_record, parse_source_map,
    CheckerError, CheckerRejectionCode, CheckerReport, EvidenceState,
};
pub use elf::{parse_elf, ElfSummary, ParsedElf};
pub use policy::{
    validate_policy_metadata, PolicyWitnessContract, PolicyWitnessVariant, POLICY_DISPATCH_SCHEMA, POLICY_DISPATCH_VERSION,
    POLICY_PAYLOAD_ABI, POLICY_PLACEMENT_ABI, POLICY_SELECTOR_FIELD, POLICY_WITNESS_SOURCE,
};
pub use schema::*;

pub const CKB_HASH_PERSONALIZATION: &[u8; 16] = b"ckb-default-hash";

pub fn ckb_blake2b256(data: &[u8]) -> [u8; 32] {
    let digest = blake2b_simd::Params::new().hash_length(32).personal(CKB_HASH_PERSONALIZATION).hash(data);
    let mut output = [0u8; 32];
    output.copy_from_slice(digest.as_bytes());
    output
}

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
