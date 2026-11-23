use super::error::{CompileError, Result};
use super::{ckb_blake2b256, hex_encode, is_canonical_hash_hex, CompileMetadata, CompileResult, SourceUnitMetadata};

pub(super) fn bind_source_metadata(metadata: &mut CompileMetadata, mut source_units: Vec<SourceUnitMetadata>) {
    if source_units.is_empty() {
        metadata.source_hash = None;
        metadata.source_content_hash = None;
        metadata.source_units.clear();
        return;
    }

    source_units.sort_by(|left, right| left.path.cmp(&right.path).then(left.role.cmp(&right.role)));
    metadata.source_hash = Some(compute_source_set_hash(&source_units));
    metadata.source_content_hash = Some(compute_source_content_hash(&source_units));
    metadata.source_units = source_units;
}

pub(super) fn bind_compile_result_source_metadata(result: &mut CompileResult, source_units: Vec<SourceUnitMetadata>) -> Result<()> {
    bind_source_metadata(&mut result.metadata, source_units);
    result.refresh_verified_artifact_boundary()
}

fn append_source_hash_material(out: &mut Vec<u8>, unit: &SourceUnitMetadata, include_path: bool) {
    out.extend_from_slice(unit.role.as_bytes());
    out.push(0);
    if include_path {
        out.extend_from_slice(unit.path.as_bytes());
        out.push(0);
    }
    out.extend_from_slice(unit.hash.as_bytes());
    out.push(0);
    out.extend_from_slice(&unit.size_bytes.to_le_bytes());
    out.push(0);
}

fn compute_source_set_hash(source_units: &[SourceUnitMetadata]) -> String {
    let mut material = Vec::new();
    for unit in source_units {
        append_source_hash_material(&mut material, unit, true);
    }
    hex_encode(&ckb_blake2b256(&material))
}

fn compute_source_content_hash(source_units: &[SourceUnitMetadata]) -> String {
    let mut stable_units = source_units.iter().collect::<Vec<_>>();
    stable_units.sort_by(|left, right| {
        left.role.cmp(&right.role).then(left.hash.cmp(&right.hash)).then(left.size_bytes.cmp(&right.size_bytes))
    });
    let mut material = Vec::new();
    for unit in stable_units {
        append_source_hash_material(&mut material, unit, false);
    }
    hex_encode(&ckb_blake2b256(&material))
}

pub(super) fn validate_source_metadata(metadata: &CompileMetadata) -> Result<()> {
    if metadata.source_units.is_empty() {
        if metadata.source_hash.is_some() {
            return Err(CompileError::without_span("metadata source_hash is present but source_units is empty"));
        }
        if metadata.source_content_hash.is_some() {
            return Err(CompileError::without_span("metadata source_content_hash is present but source_units is empty"));
        }
        return Ok(());
    }

    for unit in &metadata.source_units {
        if unit.path.is_empty() {
            return Err(CompileError::without_span("metadata source_units contains an empty path"));
        }
        if unit.role.is_empty() {
            return Err(CompileError::without_span(format!("metadata source unit '{}' has an empty role", unit.path)));
        }
        if !is_canonical_hash_hex(&unit.hash) {
            return Err(CompileError::without_span(format!(
                "metadata source unit '{}' has invalid hash '{}'; expected 64 lowercase hex characters",
                unit.path, unit.hash
            )));
        }
    }

    let computed_hash = compute_source_set_hash(&metadata.source_units);
    match &metadata.source_hash {
        Some(hash) if hash == &computed_hash => {}
        Some(hash) => Err(CompileError::without_span(format!(
            "metadata source_hash '{}' does not match source_units '{}'",
            hash, computed_hash
        )))?,
        None => return Err(CompileError::without_span("metadata is missing source_hash for non-empty source_units")),
    };

    let computed_content_hash = compute_source_content_hash(&metadata.source_units);
    match &metadata.source_content_hash {
        Some(hash) if hash == &computed_content_hash => Ok(()),
        Some(hash) => Err(CompileError::without_span(format!(
            "metadata source_content_hash '{}' does not match source_units '{}'",
            hash, computed_content_hash
        ))),
        None => Err(CompileError::without_span("metadata is missing source_content_hash for non-empty source_units")),
    }
}
