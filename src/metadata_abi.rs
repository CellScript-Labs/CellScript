//! Shared metadata-only ABI identity for native and browser package tooling.
use crate::error::{CompileError, Result};
use crate::{ckb_blake2b256, hex_encode, CompileMetadata};

/// Canonical ABI digest already used by package locks and Registry records.
/// Exact handles reuse this identity instead of defining a parallel ABI hash.
pub fn compile_metadata_abi_hash(metadata: &CompileMetadata) -> Result<String> {
    let abi = serde_json::json!({
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": {
            "metadata": metadata.metadata_schema_version,
            "source": metadata.source_metadata_schema_version,
            "artifact": metadata.artifact_metadata_schema_version,
            "constraints": metadata.constraints_metadata_schema_version,
        },
        "edition": metadata.edition,
        "compatibility_profile": &metadata.compatibility_profile,
        "target_profile": metadata.target_profile.name.as_str(),
        "types": &metadata.types,
        "actions": &metadata.actions,
        "functions": &metadata.functions,
        "locks": &metadata.locks,
        "molecule_schema_manifest": &metadata.molecule_schema_manifest,
        "cell_data_codec_manifest": &metadata.cell_data_codec_manifest,
    });
    let bytes = serde_json::to_vec(&abi)
        .map_err(|error| CompileError::without_span(format!("failed to serialize compile metadata ABI for digest: {error}")))?;
    Ok(hex_encode(&ckb_blake2b256(&bytes)))
}
