use super::{
    ckb_blake2b256, hex_encode, CellDataCodecManifestMetadata, CellDataCodecRuntimeAccessMetadata, CkbRuntimeAccessMetadata,
    FlowTransitionMetadata, MoleculeSchemaManifestMetadata, TargetProfile, TemplateLayoutLeafSchemaMetadata, TemplateLayoutMetadata,
    TypeMetadata,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn template_layout_metadata(types: &[TypeMetadata]) -> Vec<TemplateLayoutMetadata> {
    let mut layouts = types
        .iter()
        .filter(|ty| matches!(ty.kind.as_str(), "Resource" | "Shared" | "Receipt"))
        .filter_map(template_layout_for_type)
        .collect::<Vec<_>>();
    layouts.sort_by(|left, right| left.type_name.cmp(&right.type_name));
    layouts
}

fn template_layout_for_type(ty: &TypeMetadata) -> Option<TemplateLayoutMetadata> {
    let molecule_schema = ty.molecule_schema.as_ref()?;
    let state_machine_acyclic = flow_transitions_are_acyclic(&ty.flow_transitions);
    let cycle_policy = if state_machine_acyclic { "PathOnlyAllowed" } else { "RootRequired" };
    let leaf_schema = TemplateLayoutLeafSchemaMetadata {
        abi: molecule_schema.abi.clone(),
        layout: molecule_schema.layout.clone(),
        schema_hash: molecule_schema.schema_hash.clone(),
        encoded_size: ty.encoded_size,
        field_count: ty.fields.len(),
    };
    let mut layout = TemplateLayoutMetadata {
        schema: "cellscript-template-layout-v0.21".to_string(),
        version: 1,
        scope: "Type".to_string(),
        type_name: ty.name.clone(),
        state_field: ty.flow_state_field.clone(),
        state_machine_acyclic,
        cycle_policy: cycle_policy.to_string(),
        layout: "Flat".to_string(),
        root_hash: None,
        leaf_schema,
        consensus_checked: false,
        template_layout_hash: String::new(),
    };
    layout.template_layout_hash = template_layout_hash(&layout);
    Some(layout)
}

fn flow_transitions_are_acyclic(transitions: &[FlowTransitionMetadata]) -> bool {
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for transition in transitions {
        adjacency.entry(&transition.from).or_default().push(&transition.to);
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for node in adjacency.keys() {
        if !flow_transition_visit_is_acyclic(node, &adjacency, &mut visiting, &mut visited) {
            return false;
        }
    }
    true
}

fn flow_transition_visit_is_acyclic<'a>(
    node: &'a str,
    adjacency: &BTreeMap<&'a str, Vec<&'a str>>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> bool {
    if visited.contains(node) {
        return true;
    }
    if !visiting.insert(node) {
        return false;
    }
    if let Some(targets) = adjacency.get(node) {
        for target in targets {
            if !flow_transition_visit_is_acyclic(target, adjacency, visiting, visited) {
                return false;
            }
        }
    }
    visiting.remove(node);
    visited.insert(node);
    true
}

pub(super) fn template_layout_hash(layout: &TemplateLayoutMetadata) -> String {
    let mut canonical = String::new();
    canonical.push_str(&layout.schema);
    canonical.push('|');
    canonical.push_str(&layout.version.to_string());
    canonical.push('|');
    canonical.push_str(&layout.scope);
    canonical.push('|');
    canonical.push_str(&layout.type_name);
    canonical.push('|');
    canonical.push_str(layout.state_field.as_deref().unwrap_or(""));
    canonical.push('|');
    canonical.push_str(if layout.state_machine_acyclic { "acyclic" } else { "cyclic" });
    canonical.push('|');
    canonical.push_str(&layout.cycle_policy);
    canonical.push('|');
    canonical.push_str(&layout.layout);
    canonical.push('|');
    canonical.push_str(layout.root_hash.as_deref().unwrap_or(""));
    canonical.push('|');
    canonical.push_str(&layout.leaf_schema.abi);
    canonical.push('|');
    canonical.push_str(&layout.leaf_schema.layout);
    canonical.push('|');
    canonical.push_str(&layout.leaf_schema.schema_hash);
    canonical.push('|');
    canonical.push_str(&layout.leaf_schema.encoded_size.map(|size| size.to_string()).unwrap_or_else(|| "dynamic".to_string()));
    canonical.push('|');
    canonical.push_str(&layout.leaf_schema.field_count.to_string());
    canonical.push('|');
    canonical.push_str(if layout.consensus_checked { "consensus" } else { "metadata-only" });
    hex_encode(&ckb_blake2b256(canonical.as_bytes()))
}

pub(super) fn cell_data_codec_manifest_metadata(
    accesses: &[CkbRuntimeAccessMetadata],
    molecule_schema_manifest: &MoleculeSchemaManifestMetadata,
    target_profile: TargetProfile,
) -> CellDataCodecManifestMetadata {
    let raw_runtime_accesses = raw_cell_data_codec_accesses(accesses);
    let raw_bytes_required = !raw_runtime_accesses.is_empty();
    let external_codec_adapters = Vec::new();
    let mut manifest = CellDataCodecManifestMetadata {
        schema: "cellscript-cell-data-codec-manifest-v1".to_string(),
        version: 1,
        target_profile: target_profile.name().to_string(),
        abi: if raw_bytes_required { "molecule+raw-bytes-v1" } else { "molecule" }.to_string(),
        default_codec: "molecule".to_string(),
        molecule_schema_manifest_hash: molecule_schema_manifest.manifest_hash.clone(),
        raw_bytes_required,
        raw_runtime_access_count: raw_runtime_accesses.len(),
        raw_runtime_accesses,
        external_codec_adapters,
        manifest_hash: String::new(),
    };
    manifest.manifest_hash = cell_data_codec_manifest_hash(&manifest);
    manifest
}

pub(super) fn raw_cell_data_codec_accesses(accesses: &[CkbRuntimeAccessMetadata]) -> Vec<CellDataCodecRuntimeAccessMetadata> {
    let mut raw = accesses
        .iter()
        .filter(|access| runtime_access_uses_raw_cell_data(access))
        .map(|access| CellDataCodecRuntimeAccessMetadata {
            operation: access.operation.clone(),
            syscall: access.syscall.clone(),
            source: access.source.clone(),
            index: access.index,
            binding: access.binding.clone(),
            codec: "raw-bytes-v1".to_string(),
        })
        .collect::<Vec<_>>();
    raw.sort_by(|left, right| {
        (left.operation.as_str(), left.syscall.as_str(), left.source.as_str(), left.index, left.binding.as_str()).cmp(&(
            right.operation.as_str(),
            right.syscall.as_str(),
            right.source.as_str(),
            right.index,
            right.binding.as_str(),
        ))
    });
    raw
}

fn runtime_access_uses_raw_cell_data(access: &CkbRuntimeAccessMetadata) -> bool {
    access.syscall.split('+').any(|part| part == "LOAD_CELL_DATA") || access.syscall.split('/').any(|part| part == "LOAD_CELL_DATA")
}

pub(super) fn cell_data_codec_manifest_hash(manifest: &CellDataCodecManifestMetadata) -> String {
    let mut canonical = String::new();
    canonical.push_str(&manifest.schema);
    canonical.push('|');
    canonical.push_str(&manifest.version.to_string());
    canonical.push('|');
    canonical.push_str(&manifest.target_profile);
    canonical.push('|');
    canonical.push_str(&manifest.abi);
    canonical.push('|');
    canonical.push_str(&manifest.default_codec);
    canonical.push('|');
    canonical.push_str(&manifest.molecule_schema_manifest_hash);
    canonical.push('|');
    canonical.push_str(if manifest.raw_bytes_required { "raw-required" } else { "raw-not-required" });
    canonical.push('|');
    canonical.push_str(&manifest.raw_runtime_access_count.to_string());
    canonical.push('\n');
    for access in &manifest.raw_runtime_accesses {
        canonical.push_str(&access.operation);
        canonical.push('|');
        canonical.push_str(&access.syscall);
        canonical.push('|');
        canonical.push_str(&access.source);
        canonical.push('|');
        canonical.push_str(&access.index.to_string());
        canonical.push('|');
        canonical.push_str(&access.binding);
        canonical.push('|');
        canonical.push_str(&access.codec);
        canonical.push('\n');
    }
    for adapter in &manifest.external_codec_adapters {
        canonical.push_str(&adapter.name);
        canonical.push('|');
        canonical.push_str(&adapter.version);
        canonical.push('|');
        canonical.push_str(&adapter.package);
        canonical.push('|');
        canonical.push_str(&adapter.content_hash);
        canonical.push('|');
        canonical.push_str(&adapter.test_vector_hashes.join(","));
        canonical.push('\n');
    }
    hex_encode(&ckb_blake2b256(canonical.as_bytes()))
}
