//! Private source receipts meet the concrete ProtocolBundle byte boundary.
use super::*;
use crate::package::frozen_interface::{FrozenPackageModule, FrozenSourceCodePolicy};
use cellscript_artifact_checker::code_origin::{
    check_protocol_transaction, CheckedProtocolTransaction, OpenCellSource, OpenParticipantObservation, SuppliedDependencyCell,
    SuppliedInputCell,
};
use cellscript_artifact_checker::open_handle_policy::HandleClass;
mod file;
mod selection;
pub use file::check_protocol_bundle_with_open_handles_file;
use selection::AdmissionPolicy;

/// Inputs retain private source/receipt proofs, never deserialized `verified`
/// flags. The caller obtains transaction bytes from its actual builder.
pub struct ProtocolOpenHandleBinding<'a> {
    pub consumer: &'a str,
    pub parameter: &'a str,
    pub consumer_module: &'a FrozenPackageModule,
    pub policy: &'a FrozenSourceCodePolicy,
    pub selection: &'a [u8],
    /// Exact selected provider, already present among ProtocolBundle artifacts.
    pub participant: &'a str,
}
pub struct ProtocolOpenTransaction<'a> {
    pub full_transaction: &'a [u8],
    pub inputs: &'a [SuppliedInputCell<'a>],
    pub dependencies: &'a [SuppliedDependencyCell<'a>],
    pub bindings: &'a [ProtocolOpenHandleBinding<'a>],
}
pub struct ProtocolInstanceHandleBinding<'a> {
    pub consumer: &'a str,
    pub parameter: &'a str,
    pub consumer_module: &'a FrozenPackageModule,
    pub policy: &'a crate::package::frozen_interface::FrozenInstancePolicy,
    pub selection: &'a [u8],
    pub participant: &'a str,
}
pub struct ProtocolInstanceTransaction<'a> {
    pub full_transaction: &'a [u8],
    pub inputs: &'a [SuppliedInputCell<'a>],
    pub dependencies: &'a [SuppliedDependencyCell<'a>],
    pub bindings: &'a [ProtocolInstanceHandleBinding<'a>],
    pub open_bindings: &'a [ProtocolOpenHandleBinding<'a>],
}
struct Binding<'a> {
    consumer: &'a str,
    parameter: &'a str,
    consumer_module: &'a FrozenPackageModule,
    policy: AdmissionPolicy<'a>,
    selection: &'a [u8],
    participant: &'a str,
}
impl<'a> From<&ProtocolOpenHandleBinding<'a>> for Binding<'a> {
    fn from(value: &ProtocolOpenHandleBinding<'a>) -> Self {
        Self {
            consumer: value.consumer,
            parameter: value.parameter,
            consumer_module: value.consumer_module,
            policy: AdmissionPolicy::Open(value.policy),
            selection: value.selection,
            participant: value.participant,
        }
    }
}
impl<'a> From<&ProtocolInstanceHandleBinding<'a>> for Binding<'a> {
    fn from(value: &ProtocolInstanceHandleBinding<'a>) -> Self {
        Self {
            consumer: value.consumer,
            parameter: value.parameter,
            consumer_module: value.consumer_module,
            policy: AdmissionPolicy::Instance(value.policy),
            selection: value.selection,
            participant: value.participant,
        }
    }
}
fn invalid(message: &str) -> CompileError {
    CompileError::without_span(format!("open ProtocolBundle admission: {message}")).with_code("E2611")
}
fn checked<T>(value: std::result::Result<T, cellscript_artifact_checker::CheckerError>) -> Result<T> {
    value.map_err(|error| CompileError::without_span(error.to_string()).with_code(error.code.as_str()))
}
fn hex_bytes(value: &str) -> Result<Vec<u8>> {
    canonical_hex_bytes(value, "open transaction bytes")?;
    hex::decode(&value[2..]).map_err(|_| invalid("invalid canonical hex"))
}
fn same_module(module: &FrozenPackageModule, artifact: &ProtocolArtifactIdentity) -> Result<CompileMetadata> {
    let bundle = module.bundle();
    let metadata: CompileMetadata = serde_json::from_slice(bundle[1])?;
    if crate::hex_encode(&ckb_blake2b256(bundle[0])) != artifact.artifact_hash
        || checked(canonical_hash("cellscript-compile-metadata", &metadata))? != artifact.metadata_hash
    {
        return Err(invalid("private source module differs from the independently admitted bundle"));
    }
    Ok(metadata)
}
fn parameter_type(metadata: &CompileMetadata, entry: &ProtocolEntryIdentity, parameter: &str) -> Result<String> {
    let params = match entry.kind {
        ProtocolEntryKind::Action => metadata.actions.iter().find(|a| a.name == entry.name).map(|a| &a.params),
        ProtocolEntryKind::Lock => metadata.locks.iter().find(|a| a.name == entry.name).map(|a| &a.params),
        _ => None,
    }
    .ok_or_else(|| invalid("open consumer requires an actual Action or Lock entry"))?;
    let witnesses = params.iter().filter(|param| param.source == "witness").collect::<Vec<_>>();
    if witnesses.len() != 1 || witnesses[0].name != parameter {
        return Err(invalid("bounded open consumer requires exactly one handle witness parameter"));
    }
    Ok(witnesses[0].ty.clone())
}
fn consumer_witness(
    tx: &CheckedProtocolTransaction<'_>,
    input: &ProtocolBundleInput,
    artifact: &ProtocolArtifactIdentity,
) -> Result<usize> {
    let expected = &artifact.deployment.script;
    // At least one actual input must commit the consumer root. An output-only
    // new Script cannot self-authorize a replacement policy in this profile.
    input
        .transaction
        .inputs
        .iter()
        .position(|cell| match artifact.script_role {
            ProtocolScriptRole::Type => cell.r#type.as_ref() == Some(expected),
            ProtocolScriptRole::Lock => &cell.lock == expected,
            ProtocolScriptRole::SpawnedVerifier => false,
        })
        .filter(|index| tx.witness_field(*index, 1).is_ok())
        .ok_or_else(|| invalid("consumer has no committed input Script group"))
}
fn root_helper(
    metadata: &CompileMetadata,
    entry: &ProtocolEntryIdentity,
    parameter: &str,
    instance: bool,
) -> Result<Option<(OpenCellSource, u32, String)>> {
    let typed = metadata
        .typed_semantics
        .entries
        .iter()
        .find(|e| e.name == entry.name)
        .ok_or_else(|| invalid("consumer lacks its checked typed entry"))?;
    let contract = &metadata.typed_semantics.foundation.entry_contract;
    if contract.exact_entry != typed.id
        || !matches!(contract.dispatch, cellscript_artifact_checker::EntryDispatchContract::SingleEntry)
        || !matches!(contract.script_role.as_str(), "type" | "lock")
        || contract.entry_payload_abi != "cellscript-entry-witness-v1"
        || contract.witness_placement_abi != "cellscript-witnessargs-input-type-v2"
        || contract.witness_placement_field != "input_type"
        || contract.witness_placement_source != "group-input-0-then-group-output-0"
    {
        return Err(invalid(
            "consumer admission requires its actual single-entry Script dispatcher and canonical handle witness placement",
        ));
    }
    let param = typed.params.iter().find(|p| p.name == parameter).ok_or_else(|| invalid("typed handle parameter is absent"))?;
    let block = typed.blocks.iter().find(|b| b.id == typed.entry_block).ok_or_else(|| invalid("typed entry block is absent"))?;
    let operation = block
        .operations
        .iter()
        .find(|op| {
            op.call.as_ref().is_some_and(|call| {
                (if instance {
                    call.target == "__ckb_require_instance_script_participant_from_args"
                } else {
                    matches!(
                        call.target.as_str(),
                        "__ckb_require_cell_dep_open_script_handle_from_args"
                            | "__ckb_require_open_script_participant_from_args"
                            | "__ckb_require_cell_dep_open_verifier_handle_from_args"
                    )
                }) && op.operands.get(1).is_some_and(|operand| operand.local == Some(param.binding_id))
            })
        })
        .ok_or_else(|| invalid("consumer must unconditionally verify this handle against committed args in its entry block"))?;
    if !instance && operation.call.as_ref().is_none_or(|call| call.target != "__ckb_require_open_script_participant_from_args") {
        return Ok(None);
    }
    let operand = operation.operands.get(2).ok_or_else(|| invalid("missing participant view"))?;
    let constructor = operand
        .local
        .and_then(|local| block.operations.iter().find(|op| op.destinations.contains(&local)))
        .ok_or_else(|| invalid("open participant requires a bounded entry-block view constructor"))?;
    let source = match constructor.call.as_ref().map(|call| call.target.as_str()) {
        Some("__ckb_source_input") => OpenCellSource::Input,
        Some("__ckb_source_output") => OpenCellSource::Output,
        Some("__ckb_source_group_input") => OpenCellSource::GroupInput,
        Some("__ckb_source_group_output") => OpenCellSource::GroupOutput,
        _ => return Err(invalid("open participant source must be an actual input/output view constructor")),
    };
    let index = match constructor.operands.first().and_then(|operand| operand.constant.as_ref()) {
        Some(cellscript_artifact_checker::TypedSemanticConstant::U64(index)) => {
            index.parse::<u32>().map_err(|_| invalid("participant index exceeds u32"))?
        }
        _ => return Err(invalid("bounded open participant profile requires a constant source index")),
    };
    let name = operand
        .ty
        .split_once('<')
        .and_then(|(_, name)| name.strip_suffix('>'))
        .ok_or_else(|| invalid("participant view has no checked Cell schema"))?;
    Ok(Some((source, index, name.to_string())))
}

pub(super) fn admit(
    input: &ProtocolBundleInput,
    artifacts: &[ProtocolArtifactIdentity],
    metadata: &BTreeMap<String, CompileMetadata>,
    supplied: &ProtocolOpenTransaction<'_>,
) -> Result<Vec<ResolvedOpenHandle>> {
    let bindings = supplied.bindings.iter().map(Binding::from).collect::<Vec<_>>();
    admit_bindings(input, artifacts, metadata, supplied, &bindings)
}

pub(super) fn admit_instances(
    input: &ProtocolBundleInput,
    artifacts: &[ProtocolArtifactIdentity],
    metadata: &BTreeMap<String, CompileMetadata>,
    supplied: &ProtocolInstanceTransaction<'_>,
) -> Result<Vec<ResolvedOpenHandle>> {
    let bindings =
        supplied.open_bindings.iter().map(Binding::from).chain(supplied.bindings.iter().map(Binding::from)).collect::<Vec<_>>();
    let context = ProtocolOpenTransaction {
        full_transaction: supplied.full_transaction,
        inputs: supplied.inputs,
        dependencies: supplied.dependencies,
        bindings: supplied.open_bindings,
    };
    admit_bindings(input, artifacts, metadata, &context, &bindings)
}

fn admit_bindings(
    input: &ProtocolBundleInput,
    artifacts: &[ProtocolArtifactIdentity],
    metadata: &BTreeMap<String, CompileMetadata>,
    supplied: &ProtocolOpenTransaction<'_>,
    bindings: &[Binding<'_>],
) -> Result<Vec<ResolvedOpenHandle>> {
    let mut required = open_handle_parameters(artifacts, metadata);
    if bindings.is_empty() || bindings.len() > 32 || bindings.len() != required.len() {
        return Err(invalid("open receipt count must exactly cover 1..=32 consumer handle parameters"));
    }
    let budgets = CheckerBudgets::default();
    let tx = checked(check_protocol_transaction(
        supplied.full_transaction,
        supplied.inputs,
        &serde_json::to_value(&input.transaction)?,
        &budgets,
    ))?;
    let mut result = Vec::new();
    for binding in bindings {
        let policy = binding.policy.implementation();
        if !required.remove(&(binding.consumer.into(), binding.parameter.into())) {
            return Err(invalid("duplicate or unreferenced open receipt binding"));
        }
        let consumer = artifacts.iter().find(|a| a.id == binding.consumer).ok_or_else(|| invalid("unknown consumer"))?;
        let provider =
            artifacts.iter().find(|a| a.id == binding.participant).ok_or_else(|| invalid("missing selected participant artifact"))?;
        if binding.consumer == binding.participant {
            return Err(invalid("open participant cannot grant itself foreign authority"));
        }
        let source = same_module(binding.consumer_module, consumer)?;
        let bundle = binding.consumer_module.bundle();
        checked(cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
            bundle[0], bundle[1], bundle[2], bundle[3], &budgets,
        ))?;
        if binding.consumer_module.context_identity() != policy.sources().sources().context_identity() {
            return Err(invalid("consumer source/lock closure differs from the policy's defining-owner proof"));
        }
        let module = policy.sources().catalog().required().projection().module();
        let verifier = policy.policy().header().class == HandleClass::Verifier;
        if binding.policy.is_instance() && verifier {
            return Err(invalid(
                "instance participants require a real Lock/Type Script group; use the unchanged exact verifier profile",
            ));
        }
        let class = if binding.policy.is_instance() {
            "InstanceScriptHandle"
        } else if verifier {
            "VerifierHandle"
        } else {
            "ScriptHandle"
        };
        if parameter_type(&source, &consumer.entry, binding.parameter)? != format!("{class}<{module}>") {
            return Err(invalid("nominal handle interface differs from its resolver-owned checked baseline"));
        }
        let observed = root_helper(&source, &consumer.entry, binding.parameter, binding.policy.is_instance())?;
        let consumer_args = hex_bytes(&consumer.deployment.script.args)?;
        if !binding.policy.is_instance() && consumer_args != policy.policy().root() {
            return Err(invalid("consumer Script args do not authorize this policy root"));
        }
        let selected = binding.policy.select(&consumer_args, binding.selection, &budgets)?;
        let instance = selected.instance_record(binding.policy, &consumer_args)?;
        let expected_role = match provider.script_role {
            ProtocolScriptRole::Lock => cellscript_artifact_checker::open_handle_policy::ScriptRole::Lock,
            ProtocolScriptRole::Type => cellscript_artifact_checker::open_handle_policy::ScriptRole::Type,
            ProtocolScriptRole::SpawnedVerifier => cellscript_artifact_checker::open_handle_policy::ScriptRole::SpawnedVerifier,
        };
        if verifier != (provider.script_role == ProtocolScriptRole::SpawnedVerifier)
            || selected.membership().header().role != expected_role
            || !matches!(consumer.script_role, ProtocolScriptRole::Type | ProtocolScriptRole::Lock)
        {
            return Err(invalid("policy class or role differs from the actual participant"));
        }
        let provider_metadata = same_module(selected.candidate().module(), provider)?;
        let provider_contract = selected.receipt().entry_contract().clone();
        let check_provider_entry = |entry_id: &str| -> Result<()> {
            let actual = provider_metadata
                .typed_semantics
                .entries
                .iter()
                .find(|e| e.id == entry_id)
                .ok_or_else(|| invalid("selected participant entry is absent"))?;
            let kind = match provider.entry.kind {
                ProtocolEntryKind::Action => "action",
                ProtocolEntryKind::Lock => "lock",
                ProtocolEntryKind::Function => "function",
            };
            if actual.name != provider.entry.name || actual.kind != kind {
                return Err(invalid("declared participant entry differs from the actual selected dispatcher"));
            }
            Ok(())
        };
        if matches!(provider_contract.dispatch, cellscript_artifact_checker::EntryDispatchContract::SingleEntry) {
            check_provider_entry(&provider_contract.exact_entry)?;
        }
        let origin = selected.receipt().target_origin().origin();
        let selected_script_hash = origin.selected_script_hash().to_string();
        let selected_artifact_hash = origin.artifact_hash().to_string();
        if crate::script_handle::ckb_script_identity_hash(&provider.deployment.script)?.strip_prefix("0x")
            != Some(origin.selected_script_hash())
            || provider.deployment.code_cell_dep.dep_type != ProtocolDepType::Code
            || provider.deployment.code_cell_dep.out_point.tx_hash.strip_prefix("0x") != Some(origin.transaction_hash())
            || provider.deployment.code_cell_dep.out_point.index != origin.output_index()
            || selected.membership().header().network_genesis.as_slice() != hex_bytes(&input.network.genesis_hash)?
        {
            return Err(invalid("participant Script, deployment, role or network was substituted"));
        }
        let witness_index = consumer_witness(&tx, input, consumer)?;
        let payload = checked(tx.witness_field(witness_index, 1))?;
        if payload.len() != binding.selection.len() + 8 || !payload.starts_with(b"CSARGv1\0") || payload[8..] != *binding.selection {
            return Err(invalid("final consumer witness differs from the admitted selection"));
        }
        let dependency = selected.check_direct_dependency(tx.raw_transaction(), supplied.dependencies, &budgets)?;
        let dependency_id = dependency.dependency().identity().to_string();
        let dependency_index = dependency.dependency().raw_dependency_index();
        let selected_receipt = dependency.source_receipt().identity().to_string();
        let (group_id, group_inputs, group_outputs, verifier_invocation) = if verifier {
            if observed.is_some() {
                return Err(invalid("a spawned verifier cannot claim an independent observed Script group"));
            }
            let invocation = checked(cellscript_artifact_checker::code_origin::check_open_verifier_invocation(
                binding.consumer_module.bundle(),
                binding.parameter,
                dependency.receipt(),
                dependency.dependency(),
                &tx,
                &budgets,
            ))?;
            (String::new(), Vec::new(), Vec::new(), Some(invocation.evidence().clone()))
        } else {
            let group = dependency.check_script_group(
                &cellscript_artifact_checker::code_origin::SuppliedTypeGroupTransaction {
                    full_transaction: supplied.full_transaction,
                    dependencies: supplied.dependencies,
                    inputs: supplied.inputs,
                },
                &budgets,
            )?;
            if let cellscript_artifact_checker::EntryDispatchContract::PolicyWitnessV1(policy) = &provider_contract.dispatch {
                let variant = policy
                    .variants
                    .iter()
                    .find(|v| v.tag == group.group().selected_tag())
                    .ok_or_else(|| invalid("selected policy tag is absent"))?;
                check_provider_entry(&variant.entry_id)?;
            }
            (group.group().identity().to_string(), group.group().group_inputs().to_vec(), group.group().group_outputs().to_vec(), None)
        };
        for role in input.roles.iter().filter(|role| role.artifact == binding.consumer) {
            let indices = match role.location {
                ProtocolCellLocation::Input => &group_inputs,
                ProtocolCellLocation::Output => &group_outputs,
            };
            if indices.contains(&(role.index as usize)) && role.ownership != ProtocolRoleOwnership::SharedRead {
                return Err(invalid("an open consumer cannot claim lifecycle ownership of its foreign participant's Cells"));
            }
        }
        let observation = observed
            .map(|(domain, index, type_name)| {
                let is_input = matches!(domain, OpenCellSource::Input | OpenCellSource::GroupInput);
                let cells = if is_input { &input.transaction.inputs } else { &input.transaction.outputs };
                let transaction_index = if matches!(domain, OpenCellSource::GroupInput | OpenCellSource::GroupOutput) {
                    cells
                        .iter()
                        .enumerate()
                        .filter(|(_, cell)| match consumer.script_role {
                            ProtocolScriptRole::Lock => is_input && cell.lock == consumer.deployment.script,
                            ProtocolScriptRole::Type => cell.r#type.as_ref() == Some(&consumer.deployment.script),
                            ProtocolScriptRole::SpawnedVerifier => false,
                        })
                        .nth(index as usize)
                        .map(|(index, _)| index)
                        .ok_or_else(|| invalid("observed group-relative participant is absent"))?
                } else {
                    index as usize
                };
                let cell = cells.get(transaction_index).ok_or_else(|| invalid("observed participant is absent"))?;
                let matching = match provider.script_role {
                    ProtocolScriptRole::Lock => is_input && cell.lock == provider.deployment.script,
                    ProtocolScriptRole::Type => cell.r#type.as_ref() == Some(&provider.deployment.script),
                    ProtocolScriptRole::SpawnedVerifier => false,
                };
                if !matching {
                    return Err(invalid("observed participant source or complete Script differs from the selection"));
                }
                let source_catalog = source
                    .typed_semantics
                    .nominal_declarations
                    .as_ref()
                    .ok_or_else(|| invalid("consumer lacks declaration ownership"))?;
                let scope = source_catalog
                    .scopes
                    .iter()
                    .find(|scope| scope.module == source.typed_semantics.module)
                    .ok_or_else(|| invalid("consumer lacks its nominal scope"))?;
                let type_name = checked(cellscript_artifact_checker::interface::qualified_source_type(&type_name, scope))?;
                let (owner, name) = type_name.rsplit_once("::").ok_or_else(|| invalid("observed Cell has no defining owner"))?;
                let declaration = source_catalog
                    .declarations
                    .iter()
                    .find(|declaration| declaration.module == owner && declaration.name == name)
                    .ok_or_else(|| invalid("observed Cell declaration is absent"))?;
                let provider_catalog = provider_metadata
                    .typed_semantics
                    .nominal_declarations
                    .as_ref()
                    .ok_or_else(|| invalid("provider lacks declaration ownership"))?;
                if !provider_catalog.declarations.iter().any(|candidate| candidate == declaration) {
                    return Err(invalid("consumer and provider disagree on the complete observed Cell declaration"));
                }
                let lowered = provider_catalog
                    .layout_bindings
                    .iter()
                    .find(|binding| binding.owner_module == owner && binding.source_name == name)
                    .ok_or_else(|| invalid("provider has no independently bound Cell layout"))?;
                let schema = provider_metadata
                    .molecule_schema_manifest
                    .entries
                    .iter()
                    .find(|schema| schema.type_name == lowered.lowered_name)
                    .ok_or_else(|| invalid("provider's observed Cell schema is absent"))?;
                if hex_bytes(cell.data.as_deref().ok_or_else(|| invalid("observed Cell data is absent"))?)?.len() != schema.fixed_size
                {
                    return Err(invalid("observed participant data differs from the checked fixed Cell schema"));
                }
                Ok(OpenParticipantObservation {
                    source: domain,
                    index,
                    transaction_index: transaction_index as u32,
                    type_name,
                    schema_hash: schema.schema_hash.clone(),
                })
            })
            .transpose()?;
        result.push(ResolvedOpenHandle {
            schema: if instance.is_some() { PROTOCOL_INSTANCE_HANDLE_SCHEMA } else { PROTOCOL_OPEN_HANDLE_SCHEMA }.into(),
            consumer: binding.consumer.into(),
            parameter: binding.parameter.into(),
            participant: binding.participant.into(),
            interface_module: module.into(),
            source_policy: policy.identity().into(),
            selected_receipt,
            authorization_root: format!("0x{}", hex_encode(&policy.policy().root())),
            selection: format!("0x{}", hex_encode(binding.selection)),
            witness_index: witness_index as u32,
            witness_field: "input_type".into(),
            code_dependency_index: dependency_index as u32,
            selected_script_hash,
            selected_artifact_hash,
            checked_dependency: dependency_id,
            checked_type_group: group_id,
            verifier_invocation,
            raw_transaction_hash: tx.raw_hash(),
            serialized_transaction_hash: tx.full_hash(),
            peer_input_indices: group_inputs.iter().map(|index| *index as u32).collect(),
            peer_output_indices: group_outputs.iter().map(|index| *index as u32).collect(),
            claim_boundaries: if verifier {
                cellscript_artifact_checker::code_origin::OpenHandleClaimBoundaries::for_verifier()
            } else {
                cellscript_artifact_checker::code_origin::OpenHandleClaimBoundaries::for_observation(observation.is_some())
            },
            observation,
            instance,
        });
    }
    result.sort_by(|a, b| (&a.consumer, &a.parameter).cmp(&(&b.consumer, &b.parameter)));
    Ok(result)
}
