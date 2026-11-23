use super::proof_plan::{EvidenceTier, ProofPlanSourceSpanMetadata};
use super::{
    ast, ckb_blake2b256, hex_encode, ir, nested_values, CkbTypeIdMetadata, FieldMetadata, FlowTransitionMetadata,
    MoleculeSchemaManifestEntryMetadata, MoleculeSchemaManifestFieldMetadata, MoleculeSchemaManifestMetadata, MoleculeSchemaMetadata,
    TargetProfile, TypeMetadata, ValidityPredicateMetadata, CKB_TYPE_ID_ABI, CKB_TYPE_ID_ARGS_SOURCE, CKB_TYPE_ID_BUILDER,
    CKB_TYPE_ID_CODE_HASH, CKB_TYPE_ID_GROUP_RULE, CKB_TYPE_ID_HASH_TYPE, CKB_TYPE_ID_VERIFIER,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn type_metadata(
    type_def: &ir::IrTypeDef,
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
    target_profile: TargetProfile,
    create_paths_selected: usize,
    create_paths_checked: usize,
    update_paths_selected: usize,
) -> TypeMetadata {
    let flow_states = type_def.flow_states.clone().unwrap_or_default();
    let type_id = type_def.type_id.clone();
    let type_id_hash = type_id.as_ref().map(|value| hex_encode(&ckb_blake2b256(value.as_bytes())));
    let ckb_type_id = ckb_type_id_metadata(type_def, target_profile);
    TypeMetadata {
        name: type_def.name.clone(),
        type_id,
        default_hash_type: type_def.default_hash_type.clone(),
        hash_type_source: if type_def.default_hash_type.is_some() {
            "dsl-with_default_hash_type".to_string()
        } else {
            "target-default".to_string()
        },
        capacity_floor_shannons: type_def.capacity_floor_shannons,
        capacity_floor_source: type_def.capacity_floor_shannons.map(|_| "dsl-with_capacity_floor".to_string()),
        type_id_hash,
        ckb_type_id,
        kind: format!("{:?}", type_def.kind),
        capability_set_version: ast::Capability::REGISTRY_VERSION,
        capabilities: ast::Capability::ALL
            .into_iter()
            .filter(|capability| type_def.capabilities.contains(capability))
            .map(|capability| metadata_capability_name(&capability))
            .collect(),
        claim_output: type_def.claim_output.as_ref().map(ir_type_to_string),
        flow_state_field: type_def.flow_state_field.clone(),
        flow_transitions: if type_def.flow_rules.is_empty() {
            flow_transition_metadata(&flow_states)
        } else {
            type_def.flow_rules.iter().map(flow_rule_metadata).collect()
        },
        flow_initial_state: type_def.flow_initial_state.clone(),
        flow_terminal_states: type_def.flow_terminal_states.clone(),
        flow_terminal_discharge: type_def.flow_terminal_discharge.clone(),
        flow_terminal_evidence_tier: type_def.flow_terminal_discharge.as_ref().map(|_| EvidenceTier::CheckedRuntime),
        flow_state_model: type_def.flow_state_model.clone(),
        flow_audit_warnings: type_def.flow_audit_warnings.clone(),
        validity_predicates: type_def
            .validity_predicates
            .iter()
            .map(|predicate| {
                validity_predicate_metadata(type_def, predicate, create_paths_selected, create_paths_checked, update_paths_selected)
            })
            .collect(),
        flow_states,
        encoded_size: type_encoded_size(type_def, type_defs),
        fields: fields_metadata(type_def, type_defs),
        molecule_schema: type_molecule_schema_metadata(type_def, type_defs),
        identity_policy: metadata_identity_policy(&type_def.identity),
    }
}

fn validity_predicate_metadata(
    type_def: &ir::IrTypeDef,
    predicate: &ir::IrValidityPredicate,
    create_paths_selected: usize,
    create_paths_checked: usize,
    update_paths_selected: usize,
) -> ValidityPredicateMetadata {
    let all_create_paths_checked = create_paths_selected > 0 && create_paths_checked == create_paths_selected;
    let predicate_create_paths_checked = match predicate.evidence {
        ir::IrValidityEvidence::CheckedStatic => create_paths_selected,
        ir::IrValidityEvidence::CheckedRuntime => create_paths_checked,
        ir::IrValidityEvidence::BuilderEvidenceRequired => 0,
    };
    let evidence_tier = match predicate.evidence {
        ir::IrValidityEvidence::CheckedStatic => EvidenceTier::CheckedStatic,
        ir::IrValidityEvidence::CheckedRuntime => {
            if all_create_paths_checked {
                EvidenceTier::CheckedRuntime
            } else {
                EvidenceTier::RuntimeHelperRequired
            }
        }
        ir::IrValidityEvidence::BuilderEvidenceRequired => EvidenceTier::BuilderEvidenceRequired,
    };
    let create_path_status = match predicate.evidence {
        ir::IrValidityEvidence::CheckedStatic => "checked-static",
        ir::IrValidityEvidence::CheckedRuntime if all_create_paths_checked => "checked-runtime",
        ir::IrValidityEvidence::CheckedRuntime if create_paths_selected == 0 => "not-selected-runtime-helper-required",
        ir::IrValidityEvidence::CheckedRuntime if create_paths_checked == 0 => "selected-runtime-helper-required",
        ir::IrValidityEvidence::CheckedRuntime => "partial-runtime-helper-required",
        ir::IrValidityEvidence::BuilderEvidenceRequired => "builder-header-evidence-required",
    };
    let update_path_status = match predicate.evidence {
        ir::IrValidityEvidence::CheckedStatic => "checked-static",
        ir::IrValidityEvidence::BuilderEvidenceRequired => "builder-header-evidence-required",
        ir::IrValidityEvidence::CheckedRuntime if update_paths_selected > 0 => "runtime-helper-required",
        ir::IrValidityEvidence::CheckedRuntime if type_def.kind == ir::IrTypeKind::Struct => "local-struct-not-cell-update",
        ir::IrValidityEvidence::CheckedRuntime => "not-selected",
    };
    ValidityPredicateMetadata {
        expression: predicate.rendered.clone(),
        evidence_tier,
        dependencies: predicate.dependencies.clone(),
        runtime_checked_on_create: predicate.runtime_checked_on_create && all_create_paths_checked,
        create_paths_selected,
        create_paths_checked: predicate_create_paths_checked,
        update_paths_selected,
        create_path_status: create_path_status.to_string(),
        update_path_status: update_path_status.to_string(),
        source_span: ProofPlanSourceSpanMetadata {
            start: predicate.span.start,
            end: predicate.span.end,
            line: predicate.span.line,
            column: predicate.span.column,
        },
    }
}

pub(super) fn ckb_type_id_metadata(type_def: &ir::IrTypeDef, target_profile: TargetProfile) -> Option<CkbTypeIdMetadata> {
    if !target_profile.is_ckb() || type_def.type_id.is_none() {
        return None;
    }
    if !matches!(type_def.kind, ir::IrTypeKind::Resource | ir::IrTypeKind::Shared | ir::IrTypeKind::Receipt) {
        return None;
    }

    Some(CkbTypeIdMetadata {
        abi: CKB_TYPE_ID_ABI.to_string(),
        script_code_hash: hex_encode(&CKB_TYPE_ID_CODE_HASH),
        hash_type: CKB_TYPE_ID_HASH_TYPE.to_string(),
        args_source: CKB_TYPE_ID_ARGS_SOURCE.to_string(),
        group_rule: CKB_TYPE_ID_GROUP_RULE.to_string(),
        builder: CKB_TYPE_ID_BUILDER.to_string(),
        verifier: CKB_TYPE_ID_VERIFIER.to_string(),
    })
}

pub(super) fn type_molecule_schema_metadata(
    type_def: &ir::IrTypeDef,
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
) -> Option<MoleculeSchemaMetadata> {
    let encoded_size = type_encoded_size(type_def, type_defs);
    let fixed_size = encoded_size.unwrap_or(0);
    let mut aliases = BTreeMap::new();
    let mut structs = Vec::new();
    let mut emitted = BTreeSet::new();
    let root_type = if encoded_size.is_some() {
        molecule_struct_for_type(type_def, type_defs, &mut aliases, &mut structs, &mut emitted, &mut BTreeSet::new())?
    } else {
        molecule_table_for_type(type_def, type_defs, &mut aliases, &mut structs, &mut emitted, &mut BTreeSet::new())?
    };

    let mut schema = String::new();
    for definition in ordered_molecule_alias_definitions(&aliases) {
        schema.push_str(definition);
        schema.push('\n');
    }
    if !aliases.is_empty() {
        schema.push('\n');
    }
    for definition in structs {
        schema.push_str(&definition);
        schema.push('\n');
    }
    debug_assert_eq!(root_type, molecule_identifier(&type_def.name));

    let schema_hash = hex_encode(&ckb_blake2b256(schema.as_bytes()));
    Some(MoleculeSchemaMetadata {
        abi: "molecule".to_string(),
        layout: if encoded_size.is_some() { "fixed-struct-v1" } else { "molecule-table-v1" }.to_string(),
        name: type_def.name.clone(),
        dynamic_fields: if encoded_size.is_some() { Vec::new() } else { molecule_dynamic_fields(type_def, type_defs) },
        fixed_size,
        schema_hash,
        schema,
    })
}

pub(super) fn molecule_schema_manifest_metadata(
    types: &[TypeMetadata],
    target_profile: TargetProfile,
) -> MoleculeSchemaManifestMetadata {
    let mut entries = types
        .iter()
        .filter_map(|ty| {
            let schema = ty.molecule_schema.as_ref()?;
            Some(MoleculeSchemaManifestEntryMetadata {
                type_name: ty.name.clone(),
                kind: ty.kind.clone(),
                layout: schema.layout.clone(),
                fixed_size: schema.fixed_size,
                encoded_size: ty.encoded_size,
                dynamic_fields: schema.dynamic_fields.clone(),
                schema_hash: schema.schema_hash.clone(),
                field_offsets: ty
                    .fields
                    .iter()
                    .map(|field| MoleculeSchemaManifestFieldMetadata {
                        name: field.name.clone(),
                        ty: field.ty.clone(),
                        offset: field.offset,
                        encoded_size: field.encoded_size,
                        fixed_width: field.fixed_width,
                    })
                    .collect(),
                target_profile_compatible: true,
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.type_name.cmp(&right.type_name));

    let fixed_type_count = entries.iter().filter(|entry| entry.layout == "fixed-struct-v1").count();
    let dynamic_type_count = entries.iter().filter(|entry| entry.layout == "molecule-table-v1").count();
    let mut canonical = String::new();
    for entry in &entries {
        canonical.push_str(&entry.type_name);
        canonical.push('|');
        canonical.push_str(&entry.kind);
        canonical.push('|');
        canonical.push_str(&entry.layout);
        canonical.push('|');
        canonical.push_str(&entry.fixed_size.to_string());
        canonical.push('|');
        canonical.push_str(&entry.encoded_size.map(|size| size.to_string()).unwrap_or_else(|| "dynamic".to_string()));
        canonical.push('|');
        canonical.push_str(&entry.dynamic_fields.join(","));
        canonical.push('|');
        canonical.push_str(&entry.schema_hash);
        canonical.push('\n');
        for field in &entry.field_offsets {
            canonical.push_str("  ");
            canonical.push_str(&field.name);
            canonical.push('|');
            canonical.push_str(&field.ty);
            canonical.push('|');
            canonical.push_str(&field.offset.to_string());
            canonical.push('|');
            canonical.push_str(&field.encoded_size.map(|size| size.to_string()).unwrap_or_else(|| "dynamic".to_string()));
            canonical.push('|');
            canonical.push_str(if field.fixed_width { "fixed" } else { "dynamic" });
            canonical.push('\n');
        }
    }

    MoleculeSchemaManifestMetadata {
        schema: "cellscript-molecule-schema-manifest-v1".to_string(),
        version: 1,
        abi: "molecule".to_string(),
        target_profile: target_profile.name().to_string(),
        type_count: entries.len(),
        fixed_type_count,
        dynamic_type_count,
        manifest_hash: hex_encode(&ckb_blake2b256(canonical.as_bytes())),
        entries,
    }
}

fn molecule_dynamic_fields(type_def: &ir::IrTypeDef, type_defs: &BTreeMap<String, &ir::IrTypeDef>) -> Vec<String> {
    type_def
        .fields
        .iter()
        .filter(|field| ir_type_encoded_size(&field.ty, type_defs, &mut BTreeSet::new()).or(field.fixed_size).is_none())
        .map(|field| field.name.clone())
        .collect()
}

fn molecule_struct_for_type(
    type_def: &ir::IrTypeDef,
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
    aliases: &mut BTreeMap<String, String>,
    structs: &mut Vec<String>,
    emitted: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> Option<String> {
    if emitted.contains(&type_def.name) {
        return Some(molecule_identifier(&type_def.name));
    }
    if !visiting.insert(type_def.name.clone()) {
        return None;
    }

    let result = (|| {
        let mut fields = Vec::new();
        for field in &type_def.fields {
            let ty = molecule_type_for_ir_type(&field.ty, &type_def.name, &field.name, type_defs, aliases, structs, emitted, visiting)
                .or_else(|| {
                    field.fixed_size.map(|size| {
                        if size == 1 {
                            molecule_fixed_array_alias(aliases, "CellScriptEnumTag", "byte", 1)
                        } else {
                            let alias =
                                format!("{}{}FixedBytes", molecule_identifier(&type_def.name), molecule_identifier(&field.name));
                            molecule_fixed_array_alias(aliases, &alias, "byte", size)
                        }
                    })
                })
                .or_else(|| {
                    (field.fixed_size.is_none()).then(|| {
                        let alias = format!("{}{}Bytes", molecule_identifier(&type_def.name), molecule_identifier(&field.name));
                        molecule_bytes_alias(aliases, &alias)
                    })
                })?;
            fields.push((field.name.clone(), ty));
        }

        let name = molecule_identifier(&type_def.name);
        let mut definition = format!("struct {} {{\n", name);
        for (field, ty) in fields {
            definition.push_str(&format!("    {}: {},\n", field, ty));
        }
        definition.push_str("}\n");
        structs.push(definition);
        emitted.insert(type_def.name.clone());
        Some(name)
    })();

    visiting.remove(&type_def.name);
    result
}

fn molecule_table_for_type(
    type_def: &ir::IrTypeDef,
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
    aliases: &mut BTreeMap<String, String>,
    structs: &mut Vec<String>,
    emitted: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> Option<String> {
    let name = molecule_identifier(&type_def.name);
    if emitted.contains(&name) {
        return Some(name);
    }
    if !visiting.insert(type_def.name.clone()) {
        return None;
    }

    let result = (|| {
        let mut fields = Vec::new();
        for field in &type_def.fields {
            let ty = molecule_type_for_ir_type(&field.ty, &type_def.name, &field.name, type_defs, aliases, structs, emitted, visiting)
                .or_else(|| {
                    field.fixed_size.map(|size| {
                        if size == 1 {
                            molecule_fixed_array_alias(aliases, "CellScriptEnumTag", "byte", 1)
                        } else {
                            let alias =
                                format!("{}{}FixedBytes", molecule_identifier(&type_def.name), molecule_identifier(&field.name));
                            molecule_fixed_array_alias(aliases, &alias, "byte", size)
                        }
                    })
                })
                .or_else(|| {
                    (field.fixed_size.is_none()).then(|| {
                        let alias = format!("{}{}Bytes", molecule_identifier(&type_def.name), molecule_identifier(&field.name));
                        molecule_bytes_alias(aliases, &alias)
                    })
                })?;
            fields.push((field.name.clone(), ty));
        }

        let mut definition = format!("table {} {{\n", name);
        for (field, ty) in fields {
            definition.push_str(&format!("    {}: {},\n", field, ty));
        }
        definition.push_str("}\n");
        structs.push(definition);
        emitted.insert(name.clone());
        Some(name)
    })();

    visiting.remove(&type_def.name);
    result
}

fn molecule_type_for_ir_type(
    ty: &ir::IrType,
    owner: &str,
    field: &str,
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
    aliases: &mut BTreeMap<String, String>,
    structs: &mut Vec<String>,
    emitted: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> Option<String> {
    match ty {
        ir::IrType::OpenHandle(_) | ir::IrType::WitnessBytes(_) | ir::IrType::Selection(_) => None,
        ir::IrType::Vector(element) => {
            if let Some(element) = element {
                let inner = molecule_type_for_ir_type(element, owner, field, type_defs, aliases, structs, emitted, visiting)?;
                let alias = format!("{}{}Vec", molecule_identifier(owner), molecule_identifier(field));
                Some(molecule_dynvec_alias(aliases, &alias, &inner))
            } else {
                Some(molecule_dynvec_alias(aliases, "CellScriptUnknownVec", "byte"))
            }
        }
        ir::IrType::U8 => Some("byte".to_string()),
        ir::IrType::U16 => Some(molecule_fixed_array_alias(aliases, "CellScriptUint16", "byte", 2)),
        ir::IrType::U32 => Some(molecule_fixed_array_alias(aliases, "CellScriptUint32", "byte", 4)),
        ir::IrType::I32 => Some(molecule_fixed_array_alias(aliases, "CellScriptInt32", "byte", 4)),
        ir::IrType::U64 => Some(molecule_fixed_array_alias(aliases, "CellScriptUint64", "byte", 8)),
        ir::IrType::U128 => Some(molecule_fixed_array_alias(aliases, "CellScriptUint128", "byte", 16)),
        ir::IrType::Bool => Some(molecule_fixed_array_alias(aliases, "CellScriptBool", "byte", 1)),
        ir::IrType::Address => Some(molecule_fixed_array_alias(aliases, "CellScriptAddress", "byte", 32)),
        ir::IrType::Hash => Some(molecule_fixed_array_alias(aliases, "CellScriptHash", "byte", 32)),
        ir::IrType::Named(name) if name == "String" => Some(molecule_bytes_alias(aliases, "CellScriptString")),
        ir::IrType::Nested(view) if matches!(view.description(), nested_values::NestedType::Values(_)) => {
            let nested_values::NestedType::Values(values) = view.description() else { return None };
            let item_field = format!("{field}NestedItem");
            let element = match &values.element {
                nested_values::Element::Bytes => ir::IrType::vector(ir::IrType::U8),
                nested_values::Element::Record(record) => ir::IrType::Named(record.spelling().to_owned()),
            };
            let item = molecule_type_for_ir_type(&element, owner, &item_field, type_defs, aliases, structs, emitted, visiting)?;
            // Bounds are part of the schema identity even though Molecule's
            // vector declaration itself does not express count/byte limits.
            let alias = format!(
                "{}{}NestedN{}P{}B{}",
                molecule_identifier(owner),
                molecule_identifier(field),
                values.max_items,
                values.max_payload_bytes,
                values.max_total_bytes
            );
            Some(molecule_dynvec_alias(aliases, &alias, &item))
        }
        ir::IrType::Array(inner, len) => {
            let inner_ty = molecule_type_for_ir_type(inner, owner, field, type_defs, aliases, structs, emitted, visiting)?;
            let alias = format!("{}{}Array{}", molecule_identifier(owner), molecule_identifier(field), len);
            Some(molecule_fixed_array_alias(aliases, &alias, &inner_ty, *len))
        }
        ir::IrType::Tuple(items) => {
            let tuple_name = format!("CellScriptTuple{}{}", molecule_identifier(owner), molecule_identifier(field));
            molecule_tuple_struct_for_items(&tuple_name, items, type_defs, aliases, structs, emitted, visiting)
        }
        ir::IrType::Named(name) => {
            let type_def = type_defs.get(name.as_str())?;
            if type_encoded_size(type_def, type_defs).is_some() {
                molecule_struct_for_type(type_def, type_defs, aliases, structs, emitted, visiting)
            } else {
                molecule_table_for_type(type_def, type_defs, aliases, structs, emitted, visiting)
            }
        }
        ir::IrType::Unit
        | ir::IrType::Commitment(_)
        | ir::IrType::Opening(_)
        | ir::IrType::Bounded(_)
        | ir::IrType::Context(_)
        | ir::IrType::Nested(_)
        | ir::IrType::Applied(_)
        | ir::IrType::Ref(_)
        | ir::IrType::MutRef(_) => None,
    }
}

fn molecule_tuple_struct_for_items(
    name: &str,
    items: &[ir::IrType],
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
    aliases: &mut BTreeMap<String, String>,
    structs: &mut Vec<String>,
    emitted: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    let name = molecule_identifier(name);
    if emitted.contains(&name) {
        return Some(name);
    }

    let mut fields = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let field = format!("item{}", index);
        let ty = molecule_type_for_ir_type(item, &name, &field, type_defs, aliases, structs, emitted, visiting)?;
        fields.push((field, ty));
    }

    let mut definition = format!("struct {} {{\n", name);
    for (field, ty) in fields {
        definition.push_str(&format!("    {}: {},\n", field, ty));
    }
    definition.push_str("}\n");
    structs.push(definition);
    emitted.insert(name.clone());
    Some(name)
}

fn molecule_fixed_array_alias(aliases: &mut BTreeMap<String, String>, name: &str, item: &str, len: usize) -> String {
    let name = molecule_identifier(name);
    aliases.entry(name.clone()).or_insert_with(|| format!("array {} [{}; {}];", name, item, len));
    name
}

fn molecule_bytes_alias(aliases: &mut BTreeMap<String, String>, name: &str) -> String {
    let name = molecule_identifier(name);
    aliases.entry(name.clone()).or_insert_with(|| format!("vector {} <byte>;", name));
    name
}

fn molecule_dynvec_alias(aliases: &mut BTreeMap<String, String>, name: &str, item: &str) -> String {
    let name = molecule_identifier(name);
    aliases.entry(name.clone()).or_insert_with(|| format!("vector {} <{}>;", name, item));
    name
}

fn ordered_molecule_alias_definitions(aliases: &BTreeMap<String, String>) -> Vec<&String> {
    const PRIMITIVE_ORDER: [&str; 9] = [
        "CellScriptBool",
        "CellScriptUint16",
        "CellScriptUint32",
        "CellScriptUint64",
        "CellScriptUint128",
        "CellScriptAddress",
        "CellScriptHash",
        "CellScriptString",
        "CellScriptUnknownVec",
    ];
    let mut definitions = Vec::new();
    for name in PRIMITIVE_ORDER {
        if let Some(definition) = aliases.get(name) {
            definitions.push(definition);
        }
    }
    for (name, definition) in aliases {
        if !PRIMITIVE_ORDER.contains(&name.as_str()) {
            definitions.push(definition);
        }
    }
    definitions
}

fn molecule_identifier(input: &str) -> String {
    let mut out = String::new();
    let mut uppercase_next = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if uppercase_next {
                out.push(ch.to_ascii_uppercase());
                uppercase_next = false;
            } else {
                out.push(ch);
            }
        } else {
            uppercase_next = true;
        }
    }
    if out.is_empty() || out.as_bytes().first().is_some_and(|byte| byte.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

pub(super) fn metadata_capability_name(capability: &crate::ast::Capability) -> String {
    capability.as_str().to_string()
}

pub(super) fn is_default_identity_policy(value: &Option<String>) -> bool {
    value.as_ref().is_none_or(|s| s == "none")
}

pub(super) fn metadata_identity_policy(policy: &ir::IrIdentityPolicy) -> Option<String> {
    match policy {
        ir::IrIdentityPolicy::None => None,
        ir::IrIdentityPolicy::CkbTypeId => Some("ckb_type_id".to_string()),
        ir::IrIdentityPolicy::Field(path) => Some(format!("field({})", path)),
        ir::IrIdentityPolicy::ScriptArgs => Some("script_args".to_string()),
        ir::IrIdentityPolicy::SingletonType => Some("singleton_type".to_string()),
    }
}

fn flow_transition_metadata(states: &[String]) -> Vec<FlowTransitionMetadata> {
    states
        .windows(2)
        .enumerate()
        .map(|(index, window)| FlowTransitionMetadata {
            from: window[0].clone(),
            to: window[1].clone(),
            from_index: index,
            to_index: index + 1,
        })
        .collect()
}

fn flow_rule_metadata(rule: &ir::IrFlowRule) -> FlowTransitionMetadata {
    FlowTransitionMetadata { from: rule.from.clone(), to: rule.to.clone(), from_index: rule.from_index, to_index: rule.to_index }
}

fn fields_metadata(type_def: &ir::IrTypeDef, type_defs: &BTreeMap<String, &ir::IrTypeDef>) -> Vec<FieldMetadata> {
    let mut next_offset = Some(0usize);
    type_def
        .fields
        .iter()
        .map(|field| {
            let encoded_size = ir_type_encoded_size(&field.ty, type_defs, &mut BTreeSet::new()).or(field.fixed_size);
            let offset = next_offset.unwrap_or(field.offset);
            next_offset = next_offset.and_then(|current| encoded_size.and_then(|size| current.checked_add(size)));
            field_metadata(field, encoded_size, offset)
        })
        .collect()
}

fn field_metadata(field: &ir::IrField, encoded_size: Option<usize>, offset: usize) -> FieldMetadata {
    FieldMetadata {
        name: field.name.clone(),
        ty: ir_type_to_string(&field.ty),
        offset,
        encoded_size,
        fixed_width: encoded_size.is_some(),
    }
}

pub(super) fn type_encoded_size(type_def: &ir::IrTypeDef, type_defs: &BTreeMap<String, &ir::IrTypeDef>) -> Option<usize> {
    type_def.fields.iter().try_fold(0usize, |acc, field| {
        ir_type_encoded_size(&field.ty, type_defs, &mut BTreeSet::new()).or(field.fixed_size).and_then(|size| acc.checked_add(size))
    })
}

pub(super) fn ir_type_encoded_size(
    ty: &ir::IrType,
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
    visiting: &mut BTreeSet<String>,
) -> Option<usize> {
    match ty {
        ir::IrType::Context(_) | ir::IrType::Vector(_) | ir::IrType::Bounded(_) => None,
        ir::IrType::Commitment(_) => Some(32),
        ir::IrType::Opening(value) => ir_type_encoded_size(value, type_defs, visiting),
        ir::IrType::Applied(_) | ir::IrType::OpenHandle(_) | ir::IrType::WitnessBytes(_) | ir::IrType::Selection(_) => None,
        ir::IrType::U8 | ir::IrType::Bool => Some(1),
        ir::IrType::U16 => Some(2),
        ir::IrType::U32 => Some(4),
        ir::IrType::I32 => Some(4),
        ir::IrType::U64 => Some(8),
        ir::IrType::U128 => Some(16),
        ir::IrType::Address | ir::IrType::Hash => Some(32),
        ir::IrType::Array(inner, len) => {
            ir_type_encoded_size(inner, type_defs, visiting).and_then(|inner_size| inner_size.checked_mul(*len))
        }
        ir::IrType::Tuple(items) => items.iter().try_fold(0usize, |acc, item| {
            ir_type_encoded_size(item, type_defs, visiting).and_then(|item_size| acc.checked_add(item_size))
        }),
        ir::IrType::Unit => Some(0),
        ir::IrType::Named(name) => {
            if !visiting.insert(name.clone()) {
                return None;
            }
            let size = type_defs.get(name.as_str()).and_then(|type_def| {
                type_def.fields.iter().try_fold(0usize, |acc, field| {
                    ir_type_encoded_size(&field.ty, type_defs, visiting)
                        .or(field.fixed_size)
                        .and_then(|field_size| acc.checked_add(field_size))
                })
            });
            visiting.remove(name);
            size
        }
        ir::IrType::Nested(_) | ir::IrType::Ref(_) | ir::IrType::MutRef(_) => None,
    }
}

pub(super) fn ir_type_to_string(ty: &ir::IrType) -> String {
    ty.source_text()
}

pub(super) fn named_type_name(ty: &ir::IrType) -> Option<&str> {
    match ty {
        ir::IrType::Named(name) => Some(name.as_str()),
        ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) => named_type_name(inner),
        _ => None,
    }
}
