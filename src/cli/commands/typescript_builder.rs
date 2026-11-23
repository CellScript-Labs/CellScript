use super::super::protocol_builder::typescript_protocol_bundle_api;
use super::intent_records::action_scan_selectors_json;
use super::{hash_json_value, metadata_abi_hash, metadata_schema_versions_json};
use crate::error::{CompileError, Result};
use crate::runtime_errors::{runtime_error_info, ALL_RUNTIME_ERRORS};
use crate::{CompileMetadata, ParamMetadata};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn write_typescript_builder_package(
    output_dir: &Path,
    package_name: &str,
    metadata: &CompileMetadata,
    metadata_hash: &str,
    actions: &[&crate::ActionMetadata],
    locked_identity: Option<&serde_json::Value>,
    deployment_identity: Option<&serde_json::Value>,
    lockfile_path: Option<&Path>,
    deployed_path: Option<&Path>,
) -> Result<serde_json::Value> {
    let src_dir = output_dir.join("src");
    let test_dir = output_dir.join("test");
    std::fs::create_dir_all(&src_dir)?;
    std::fs::create_dir_all(&test_dir)?;

    let manifest = typescript_builder_manifest(package_name, metadata, actions, metadata_hash, locked_identity, deployment_identity)?;

    let package_json_path = output_dir.join("package.json");
    let tsconfig_path = output_dir.join("tsconfig.json");
    let manifest_path = output_dir.join("cellscript-builder-manifest.json");
    let metadata_path = src_dir.join("metadata.json");
    let index_path = src_dir.join("index.ts");
    let test_path = test_dir.join("builder.test.mjs");

    std::fs::write(&package_json_path, json_bytes_pretty("package.json", &typescript_package_json(package_name))?)?;
    std::fs::write(&tsconfig_path, json_bytes_pretty("tsconfig.json", &typescript_tsconfig_json())?)?;
    std::fs::write(&manifest_path, json_bytes_pretty("builder manifest", &manifest)?)?;
    std::fs::write(&metadata_path, json_bytes_pretty("metadata", metadata)?)?;
    std::fs::write(
        &index_path,
        typescript_builder_index(package_name, metadata, actions, metadata_hash, locked_identity, deployment_identity)?,
    )?;
    std::fs::write(&test_path, typescript_builder_test(metadata, actions)?)?;
    let policy_test_path = test_dir.join("policy-witness.test.mjs");
    let policy_test_source = include_str!("../policy_builder_test.mjs");
    if metadata.runtime.policy_artifact.is_some() {
        std::fs::write(&policy_test_path, policy_test_source)?;
    } else if std::fs::read(&policy_test_path).is_ok_and(|bytes| bytes == policy_test_source.as_bytes()) {
        // Remove only our exact generated policy test when this output is
        // regenerated for the legacy path; preserve any user-edited test.
        std::fs::remove_file(&policy_test_path)?;
    }

    Ok(serde_json::json!({
        "status": "ok",
        "schema": "cellscript-generated-builder-summary-v0.20",
        "target": "typescript",
        "output_dir": output_dir.display().to_string(),
        "package_name": package_name,
        "metadata_hash": metadata_hash,
        "artifact_hash": metadata.artifact_hash,
        "cell_data_codec_abi": metadata.cell_data_codec_manifest.abi,
        "raw_cell_data_required": metadata.cell_data_codec_manifest.raw_bytes_required,
        "lockfile_verified": locked_identity.is_some(),
        "deployment_verified": deployment_identity.is_some(),
        "protocol_bundle_api_schema": manifest["protocol_bundle_contract"]["schema"],
        "protocol_bundle_artifact_binding_schema": crate::protocol_bundle::PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA,
        "protocol_bundle_closed_role_schema": crate::protocol_bundle::PROTOCOL_CLOSED_ROLE_SCHEMA,
        "exact_script_handle_receipt_schema": crate::script_handle::EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA,
        "exact_script_handle_value_schema": crate::script_handle::EXACT_SCRIPT_HANDLE_VALUE_SCHEMA,
        "exact_script_handle_encoding": crate::script_handle::EXACT_SCRIPT_HANDLE_ENCODING,
        "resumable_external_signing": true,
        "private_keys_in_generated_api": false,
        "lockfile": lockfile_path.map(|path| path.display().to_string()),
        "deployed": deployed_path.map(|path| path.display().to_string()),
        "action_count": actions.len(),
        "actions": actions.iter().map(|action| action.name.as_str()).collect::<Vec<_>>(),
        "files": [
            package_json_path.display().to_string(),
            tsconfig_path.display().to_string(),
            manifest_path.display().to_string(),
            metadata_path.display().to_string(),
            index_path.display().to_string(),
            test_path.display().to_string()
        ],
        "non_claims": [
            "generated package does not prove live-cell availability",
            "generated package does not sign or submit transactions by itself",
            "generated package does not materialize raw cell-data codecs by itself",
            "generated package requires a runtime adapter for CCC or ckb-sdk-rust"
        ]
    }))
}

fn runtime_error_catalog_json(metadata: &CompileMetadata) -> Vec<serde_json::Value> {
    ALL_RUNTIME_ERRORS
        .iter()
        .copied()
        .filter(|error| metadata.constraints.runtime_errors.iter().any(|declared| declared.code == error.code()))
        .map(|error| {
            let info = runtime_error_info(error);
            serde_json::json!({
                "code": info.code,
                "name": info.name,
                "description": info.description,
                "hint": info.hint,
            })
        })
        .collect()
}

pub(super) fn action_requires_entry_witness(action: &crate::ActionMetadata) -> bool {
    let runtime_bound = crate::runtime_bound_param_names(&action.params);
    action.params.iter().any(|param| crate::param_consumes_entry_witness_payload(param, &runtime_bound))
}

pub(super) fn builder_action_error_contexts_json(actions: &[&crate::ActionMetadata]) -> Vec<serde_json::Value> {
    actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "action": action.name,
                "fields": action
                    .params
                    .iter()
                    .map(|param| {
                        serde_json::json!({
                            "name": param.name,
                            "type": param.ty,
                            "source": param.source,
                            "is_mut": param.is_mut,
                            "is_ref": param.is_ref,
                            "witness_data_source": param.witness_data_source,
                            "lock_args_data_source": param.lock_args_data_source,
                            "protected_spend_surface": param.protected_spend_surface,
                            "cell_bound_abi": param.cell_bound_abi,
                            "schema_pointer_abi": param.schema_pointer_abi,
                            "schema_length_abi": param.schema_length_abi,
                            "fixed_byte_len": param.fixed_byte_len,
                        })
                    })
                    .collect::<Vec<_>>(),
                "entry_witness_required": action_requires_entry_witness(action),
                "runtimeInputRequirements": action.transaction_runtime_input_requirements,
                "actionScanSelectors": action_scan_selectors_json(action),
                "verifierObligations": action.verifier_obligations,
                "failClosedRuntimeFeatures": action.fail_closed_runtime_features,
            })
        })
        .collect()
}

pub(super) fn typescript_builder_manifest(
    package_name: &str,
    metadata: &CompileMetadata,
    actions: &[&crate::ActionMetadata],
    metadata_hash: &str,
    locked_identity: Option<&serde_json::Value>,
    deployment_identity: Option<&serde_json::Value>,
) -> Result<serde_json::Value> {
    let role_contracts = crate::protocol_bundle::builder_role_contracts(
        metadata,
        &actions.iter().map(|action| action.name.as_str()).collect::<Vec<_>>(),
    )?;
    let selection_profile = !role_contracts.is_empty();
    let runtime_abi_hash = metadata_abi_hash(metadata)?;
    let target_profile_hash = cellscript_artifact_checker::canonical_hash("cellscript-target-profile", &metadata.target_profile)
        .map_err(|error| CompileError::without_span(format!("failed to hash generated builder target profile: {error}")))?;
    let mut manifest = serde_json::json!({
        "schema": "cellscript-generated-action-builder-v1",
        "target": "typescript",
        "package_name": package_name,
        "module": metadata.module,
        "compiler_version": metadata.compiler_version,
        "edition": metadata.edition,
        "compatibility_profile": metadata.compatibility_profile,
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": metadata_schema_versions_json(metadata),
        "metadata_hash": metadata_hash,
        "artifact_hash": metadata.artifact_hash,
        "source_hash": metadata.source_hash,
        "target_profile": metadata.target_profile.name,
        "molecule_schema_manifest": metadata.molecule_schema_manifest,
        "cell_data_codec_manifest": metadata.cell_data_codec_manifest,
        "locked_identity": locked_identity,
        "deployment_identity": deployment_identity,
        "locks": metadata.locks,
        "actions": actions
            .iter()
            .map(|action| {
                serde_json::json!({
                    "name": action.name,
                    "params": action.params,
                    "created_outputs": action.create_set.len(),
                    "mutated_outputs": action.mutate_set.len(),
                    "runtime_input_requirements": action.transaction_runtime_input_requirements.len(),
                    "runtime_accesses": action.ckb_runtime_accesses,
                    "action_scan_selectors": action_scan_selectors_json(action),
                    "entry_witness_required": action_requires_entry_witness(action),
                    "bounded_output_plans": bounded_output_plan_contracts_for_action(metadata, &action.name),
                })
            })
            .collect::<Vec<_>>(),
        "transaction_view_handles": metadata.runtime.transaction_view_handles,
        "signing_message_domains": metadata.runtime.signing_message_domains,
        "protocol_bundle_contract": {
            "schema": if selection_profile { crate::protocol_bundle::PROTOCOL_BUNDLE_SELECTION_SCHEMA } else { crate::protocol_bundle::PROTOCOL_BUNDLE_SCHEMA },
            "report_schema": if selection_profile { crate::protocol_bundle::PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA } else { crate::protocol_bundle::PROTOCOL_BUNDLE_REPORT_SCHEMA },
            "artifact_binding_schema": crate::protocol_bundle::PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA,
            "closed_role_schema": crate::protocol_bundle::PROTOCOL_CLOSED_ROLE_SCHEMA,
            "bounded_output_plan_evidence_schema": crate::protocol_bundle::PROTOCOL_BUNDLE_BOUNDED_OUTPUT_PLAN_EVIDENCE_SCHEMA,
            "deployment_line_admission_evidence_schema": crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_EVIDENCE_SCHEMA,
            "deployment_line_admission_transition_schema": crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_TRANSITION_SCHEMA,
            "requires_deployment_line_admission": metadata.target_profile.name == "ckb-type-hash",
            "runtime_adapter": "cellscript-ckb-adapter",
            "states": [
                "MaterializedProtocolBundleTx",
                "LiveResolvedProtocolBundleTx",
                "LiveDependenciesResolvedProtocolBundleTx",
                "ReadyToSignProtocolBundleTx",
                "SignedProtocolBundleTx",
                "SignedDryRunProtocolBundleTx",
                "TxPoolAcceptedProtocolBundleTx",
                "SubmittedProtocolBundleTx",
                "ConfirmedProtocolBundleTx"
            ],
            "private_keys": "never-in-bundle-or-evidence"
        },
        "runtime_error_catalog": runtime_error_catalog_json(metadata),
        "runtime_contract": {
            "requires_live_cell_resolution": true,
            "requires_deployment_resolution": true,
            "requires_capacity_and_fee_policy": true,
            "requires_witness_materialization": true,
            "requires_cell_data_codec_materialization": true,
            "requires_external_cell_data_codec_adapter": metadata.cell_data_codec_manifest.raw_bytes_required,
            "cell_data_codec_abi": metadata.cell_data_codec_manifest.abi,
            "requires_dry_run_before_submit": true,
            "must_not_infer_protocol_semantics_from_action_name": true,
            "action_scan_selectors_schema": "cellscript-action-scan-selectors-v0.21",
            "action_scan_selector_source": "transaction_runtime_input_requirements",
            "runtime_access_provenance": metadata.runtime.ckb_runtime_access_provenance_contract,
            "requires_pre_signing_witness_placement": !metadata.runtime.signing_message_domains.is_empty(),
            "temporal_interface": metadata.public_interface.runtime_contract.temporal,
        }
    });
    manifest["target_profile_hash"] = serde_json::json!(target_profile_hash);
    manifest["entry_witness_contract"] = crate::cli::entry_witness_builder::contracts(metadata, actions);
    manifest["runtime_abi_hash"] = serde_json::json!(runtime_abi_hash);
    manifest["interface_hash"] = serde_json::json!(metadata.interface_hash);
    manifest["typed_semantics_hash"] = serde_json::json!(metadata.typed_semantics_hash);
    manifest["verified_bundle_id"] = serde_json::json!(metadata.verified_artifact.verified_bundle_id);
    manifest["protocol_bundle_contract"]["exact_handle_receipt_schema"] =
        serde_json::json!(crate::script_handle::EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA);
    manifest["protocol_bundle_contract"]["exact_handle_value_schema"] =
        serde_json::json!(crate::script_handle::EXACT_SCRIPT_HANDLE_VALUE_SCHEMA);
    manifest["protocol_bundle_contract"]["exact_handle_encoding"] =
        serde_json::json!(crate::script_handle::EXACT_SCRIPT_HANDLE_ENCODING);
    manifest["protocol_bundle_contract"]["exact_handle_hash_algorithm"] = serde_json::json!("ckb-blake2b-256");
    manifest["protocol_bundle_contract"]["exact_handle_hash_personalization"] = serde_json::json!("ckb-default-hash");
    if let Some(policy) = &metadata.runtime.policy_artifact {
        manifest["policy_artifact"] = serde_json::json!(policy);
        manifest["runtime_contract"]["requires_policy_witness_bundle"] = serde_json::json!(true);
        manifest["runtime_contract"]["requires_full_script_hash"] = serde_json::json!(true);
        manifest["runtime_contract"]["requires_shared_witness_aggregation"] = serde_json::json!(true);
        manifest["runtime_contract"]["requires_pre_signing_placement"] = serde_json::json!(true);
        manifest["runtime_contract"]["policy_args_encoding"] = serde_json::json!("pre-encoded-inner-CSARGv1");
        for action in manifest["actions"].as_array_mut().expect("generated actions array") {
            let name = action["name"].as_str().expect("generated action name");
            action["policy_tag"] = serde_json::json!(policy.declaration.action(name).expect("selected exported action").tag);
            action["policy_witness_required"] = serde_json::json!(true);
        }
    }
    if selection_profile {
        manifest["exact_role_contracts"] = serde_json::to_value(role_contracts)?;
    }
    Ok(manifest)
}

fn bounded_output_plan_contracts_for_action<'a>(
    metadata: &'a CompileMetadata,
    action_name: &str,
) -> Vec<&'a crate::BoundedOutputPlanContractMetadata> {
    metadata
        .runtime
        .collection_instantiations
        .iter()
        .filter(|collection| collection.scope_kind == "action" && collection.scope_name == action_name)
        .filter_map(|collection| collection.bounded_output_plan.as_ref())
        .collect()
}

fn typescript_package_json(package_name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": package_name,
        "version": "0.0.0-cellscript-generated",
        "private": true,
        "type": "module",
        "main": "dist/index.js",
        "types": "dist/index.d.ts",
        "scripts": {
            "build": "tsc -p tsconfig.json",
            "test": "npm run build && node --test test/*.test.mjs"
        },
        "devDependencies": {
            "typescript": "^5.0.0"
        }
    })
}

fn typescript_tsconfig_json() -> serde_json::Value {
    serde_json::json!({
        "compilerOptions": {
            "target": "ES2022",
            "module": "NodeNext",
            "moduleResolution": "NodeNext",
            "declaration": true,
            "outDir": "dist",
            "rootDir": "src",
            "strict": true,
            "resolveJsonModule": true,
            "esModuleInterop": true,
            "skipLibCheck": true
        },
        "include": ["src/**/*.ts", "src/**/*.json"]
    })
}

pub(super) fn typescript_builder_index(
    package_name: &str,
    metadata: &CompileMetadata,
    actions: &[&crate::ActionMetadata],
    metadata_hash: &str,
    locked_identity: Option<&serde_json::Value>,
    deployment_identity: Option<&serde_json::Value>,
) -> Result<String> {
    let action_specs = actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "name": action.name,
                "params": action.params,
                "createSet": action.create_set,
                "mutateSet": action.mutate_set,
                "readRefs": action.read_refs,
                "verifierObligations": action.verifier_obligations,
                "runtimeInputRequirements": action.transaction_runtime_input_requirements,
                "runtimeAccesses": action.ckb_runtime_accesses,
                "actionScanSelectors": action_scan_selectors_json(action),
                "failClosedRuntimeFeatures": action.fail_closed_runtime_features,
                "boundedOutputPlans": bounded_output_plan_contracts_for_action(metadata, &action.name),
            })
        })
        .collect::<Vec<_>>();
    let action_specs_json = json_string_pretty("action specs", &action_specs)?;
    let action_error_contexts_json = json_string_pretty("action error contexts", &builder_action_error_contexts_json(actions))?;
    let runtime_error_catalog_json = json_string_pretty("runtime error catalog", &runtime_error_catalog_json(metadata))?;
    let builder_manifest =
        typescript_builder_manifest(package_name, metadata, actions, metadata_hash, locked_identity, deployment_identity)?;
    let manifest_json = json_string_pretty("builder manifest", &builder_manifest)?;
    let metadata_json = json_string_pretty("metadata", metadata)?;
    let signing_message_domains_json = json_string_pretty("signing message domains", &metadata.runtime.signing_message_domains)?;

    let mut ts = String::new();
    ts.push_str("export const CELLSCRIPT_BUILDER_SCHEMA = \"cellscript-generated-action-builder-v1\" as const;\n");
    ts.push_str("export const ACTION_SCAN_SELECTORS_SCHEMA = \"cellscript-action-scan-selectors-v0.21\" as const;\n");
    ts.push_str(&format!("export const builderManifest = {manifest_json} as const;\n"));
    ts.push_str(&format!("export const metadata = {metadata_json} as CellScriptMetadata;\n"));
    for (name, value) in [
        ("cellDataCodecManifest", serde_json::to_value(&metadata.cell_data_codec_manifest)?),
        ("temporalContract", serde_json::to_value(&metadata.public_interface.runtime_contract.temporal)?),
        ("runtimeAccessProvenanceContract", serde_json::to_value(&metadata.runtime.ckb_runtime_access_provenance_contract)?),
    ] {
        ts.push_str(&format!("export const {name} = {} as const;\n", json_string_pretty(name, &value)?));
    }
    ts.push_str(&format!(
        "export const entryWitnessContracts: EntryWitnessContracts = {};\n",
        json_string_pretty("entry witness contracts", &crate::cli::entry_witness_builder::contracts(metadata, actions))?
    ));
    ts.push_str(include_str!("../entry_witness_builder.ts"));
    ts.push_str("export const transactionViewHandles = builderManifest.transaction_view_handles;\n");
    ts.push_str(&format!("export const signingMessageDomains = {signing_message_domains_json} as const;\n"));
    ts.push_str(include_str!("../zk_builder.ts"));
    ts.push_str(include_str!("../zk_witness_builder.ts"));
    ts.push_str(&format!(
        "export const nestedValueContracts = {} as const;\n",
        json_string_pretty("nested value contracts", &crate::nested_builder::nested_value_contracts(metadata)?)?
    ));
    ts.push_str(include_str!("../nested_builder.ts"));
    ts.push_str(&format!(
        "export const fixedCellContracts = Object.fromEntries({}) as Readonly<Record<string, FixedCellWireContract>>;\n",
        json_string_pretty(
            "fixed Cell contracts",
            &crate::fixed_cell_builder::fixed_cell_contracts(metadata)?.into_iter().collect::<Vec<_>>()
        )?
    ));
    ts.push_str(include_str!("../fixed_cell_builder.ts"));
    ts.push_str(&format!(
        "export const nestedCellContracts = {} as const;\n",
        json_string_pretty("nested Cell contracts", &crate::nested_builder::nested_cell_contracts(metadata)?)?
    ));
    ts.push_str(&format!(
        "export const zkVerifierContracts = {} as const;\n",
        json_string_pretty("ZK verifier contracts", &metadata.runtime.zk_verifiers)?
    ));
    if actions.is_empty() {
        ts.push_str("export const actionSpecs: readonly GeneratedActionSpec[] = [];\n");
        ts.push_str("export const actionErrorContexts: readonly CellScriptActionErrorContext[] = [];\n");
    } else {
        ts.push_str(&format!("export const actionSpecs = {action_specs_json} as const;\n\n"));
        ts.push_str(&format!("export const actionErrorContexts = {action_error_contexts_json} as const;\n"));
    }
    ts.push_str(&format!("export const runtimeErrorCatalog = {runtime_error_catalog_json} as const;\n\n"));
    ts.push_str(&typescript_protocol_bundle_api());
    ts.push_str(
        "export type HexString = `0x${string}`;\n\
         export type CellScriptScalarInput = bigint | number | string;\n\
         export type CellScriptTemporalDomain = \"EpochNumber\" | \"EpochDuration\" | \"BlockNumber\" | \"EpochLength\" | \"TimestampMillis\" | \"EncodedSince\" | \"DecodedSince\" | \"AbsoluteBlockSince\" | \"AbsoluteEpochSince\" | \"AbsoluteTimestampSince\" | \"RelativeBlockSince\" | \"RelativeEpochSince\" | \"RelativeTimestampSince\";\n\
         declare const cellScriptTemporalDomain: unique symbol;\n\
         export type CellScriptTemporalInput<D extends CellScriptTemporalDomain> = CellScriptScalarInput & { readonly [cellScriptTemporalDomain]: D };\n\
         export type EpochNumber = CellScriptTemporalInput<\"EpochNumber\">;\n\
         export type EpochDuration = CellScriptTemporalInput<\"EpochDuration\">;\n\
         export type BlockNumber = CellScriptTemporalInput<\"BlockNumber\">;\n\
         export type EpochLength = CellScriptTemporalInput<\"EpochLength\">;\n\
         export type TimestampMillis = CellScriptTemporalInput<\"TimestampMillis\">;\n\
         export type EncodedSince = CellScriptTemporalInput<\"EncodedSince\">;\n\
         export type DecodedSince = CellScriptTemporalInput<\"DecodedSince\">;\n\
         export type AbsoluteBlockSince = CellScriptTemporalInput<\"AbsoluteBlockSince\">;\n\
         export type AbsoluteEpochSince = CellScriptTemporalInput<\"AbsoluteEpochSince\">;\n\
         export type AbsoluteTimestampSince = CellScriptTemporalInput<\"AbsoluteTimestampSince\">;\n\
         export type RelativeBlockSince = CellScriptTemporalInput<\"RelativeBlockSince\">;\n\
         export type RelativeEpochSince = CellScriptTemporalInput<\"RelativeEpochSince\">;\n\
         export type RelativeTimestampSince = CellScriptTemporalInput<\"RelativeTimestampSince\">;\n\n\
         export function temporalValue<D extends CellScriptTemporalDomain>(domain: D, value: CellScriptScalarInput): CellScriptTemporalInput<D> {\n\
           void domain;\n\
           return value as CellScriptTemporalInput<D>;\n\
         }\n\n\
         export type CellScriptValue = string | number | bigint | boolean | Uint8Array | Record<string, unknown> | null;\n\
         export type CellScriptParams = object;\n\n\
         export type ActionScanSelectors = Record<string, unknown> & {\n\
           schema: typeof ACTION_SCAN_SELECTORS_SCHEMA;\n\
           source?: string;\n\
           status?: string;\n\
           selector_count?: number;\n\
           selectors?: readonly unknown[];\n\
         };\n\n\
         export type ScanSelectorEvidence = Record<string, unknown> & {\n\
           selector_index: number;\n\
           status: \"resolved\";\n\
           source?: string;\n\
           role?: string;\n\
           binding?: string;\n\
           feature?: string;\n\
           component?: string;\n\
           script_field?: string | null;\n\
         };\n\n\
         export interface CellScriptLockfilePackage {\n\
           edition: \"2027\";\n\
           name?: string;\n\
           version?: string;\n\
           namespace?: string | null;\n\
           source_hash?: string | null;\n\
           compiler_source_hash?: string | null;\n\
           compiler_requirement: string;\n\
           resolver_compiler_version: string;\n\
         }\n\n\
         export interface CellScriptLockfileBuild {\n\
           edition: \"2027\";\n\
           compatibility_profile_hash: string;\n\
           compiler_version?: string | null;\n\
           target_profile?: string | null;\n\
           artifact_hash?: string | null;\n\
           metadata_hash?: string | null;\n\
           schema_hash?: string | null;\n\
           cell_data_codec_manifest_hash?: string | null;\n\
           abi_hash?: string | null;\n\
           constraints_hash?: string | null;\n\
         }\n\n\
         export interface CellScriptLockfileDeployment {\n\
           record?: string | null;\n\
           record_hash?: string | null;\n\
           code_hash?: string | null;\n\
           out_point?: string | null;\n\
           data_hash?: string | null;\n\
         }\n\n\
         export interface CellScriptLockfile {\n\
           version: 5;\n\
           schema: \"cellscript-lock-v0.30-single-package-coordinate-v1\";\n\
           resolver_model: \"single-package-coordinate-v1\";\n\
           package?: CellScriptLockfilePackage;\n\
           package_build?: CellScriptLockfileBuild | null;\n\
           deployment?: Record<string, CellScriptLockfileDeployment | null | undefined>;\n\
         }\n\n\
         export interface CellScriptDeploymentRecord {\n\
           edition: \"2027\";\n\
           network: string;\n\
           chain_id: string;\n\
           tx_hash: string;\n\
           output_index: number;\n\
           code_hash: string;\n\
           hash_type: string;\n\
           dep_type: string;\n\
           data_hash: string;\n\
           out_point: string;\n\
           artifact_hash?: string | null;\n\
           metadata_hash?: string | null;\n\
           schema_hash?: string | null;\n\
           cell_data_codec_manifest_hash?: string | null;\n\
           abi_hash?: string | null;\n\
           constraints_hash?: string | null;\n\
           compiler_version?: string | null;\n\
           compatibility_profile_hash: string;\n\
           type_id?: string | null;\n\
           status?: string | null;\n\
           audit_report_hash?: string | null;\n\
           publisher_signature?: string | null;\n\
         }\n\n\
         export interface CellScriptLiveDeploymentEvidence {\n\
           network?: string;\n\
           out_point?: string;\n\
           rpc_status?: string;\n\
           status?: string;\n\
           deployment_status?: string | null;\n\
           expected_data_hash?: string;\n\
           rpc_data_hash?: string | null;\n\
           expected_code_hash?: string;\n\
           rpc_code_hash?: string | null;\n\
           violations?: readonly string[];\n\
         }\n\n\
         export interface CellScriptTrustPolicy {\n\
           requirePublisherSignature?: boolean;\n\
           requireAuditReportHash?: boolean;\n\
         }\n\n\
         export interface BuildOptions {\n\
           lockfile?: CellScriptLockfile;\n\
           deployment?: CellScriptDeploymentRecord;\n\
           liveDeploymentEvidence?: CellScriptLiveDeploymentEvidence;\n\
           trustPolicy?: CellScriptTrustPolicy;\n\
           deploymentRef?: string;\n\
           dryRun?: boolean;\n\
           submit?: boolean;\n\
           feeRate?: bigint | number | string;\n\
           changeLock?: unknown;\n\
         }\n\n\
         export interface ActionBuilderPlan<P extends CellScriptParams = CellScriptParams> {\n\
           schema: typeof CELLSCRIPT_BUILDER_SCHEMA;\n\
           state: \"GeneratedActionPlan\";\n\
           status: \"requires-runtime-resolution\";\n\
           action: string;\n\
           params: P;\n\
           options: BuildOptions;\n\
           metadataHash: string;\n\
           artifactHash: string | null;\n\
           targetProfile: string;\n\
           canSubmit: false;\n\
         requiresLiveCellResolution: true;\n\
         requiresDeploymentResolution: true;\n\
         cellDataCodecManifest: typeof cellDataCodecManifest;\n\
         temporalContract: typeof temporalContract;\n\
         runtimeAccessProvenanceContract: typeof runtimeAccessProvenanceContract;\n\
         signingMessageDomains: typeof signingMessageDomains;\n\
         runtimeAccesses: readonly unknown[];\n\
         runtimeInputRequirements: readonly unknown[];\n\
         actionScanSelectors: ActionScanSelectors;\n\
         verifierObligations: readonly unknown[];\n\
         failClosedRuntimeFeatures: readonly string[];\n\
         boundedOutputPlans: readonly BoundedOutputPlanMaterialization[];\n\
         notProvenByGeneratedBuilder: readonly string[];\n\
       }\n\n\
         export type ActionBuilderMode = \"build\" | \"dry-run\" | \"submit\";\n\n\
         export interface ActionBuilderResult<P extends CellScriptParams = CellScriptParams> {\n\
           schema: typeof CELLSCRIPT_BUILDER_SCHEMA;\n\
           state: \"ActionBuilderResult\";\n\
           status: \"built-by-runtime\" | \"dry-run-by-runtime\" | \"submitted-by-runtime\";\n\
           mode: ActionBuilderMode;\n\
           plan: ActionBuilderPlan<P>;\n\
           liveCellResolution: LiveCellResolutionResult;\n\
           transaction: unknown;\n\
           dryRunResult?: unknown;\n\
           submitResult?: unknown;\n\
           submittedTxHash?: string | null;\n\
           canSubmit: false;\n\
           notProvenByGeneratedBuilder: readonly string[];\n\
         }\n\n\
         export interface LiveCellResolutionRequest<P extends CellScriptParams = CellScriptParams> {\n\
           plan: ActionBuilderPlan<P>;\n\
           options: BuildOptions;\n\
         }\n\n\
         export interface LiveCellResolutionResult {\n\
           inputs?: readonly unknown[];\n\
           referenceInputs?: readonly unknown[];\n\
           cellDeps?: readonly unknown[];\n\
           headerDeps?: readonly unknown[];\n\
           deploymentRef?: unknown;\n\
           lineage?: readonly unknown[];\n\
           scanSelectorEvidence?: readonly ScanSelectorEvidence[];\n\
         }\n\n\
         export interface CellScriptBuilderRuntime {\n\
           resolveLiveCells<P extends CellScriptParams>(request: LiveCellResolutionRequest<P>): Promise<LiveCellResolutionResult>;\n\
           buildTransaction<P extends CellScriptParams>(plan: ActionBuilderPlan<P> & { liveCellResolution: LiveCellResolutionResult }): Promise<unknown>;\n\
           dryRun?(transaction: unknown): Promise<unknown>;\n\
           submit?(transaction: unknown): Promise<unknown>;\n\
         }\n\n\
         export interface CellScriptRuntimeErrorInfo {\n\
           code: number;\n\
           name: string;\n\
           description: string;\n\
           hint: string;\n\
         }\n\n\
         export interface CellScriptActionFieldContext {\n\
           name: string;\n\
           type: string;\n\
           source: string;\n\
           is_mut: boolean;\n\
           is_ref: boolean;\n\
           witness_data_source: boolean;\n\
           lock_args_data_source: boolean;\n\
           protected_spend_surface: boolean;\n\
           cell_bound_abi: boolean;\n\
           schema_pointer_abi: boolean;\n\
           schema_length_abi: boolean;\n\
           fixed_byte_len?: number | null;\n\
         }\n\n\
         export interface CellScriptActionErrorContext {\n\
           action: string;\n\
           fields: readonly CellScriptActionFieldContext[];\n\
         entry_witness_required: boolean;\n\
         runtimeInputRequirements: readonly unknown[];\n\
         actionScanSelectors: ActionScanSelectors;\n\
         verifierObligations: readonly unknown[];\n\
         failClosedRuntimeFeatures: readonly string[];\n\
       }\n\n\
         export interface CellScriptRuntimeErrorExplanation extends CellScriptRuntimeErrorInfo {\n\
           action?: string;\n\
           actionFields: readonly CellScriptActionFieldContext[];\n\
         entryWitnessRequired: boolean;\n\
         runtimeInputRequirements: readonly unknown[];\n\
         actionScanSelectors?: ActionScanSelectors;\n\
         verifierObligations: readonly unknown[];\n\
         failClosedRuntimeFeatures: readonly string[];\n\
       }\n\n",
    );
    ts.push_str(&format!(
        "const GENERATED_METADATA_HASH = {};\n\
         const GENERATED_ARTIFACT_HASH: string | null = {};\n\
         const GENERATED_SOURCE_HASH: string | null = {};\n\
         const GENERATED_COMPILER_VERSION = {};\n\
         const GENERATED_EDITION = {};\n\
         const GENERATED_COMPATIBILITY_PROFILE_HASH = {};\n\
         const GENERATED_TARGET_PROFILE = {};\n\
         const GENERATED_SCHEMA_HASH = {};\n\
         const GENERATED_CELL_DATA_CODEC_MANIFEST_HASH = {};\n\
         const GENERATED_ABI_HASH = {};\n\
         const GENERATED_CONSTRAINTS_HASH = {};\n\
         const BUILDER_MANIFEST_RUNTIME = builderManifest as unknown as {{\n\
           deployment_identity?: {{ deployments?: readonly CellScriptDeploymentRecord[] }} | null;\n\
         }};\n\n",
        typescript_string_literal(metadata_hash),
        metadata.artifact_hash.as_deref().map(typescript_string_literal).unwrap_or_else(|| "null".to_string()),
        metadata.source_hash.as_deref().map(typescript_string_literal).unwrap_or_else(|| "null".to_string()),
        typescript_string_literal(&metadata.compiler_version),
        typescript_string_literal(metadata.edition.as_str()),
        typescript_string_literal(&hash_json_value("compatibility_profile", &metadata.compatibility_profile,)?),
        typescript_string_literal(&metadata.target_profile.name),
        typescript_string_literal(&metadata.molecule_schema_manifest.manifest_hash),
        typescript_string_literal(&metadata.cell_data_codec_manifest.manifest_hash),
        typescript_string_literal(&metadata_abi_hash(metadata)?),
        typescript_string_literal(&hash_json_value("constraints", &metadata.constraints)?),
    ));
    ts.push_str(
        "export function runtimeErrorInfoByCode(code: number | string | bigint): CellScriptRuntimeErrorInfo | null {\n\
           const parsed = runtimeErrorCodeFrom(code);\n\
           if (parsed === null) {\n\
             return null;\n\
           }\n\
           const item = runtimeErrorCatalog.find((error) => error.code === parsed);\n\
           return item ? { code: item.code, name: item.name, description: item.description, hint: item.hint } : null;\n\
         }\n\n\
         export function runtimeErrorInfoByName(name: string): CellScriptRuntimeErrorInfo | null {\n\
           const normalized = name.trim().toLowerCase();\n\
           const item = runtimeErrorCatalog.find((error) => error.name === normalized);\n\
           return item ? { code: item.code, name: item.name, description: item.description, hint: item.hint } : null;\n\
         }\n\n\
         export function runtimeErrorContextForAction(action: string): CellScriptActionErrorContext | null {\n\
           const context = actionErrorContexts.find((item) => item.action === action);\n\
           if (!context) {\n\
             return null;\n\
           }\n\
           return {\n\
             action: context.action,\n\
             fields: context.fields.map((field: CellScriptActionFieldContext) => ({ ...field })),\n\
             entry_witness_required: context.entry_witness_required,\n\
             runtimeInputRequirements: [...context.runtimeInputRequirements],\n\
             actionScanSelectors: context.actionScanSelectors as ActionScanSelectors,\n\
             verifierObligations: [...context.verifierObligations],\n\
             failClosedRuntimeFeatures: [...context.failClosedRuntimeFeatures],\n\
           };\n\
         }\n\n\
         export function explainCellScriptRuntimeError(error: unknown, action?: string): CellScriptRuntimeErrorExplanation | null {\n\
           const code = runtimeErrorCodeFrom(error);\n\
           const name = runtimeErrorNameFrom(error);\n\
           const message = runtimeErrorMessageFrom(error);\n\
           let info = code === null ? null : runtimeErrorInfoByCode(code);\n\
           if (!info && name) {\n\
             info = runtimeErrorInfoByName(name);\n\
           }\n\
           if (!info && message) {\n\
             const normalizedMessage = message.toLowerCase();\n\
             const item = runtimeErrorCatalog.find((known) => normalizedMessage.includes(known.name));\n\
             info = item ? { code: item.code, name: item.name, description: item.description, hint: item.hint } : null;\n\
             if (!info && (normalizedMessage.includes(\"entry witness\") || normalizedMessage.includes(\"entry-witness\"))) {\n\
               info = runtimeErrorInfoByName(\"entry-witness-abi-invalid\");\n\
             }\n\
             if (!info && normalizedMessage.includes(\"collection\")) {\n\
               info = runtimeErrorInfoByName(\"collection-runtime-unsupported\");\n\
             }\n\
           }\n\
           if (!info) {\n\
             return null;\n\
           }\n\
           const context = action ? runtimeErrorContextForAction(action) : null;\n\
           return {\n\
             ...info,\n\
             action: context?.action ?? action,\n\
             actionFields: context?.fields ?? [],\n\
             entryWitnessRequired: context?.entry_witness_required ?? false,\n\
             runtimeInputRequirements: context?.runtimeInputRequirements ?? [],\n\
             actionScanSelectors: context?.actionScanSelectors,\n\
             verifierObligations: context?.verifierObligations ?? [],\n\
             failClosedRuntimeFeatures: context?.failClosedRuntimeFeatures ?? [],\n\
           };\n\
         }\n\n\
         function runtimeErrorCodeFrom(error: unknown): number | null {\n\
           if (typeof error === \"number\" && Number.isFinite(error)) {\n\
             return Math.trunc(error);\n\
           }\n\
           if (typeof error === \"bigint\") {\n\
             const value = Number(error);\n\
             return Number.isSafeInteger(value) ? value : null;\n\
           }\n\
           if (typeof error === \"string\") {\n\
             const trimmed = error.trim();\n\
             return /^-?\\d+$/.test(trimmed) ? Number(trimmed) : null;\n\
           }\n\
           if (typeof error === \"object\" && error !== null) {\n\
             const record = error as Record<string, unknown>;\n\
             for (const key of [\"code\", \"exitCode\", \"errorCode\", \"error_code\", \"ecode\"] as const) {\n\
               const parsed = runtimeErrorCodeFrom(record[key]);\n\
               if (parsed !== null) {\n\
                 return parsed;\n\
               }\n\
             }\n\
           }\n\
           return null;\n\
         }\n\n\
         function runtimeErrorNameFrom(error: unknown): string | null {\n\
           if (typeof error === \"string\") {\n\
             const trimmed = error.trim().toLowerCase();\n\
             return runtimeErrorCatalog.some((known) => known.name === trimmed) ? trimmed : null;\n\
           }\n\
           if (typeof error === \"object\" && error !== null) {\n\
             const record = error as Record<string, unknown>;\n\
             for (const key of [\"name\", \"error\", \"errorName\", \"runtime_error\"] as const) {\n\
               const value = record[key];\n\
               if (typeof value === \"string\") {\n\
                 const match = runtimeErrorNameFrom(value);\n\
                 if (match) {\n\
                   return match;\n\
                 }\n\
               }\n\
             }\n\
           }\n\
           return null;\n\
         }\n\n\
         function runtimeErrorMessageFrom(error: unknown): string | null {\n\
           if (typeof error === \"string\") {\n\
             return error;\n\
           }\n\
           if (typeof error === \"object\" && error !== null) {\n\
             const record = error as Record<string, unknown>;\n\
             for (const key of [\"message\", \"stderr\", \"reason\"] as const) {\n\
               const value = record[key];\n\
               if (typeof value === \"string\") {\n\
                 return value;\n\
               }\n\
             }\n\
           }\n\
           return null;\n\
         }\n\n\
         export function validateCellScriptLockfile(lockfile: CellScriptLockfile): string[] {\n\
           const violations: string[] = [];\n\
           if (lockfile.version !== 5) {\n\
             violations.push(`Cell.lock version: expected 5, got ${String(lockfile.version)}`);\n\
           }\n\
           compareRequiredIdentity(\"schema\", lockfile.schema, \"cellscript-lock-v0.30-single-package-coordinate-v1\", violations);\n\
           compareRequiredIdentity(\"resolver_model\", lockfile.resolver_model, \"single-package-coordinate-v1\", violations);\n\
           const pkg = lockfile.package;\n\
           if (!pkg) {\n\
             violations.push(\"Cell.lock has no [package]\");\n\
           } else {\n\
             compareRequiredIdentity(\"edition\", pkg.edition, GENERATED_EDITION, violations);\n\
             compareRequiredIdentity(\"compiler_source_hash\", pkg.compiler_source_hash ?? pkg.source_hash, GENERATED_SOURCE_HASH, violations);\n\
           }\n\
           const build = lockfile.package_build;\n\
           if (!build) {\n\
             violations.push(\"Cell.lock has no [package.build]\");\n\
           } else {\n\
             compareRequiredIdentity(\"edition\", build.edition, GENERATED_EDITION, violations);\n\
             compareRequiredIdentity(\"compatibility_profile_hash\", build.compatibility_profile_hash, GENERATED_COMPATIBILITY_PROFILE_HASH, violations);\n\
             compareRequiredIdentity(\"compiler_version\", build.compiler_version, GENERATED_COMPILER_VERSION, violations);\n\
             compareRequiredIdentity(\"target_profile\", build.target_profile, GENERATED_TARGET_PROFILE, violations);\n\
             compareRequiredIdentity(\"artifact_hash\", build.artifact_hash, GENERATED_ARTIFACT_HASH, violations);\n\
             compareRequiredIdentity(\"metadata_hash\", build.metadata_hash, GENERATED_METADATA_HASH, violations);\n\
             compareRequiredIdentity(\"schema_hash\", build.schema_hash, GENERATED_SCHEMA_HASH, violations);\n\
             compareRequiredIdentity(\"cell_data_codec_manifest_hash\", build.cell_data_codec_manifest_hash, GENERATED_CELL_DATA_CODEC_MANIFEST_HASH, violations);\n\
             compareRequiredIdentity(\"abi_hash\", build.abi_hash, GENERATED_ABI_HASH, violations);\n\
             compareRequiredIdentity(\"constraints_hash\", build.constraints_hash, GENERATED_CONSTRAINTS_HASH, violations);\n\
           }\n\
           return violations;\n\
         }\n\n\
         export function assertCellScriptLockfile(lockfile: CellScriptLockfile): void {\n\
           const violations = validateCellScriptLockfile(lockfile);\n\
           if (violations.length > 0) {\n\
             throw new Error(\"CellScript builder identity mismatch: \" + violations.join(\"; \"));\n\
           }\n\
         }\n\n\
         export function validateCellScriptDeployment(\n\
           lockfile?: CellScriptLockfile,\n\
           deployment?: CellScriptDeploymentRecord,\n\
           liveEvidence?: CellScriptLiveDeploymentEvidence,\n\
           trustPolicy?: CellScriptTrustPolicy,\n\
         ): string[] {\n\
           const violations: string[] = [];\n\
           if (!deployment) {\n\
             if (liveEvidence) {\n\
               violations.push(\"live deployment evidence requires a deployment record\");\n\
             }\n\
             violations.push(...validateCellScriptDeploymentTrust(deployment, trustPolicy));\n\
             return violations;\n\
           }\n\
           violations.push(...validateCellScriptDeploymentTrust(deployment, trustPolicy));\n\
           compareDeploymentIdentity(\"edition\", deployment.edition, GENERATED_EDITION, violations);\n\
           compareDeploymentIdentity(\"compatibility_profile_hash\", deployment.compatibility_profile_hash, GENERATED_COMPATIBILITY_PROFILE_HASH, violations);\n\
           if (!deployment.status) {\n\
             violations.push(\"deployment record has no status; expected 'active'\");\n\
           } else if (deployment.status !== \"active\") {\n\
             violations.push(\"deployment status is '\" + deployment.status + \"'\");\n\
           }\n\
           compareDeploymentIdentity(\"compiler_version\", deployment.compiler_version, GENERATED_COMPILER_VERSION, violations);\n\
           compareDeploymentIdentity(\"artifact_hash\", deployment.artifact_hash, GENERATED_ARTIFACT_HASH, violations);\n\
           compareDeploymentIdentity(\"metadata_hash\", deployment.metadata_hash, GENERATED_METADATA_HASH, violations);\n\
           compareDeploymentIdentity(\"schema_hash\", deployment.schema_hash, GENERATED_SCHEMA_HASH, violations);\n\
           compareDeploymentIdentity(\"cell_data_codec_manifest_hash\", deployment.cell_data_codec_manifest_hash, GENERATED_CELL_DATA_CODEC_MANIFEST_HASH, violations);\n\
           compareDeploymentIdentity(\"abi_hash\", deployment.abi_hash, GENERATED_ABI_HASH, violations);\n\
           compareDeploymentIdentity(\"constraints_hash\", deployment.constraints_hash, GENERATED_CONSTRAINTS_HASH, violations);\n\
\n\
           const lockDeployment = lockfile?.deployment?.[deployment.network];\n\
           if (lockfile && !lockDeployment) {\n\
             violations.push(\"Cell.lock has no deployment ref for network '\" + deployment.network + \"'\");\n\
           } else if (lockDeployment) {\n\
             compareHexIdentity(\"deployment.code_hash\", lockDeployment.code_hash, deployment.code_hash, violations);\n\
             compareStringIdentity(\"deployment.out_point\", lockDeployment.out_point, deployment.out_point, violations);\n\
             compareHexIdentity(\"deployment.data_hash\", lockDeployment.data_hash, deployment.data_hash, violations);\n\
           }\n\
\n\
           const embeddedDeployments = BUILDER_MANIFEST_RUNTIME.deployment_identity?.deployments ?? [];\n\
           if (embeddedDeployments.length > 0) {\n\
             const embedded = embeddedDeployments.find((item) => item.network === deployment.network);\n\
             if (!embedded) {\n\
               violations.push(\"builder manifest has no embedded deployment for network '\" + deployment.network + \"'\");\n\
             } else {\n\
               compareHexIdentity(\"embedded_deployment.code_hash\", deployment.code_hash, embedded.code_hash, violations);\n\
               compareStringIdentity(\"embedded_deployment.out_point\", deployment.out_point, embedded.out_point, violations);\n\
               compareHexIdentity(\"embedded_deployment.data_hash\", deployment.data_hash, embedded.data_hash, violations);\n\
               compareStringIdentity(\"embedded_deployment.hash_type\", deployment.hash_type, embedded.hash_type, violations);\n\
               if (embedded.status || deployment.status) {\n\
                 compareStringIdentity(\"embedded_deployment.status\", deployment.status, embedded.status, violations);\n\
               }\n\
               if (embedded.type_id || deployment.type_id) {\n\
                 compareHexIdentity(\"embedded_deployment.type_id\", deployment.type_id, embedded.type_id, violations);\n\
               }\n\
             }\n\
           }\n\
\n\
           if (liveEvidence) {\n\
             if (liveEvidence.status && liveEvidence.status !== \"live-verified\") {\n\
               violations.push(\"live deployment evidence status is '\" + liveEvidence.status + \"'\");\n\
             }\n\
             if (liveEvidence.rpc_status && liveEvidence.rpc_status !== \"live\") {\n\
               violations.push(\"live deployment RPC status is '\" + liveEvidence.rpc_status + \"'\");\n\
             }\n\
             if (!liveEvidence.deployment_status) {\n\
               violations.push(\"live deployment evidence has no deployment_status\");\n\
             } else if (liveEvidence.deployment_status !== \"active\") {\n\
               violations.push(\"live deployment evidence deployment_status is '\" + liveEvidence.deployment_status + \"'\");\n\
             }\n\
             if (liveEvidence.violations && liveEvidence.violations.length > 0) {\n\
               violations.push(\"live deployment evidence reports violations: \" + liveEvidence.violations.join(\"; \"));\n\
             }\n\
             compareStringIdentity(\"live_deployment.out_point\", liveEvidence.out_point, deployment.out_point, violations);\n\
             compareHexIdentity(\"live_deployment.data_hash\", liveEvidence.rpc_data_hash, deployment.data_hash, violations);\n\
             compareHexIdentity(\"live_deployment.code_hash\", liveEvidence.rpc_code_hash, deployment.code_hash, violations);\n\
           }\n\
           return violations;\n\
         }\n\n\
         export function validateCellScriptDeploymentTrust(\n\
           deployment?: CellScriptDeploymentRecord,\n\
           trustPolicy?: CellScriptTrustPolicy,\n\
         ): string[] {\n\
           const violations: string[] = [];\n\
           const requirePublisherSignature = trustPolicy?.requirePublisherSignature ?? false;\n\
           const requireAuditReportHash = trustPolicy?.requireAuditReportHash ?? false;\n\
           if (!requirePublisherSignature && !requireAuditReportHash) {\n\
             return violations;\n\
           }\n\
           if (!deployment) {\n\
             violations.push(\"trust policy requires a deployment record\");\n\
             return violations;\n\
           }\n\
           if (requirePublisherSignature && !deployment.publisher_signature) {\n\
             violations.push(\"deployment record has no publisher_signature required by trust policy\");\n\
           }\n\
           if (requireAuditReportHash && !deployment.audit_report_hash) {\n\
             violations.push(\"deployment record has no audit_report_hash required by trust policy\");\n\
           }\n\
           return violations;\n\
         }\n\n\
         export function assertCellScriptDeployment(\n\
           lockfile?: CellScriptLockfile,\n\
           deployment?: CellScriptDeploymentRecord,\n\
           liveEvidence?: CellScriptLiveDeploymentEvidence,\n\
           trustPolicy?: CellScriptTrustPolicy,\n\
         ): void {\n\
           const violations = validateCellScriptDeployment(lockfile, deployment, liveEvidence, trustPolicy);\n\
           if (violations.length > 0) {\n\
             throw new Error(\"CellScript deployment identity mismatch: \" + violations.join(\"; \"));\n\
           }\n\
         }\n\n\
         function compareRequiredIdentity(\n\
           field: string,\n\
           actual: string | null | undefined,\n\
           expected: string | null,\n\
           violations: string[],\n\
         ): void {\n\
           if (expected === null || expected === \"\") {\n\
             violations.push(\"generated metadata has no \" + field);\n\
             return;\n\
           }\n\
           if (actual === undefined || actual === null || actual === \"\") {\n\
             violations.push(\"Cell.lock has no \" + field);\n\
             return;\n\
           }\n\
           if (actual !== expected) {\n\
             violations.push(field + \" mismatch: Cell.lock has '\" + actual + \"', metadata has '\" + expected + \"'\");\n\
           }\n\
         }\n\n\
         function compareDeploymentIdentity(\n\
           field: string,\n\
           actual: string | null | undefined,\n\
           expected: string | null,\n\
           violations: string[],\n\
         ): void {\n\
           if (expected === null || expected === \"\") {\n\
             violations.push(\"generated metadata has no \" + field);\n\
             return;\n\
           }\n\
           if (actual === undefined || actual === null || actual === \"\") {\n\
             violations.push(\"deployment record has no \" + field);\n\
             return;\n\
           }\n\
           if (!identityEquals(actual, expected)) {\n\
             violations.push(field + \" mismatch: deployment has '\" + actual + \"', metadata has '\" + expected + \"'\");\n\
           }\n\
         }\n\n\
         function compareStringIdentity(\n\
           field: string,\n\
           actual: string | null | undefined,\n\
           expected: string | null | undefined,\n\
           violations: string[],\n\
         ): void {\n\
           if (expected === undefined || expected === null || expected === \"\") {\n\
             violations.push(\"expected \" + field + \" is missing\");\n\
             return;\n\
           }\n\
           if (actual === undefined || actual === null || actual === \"\") {\n\
             violations.push(field + \" is missing\");\n\
             return;\n\
           }\n\
           if (actual !== expected) {\n\
             violations.push(field + \" mismatch: actual '\" + actual + \"', expected '\" + expected + \"'\");\n\
           }\n\
         }\n\n\
         function compareHexIdentity(\n\
           field: string,\n\
           actual: string | null | undefined,\n\
           expected: string | null | undefined,\n\
           violations: string[],\n\
         ): void {\n\
           if (expected === undefined || expected === null || expected === \"\") {\n\
             violations.push(\"expected \" + field + \" is missing\");\n\
             return;\n\
           }\n\
           if (actual === undefined || actual === null || actual === \"\") {\n\
             violations.push(field + \" is missing\");\n\
             return;\n\
           }\n\
           if (!hexEquals(actual, expected)) {\n\
             violations.push(field + \" mismatch: actual '\" + actual + \"', expected '\" + expected + \"'\");\n\
           }\n\
         }\n\n\
         function identityEquals(actual: string, expected: string): boolean {\n\
           return actual === expected || hexEquals(actual, expected);\n\
         }\n\n\
         function hexEquals(actual: string, expected: string): boolean {\n\
           return actual.replace(/^0x/i, \"\").toLowerCase() === expected.replace(/^0x/i, \"\").toLowerCase();\n\
         }\n\n",
    );

    ts.push_str(
        r#"export interface BoundedOutputPlanOutput {
  ordinal: number;
  outputType: string;
  data: Uint8Array;
  lockHash: Uint8Array;
  minimumCapacityShannons: string;
  typeScriptPolicy: "exact-current-script-hash";
}

export interface BoundedOutputPlanMaterialization {
  schema: "cellscript-bounded-output-plan-materialization-v1";
  contract: Readonly<Record<string, unknown>>;
  binding: string;
  count: number;
  witnessPayload: Uint8Array;
  outputs: readonly BoundedOutputPlanOutput[];
}

function boundedOutputBytes(value: unknown, label: string): Uint8Array {
  if (value instanceof Uint8Array) return new Uint8Array(value);
  if (typeof value !== "string" || !/^0x(?:[0-9a-fA-F]{2})*$/.test(value)) {
    throw new Error("CellScript bounded output plan '" + label + "' must be an even-length 0x hex string or Uint8Array");
  }
  const bytes = new Uint8Array((value.length - 2) / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(value.slice(2 + index * 2, 4 + index * 2), 16);
  }
  return bytes;
}

function boundedOutputInteger(record: Record<string, unknown>, field: string): number {
  const value = record[field];
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new Error("CellScript bounded output plan contract field '" + field + "' must be a non-negative safe integer");
  }
  return value;
}

function boundedOutputU32Le(bytes: Uint8Array, offset: number): number {
  return (bytes[offset] | (bytes[offset + 1] << 8) | (bytes[offset + 2] << 16) | (bytes[offset + 3] << 24)) >>> 0;
}

export function encodeBoundedOutputPlanV1(
  elements: readonly (HexString | Uint8Array)[],
  elementWidthBytes: number,
  maxElements: number,
): Uint8Array {
  if (!Number.isSafeInteger(elementWidthBytes) || elementWidthBytes < 1
    || !Number.isSafeInteger(maxElements) || maxElements < 1 || elements.length > maxElements) {
    throw new Error("CellScript bounded output plan encoder received an invalid width, bound, or element count");
  }
  const totalBytes = 12 + elements.length * elementWidthBytes;
  if (totalBytes > 4084) {
    throw new Error("CellScript bounded output plan exceeds the 4084-byte runtime limit");
  }
  const output = new Uint8Array(totalBytes);
  output.set([0x43, 0x53, 0x42, 0x50, 0x4c, 0x76, 0x31, 0x00], 0);
  new DataView(output.buffer, output.byteOffset, output.byteLength).setUint32(8, elements.length, true);
  elements.forEach((element, index) => {
    const bytes = boundedOutputBytes(element, "element[" + index + "]");
    if (bytes.length !== elementWidthBytes) {
      throw new Error("CellScript bounded output plan element[" + index + "] has width " + bytes.length + ", expected " + elementWidthBytes);
    }
    output.set(bytes, 12 + index * elementWidthBytes);
  });
  return output;
}

export function materializeBoundedOutputPlanV1(
  contractValue: unknown,
  payloadValue: HexString | Uint8Array,
): BoundedOutputPlanMaterialization {
  const contract = assertRuntimeObject(contractValue, "bounded output plan contract");
  if (contract.schema !== "cellscript-bounded-output-plan-contract" || contract.version !== 1
    || contract.codec !== "molecule-fixvec-fixed-item-v1" || contract.codec_magic_hex !== "0x435342504c763100"
    || contract.ordering !== "plan-index-equals-group-output-index"
    || contract.correspondence !== "exactly-one-group-output-per-plan-element") {
    throw new Error("CellScript bounded output plan contract identity or ordering is invalid");
  }
  const binding = contract.binding;
  const outputType = contract.output_ty;
  if (typeof binding !== "string" || binding.length === 0 || typeof outputType !== "string" || outputType.length === 0) {
    throw new Error("CellScript bounded output plan contract has no binding or output type");
  }
  const elementWidth = boundedOutputInteger(contract, "element_width_bytes");
  const maxElements = boundedOutputInteger(contract, "max_elements");
  const outputWidth = boundedOutputInteger(contract, "output_width_bytes");
  const capacityFloor = boundedOutputInteger(contract, "capacity_floor_shannons");
  if (elementWidth < 1 || maxElements < 1 || outputWidth < 1 || outputWidth > 512 || capacityFloor < 1) {
    throw new Error("CellScript bounded output plan contract has invalid resource bounds");
  }
  const payload = boundedOutputBytes(payloadValue, binding);
  const magic = [0x43, 0x53, 0x42, 0x50, 0x4c, 0x76, 0x31, 0x00];
  if (payload.length < 12 || magic.some((byte, index) => payload[index] !== byte)) {
    throw new Error("CellScript bounded output plan '" + binding + "' has invalid CSBPLv1 magic");
  }
  const count = boundedOutputU32Le(payload, 8);
  const expectedLength = 12 + count * elementWidth;
  if (count > maxElements || expectedLength !== payload.length || payload.length > 4084) {
    throw new Error("CellScript bounded output plan '" + binding + "' violates its exact count/length bound");
  }
  if (!Array.isArray(contract.field_bindings) || contract.field_bindings.length === 0) {
    throw new Error("CellScript bounded output plan contract has no field bindings");
  }
  const lockBinding = assertRuntimeObject(contract.lock_binding, "bounded output lock binding");
  const lockOffset = boundedOutputInteger(lockBinding, "plan_offset_bytes");
  const lockWidth = boundedOutputInteger(lockBinding, "width_bytes");
  if (lockWidth !== 32 || lockOffset + lockWidth > elementWidth) {
    throw new Error("CellScript bounded output lock binding is outside the plan element");
  }
  const outputs: BoundedOutputPlanOutput[] = [];
  for (let ordinal = 0; ordinal < count; ordinal += 1) {
    const elementStart = 12 + ordinal * elementWidth;
    const data = new Uint8Array(outputWidth);
    const written = new Uint8Array(outputWidth);
    for (const candidate of contract.field_bindings) {
      const field = assertRuntimeObject(candidate, "bounded output field binding");
      const planOffset = boundedOutputInteger(field, "plan_offset_bytes");
      const outputOffset = boundedOutputInteger(field, "output_offset_bytes");
      const width = boundedOutputInteger(field, "width_bytes");
      if (width < 1 || planOffset + width > elementWidth || outputOffset + width > outputWidth) {
        throw new Error("CellScript bounded output field binding is outside its fixed layout");
      }
      for (let byte = outputOffset; byte < outputOffset + width; byte += 1) {
        if (written[byte] !== 0) throw new Error("CellScript bounded output field bindings overlap");
        written[byte] = 1;
      }
      data.set(payload.subarray(elementStart + planOffset, elementStart + planOffset + width), outputOffset);
    }
    if (written.some((byte) => byte !== 1)) {
      throw new Error("CellScript bounded output field bindings do not cover the complete output data");
    }
    outputs.push({
      ordinal,
      outputType,
      data,
      lockHash: payload.slice(elementStart + lockOffset, elementStart + lockOffset + lockWidth),
      minimumCapacityShannons: capacityFloor.toString(),
      typeScriptPolicy: "exact-current-script-hash",
    });
  }
  return {
    schema: "cellscript-bounded-output-plan-materialization-v1",
    contract: contract as Readonly<Record<string, unknown>>,
    binding,
    count,
    witnessPayload: payload,
    outputs,
  };
}

function materializeBoundedOutputPlans(
  contracts: readonly unknown[],
  params: CellScriptParams,
): BoundedOutputPlanMaterialization[] {
  const values = params as Record<string, unknown>;
  return contracts.map((contractValue) => {
    const contract = assertRuntimeObject(contractValue, "bounded output plan contract");
    const binding = contract.binding;
    if (typeof binding !== "string" || !(binding in values)) {
      throw new Error("CellScript bounded output plan parameter is missing");
    }
    return materializeBoundedOutputPlanV1(contract, values[binding] as HexString | Uint8Array);
  });
}

"#,
    );

    for action in actions {
        let suffix = typescript_type_suffix(&action.name);
        let params_type = typescript_action_params_type(action);
        ts.push_str(&format!("export interface {suffix}Params {{\n"));
        for param in &action.params {
            ts.push_str(&format!("  {}: {};\n", typescript_object_key(&param.name), typescript_param_type(param)));
        }
        if action.params.is_empty() {
            ts.push_str("  readonly __noParams?: never;\n");
        }
        ts.push_str("}\n\n");
        ts.push_str(&format!(
            "export function plan{suffix}(params: {params_type}, options: BuildOptions = {{}}): ActionBuilderPlan<{params_type}> {{\n  \
             return makeActionPlan({}, params, options);\n}}\n\n",
            typescript_string_literal(&action.name)
        ));
    }

    ts.push_str(
        "function assertRuntimeAccessParams(accesses: readonly unknown[], params: CellScriptParams): void {\n  \
         const values = params as Record<string, unknown>;\n  \
         for (const candidate of accesses) {\n    \
           if (typeof candidate !== \"object\" || candidate === null || Array.isArray(candidate)) {\n      continue;\n    }\n    \
           const access = candidate as Record<string, unknown>;\n    \
           const provenanceValue = access.provenance;\n    \
           if (typeof provenanceValue !== \"object\" || provenanceValue === null || Array.isArray(provenanceValue)) {\n      continue;\n    }\n    \
           const provenance = provenanceValue as Record<string, unknown>;\n    \
           const indexValue = provenance.index;\n    \
           if (typeof indexValue !== \"object\" || indexValue === null || Array.isArray(indexValue)) {\n      continue;\n    }\n    \
           const index = indexValue as Record<string, unknown>;\n    \
           if (index.kind !== \"dynamic\" || typeof index.binding !== \"string\" || !(index.binding in values)) {\n      continue;\n    }\n    \
           const value = runtimeUnsignedBigInt(values[index.binding], index.binding);\n    \
           const maximum = runtimeUnsignedBigInt(index.max_inclusive, index.binding + \" max_inclusive\");\n    \
           if (value > maximum) {\n      \
             throw new Error(\"CellScript runtime access index '\" + index.binding + \"' exceeds max_inclusive \" + maximum.toString());\n    \
           }\n  \
         }\n\
       }\n\n\
         function runtimeUnsignedBigInt(value: unknown, label: string): bigint {\n  \
           if (typeof value === \"bigint\") {\n    \
             if (value < 0n) {\n      throw new Error(\"CellScript runtime access index '\" + label + \"' must be unsigned\");\n    }\n    \
             return value;\n  \
           }\n  \
           if (typeof value === \"number\") {\n    \
             if (!Number.isSafeInteger(value) || value < 0) {\n      \
               throw new Error(\"CellScript runtime access index '\" + label + \"' must be a non-negative safe integer\");\n    \
             }\n    \
             return BigInt(value);\n  \
           }\n  \
           if (typeof value === \"string\" && /^(0|[1-9]\\d*)$/.test(value)) {\n    return BigInt(value);\n  }\n  \
           throw new Error(\"CellScript runtime access index '\" + label + \"' must be bigint, integer number, or unsigned decimal string\");\n\
         }\n\n\
         function makeActionPlan<P extends CellScriptParams>(action: string, params: P, options: BuildOptions): ActionBuilderPlan<P> {\n  \
         const actionSpec = actionSpecs.find((item) => item.name === action);\n  \
         if (!actionSpec) {\n    throw new Error(\"CellScript generated builder has no action spec for '\" + action + \"'\");\n  }\n",
    );
    ts.push_str("  if (options.lockfile) {\n    assertCellScriptLockfile(options.lockfile);\n  }\n");
    ts.push_str(
        "  if (options.deployment || options.liveDeploymentEvidence || options.trustPolicy) {\n    \
         assertCellScriptDeployment(options.lockfile, options.deployment, options.liveDeploymentEvidence, options.trustPolicy);\n  }\n",
    );
    ts.push_str("  assertRuntimeAccessParams(actionSpec.runtimeAccesses, params);\n");
    ts.push_str(&format!(
        "  return {{\n    schema: CELLSCRIPT_BUILDER_SCHEMA,\n    state: \"GeneratedActionPlan\",\n    status: \"requires-runtime-resolution\",\n    action,\n    params,\n    options,\n    metadataHash: {},\n    artifactHash: {},\n    targetProfile: {},\n    canSubmit: false,\n    requiresLiveCellResolution: true,\n    requiresDeploymentResolution: true,\n    cellDataCodecManifest,\n    temporalContract,\n    runtimeAccessProvenanceContract,\n    signingMessageDomains,\n    runtimeAccesses: [...actionSpec.runtimeAccesses],\n    runtimeInputRequirements: [...actionSpec.runtimeInputRequirements],\n    actionScanSelectors: actionSpec.actionScanSelectors as ActionScanSelectors,\n    verifierObligations: [...actionSpec.verifierObligations],\n    failClosedRuntimeFeatures: [...actionSpec.failClosedRuntimeFeatures],\n    boundedOutputPlans: materializeBoundedOutputPlans(actionSpec.boundedOutputPlans, params),\n    notProvenByGeneratedBuilder: [\n      \"live_cell_availability\",\n      \"deployment_live_chain_match\",\n      \"capacity_fee_balance\",\n      \"signature_authority\",\n      \"ckb_vm_execution\",\n      \"cell_data_codec_materialization\",\n      \"tx_pool_acceptance\",\n      \"submission\"\n    ] as const,\n  }};\n}}\n\n",
        typescript_string_literal(metadata_hash),
        metadata.artifact_hash.as_deref().map(typescript_string_literal).unwrap_or_else(|| "null".to_string()),
        typescript_string_literal(&metadata.target_profile.name)
    ));

    ts.push_str(
        "function assertRuntimeObject(value: unknown, label: string): Record<string, unknown> {\n\
           if (typeof value !== \"object\" || value === null || Array.isArray(value)) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: \" + label + \" must be an object\");\n\
           }\n\
           return value as Record<string, unknown>;\n\
         }\n\n\
         function assertLiveCellResolutionResult<P extends CellScriptParams>(value: unknown, plan: ActionBuilderPlan<P>): LiveCellResolutionResult {\n\
           const record = assertRuntimeObject(value, \"resolveLiveCells result\");\n\
           for (const field of [\"inputs\", \"referenceInputs\", \"cellDeps\", \"headerDeps\", \"lineage\"] as const) {\n\
             const candidate = record[field];\n\
             if (candidate !== undefined && !Array.isArray(candidate)) {\n\
               throw new Error(\"CellScript runtime builder-shape mismatch: resolveLiveCells.\" + field + \" must be an array when present\");\n\
             }\n\
           }\n\
           assertScanSelectorEvidence(record.scanSelectorEvidence, plan);\n\
           return value as LiveCellResolutionResult;\n\
         }\n\n\
         function assertScanSelectorEvidence<P extends CellScriptParams>(value: unknown, plan: ActionBuilderPlan<P>): void {\n\
           const selectors = actionScanSelectorItems(plan.actionScanSelectors);\n\
           if (selectors.length === 0) {\n\
             if (value !== undefined && !Array.isArray(value)) {\n\
               throw new Error(\"CellScript runtime builder-shape mismatch: resolveLiveCells.scanSelectorEvidence must be an array when present\");\n\
             }\n\
             return;\n\
           }\n\
           if (!Array.isArray(value)) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: resolveLiveCells.scanSelectorEvidence is required for action scan selectors\");\n\
           }\n\
           if (value.length !== selectors.length) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: resolveLiveCells.scanSelectorEvidence length \" + value.length + \" does not match actionScanSelectors.selector_count \" + selectors.length);\n\
           }\n\
         const selectorsByIndex = new Map<number, Record<string, unknown>>();\n\
         const declaredSelectorIndexes: number[] = [];\n\
         selectors.forEach((selector, fallbackIndex) => {\n\
           const index = selectorIndex(selector, fallbackIndex);\n\
           if (selectorsByIndex.has(index)) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: actionScanSelectors contains duplicate selector_index \" + index);\n\
           }\n\
           selectorsByIndex.set(index, selector);\n\
           declaredSelectorIndexes.push(index);\n\
         });\n\
         const seenEvidenceIndexes = new Set<number>();\n\
         for (const item of value) {\n\
           const evidence = assertRuntimeObject(item, \"scanSelectorEvidence item\");\n\
           const rawIndex = evidence.selector_index;\n\
           if (typeof rawIndex !== \"number\" || !Number.isInteger(rawIndex)) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: scanSelectorEvidence.selector_index must be an integer\");\n\
           }\n\
           if (seenEvidenceIndexes.has(rawIndex)) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: scanSelectorEvidence contains duplicate selector_index \" + rawIndex);\n\
           }\n\
           const selector = selectorsByIndex.get(rawIndex);\n\
           if (!selector) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: scanSelectorEvidence selector_index \" + rawIndex + \" is not declared by actionScanSelectors\");\n\
           }\n\
           seenEvidenceIndexes.add(rawIndex);\n\
           if (evidence.status !== \"resolved\") {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: scanSelectorEvidence.status for selector \" + rawIndex + \" must be 'resolved'\");\n\
           }\n\
             assertSelectorEvidenceField(evidence, selector, \"source\", \"ckb_source\", rawIndex);\n\
             assertSelectorEvidenceField(evidence, selector, \"role\", \"role\", rawIndex);\n\
             assertSelectorEvidenceField(evidence, selector, \"binding\", \"binding\", rawIndex);\n\
             assertSelectorEvidenceField(evidence, selector, \"feature\", \"feature\", rawIndex);\n\
           assertSelectorEvidenceField(evidence, selector, \"component\", \"component\", rawIndex);\n\
           assertSelectorEvidenceField(evidence, selector, \"script_field\", \"script_field\", rawIndex);\n\
         }\n\
         for (const index of declaredSelectorIndexes) {\n\
           if (!seenEvidenceIndexes.has(index)) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: scanSelectorEvidence is missing selector_index \" + index);\n\
           }\n\
         }\n\
       }\n\n\
         function actionScanSelectorItems(actionScanSelectors: ActionScanSelectors): Record<string, unknown>[] {\n\
           const selectors = actionScanSelectors.selectors;\n\
           return Array.isArray(selectors) ? selectors.filter(isRuntimeRecord) : [];\n\
         }\n\n\
         function isRuntimeRecord(value: unknown): value is Record<string, unknown> {\n\
           return typeof value === \"object\" && value !== null && !Array.isArray(value);\n\
         }\n\n\
         function selectorIndex(selector: Record<string, unknown>, fallbackIndex: number): number {\n\
           const value = selector.selector_index;\n\
           return typeof value === \"number\" && Number.isInteger(value) ? value : fallbackIndex;\n\
         }\n\n\
         function assertSelectorEvidenceField(\n\
           evidence: Record<string, unknown>,\n\
           selector: Record<string, unknown>,\n\
           evidenceField: string,\n\
           selectorField: string,\n\
           selectorIndex: number,\n\
         ): void {\n\
         const expected = selector[selectorField];\n\
         const actual = evidence[evidenceField];\n\
         if (expected === undefined || expected === null) {\n\
           if (actual !== undefined && actual !== null) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: scanSelectorEvidence.\" + evidenceField + \" unexpected for selector \" + selectorIndex);\n\
           }\n\
           return;\n\
         }\n\
         if (actual === undefined || actual === null) {\n\
           throw new Error(\"CellScript runtime builder-shape mismatch: scanSelectorEvidence.\" + evidenceField + \" missing for selector \" + selectorIndex);\n\
         }\n\
         if (actual !== expected) {\n\
           throw new Error(\"CellScript runtime builder-shape mismatch: scanSelectorEvidence.\" + evidenceField + \" mismatch for selector \" + selectorIndex);\n\
         }\n\
         }\n\n\
         function assertBuiltTransaction(value: unknown): unknown {\n\
           if (value === undefined || value === null) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: buildTransaction returned no transaction\");\n\
           }\n\
           return value;\n\
         }\n\n\
         function submittedTxHashFromRuntime(submitResult: unknown): string | null {\n\
           if (typeof submitResult === \"string\") {\n\
             return submitResult;\n\
           }\n\
           if (typeof submitResult === \"object\" && submitResult !== null) {\n\
             const record = submitResult as Record<string, unknown>;\n\
             if (typeof record.txHash === \"string\") {\n\
               return record.txHash;\n\
             }\n\
             if (typeof record.hash === \"string\") {\n\
               return record.hash;\n\
             }\n\
           }\n\
           return null;\n\
         }\n\n",
    );

    ts.push_str("export function createActionBuilder(runtime: CellScriptBuilderRuntime) {\n  return {\n");
    for action in actions {
        let method = typescript_identifier(&action.name, "action");
        let suffix = typescript_type_suffix(&action.name);
        let params_type = typescript_action_params_type(action);
        ts.push_str(&format!(
            "    async {method}(params: {params_type}, options: BuildOptions = {{}}) {{\n      \
             const plan = plan{suffix}(params, options);\n      \
             return executeActionBuilderPlan(runtime, plan, options);\n    }},\n"
        ));
    }
    ts.push_str(
        "  };\n}\n\n\
         async function executeActionBuilderPlan<P extends CellScriptParams>(\n\
           runtime: CellScriptBuilderRuntime,\n\
           plan: ActionBuilderPlan<P>,\n\
           options: BuildOptions,\n\
         ): Promise<ActionBuilderResult<P>> {\n\
           const liveCellResolution = assertLiveCellResolutionResult(await runtime.resolveLiveCells({ plan, options }), plan);\n\
           const transaction = assertBuiltTransaction(await runtime.buildTransaction({ ...plan, liveCellResolution }));\n\
           const result: ActionBuilderResult<P> = {\n\
             schema: CELLSCRIPT_BUILDER_SCHEMA,\n\
             state: \"ActionBuilderResult\",\n\
             status: \"built-by-runtime\",\n\
             mode: \"build\",\n\
             plan,\n\
             liveCellResolution,\n\
             transaction,\n\
             canSubmit: false,\n\
             notProvenByGeneratedBuilder: plan.notProvenByGeneratedBuilder,\n\
           };\n\
           if (options.dryRun || options.submit) {\n\
             if (!runtime.dryRun) {\n\
               throw new Error(\"CellScript builder runtime missing dryRun adapter\");\n\
             }\n\
             result.dryRunResult = await runtime.dryRun(transaction);\n\
             result.status = \"dry-run-by-runtime\";\n\
             result.mode = \"dry-run\";\n\
           }\n\
           if (options.submit) {\n\
             if (!runtime.submit) {\n\
               throw new Error(\"CellScript builder runtime missing submit adapter\");\n\
             }\n\
             result.submitResult = await runtime.submit(transaction);\n\
             result.submittedTxHash = submittedTxHashFromRuntime(result.submitResult);\n\
             result.status = \"submitted-by-runtime\";\n\
             result.mode = \"submit\";\n\
           }\n\
           return result;\n\
         }\n",
    );

    if let Some(policy) = &metadata.runtime.policy_artifact {
        // The existing typed runtime contract remains intact. Its plan now
        // carries an explicit outer-envelope obligation for policy artifacts.
        ts = ts.replace("canSubmit: false;\n", "canSubmit: false;\n  policyWitness?: Readonly<Record<string, unknown>>;\n");
        ts = ts.replace(
            "    state: \"GeneratedActionPlan\",\n",
            "    state: \"GeneratedActionPlan\",\n    policyWitness: policyWitnessRequest(action),\n",
        );
        ts.push_str(&format!("\nexport const policyArtifact = {} as const;\n", json_string_pretty("policy artifact", policy)?));
        let tags = actions
            .iter()
            .map(|action| {
                let variant = policy.declaration.action(&action.name).expect("selected exported action");
                (action.name.clone(), serde_json::json!({ "tag": variant.tag, "requiresArgs": action_requires_entry_witness(action) }))
            })
            .collect::<BTreeMap<_, _>>();
        ts.push_str(&format!(
            "const POLICY_VARIANTS: Readonly<Record<string, {{ tag: number; requiresArgs: boolean }}>> = Object.freeze(JSON.parse({}));\n",
            typescript_string_literal(&json_string_pretty("policy variants", &tags)?)
        ));
        ts.push_str(include_str!("../policy_builder.ts"));
    }
    Ok(ts)
}

fn typescript_builder_test(metadata: &CompileMetadata, actions: &[&crate::ActionMetadata]) -> Result<String> {
    let cases = actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "name": action.name,
                "plan": format!("plan{}", typescript_type_suffix(&action.name)),
                "method": typescript_identifier(&action.name, "action"),
                "params": javascript_sample_params(action, &bounded_output_plan_contracts_for_action(metadata, &action.name)),
            })
        })
        .collect::<Vec<_>>();
    let cases_json = json_string_pretty("builder test cases", &cases)?;
    let bounded_output_plan_cases = actions
        .iter()
        .flat_map(|action| {
            bounded_output_plan_contracts_for_action(metadata, &action.name).into_iter().map(|contract| {
                serde_json::json!({
                    "name": action.name,
                    "plan": format!("plan{}", typescript_type_suffix(&action.name)),
                    "binding": contract.binding,
                    "elementWidth": contract.element_width_bytes,
                    "maxElements": contract.max_elements,
                    "outputWidth": contract.output_width_bytes,
                })
            })
        })
        .collect::<Vec<_>>();
    let bounded_output_plan_cases_json = json_string_pretty("bounded output plan builder test cases", &bounded_output_plan_cases)?;
    let dynamic_index_cases = actions
        .iter()
        .flat_map(|action| {
            action.ckb_runtime_accesses.iter().filter_map(|access| {
                let index = &access.provenance.index;
                if index.kind != "dynamic" {
                    return None;
                }
                let binding = index.binding.as_deref()?;
                let max_inclusive = index.max_inclusive?;
                action.params.iter().any(|param| param.name == binding).then(|| {
                    serde_json::json!({
                        "name": action.name,
                        "plan": format!("plan{}", typescript_type_suffix(&action.name)),
                        "binding": binding,
                        "maxInclusive": max_inclusive,
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    let dynamic_index_cases = dynamic_index_cases
        .into_iter()
        .fold(BTreeMap::<(String, String), serde_json::Value>::new(), |mut unique, value| {
            let key = (
                value["name"].as_str().expect("dynamic index action name").to_string(),
                value["binding"].as_str().expect("dynamic index binding").to_string(),
            );
            unique.entry(key).or_insert(value);
            unique
        })
        .into_values()
        .collect::<Vec<_>>();
    let dynamic_index_cases_json = json_string_pretty("dynamic source-index builder test cases", &dynamic_index_cases)?;

    let mut js = String::new();
    js.push_str("import assert from \"node:assert/strict\";\n");
    js.push_str("import test from \"node:test\";\n");
    js.push_str("import * as builder from \"../dist/index.js\";\n\n");
    js.push_str(&format!("const actionCases = {cases_json};\n"));
    let entry_cases = crate::cli::entry_witness_builder::test_cases(metadata, actions);
    js.push_str(&format!("const entryCases = {};\n", json_string_pretty("entry codec test cases", &entry_cases)?));
    let bundle_entry = actions
        .first()
        .map(|entry| serde_json::json!({"kind":"action","name":entry.name}))
        .or_else(|| metadata.locks.first().map(|entry| serde_json::json!({"kind":"lock","name":entry.name})));
    js.push_str(&format!("const bundleEntry = {};\n", json_string_pretty("bundle test entry", &bundle_entry)?));
    js.push_str("const actionTestOptions = { skip: actionCases.length === 0 ? \"This package declares no actions; Lock codecs and ProtocolBundle have separate tests\" : false };\n");
    js.push_str(include_str!("../entry_witness_builder_test.mjs"));
    js.push_str(&format!("const dynamicIndexCases = {dynamic_index_cases_json};\n"));
    js.push_str(&format!("const boundedOutputPlanCases = {bounded_output_plan_cases_json};\n"));
    js.push_str("const WRONG_HASH = \"0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\";\n\n");
    js.push_str(
        "test(\"exposes a resumable ProtocolBundle runtime state machine without private keys\", { skip: bundleEntry === null }, async () => {\n\
           const artifactHash = builder.protocolBundleArtifactIdentity.artifactHash;\n\
           assert.equal(typeof artifactHash, \"string\");\n\
           const deployment = {\n\
             network: { chainId: \"ckb-testnet\", genesisHash: \"0x\" + \"0\".repeat(64) },\n\
             artifactHash,\n\
             script: { codeHash: \"0x\" + artifactHash, hashType: \"data2\", args: \"0x\" },\n\
             codeCellDep: { outPoint: { txHash: \"0x\" + \"1\".repeat(64), index: 0 }, depType: \"code\" },\n\
           };\n\
           const first = builder.bindProtocolBundleArtifact({\n\
             id: \"first\", packageCoordinate: \"example/first@1.0.0\",\n\
             lockNodeId: \"first@1.0.0|path:first|env=default|features=default\",\n\
             entry: bundleEntry, scriptRole: bundleEntry.kind === \"lock\" ? \"lock\" : \"type\", deployment,\n\
           });\n\
           const second = Object.freeze({ ...first, id: \"second\" });\n\
           const sharedSchema = first.schemaContracts[0] ?? { type_name: \"AbsentTutorialSchema\", schema_hash: \"a\".repeat(64) };\n\
           const roleOptions = {\n\
             roleId: \"shared-token\", kind: \"cell\", schemaIdentity: sharedSchema,\n\
             provider: { artifact: first, claim: \"shared-output\" },\n\
             consumers: [{ artifact: second, claim: \"shared-output\" }],\n\
           };\n\
           if (first.schemaContracts.length > 0) {\n\
             const closedRole = builder.bindClosedProtocolRole(roleOptions);\n\
             assert.equal(closedRole.schema, builder.PROTOCOL_CLOSED_ROLE_SCHEMA);\n\
             assert.deepEqual(closedRole.provider, { artifact: \"first\", claim: \"shared-output\" });\n\
           } else {\n\
             assert.throws(() => builder.bindClosedProtocolRole(roleOptions), /does not expose the closed-role schema identity/);\n\
           }\n\
           if (typeof first.verifiedBundleId === \"string\") {\n\
             const exactReceipt = {\n\
               schema: builder.EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA, version: builder.EXACT_SCRIPT_HANDLE_RECEIPT_VERSION, class: \"script\", script_role: first.scriptRole,\n\
               package_coordinate: first.packageCoordinate, lock_node_id: first.lockNodeId, entry: first.entry,\n\
               interface_hash: first.interfaceHash, typed_semantics_hash: first.typedSemanticsHash,\n\
               artifact_hash: first.artifactHash, target_profile_hash: first.targetProfileHash,\n\
               runtime_abi_hash: first.runtimeAbiHash, verified_bundle_id: first.verifiedBundleId,\n\
               deployment: {\n\
                 network: { chain_id: deployment.network.chainId, genesis_hash: deployment.network.genesisHash },\n\
                 artifact_hash: deployment.artifactHash,\n\
                 script: { code_hash: deployment.script.codeHash, hash_type: deployment.script.hashType, args: deployment.script.args },\n\
                 code_cell_dep: { out_point: { tx_hash: deployment.codeCellDep.outPoint.txHash, index: 0 }, dep_type: \"code\" },\n\
               },\n\
               code_identity_policy: \"data-hash-exact-artifact\", upgrade_policy: \"exact-artifact\",\n\
             };\n\
             const exactValue = {\n\
               schema: builder.EXACT_SCRIPT_HANDLE_VALUE_SCHEMA, version: 1, encoding: builder.EXACT_SCRIPT_HANDLE_ENCODING,\n\
               encoded: \"0x\" + \"0\".repeat(404),\n\
             };\n\
             const exactHandle = builder.bindCheckedExactScriptHandle(first, { exact_handle_receipt: exactReceipt, exact_handle: exactValue, exact_handle_hash: \"0\".repeat(64) });\n\
             assert.equal(exactHandle.value.encoding, \"CSHDLv1-fixed-202\");\n\
             assert.equal(exactHandle.handleHash, \"0\".repeat(64));\n\
             assert.throws(() => builder.bindCheckedExactScriptHandle(first, {\n\
               exact_handle_receipt: { ...exactReceipt, version: 2 }, exact_handle: exactValue, exact_handle_hash: \"0\".repeat(64),\n\
             }), /does not match generated artifact/);\n\
             assert.throws(() => builder.bindCheckedExactScriptHandle(first, {\n\
               exact_handle_receipt: { ...exactReceipt, runtime_abi_hash: WRONG_HASH.slice(2) }, exact_handle: exactValue, exact_handle_hash: \"0\".repeat(64),\n\
             }), /does not match generated artifact/);\n\
           } else {\n\
             assert.throws(() => builder.bindCheckedExactScriptHandle(first, {}), /no verified bundle identity/);\n\
           }\n\
           assert.throws(() => builder.bindClosedProtocolRole({\n\
             roleId: \"bad\", kind: \"cell\", schemaIdentity: sharedSchema,\n\
             provider: { artifact: first, claim: \"shared-output\" },\n\
             consumers: [{ artifact: first, claim: \"shared-input\" }],\n\
           }), /participants and claims must be distinct/);\n\
           const calls = [];\n\
           const stage = (state, previous, transaction = { state }) => ({\n\
             state, bundleHash: previous?.bundleHash ?? \"bundle-hash\",\n\
             rawTransactionHash: previous?.rawTransactionHash ?? \"raw-tx-hash\", transaction, evidence: { state },\n\
           });\n\
           const runtime = {\n\
             async checkBundle(_input, artifacts) { calls.push(\"check\"); assert.equal(artifacts.length, 2); const selected = artifacts.filter(a => a.exactRoleContracts !== undefined); return { schema: selected.length ? builder.PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA : builder.PROTOCOL_BUNDLE_REPORT_SCHEMA, status: \"ok\", bundle_hash: \"bundle-hash\", bundle: { schema: selected.length ? builder.PROTOCOL_BUNDLE_SELECTION_SCHEMA : builder.PROTOCOL_BUNDLE_SCHEMA, artifacts: selected.map(a => ({ id: a.id, exact_role_contracts: a.exactRoleContracts })) }, evidence: { exact_role_selections: Object.fromEntries(selected.map(a => [a.id, []])) } }; },\n\
             async materializeBundle() { calls.push(\"materialize\"); return stage(\"MaterializedProtocolBundleTx\"); },\n\
             async resolveLiveInputs(value) { calls.push(\"live-inputs\"); return stage(\"LiveResolvedProtocolBundleTx\", value); },\n\
             async resolveLiveDependencies(_materialized, value) { calls.push(\"live-dependencies\"); return stage(\"LiveDependenciesResolvedProtocolBundleTx\", value); },\n\
             async readyToSign(_materialized, _live, value) { calls.push(\"ready\"); return stage(\"ReadyToSignProtocolBundleTx\", value); },\n\
             async bindSignedTransaction(value, signedTransaction) { calls.push(\"signed\"); return stage(\"SignedProtocolBundleTx\", value, signedTransaction); },\n\
             async dryRunSigned(value) { calls.push(\"dry-run\"); return stage(\"SignedDryRunProtocolBundleTx\", value); },\n\
             async testTxPool(_signed, value) { calls.push(\"tx-pool\"); return stage(\"TxPoolAcceptedProtocolBundleTx\", value); },\n\
             async submit(_signed, value) { calls.push(\"submit\"); return stage(\"SubmittedProtocolBundleTx\", value); },\n\
             async confirm(value, policy) { calls.push(\"confirm\"); assert.equal(policy.requiredConfirmations, 3); return stage(\"ConfirmedProtocolBundleTx\", value); },\n\
           };\n\
           const client = builder.createProtocolBundleClient(runtime);\n\
           const prepared = await client.prepare({}, [first, second]);\n\
           const request = client.signingRequest(prepared.prepared);\n\
           assert.equal(request.privateKeysIncluded, false);\n\
           assert.equal(Object.hasOwn(request, \"privateKey\"), false);\n\
           const signed = await client.resumeSigned(prepared.prepared, { signed: true });\n\
           const accepted = await client.acceptSigned(signed);\n\
           const submitted = await client.submit(signed, accepted.accepted);\n\
           assert.equal(submitted.state, \"SubmittedProtocolBundleTx\");\n\
           const confirmed = await client.confirm(submitted, { requiredConfirmations: 3, maxAttempts: 20, pollingIntervalMs: 1000 });\n\
           assert.equal(confirmed.state, \"ConfirmedProtocolBundleTx\");\n\
           assert.deepEqual(calls, [\"check\", \"materialize\", \"live-inputs\", \"live-dependencies\", \"ready\", \"signed\", \"dry-run\", \"tx-pool\", \"submit\", \"confirm\"]);\n\
           const wrongRuntime = { ...runtime, async resolveLiveInputs(value) { return stage(\"WrongState\", value); } };\n\
           await assert.rejects(() => builder.createProtocolBundleClient(wrongRuntime).prepare({}, [first, second]), /invalid LiveResolvedProtocolBundleTx evidence/);\n\
         });\n\n",
    );
    js.push_str(
        "test(\"encodes and materializes bounded output plans with exact order and length\", { skip: boundedOutputPlanCases.length === 0 }, () => {\n\
           for (const boundedCase of boundedOutputPlanCases) {\n\
             const actionCase = actionCases.find((candidate) => candidate.name === boundedCase.name);\n\
             assert.ok(actionCase);\n\
             const zeroPlan = builder[boundedCase.plan](actionCase.params);\n\
             const zero = zeroPlan.boundedOutputPlans.find((candidate) => candidate.binding === boundedCase.binding);\n\
             assert.ok(zero);\n\
             assert.equal(zero.count, 0);\n\
             assert.deepEqual(zero.outputs, []);\n\
             const first = new Uint8Array(boundedCase.elementWidth);\n\
             first[first.length - 1] = 7;\n\
             const payload = builder.encodeBoundedOutputPlanV1([first], boundedCase.elementWidth, boundedCase.maxElements);\n\
             const params = { ...actionCase.params, [boundedCase.binding]: payload };\n\
             const onePlan = builder[boundedCase.plan](params);\n\
             const one = onePlan.boundedOutputPlans.find((candidate) => candidate.binding === boundedCase.binding);\n\
             assert.ok(one);\n\
             assert.equal(one.count, 1);\n\
             assert.equal(one.outputs.length, 1);\n\
             assert.equal(one.outputs[0].ordinal, 0);\n\
             assert.equal(one.outputs[0].data.length, boundedCase.outputWidth);\n\
             assert.equal(one.outputs[0].lockHash.length, 32);\n\
             const maximum = Array.from({ length: boundedCase.maxElements }, (_, index) => {\n\
               const element = new Uint8Array(boundedCase.elementWidth);\n\
               element[element.length - 1] = index & 0xff;\n\
               return element;\n\
             });\n\
             const maximumPayload = builder.encodeBoundedOutputPlanV1(maximum, boundedCase.elementWidth, boundedCase.maxElements);\n\
             const maximumPlan = builder[boundedCase.plan]({ ...actionCase.params, [boundedCase.binding]: maximumPayload });\n\
             const materialized = maximumPlan.boundedOutputPlans.find((candidate) => candidate.binding === boundedCase.binding);\n\
             assert.deepEqual(materialized.outputs.map((output) => output.ordinal), maximum.map((_, index) => index));\n\
             const wrongMagic = payload.slice();\n\
             wrongMagic[0] ^= 0xff;\n\
             assert.throws(() => builder[boundedCase.plan]({ ...actionCase.params, [boundedCase.binding]: wrongMagic }), /invalid CSBPLv1 magic/);\n\
             const trailing = new Uint8Array(payload.length + 1);\n\
             trailing.set(payload);\n\
             assert.throws(() => builder[boundedCase.plan]({ ...actionCase.params, [boundedCase.binding]: trailing }), /exact count\\/length bound/);\n\
             assert.throws(\n\
               () => builder.encodeBoundedOutputPlanV1([...maximum, new Uint8Array(boundedCase.elementWidth)], boundedCase.elementWidth, boundedCase.maxElements),\n\
               /invalid width, bound, or element count/,\n\
             );\n\
           }\n\
         });\n\n",
    );
    js.push_str(
        "function selectorEvidenceForPlan(plan) {\n\
           const selectors = Array.isArray(plan.actionScanSelectors?.selectors) ? plan.actionScanSelectors.selectors : [];\n\
           return selectors.map((selector, fallbackIndex) => ({\n\
             selector_index: Number.isInteger(selector.selector_index) ? selector.selector_index : fallbackIndex,\n\
             status: \"resolved\",\n\
             source: selector.ckb_source,\n\
             role: selector.role,\n\
             binding: selector.binding,\n\
             feature: selector.feature,\n\
             component: selector.component,\n\
             script_field: selector.script_field,\n\
           }));\n\
         }\n\n",
    );
    js.push_str(
        "test(\"plans all generated actions without submitting\", actionTestOptions, () => {\n\
           for (const actionCase of actionCases) {\n\
             const plan = builder[actionCase.plan](actionCase.params);\n\
             assert.equal(plan.schema, builder.CELLSCRIPT_BUILDER_SCHEMA);\n\
             assert.equal(plan.state, \"GeneratedActionPlan\");\n\
             assert.equal(plan.status, \"requires-runtime-resolution\");\n\
             assert.equal(plan.action, actionCase.name);\n\
             assert.equal(plan.canSubmit, false);\n\
             assert.equal(plan.requiresLiveCellResolution, true);\n\
             assert.equal(plan.requiresDeploymentResolution, true);\n\
             assert.equal(plan.actionScanSelectors.schema, builder.ACTION_SCAN_SELECTORS_SCHEMA);\n\
             assert.equal(plan.actionScanSelectors.source, \"transaction_runtime_input_requirements\");\n\
             assert.equal(plan.runtimeInputRequirements.length, plan.actionScanSelectors.selector_count);\n\
             assert.deepEqual(plan.params, actionCase.params);\n\
           }\n\
         });\n\n\
         test(\"rejects dynamic source indexes above their declared bound\", { skip: dynamicIndexCases.length === 0 }, () => {\n\
           for (const dynamicCase of dynamicIndexCases) {\n\
             const actionCase = actionCases.find((candidate) => candidate.name === dynamicCase.name);\n\
             assert.ok(actionCase);\n\
             const params = { ...actionCase.params, [dynamicCase.binding]: BigInt(dynamicCase.maxInclusive) + 1n };\n\
             assert.throws(() => builder[dynamicCase.plan](params), /exceeds max_inclusive/);\n\
           }\n\
         });\n\n\
         test(\"delegates live-cell resolution and transaction build to runtime\", actionTestOptions, async () => {\n\
           const [first] = actionCases;\n\
           const calls = [];\n\
           const runtime = {\n\
             async resolveLiveCells(request) {\n\
               calls.push([\"resolveLiveCells\", request.plan.action]);\n\
               assert.equal(request.plan.actionScanSelectors.schema, builder.ACTION_SCAN_SELECTORS_SCHEMA);\n\
               assert.equal(request.plan.actionScanSelectors.source, \"transaction_runtime_input_requirements\");\n\
               return { inputs: [\"input-0\"], cellDeps: [\"dep-0\"], lineage: [], scanSelectorEvidence: selectorEvidenceForPlan(request.plan) };\n\
             },\n\
             async buildTransaction(plan) {\n\
               calls.push([\"buildTransaction\", plan.action]);\n\
               return { action: plan.action, inputs: plan.liveCellResolution.inputs };\n\
             },\n\
           };\n\
           const api = builder.createActionBuilder(runtime);\n\
           const result = await api[first.method](first.params);\n\
           assert.equal(result.state, \"ActionBuilderResult\");\n\
           assert.equal(result.status, \"built-by-runtime\");\n\
           assert.equal(result.mode, \"build\");\n\
           assert.equal(result.plan.action, first.name);\n\
           assert.equal(result.transaction.action, first.name);\n\
           assert.deepEqual(result.liveCellResolution.inputs, [\"input-0\"]);\n\
           assert.deepEqual(calls, [[\"resolveLiveCells\", first.name], [\"buildTransaction\", first.name]]);\n\
         });\n\n\
         test(\"delegates dry-run and submit modes to runtime\", actionTestOptions, async () => {\n\
           const [first] = actionCases;\n\
           const calls = [];\n\
           const runtime = {\n\
             async resolveLiveCells(request) {\n\
               calls.push([\"resolveLiveCells\", request.plan.action]);\n\
               return { inputs: [\"input-0\"], cellDeps: [\"dep-0\"], lineage: [], scanSelectorEvidence: selectorEvidenceForPlan(request.plan) };\n\
             },\n\
             async buildTransaction(plan) {\n\
               calls.push([\"buildTransaction\", plan.action]);\n\
               return { action: plan.action, built: true };\n\
             },\n\
             async dryRun(transaction) {\n\
               calls.push([\"dryRun\", transaction.action]);\n\
               return { cycles: 42, exitCode: 0 };\n\
             },\n\
             async submit(transaction) {\n\
               calls.push([\"submit\", transaction.action]);\n\
               return { txHash: \"0x1234\" };\n\
             },\n\
           };\n\
           const api = builder.createActionBuilder(runtime);\n\
           const dryRunResult = await api[first.method](first.params, { dryRun: true });\n\
           assert.equal(dryRunResult.mode, \"dry-run\");\n\
           assert.deepEqual(dryRunResult.dryRunResult, { cycles: 42, exitCode: 0 });\n\
           calls.length = 0;\n\
           const submitResult = await api[first.method](first.params, { submit: true });\n\
           assert.equal(submitResult.mode, \"submit\");\n\
           assert.equal(submitResult.submittedTxHash, \"0x1234\");\n\
           assert.deepEqual(calls, [\n\
             [\"resolveLiveCells\", first.name],\n\
             [\"buildTransaction\", first.name],\n\
             [\"dryRun\", first.name],\n\
             [\"submit\", first.name],\n\
           ]);\n\
         });\n\n\
         test(\"rejects missing runtime adapters and malformed runtime shapes\", actionTestOptions, async () => {\n\
           const [first] = actionCases;\n\
           const firstPlan = builder[first.plan](first.params);\n\
           const noDryRunRuntime = {\n\
             async resolveLiveCells(request) { return { inputs: [], scanSelectorEvidence: selectorEvidenceForPlan(request.plan) }; },\n\
             async buildTransaction() { return { tx: true }; },\n\
           };\n\
           const badShapeRuntime = {\n\
             async resolveLiveCells() { return { inputs: \"not-an-array\" }; },\n\
             async buildTransaction() { return { tx: true }; },\n\
           };\n\
           const missingSelectorEvidenceRuntime = {\n\
             async resolveLiveCells() { return { inputs: [] }; },\n\
             async buildTransaction() { return { tx: true }; },\n\
           };\n\
         const mismatchedSelectorEvidenceRuntime = {\n\
           async resolveLiveCells(request) {\n\
             const evidence = selectorEvidenceForPlan(request.plan);\n\
             if (evidence.length > 0) {\n\
               evidence[0] = { ...evidence[0], role: \"wrong-role\" };\n\
             }\n\
             return { inputs: [], scanSelectorEvidence: evidence };\n\
           },\n\
           async buildTransaction() { return { tx: true }; },\n\
         };\n\
         const missingSelectorFieldRuntime = {\n\
           async resolveLiveCells(request) {\n\
             const evidence = selectorEvidenceForPlan(request.plan);\n\
             if (evidence.length > 0) {\n\
               delete evidence[0].source;\n\
             }\n\
             return { inputs: [], scanSelectorEvidence: evidence };\n\
           },\n\
           async buildTransaction() { return { tx: true }; },\n\
         };\n\
         const duplicateSelectorEvidenceRuntime = {\n\
           async resolveLiveCells(request) {\n\
             const evidence = selectorEvidenceForPlan(request.plan);\n\
             if (evidence.length > 1) {\n\
               evidence[1] = { ...evidence[0] };\n\
             }\n\
             return { inputs: [], scanSelectorEvidence: evidence };\n\
           },\n\
           async buildTransaction() { return { tx: true }; },\n\
         };\n\
         await assert.rejects(\n\
           () => builder.createActionBuilder(noDryRunRuntime)[first.method](first.params, { dryRun: true }),\n\
           /missing dryRun adapter/,\n\
           );\n\
           await assert.rejects(\n\
             () => builder.createActionBuilder(badShapeRuntime)[first.method](first.params),\n\
             /builder-shape mismatch/,\n\
           );\n\
           if ((firstPlan.actionScanSelectors.selectors ?? []).length > 0) {\n\
             await assert.rejects(\n\
               () => builder.createActionBuilder(missingSelectorEvidenceRuntime)[first.method](first.params),\n\
               /scanSelectorEvidence/,\n\
             );\n\
           await assert.rejects(\n\
             () => builder.createActionBuilder(mismatchedSelectorEvidenceRuntime)[first.method](first.params),\n\
             /scanSelectorEvidence.role mismatch/,\n\
           );\n\
           await assert.rejects(\n\
             () => builder.createActionBuilder(missingSelectorFieldRuntime)[first.method](first.params),\n\
             /scanSelectorEvidence.source missing/,\n\
           );\n\
           if ((firstPlan.actionScanSelectors.selectors ?? []).length > 1) {\n\
             await assert.rejects(\n\
               () => builder.createActionBuilder(duplicateSelectorEvidenceRuntime)[first.method](first.params),\n\
               /duplicate selector_index/,\n\
             );\n\
           }\n\
         }\n\
       });\n\n\
         test(\"maps runtime errors to action field context\", actionTestOptions, () => {\n\
           const [first] = actionCases;\n\
           const context = builder.runtimeErrorContextForAction(first.name);\n\
           assert.equal(context.action, first.name);\n\
           assert.equal(context.fields.length, Object.keys(first.params).length);\n\
           const byCode = builder.runtimeErrorInfoByCode(25);\n\
           assert.equal(byCode.name, \"entry-witness-abi-invalid\");\n\
           const fromObject = builder.explainCellScriptRuntimeError({ exitCode: 25 }, first.name);\n\
           assert.equal(fromObject.code, 25);\n\
           assert.equal(fromObject.action, first.name);\n\
           assert.equal(fromObject.actionFields.length, context.fields.length);\n\
           const fromMessage = builder.explainCellScriptRuntimeError({ message: \"entry witness payload layout failed\" }, first.name);\n\
           assert.equal(fromMessage.name, \"entry-witness-abi-invalid\");\n\
           assert.equal(builder.explainCellScriptRuntimeError({ exitCode: 999999 }, first.name), null);\n\
         });\n\n\
         test(\"rejects mismatched lockfile identity\", actionTestOptions, () => {\n\
           const [first] = actionCases;\n\
           const badLockfile = {\n\
             package: { source_hash: WRONG_HASH },\n\
             package_build: {\n\
               compiler_version: \"wrong-compiler\",\n\
               target_profile: builder.builderManifest.target_profile,\n\
               artifact_hash: builder.builderManifest.artifact_hash,\n\
               metadata_hash: builder.builderManifest.metadata_hash,\n\
             },\n\
           };\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { lockfile: badLockfile }),\n\
             /CellScript builder identity mismatch/,\n\
           );\n\
         });\n\n\
         test(\"rejects mismatched deployment identity when deployment binding is embedded\", actionTestOptions, () => {\n\
           const [first] = actionCases;\n\
           const deployment = builder.builderManifest.deployment_identity?.deployments?.[0];\n\
           if (!deployment) {\n\
             assert.deepEqual(builder.validateCellScriptDeployment(undefined, undefined), []);\n\
             assert.deepEqual(builder.validateCellScriptDeploymentTrust(undefined, undefined), []);\n\
             assert.deepEqual(\n\
               builder.validateCellScriptDeploymentTrust(undefined, { requirePublisherSignature: true }),\n\
               [\"trust policy requires a deployment record\"],\n\
             );\n\
             assert.throws(\n\
               () => builder[first.plan](first.params, { trustPolicy: { requirePublisherSignature: true } }),\n\
               /trust policy requires a deployment record/,\n\
             );\n\
             return;\n\
           }\n\
           const badDeployment = { ...deployment, code_hash: WRONG_HASH };\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { deployment: badDeployment }),\n\
             /CellScript deployment identity mismatch/,\n\
           );\n\
           const deprecatedDeployment = { ...deployment, status: \"deprecated\" };\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { deployment: deprecatedDeployment }),\n\
             /deployment status/,\n\
           );\n\
           const { status: _deploymentStatus, ...missingStatusDeployment } = deployment;\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { deployment: missingStatusDeployment }),\n\
             /no status/,\n\
           );\n\
           const { publisher_signature: _publisherSignature, audit_report_hash: _auditReportHash, ...unsignedDeployment } = deployment;\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { deployment: unsignedDeployment, trustPolicy: { requirePublisherSignature: true } }),\n\
             /publisher_signature/,\n\
           );\n\
           const signedDeployment = { ...deployment, publisher_signature: \"sig:fixture\", audit_report_hash: \"0xaaa\" };\n\
           assert.deepEqual(\n\
             builder.validateCellScriptDeploymentTrust(signedDeployment, { requirePublisherSignature: true, requireAuditReportHash: true }),\n\
             [],\n\
           );\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, {\n\
               deployment,\n\
               liveDeploymentEvidence: {\n\
                 status: \"failed\",\n\
                 deployment_status: \"deprecated\",\n\
                 rpc_status: \"dead\",\n\
                 out_point: deployment.out_point,\n\
                 rpc_data_hash: deployment.data_hash,\n\
                 rpc_code_hash: deployment.code_hash,\n\
                 violations: [\"deployment for network 'aggron4' is not active: Deprecated\"],\n\
               },\n\
             }),\n\
             /CellScript deployment identity mismatch/,\n\
           );\n\
         });\n",
    );
    Ok(js)
}

fn javascript_sample_params(
    action: &crate::ActionMetadata,
    bounded_output_plans: &[&crate::BoundedOutputPlanContractMetadata],
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    for param in &action.params {
        params.insert(param.name.clone(), javascript_sample_param_value(param));
    }
    for contract in bounded_output_plans {
        params.insert(contract.binding.clone(), serde_json::Value::String("0x435342504c76310000000000".to_string()));
    }
    serde_json::Value::Object(params)
}

fn javascript_sample_param_value(param: &ParamMetadata) -> serde_json::Value {
    if param.schema_pointer_abi
        || param.schema_length_abi
        || param.ty == "Address"
        || param.ty == "Hash"
        || param.fixed_byte_len.is_some()
    {
        let bytes = param.fixed_byte_len.unwrap_or(if param.ty == "Address" || param.ty == "Hash" { 32 } else { 0 });
        return serde_json::Value::String(format!("0x{}", "00".repeat(bytes)));
    }

    match param.ty.as_str() {
        "bool" => serde_json::Value::Bool(false),
        "u8" | "u16" | "u32" | "u64" | "u128" | "i8" | "i16" | "i32" | "i64" | "i128" => serde_json::json!(0),
        temporal if crate::ir::is_ckb_temporal_scalar_name(temporal) => serde_json::json!(0),
        "()" => serde_json::Value::Null,
        _ => serde_json::Value::String("0x".to_string()),
    }
}

pub(super) fn default_builder_package_name(metadata: &CompileMetadata) -> String {
    let module = metadata.module.replace("::", "-").replace('_', "-");
    let trimmed = module.trim_matches('-');
    if trimmed.is_empty() {
        "@cellscript/generated-builder".to_string()
    } else {
        format!("@cellscript/{}-builder", trimmed.to_ascii_lowercase())
    }
}

fn typescript_action_params_type(action: &crate::ActionMetadata) -> String {
    format!("{}Params", typescript_type_suffix(&action.name))
}

fn typescript_type_suffix(name: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if uppercase_next {
                output.push(ch.to_ascii_uppercase());
                uppercase_next = false;
            } else {
                output.push(ch);
            }
        } else {
            uppercase_next = true;
        }
    }
    if output.is_empty() || output.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        output.insert_str(0, "Action");
    }
    output
}

fn typescript_identifier(name: &str, fallback: &str) -> String {
    let mut ident = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch == '_' || ch == '$' || ch.is_ascii_alphabetic() || (index > 0 && ch.is_ascii_digit()) {
            ident.push(ch);
        } else if ch.is_ascii_digit() && index == 0 {
            ident.push('_');
            ident.push(ch);
        } else {
            ident.push('_');
        }
    }
    if ident.is_empty() || TYPESCRIPT_RESERVED_WORDS.contains(&ident.as_str()) {
        format!("{}_{}", fallback, crate::hex_encode(&crate::ckb_blake2b256(name.as_bytes()))[..8].to_ascii_lowercase())
    } else {
        ident
    }
}

fn typescript_object_key(name: &str) -> String {
    let ident = typescript_identifier(name, "param");
    if ident == name {
        ident
    } else {
        typescript_string_literal(name)
    }
}

fn typescript_param_type(param: &ParamMetadata) -> &str {
    if param.schema_pointer_abi
        || param.schema_length_abi
        || param.fixed_byte_len.is_some()
        || param.ty == "Address"
        || param.ty == "Hash"
    {
        return "HexString | Uint8Array";
    }

    match param.ty.as_str() {
        "bool" => "boolean",
        "u8" | "u16" | "u32" | "u64" | "u128" | "i8" | "i16" | "i32" | "i64" | "i128" => "bigint | number | string",
        temporal if crate::ir::is_ckb_temporal_scalar_name(temporal) => temporal,
        "()" => "null | undefined",
        _ => "CellScriptValue",
    }
}

fn typescript_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn json_bytes_pretty<T: serde::Serialize>(label: &str, value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize {label}: {error}")))
}

fn json_string_pretty<T: serde::Serialize>(label: &str, value: &T) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize {label}: {error}")))
}

const TYPESCRIPT_RESERVED_WORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "as",
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "yield",
];
