use super::error::{CompileError, Result};
use super::schema_metadata::molecule_schema_manifest_metadata;
use super::source_provenance::validate_source_metadata;
use super::template_metadata::{
    cell_data_codec_manifest_hash, raw_cell_data_codec_accesses, template_layout_hash, template_layout_metadata,
};
use super::{ckb_blake2b256, hex_encode, is_canonical_hash_hex, CompileMetadata, TargetProfile};
use camino::Utf8Path;

pub(super) fn validate_molecule_schema_metadata(metadata: &CompileMetadata) -> Result<()> {
    for ty in &metadata.types {
        let Some(schema) = &ty.molecule_schema else {
            continue;
        };
        if schema.abi != "molecule" {
            return Err(CompileError::without_span(format!(
                "metadata type '{}' has unsupported molecule_schema.abi '{}'",
                ty.name, schema.abi
            )));
        }
        if !matches!(schema.layout.as_str(), "fixed-struct-v1" | "molecule-table-v1") {
            return Err(CompileError::without_span(format!(
                "metadata type '{}' has unsupported molecule_schema.layout '{}'",
                ty.name, schema.layout
            )));
        }
        if schema.name != ty.name {
            return Err(CompileError::without_span(format!(
                "metadata type '{}' molecule_schema.name '{}' does not match type name",
                ty.name, schema.name
            )));
        }
        match schema.layout.as_str() {
            "fixed-struct-v1" => {
                if ty.encoded_size != Some(schema.fixed_size) {
                    return Err(CompileError::without_span(format!(
                        "metadata type '{}' molecule_schema.fixed_size {} does not match encoded_size {:?}",
                        ty.name, schema.fixed_size, ty.encoded_size
                    )));
                }
            }
            "molecule-table-v1" => {
                if schema.fixed_size != 0 {
                    return Err(CompileError::without_span(format!(
                        "metadata type '{}' molecule_schema.fixed_size {} must be 0 for molecule-table-v1",
                        ty.name, schema.fixed_size
                    )));
                }
                if ty.encoded_size.is_some() {
                    return Err(CompileError::without_span(format!(
                        "metadata type '{}' uses molecule-table-v1 but encoded_size is {:?}",
                        ty.name, ty.encoded_size
                    )));
                }
            }
            _ => unreachable!("layout match guarded above"),
        }
        if schema.schema.is_empty() {
            return Err(CompileError::without_span(format!("metadata type '{}' has empty molecule_schema.schema", ty.name)));
        }
        if !is_canonical_hash_hex(&schema.schema_hash) {
            return Err(CompileError::without_span(format!(
                "metadata type '{}' has invalid molecule_schema.schema_hash '{}'; expected 64 lowercase hex characters",
                ty.name, schema.schema_hash
            )));
        }
        let expected = hex_encode(&ckb_blake2b256(schema.schema.as_bytes()));
        if schema.schema_hash != expected {
            return Err(CompileError::without_span(format!(
                "metadata type '{}' molecule_schema.schema_hash '{}' does not match schema bytes",
                ty.name, schema.schema_hash
            )));
        }
    }

    Ok(())
}

pub(super) fn validate_molecule_schema_manifest_metadata(metadata: &CompileMetadata) -> Result<()> {
    let manifest = &metadata.molecule_schema_manifest;
    if manifest.schema != "cellscript-molecule-schema-manifest-v1" {
        return Err(CompileError::without_span(format!(
            "metadata molecule_schema_manifest.schema '{}' is unsupported",
            manifest.schema
        )));
    }
    if manifest.version != 1 {
        return Err(CompileError::without_span(format!(
            "metadata molecule_schema_manifest.version {} is unsupported",
            manifest.version
        )));
    }
    if manifest.abi != "molecule" {
        return Err(CompileError::without_span(format!("metadata molecule_schema_manifest.abi '{}' is unsupported", manifest.abi)));
    }
    if manifest.target_profile != metadata.target_profile.name {
        return Err(CompileError::without_span(format!(
            "metadata molecule_schema_manifest.target_profile '{}' does not match target profile '{}'",
            manifest.target_profile, metadata.target_profile.name
        )));
    }

    let schema_types = metadata.types.iter().filter(|ty| ty.molecule_schema.is_some()).collect::<Vec<_>>();
    if manifest.type_count != schema_types.len() || manifest.entries.len() != schema_types.len() {
        return Err(CompileError::without_span(format!(
            "metadata molecule_schema_manifest.type_count {} does not match schema type count {}",
            manifest.type_count,
            schema_types.len()
        )));
    }
    if manifest.fixed_type_count + manifest.dynamic_type_count != manifest.type_count {
        return Err(CompileError::without_span("metadata molecule_schema_manifest fixed/dynamic counts do not sum to type_count"));
    }
    if !is_canonical_hash_hex(&manifest.manifest_hash) {
        return Err(CompileError::without_span(format!(
            "metadata molecule_schema_manifest.manifest_hash '{}' is invalid",
            manifest.manifest_hash
        )));
    }

    let mut entries = manifest.entries.clone();
    entries.sort_by(|left, right| left.type_name.cmp(&right.type_name));
    if entries.iter().map(|entry| entry.type_name.as_str()).collect::<Vec<_>>()
        != manifest.entries.iter().map(|entry| entry.type_name.as_str()).collect::<Vec<_>>()
    {
        return Err(CompileError::without_span("metadata molecule_schema_manifest.entries must be sorted by type_name"));
    }

    for entry in &manifest.entries {
        let Some(ty) = metadata.types.iter().find(|ty| ty.name == entry.type_name) else {
            return Err(CompileError::without_span(format!(
                "metadata molecule_schema_manifest entry '{}' does not match a metadata type",
                entry.type_name
            )));
        };
        let Some(schema) = &ty.molecule_schema else {
            return Err(CompileError::without_span(format!(
                "metadata molecule_schema_manifest entry '{}' points at a type without molecule_schema",
                entry.type_name
            )));
        };
        if entry.kind != ty.kind
            || entry.layout != schema.layout
            || entry.fixed_size != schema.fixed_size
            || entry.encoded_size != ty.encoded_size
            || entry.dynamic_fields != schema.dynamic_fields
            || entry.schema_hash != schema.schema_hash
        {
            return Err(CompileError::without_span(format!(
                "metadata molecule_schema_manifest entry '{}' does not match type molecule_schema metadata",
                entry.type_name
            )));
        }
        if entry.field_offsets.len() != ty.fields.len() {
            return Err(CompileError::without_span(format!(
                "metadata molecule_schema_manifest entry '{}' field count does not match type metadata",
                entry.type_name
            )));
        }
        for (manifest_field, ty_field) in entry.field_offsets.iter().zip(&ty.fields) {
            if manifest_field.name != ty_field.name
                || manifest_field.ty != ty_field.ty
                || manifest_field.offset != ty_field.offset
                || manifest_field.encoded_size != ty_field.encoded_size
                || manifest_field.fixed_width != ty_field.fixed_width
            {
                return Err(CompileError::without_span(format!(
                    "metadata molecule_schema_manifest entry '{}.{}' does not match type field metadata",
                    entry.type_name, manifest_field.name
                )));
            }
        }
    }

    let expected = molecule_schema_manifest_metadata(&metadata.types, TargetProfile::from_name(&metadata.target_profile.name)?);
    if manifest.manifest_hash != expected.manifest_hash {
        return Err(CompileError::without_span(format!(
            "metadata molecule_schema_manifest.manifest_hash '{}' does not match manifest entries",
            manifest.manifest_hash
        )));
    }

    Ok(())
}

pub(super) fn validate_cell_data_codec_manifest_metadata(metadata: &CompileMetadata) -> Result<()> {
    let manifest = &metadata.cell_data_codec_manifest;
    if manifest.schema != "cellscript-cell-data-codec-manifest-v1" {
        return Err(CompileError::without_span(format!(
            "metadata cell_data_codec_manifest.schema '{}' is unsupported",
            manifest.schema
        )));
    }
    if manifest.version != 1 {
        return Err(CompileError::without_span(format!(
            "metadata cell_data_codec_manifest.version {} is unsupported",
            manifest.version
        )));
    }
    if manifest.target_profile != metadata.target_profile.name {
        return Err(CompileError::without_span(format!(
            "metadata cell_data_codec_manifest.target_profile '{}' does not match target profile '{}'",
            manifest.target_profile, metadata.target_profile.name
        )));
    }
    if manifest.default_codec != "molecule" {
        return Err(CompileError::without_span(format!(
            "metadata cell_data_codec_manifest.default_codec must be 'molecule', got '{}'",
            manifest.default_codec
        )));
    }
    if manifest.molecule_schema_manifest_hash != metadata.molecule_schema_manifest.manifest_hash {
        return Err(CompileError::without_span(
            "metadata cell_data_codec_manifest.molecule_schema_manifest_hash does not match molecule_schema_manifest.manifest_hash",
        ));
    }

    let expected_raw_accesses = raw_cell_data_codec_accesses(&metadata.runtime.ckb_runtime_accesses);
    let raw_required = !expected_raw_accesses.is_empty();
    let expected_abi = if raw_required { "molecule+raw-bytes-v1" } else { "molecule" };
    if manifest.abi != expected_abi {
        return Err(CompileError::without_span(format!(
            "metadata cell_data_codec_manifest.abi must be '{}' for runtime cell-data accesses, got '{}'",
            expected_abi, manifest.abi
        )));
    }
    if manifest.raw_bytes_required != raw_required {
        return Err(CompileError::without_span(format!(
            "metadata cell_data_codec_manifest.raw_bytes_required must be {}, got {}",
            raw_required, manifest.raw_bytes_required
        )));
    }
    if manifest.raw_runtime_access_count != expected_raw_accesses.len() {
        return Err(CompileError::without_span(format!(
            "metadata cell_data_codec_manifest.raw_runtime_access_count {} does not match runtime access count {}",
            manifest.raw_runtime_access_count,
            expected_raw_accesses.len()
        )));
    }
    if manifest.raw_runtime_accesses != expected_raw_accesses {
        return Err(CompileError::without_span(
            "metadata cell_data_codec_manifest.raw_runtime_accesses do not match runtime.ckb_runtime_accesses",
        ));
    }
    for (index, adapter) in manifest.external_codec_adapters.iter().enumerate() {
        let prefix = format!("metadata cell_data_codec_manifest.external_codec_adapters[{index}]");
        if adapter.name.trim().is_empty() {
            return Err(CompileError::without_span(format!("{prefix}.name must be non-empty")));
        }
        if adapter.version.trim().is_empty() {
            return Err(CompileError::without_span(format!("{prefix}.version must be non-empty")));
        }
        if adapter.package.trim().is_empty() {
            return Err(CompileError::without_span(format!("{prefix}.package must be non-empty")));
        }
        if !is_canonical_hash_hex(adapter.content_hash.strip_prefix("0x").unwrap_or(adapter.content_hash.as_str())) {
            return Err(CompileError::without_span(format!(
                "{prefix}.content_hash '{}' must be a canonical 32-byte hex hash",
                adapter.content_hash
            )));
        }
        for (vector_index, hash) in adapter.test_vector_hashes.iter().enumerate() {
            if !is_canonical_hash_hex(hash.strip_prefix("0x").unwrap_or(hash.as_str())) {
                return Err(CompileError::without_span(format!(
                    "{prefix}.test_vector_hashes[{vector_index}] '{}' must be a canonical 32-byte hex hash",
                    hash
                )));
            }
        }
    }
    if !is_canonical_hash_hex(&manifest.manifest_hash) {
        return Err(CompileError::without_span(format!(
            "metadata cell_data_codec_manifest.manifest_hash '{}' is invalid",
            manifest.manifest_hash
        )));
    }
    let expected_hash = cell_data_codec_manifest_hash(manifest);
    if manifest.manifest_hash != expected_hash {
        return Err(CompileError::without_span(format!(
            "metadata cell_data_codec_manifest.manifest_hash '{}' does not match manifest entries",
            manifest.manifest_hash
        )));
    }

    Ok(())
}

pub(super) fn validate_template_layout_metadata(metadata: &CompileMetadata) -> Result<()> {
    let expected = template_layout_metadata(&metadata.types);
    if metadata.template_layouts.len() != expected.len() {
        return Err(CompileError::without_span(format!(
            "metadata template_layouts length {} does not match derived type layout count {}",
            metadata.template_layouts.len(),
            expected.len()
        )));
    }

    let mut sorted = metadata.template_layouts.clone();
    sorted.sort_by(|left, right| left.type_name.cmp(&right.type_name));
    if sorted != metadata.template_layouts {
        return Err(CompileError::without_span("metadata template_layouts must be sorted by type_name"));
    }

    for (layout, expected_layout) in metadata.template_layouts.iter().zip(expected.iter()) {
        if layout.schema != "cellscript-template-layout-v0.21" {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' has unsupported schema '{}'",
                layout.type_name, layout.schema
            )));
        }
        if layout.version != 1 {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' has unsupported version {}",
                layout.type_name, layout.version
            )));
        }
        if !matches!(layout.scope.as_str(), "Type" | "TypeFamily") {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' has unsupported scope '{}'",
                layout.type_name, layout.scope
            )));
        }
        if !matches!(layout.layout.as_str(), "Flat" | "MerkleCandidate") {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' has unsupported layout '{}'",
                layout.type_name, layout.layout
            )));
        }
        if !matches!(layout.cycle_policy.as_str(), "RootRequired" | "PathOnlyAllowed") {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' has unsupported cycle_policy '{}'",
                layout.type_name, layout.cycle_policy
            )));
        }
        if layout.consensus_checked {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' cannot set consensus_checked=true until a backend verifier supports TemplateLayout commitments",
                layout.type_name
            )));
        }
        if let Some(root_hash) = layout.root_hash.as_deref()
            && !is_canonical_hash_hex(root_hash)
        {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' root_hash '{}' is not a canonical 32-byte lowercase hex hash",
                layout.type_name, root_hash
            )));
        }
        if !is_canonical_hash_hex(&layout.template_layout_hash) {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' template_layout_hash '{}' is invalid",
                layout.type_name, layout.template_layout_hash
            )));
        }
        if layout.template_layout_hash != template_layout_hash(layout) {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' template_layout_hash '{}' does not match layout fields",
                layout.type_name, layout.template_layout_hash
            )));
        }
        if layout != expected_layout {
            return Err(CompileError::without_span(format!(
                "metadata template_layout '{}' does not match derived type metadata",
                layout.type_name
            )));
        }
    }

    Ok(())
}

pub fn validate_source_units_on_disk(metadata: &CompileMetadata) -> Result<()> {
    validate_source_metadata(metadata)?;
    if metadata.source_units.is_empty() {
        return Err(CompileError::without_span("metadata has no source_units to verify"));
    }

    for unit in &metadata.source_units {
        if unit.path.starts_with('<') && unit.path.ends_with('>') {
            return Err(CompileError::without_span(format!(
                "source unit '{}' is not backed by a disk file and cannot be verified",
                unit.path
            )));
        }

        let path = Utf8Path::new(&unit.path);
        let bytes = std::fs::read(path)
            .map_err(|error| CompileError::without_span(format!("failed to read source unit '{}': {}", unit.path, error)))?;
        if bytes.len() != unit.size_bytes {
            return Err(CompileError::without_span(format!(
                "source unit '{}' size {} does not match metadata size {}",
                unit.path,
                bytes.len(),
                unit.size_bytes
            )));
        }

        let hash = hex_encode(&ckb_blake2b256(&bytes));
        if hash != unit.hash {
            return Err(CompileError::without_span(format!(
                "source unit '{}' hash '{}' does not match metadata hash '{}'",
                unit.path, hash, unit.hash
            )));
        }
    }

    Ok(())
}
