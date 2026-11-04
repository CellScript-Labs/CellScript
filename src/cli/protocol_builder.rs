//! Generated ProtocolBundle, current receipt and checked handle APIs.
pub(super) fn typescript_protocol_bundle_api() -> String {
    let mut source = format!(
        "export const EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA = {exact_schema:?} as const;\n\
         export const EXACT_SCRIPT_HANDLE_RECEIPT_VERSION = {exact_version} as const;\n\
         export const DEPLOYMENT_LINE_RECEIPT_SCHEMA = {line_schema:?} as const;\n\
         export const DEPLOYMENT_LINE_RECEIPT_VERSION = {line_version} as const;\n",
        exact_schema = crate::script_handle::EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA,
        exact_version = crate::script_handle::EXACT_SCRIPT_HANDLE_RECEIPT_VERSION,
        line_schema = crate::deployment_line_handle::DEPLOYMENT_LINE_RECEIPT_SCHEMA,
        line_version = crate::deployment_line_handle::DEPLOYMENT_LINE_RECEIPT_VERSION,
    );
    source.push('\n');
    for (name, value) in [
        ("PROTOCOL_BUNDLE_SELECTION_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_SELECTION_SCHEMA),
        ("PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA),
        ("PROTOCOL_BUNDLE_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_SCHEMA),
        ("PROTOCOL_BUNDLE_REPORT_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_REPORT_SCHEMA),
        ("PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA),
        ("PROTOCOL_CLOSED_ROLE_SCHEMA", crate::protocol_bundle::PROTOCOL_CLOSED_ROLE_SCHEMA),
        (
            "PROTOCOL_BUNDLE_BOUNDED_OUTPUT_PLAN_EVIDENCE_SCHEMA",
            crate::protocol_bundle::PROTOCOL_BUNDLE_BOUNDED_OUTPUT_PLAN_EVIDENCE_SCHEMA,
        ),
        ("EXACT_SCRIPT_HANDLE_VALUE_SCHEMA", crate::script_handle::EXACT_SCRIPT_HANDLE_VALUE_SCHEMA),
        ("EXACT_SCRIPT_HANDLE_ENCODING", crate::script_handle::EXACT_SCRIPT_HANDLE_ENCODING),
        ("DEPLOYMENT_LINE_VALUE_SCHEMA", crate::deployment_line_handle::DEPLOYMENT_LINE_VALUE_SCHEMA),
        ("DEPLOYMENT_LINE_HANDLE_ENCODING", crate::deployment_line_handle::DEPLOYMENT_LINE_HANDLE_ENCODING),
        ("DEPLOYMENT_LINE_ADMISSION_EVIDENCE_SCHEMA", crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_EVIDENCE_SCHEMA),
        ("DEPLOYMENT_LINE_ADMISSION_TRANSITION_SCHEMA", crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_TRANSITION_SCHEMA),
    ] {
        source.push_str(&format!("export const {name} = {value:?} as const;\n"));
    }
    source.push_str(
        r#"
export type ProtocolBundleScriptRole = "lock" | "type" | "spawned-verifier";

export interface ProtocolBundleBoundedOutputPlanEvidence {
  schema: typeof PROTOCOL_BUNDLE_BOUNDED_OUTPUT_PLAN_EVIDENCE_SCHEMA;
  version: "#,
    );
    source.push_str(&(crate::protocol_bundle::PROTOCOL_BUNDLE_BOUNDED_OUTPUT_PLAN_EVIDENCE_VERSION).to_string());
    source.push_str(
        r#";
  action: string;
  binding: string;
  witness_index: number;
  witness_field: "input_type";
  plan_payload: HexString;
  current_script_hash: HexString;
  group_output_indexes: readonly number[];
}

export interface ProtocolBundleDeploymentBinding {
  network: { chainId: string; genesisHash: HexString };
  artifactHash: string;
  script: { codeHash: HexString; hashType: "data" | "data1" | "data2" | "type"; args: HexString };
  codeCellDep: { outPoint: { txHash: HexString; index: number }; depType: "code" | "dep_group" };
}

export interface ExactScriptHandleReceipt {
  schema: typeof EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA;
  version: typeof EXACT_SCRIPT_HANDLE_RECEIPT_VERSION;
  class: "script" | "verifier";
  script_role: ProtocolBundleScriptRole;
  package_coordinate: string;
  lock_node_id: string;
  entry: { kind: "action" | "lock" | "function"; name: string };
  interface_hash: string;
  typed_semantics_hash: string;
  artifact_hash: string;
  target_profile_hash: string;
  runtime_abi_hash: string;
  verified_bundle_id: string;
  deployment: {
    network: { chain_id: string; genesis_hash: HexString };
    artifact_hash: string;
    script: { code_hash: HexString; hash_type: "data" | "data1" | "data2" | "type"; args: HexString };
    code_cell_dep: { out_point: { tx_hash: HexString; index: number }; dep_type: "code" | "dep_group" };
  };
  code_identity_policy: "data-hash-exact-artifact" | "type-hash-exact-code-cell";
  upgrade_policy: "exact-artifact";
}

export interface ExactScriptHandleValue {
  schema: typeof EXACT_SCRIPT_HANDLE_VALUE_SCHEMA;
  version: "#,
    );
    source.push_str(&(crate::script_handle::EXACT_SCRIPT_HANDLE_VALUE_VERSION).to_string());
    source.push_str(
        r#";
  encoding: typeof EXACT_SCRIPT_HANDLE_ENCODING;
  encoded: HexString;
}

export interface DeploymentLineReceipt {
  schema: typeof DEPLOYMENT_LINE_RECEIPT_SCHEMA;
  version: typeof DEPLOYMENT_LINE_RECEIPT_VERSION;
  line_id: string;
  package_line: string;
  sequence: number;
  status: "active" | "yanked";
  previous_receipt_hash?: string;
  class: "script" | "verifier";
  script_role: ProtocolBundleScriptRole;
  entry: { kind: "action" | "lock" | "function"; name: string };
  network: { chain_id: string; genesis_hash: HexString };
  stable_script: { code_hash: HexString; hash_type: "type"; args: HexString };
  baseline_interface_hash: string;
  previous_interface_hash: string;
  current_interface_hash: string;
  predecessor_compatibility: unknown;
  baseline_compatibility: unknown;
  policy: unknown;
  policy_hash: string;
  admission_cell_type_hash: HexString;
  current_exact_receipt: ExactScriptHandleReceipt;
  current_exact_handle_hash: string;
}

export interface DeploymentLineHandleValue {
  schema: typeof DEPLOYMENT_LINE_VALUE_SCHEMA;
  version: "#,
    );
    source.push_str(&(crate::deployment_line_handle::DEPLOYMENT_LINE_VALUE_VERSION).to_string());
    source.push_str(
        r#";
  encoding: typeof DEPLOYMENT_LINE_HANDLE_ENCODING;
  encoded: HexString;
}

export interface DeploymentLineCellEvidence {
  out_point: { tx_hash: HexString; index: number };
  lock: { code_hash: HexString; hash_type: "data" | "data1" | "data2" | "type"; args: HexString };
  type_script: { code_hash: HexString; hash_type: "type"; args: HexString };
  data_hash: string;
}

export interface DeploymentLineAdmissionEvidence {
  schema: typeof DEPLOYMENT_LINE_ADMISSION_EVIDENCE_SCHEMA;
  version: "#,
    );
    source.push_str(&(crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_EVIDENCE_VERSION).to_string());
    source.push_str(
        r#";
  artifact: string;
  receipt: DeploymentLineReceipt;
  handle: DeploymentLineHandleValue;
  admission_cell: DeploymentLineCellEvidence;
  admission_cell_data: HexString;
  admission_cell_dep_index: number;
  code_cell: DeploymentLineCellEvidence;
  code_cell_dep_index: number;
}

export interface DeploymentLineAdmissionTransition {
  schema: typeof DEPLOYMENT_LINE_ADMISSION_TRANSITION_SCHEMA;
  version: "#,
    );
    source.push_str(&(crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_TRANSITION_VERSION).to_string());
    source.push_str(r#";
  previous_receipt?: DeploymentLineReceipt;
  previous_handle?: DeploymentLineHandleValue;
  admission_input_index?: number;
  next_receipt: DeploymentLineReceipt;
  next_handle: DeploymentLineHandleValue;
  admission_output_index: number;
}

export interface ProtocolBundleArtifactBinding {
  schema: typeof PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA;
  id: string;
  packageName: string;
  packageCoordinate: string;
  lockNodeId: string;
  metadataHash: string;
  artifactHash: string;
  interfaceHash: string;
  typedSemanticsHash: string;
  targetProfile: "ckb" | "ckb-type-hash";
  targetProfileHash: string;
  runtimeAbiHash: string;
  verifiedBundleId: string | null;
  schemaContracts: readonly ProtocolBundleRoleSchemaIdentity[];
  exactRoleContracts?: unknown;
  builderManifestSchema: typeof CELLSCRIPT_BUILDER_SCHEMA;
  entry: { kind: "action" | "lock"; name: string };
  scriptRole: ProtocolBundleScriptRole;
  deployment: ProtocolBundleDeploymentBinding;
}

export interface ProtocolBundleRoleSchemaIdentity {
  type_name: string;
  schema_hash: string;
}

export const protocolBundleArtifactIdentity = Object.freeze({
  packageName: builderManifest.package_name,
  metadataHash: builderManifest.metadata_hash,
  artifactHash: builderManifest.artifact_hash,
  interfaceHash: metadata.interface_hash,
  typedSemanticsHash: builderManifest.typed_semantics_hash,
  targetProfile: builderManifest.target_profile,
  targetProfileHash: builderManifest.target_profile_hash,
  runtimeAbiHash: builderManifest.runtime_abi_hash,
  verifiedBundleId: builderManifest.verified_bundle_id,
  schemaContracts: Object.freeze(((builderManifest.molecule_schema_manifest as {
    entries?: readonly ProtocolBundleRoleSchemaIdentity[];
  }).entries ?? []).map((entry) => Object.freeze({
    type_name: entry.type_name,
    schema_hash: entry.schema_hash,
  }))),
  builderManifestSchema: CELLSCRIPT_BUILDER_SCHEMA,
});

export function bindProtocolBundleArtifact(options: {
  id: string;
  packageCoordinate: string;
  lockNodeId: string;
  entry: { kind: "action" | "lock"; name: string };
  scriptRole: ProtocolBundleScriptRole;
  deployment: ProtocolBundleDeploymentBinding;
}): ProtocolBundleArtifactBinding {
  if (!options.id || !options.packageCoordinate || !options.lockNodeId || !options.entry.name) {
    throw new Error("ProtocolBundle artifact id, package coordinate, lock node, and entry name are required");
  }
  const artifactHash = protocolBundleArtifactIdentity.artifactHash;
  if (typeof artifactHash !== "string") {
    throw new Error("generated artifact has no deployable artifact hash");
  }
  if (options.deployment.artifactHash !== artifactHash) {
    throw new Error("ProtocolBundle deployment artifact hash does not match generated builder identity");
  }
  if (!options.deployment.network.chainId || !/^0x[0-9a-f]{64}$/.test(options.deployment.network.genesisHash)) {
    throw new Error("ProtocolBundle deployment network identity is invalid");
  }
  return Object.freeze({
    schema: PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA,
    id: options.id,
    packageName: protocolBundleArtifactIdentity.packageName,
    packageCoordinate: options.packageCoordinate,
    lockNodeId: options.lockNodeId,
    metadataHash: protocolBundleArtifactIdentity.metadataHash,
    artifactHash,
    interfaceHash: protocolBundleArtifactIdentity.interfaceHash,
    typedSemanticsHash: protocolBundleArtifactIdentity.typedSemanticsHash,
    targetProfile: protocolBundleArtifactIdentity.targetProfile,
    targetProfileHash: protocolBundleArtifactIdentity.targetProfileHash,
    runtimeAbiHash: protocolBundleArtifactIdentity.runtimeAbiHash,
    verifiedBundleId: protocolBundleArtifactIdentity.verifiedBundleId,
    schemaContracts: protocolBundleArtifactIdentity.schemaContracts,
    exactRoleContracts: (builderManifest as { exact_role_contracts?: Readonly<Record<string, unknown>> }).exact_role_contracts?.[`${options.entry.kind}:${options.entry.name}`],
    builderManifestSchema: protocolBundleArtifactIdentity.builderManifestSchema,
    entry: Object.freeze({ ...options.entry }),
    scriptRole: options.scriptRole,
    deployment: Object.freeze(options.deployment),
  });
}

export interface BoundExactScriptHandle {
  artifact: ProtocolBundleArtifactBinding;
  receipt: ExactScriptHandleReceipt;
  value: ExactScriptHandleValue;
  handleHash: string;
}

export interface BoundOpenHandleWitness {
  consumer: string;
  parameter: string;
  participant: string;
  sourcePolicy: string;
  selectedReceipt: string;
  authorizationRoot: HexString;
  selection: HexString;
  witnessPayload: Uint8Array;
}

/** Encode the bounded one-handle CSARGv1 envelope. This performs serialization,
 * not receipt admission, membership verification or root authorization. */
export function encodeOpenHandleWitness(selection: HexString | Uint8Array): Uint8Array {
  if ((typeof selection === "string" && !/^0x[0-9a-f]{1312}$/.test(selection))
    || (selection instanceof Uint8Array && selection.length !== 656)) {
    throw new Error("open handle selection must be exactly 656 canonical bytes");
  }
  const bytes = boundedOutputBytes(selection, "open handle selection");
  const magic = new TextEncoder().encode("CSOHWv1\0");
  if (bytes.length !== 656 || magic.some((byte, index) => bytes[index] !== byte)) {
    throw new Error("open handle selection has invalid CSOHWv1 magic");
  }
  const payload = new Uint8Array(664);
  payload.set(new TextEncoder().encode("CSARGv1\0"));
  payload.set(bytes, 8);
  return payload;
}

/** Extract an already admitted witness from the native private-receipt path.
 * The runtime adapter must still recheck materialization and protect the final
 * witness through signing. This helper cannot construct a native receipt. */
export function openHandleWitnessFromCheckedBundle(
  checked: CheckedProtocolBundle,
  consumer: ProtocolBundleArtifactBinding,
  parameter: string,
): BoundOpenHandleWitness {
  if ((checked.schema !== PROTOCOL_BUNDLE_REPORT_SCHEMA && checked.schema !== PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA) || checked.status !== "ok"
    || !/^[0-9a-f]{64}$/.test(checked.bundle_hash)) {
    throw new Error("open handle requires a successful native ProtocolBundle report");
  }
  const bundle = checked.bundle as {
    artifacts?: readonly Record<string, unknown>[];
    open_handles?: readonly Record<string, unknown>[];
  };
  exactScriptHandleFromCheckedBundle(checked, consumer);
  const artifact = bundle.artifacts?.find((value) => value.id === consumer.id);
  if (!artifact || artifact.artifact_hash !== consumer.artifactHash
    || artifact.verified_bundle_id !== consumer.verifiedBundleId) {
    throw new Error("open handle consumer differs from the generated checked artifact");
  }
  const bindings = bundle.open_handles?.filter((value) => value.consumer === consumer.id && value.parameter === parameter) ?? [];
  if (bindings.length !== 1) throw new Error("missing or duplicate native open handle admission");
  const binding = bindings[0];
  const invocation = binding.verifier_invocation as Record<string, unknown> | undefined;
  const hashes = [binding.source_policy, binding.selected_receipt, binding.checked_dependency,
    invocation ? invocation.identity : binding.checked_type_group];
  if (binding.schema !== "#);
    source.push_str(&format!("{:?}", crate::protocol_bundle::PROTOCOL_OPEN_HANDLE_SCHEMA));
    source.push_str(r#"
    || hashes.some((value) => typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value))
    || binding.authorization_root !== consumer.deployment.script.args
    || binding.witness_field !== "input_type"
    || typeof binding.selection !== "string"
    || typeof binding.participant !== "string" || binding.participant === consumer.id
    || !bundle.artifacts?.some((value) => value.id === binding.participant && value.artifact_hash === binding.selected_artifact_hash)) {
    throw new Error("native open handle admission differs from the consumer root or selected participant");
  }
  const actions = (consumer.entry.kind === "lock" ? builderManifest.locks : builderManifest.actions) as readonly { name: string; params: readonly Record<string, unknown>[] }[];
  const declared = actions.find((action) => action.name === consumer.entry.name)?.params.find((value) => value.name === parameter);
  const handleClass = invocation ? "VerifierHandle" : "ScriptHandle";
  if (!declared || declared.ty !== `${handleClass}<${binding.interface_module}>` || declared.fixed_byte_len !== 656) {
    throw new Error("open handle parameter differs from the generated nominal interface");
  }
  const witnessPayload = encodeOpenHandleWitness(binding.selection as HexString);
  return Object.freeze({
    consumer: consumer.id, parameter, participant: binding.participant,
    sourcePolicy: binding.source_policy as string, selectedReceipt: binding.selected_receipt as string,
    authorizationRoot: binding.authorization_root as HexString,
    selection: binding.selection as HexString, witnessPayload,
  });
}

export interface BoundInstanceHandleWitness extends BoundOpenHandleWitness {
  instancePolicyRoot: HexString;
  instanceSourcePolicy: string;
  instanceSelectionIdentity: string;
}

/** Serialize the separate instance profile; old CSOH selections are not accepted. */
export function encodeInstanceHandleWitness(selection: HexString | Uint8Array): Uint8Array {
  if ((typeof selection === "string" && !/^0x[0-9a-f]{3536}$/.test(selection))
    || (selection instanceof Uint8Array && selection.length !== 1768)) {
    throw new Error("instance handle selection must be exactly 1768 canonical bytes");
  }
  const bytes = boundedOutputBytes(selection, "instance handle selection");
  const magic = new TextEncoder().encode("CSINWv1\0");
  if (bytes.length !== 1768 || magic.some((byte, index) => bytes[index] !== byte)) {
    throw new Error("instance handle selection has invalid CSINWv1 magic");
  }
  const payload = new Uint8Array(1776);
  payload.set(new TextEncoder().encode("CSARGv1\0"));
  payload.set(bytes, 8);
  return payload;
}

/** Extract native instance admission evidence. This is not an independent
 * policy verifier; the adapter rechecks the policy and final transaction. */
export function instanceHandleWitnessFromCheckedBundle(
  checked: CheckedProtocolBundle,
  consumer: ProtocolBundleArtifactBinding,
  parameter: string,
): BoundInstanceHandleWitness {
  if ((checked.schema !== PROTOCOL_BUNDLE_REPORT_SCHEMA && checked.schema !== PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA) || checked.status !== "ok"
    || !/^[0-9a-f]{64}$/.test(checked.bundle_hash)) {
    throw new Error("instance handle requires a successful native ProtocolBundle report");
  }
  exactScriptHandleFromCheckedBundle(checked, consumer);
  const bundle = checked.bundle as {
    artifacts?: readonly Record<string, unknown>[];
    open_handles?: readonly Record<string, unknown>[];
  };
  const artifact = bundle.artifacts?.find((value) => value.id === consumer.id);
  const bindings = bundle.open_handles?.filter((value) => value.consumer === consumer.id && value.parameter === parameter) ?? [];
  if (!artifact || artifact.artifact_hash !== consumer.artifactHash
    || artifact.verified_bundle_id !== consumer.verifiedBundleId || bindings.length !== 1) {
    throw new Error("missing, duplicate or mismatched native instance admission");
  }
  const binding = bindings[0];
  const instance = binding.instance as Record<string, unknown> | undefined;
  const args = consumer.deployment.script.args;
  if (!instance || instance.schema !== "#);
    source.push_str(&format!("{:?}", crate::protocol_bundle::PROTOCOL_INSTANCE_ADMISSION_SCHEMA));
    source.push_str(
        r#"
    || binding.schema !== "#,
    );
    source.push_str(&format!("{:?}", crate::protocol_bundle::PROTOCOL_INSTANCE_HANDLE_SCHEMA));
    source.push_str(r#" || binding.verifier_invocation != null
    || [binding.source_policy, binding.selected_receipt, binding.checked_dependency, binding.checked_type_group,
      instance.source_policy, instance.selection_identity, instance.implementation_receipt, instance.args_schema]
      .some((value) => typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value))
    || !/^0x4353494e43763100[0-9a-f]{400}$/.test(args) || instance.consumer_args !== args
    || instance.policy_root !== `0x${args.slice(18, 82)}`
    || (instance.instance_mode !== "exact" && instance.instance_mode !== "constrained")
    || binding.witness_field !== "input_type" || typeof binding.selection !== "string"
    || typeof binding.authorization_root !== "string" || !/^0x[0-9a-f]{64}$/.test(binding.authorization_root)
    || typeof binding.participant !== "string" || binding.participant === consumer.id
    || !bundle.artifacts?.some((value) => value.id === binding.participant && value.artifact_hash === binding.selected_artifact_hash)) {
    throw new Error("native instance admission differs from the consumer configuration or selected participant");
  }
  const actions = (consumer.entry.kind === "lock" ? builderManifest.locks : builderManifest.actions) as readonly { name: string; params: readonly Record<string, unknown>[] }[];
  const declared = actions.find((action) => action.name === consumer.entry.name)?.params.find((value) => value.name === parameter);
  if (!declared || declared.ty !== `InstanceScriptHandle<${binding.interface_module}>` || declared.fixed_byte_len !== 1768) {
    throw new Error("instance handle parameter differs from the generated nominal interface");
  }
  const witnessPayload = encodeInstanceHandleWitness(binding.selection as HexString);
  if (binding.authorization_root !== `0x${binding.selection.slice(50, 114)}`
    || instance.args_schema !== binding.selection.slice(114, 178)
    || binding.selection.slice(34, 36) !== (instance.instance_mode === "exact" ? "00" : "01")) {
    throw new Error("instance witness differs from its admitted implementation, schema or mode");
  }
  return Object.freeze({
    consumer: consumer.id, parameter, participant: binding.participant,
    sourcePolicy: binding.source_policy as string, selectedReceipt: binding.selected_receipt as string,
    authorizationRoot: binding.authorization_root as HexString, selection: binding.selection as HexString, witnessPayload,
    instancePolicyRoot: instance.policy_root as HexString, instanceSourcePolicy: instance.source_policy as string,
    instanceSelectionIdentity: instance.selection_identity as string,
  });
}

function exactDeploymentMatches(artifact: ProtocolBundleArtifactBinding, receipt: ExactScriptHandleReceipt): boolean {
  const left = artifact.deployment;
  const right = receipt.deployment;
  return left.network.chainId === right.network.chain_id
    && left.network.genesisHash === right.network.genesis_hash
    && left.artifactHash === right.artifact_hash
    && left.script.codeHash === right.script.code_hash
    && left.script.hashType === right.script.hash_type
    && left.script.args === right.script.args
    && left.codeCellDep.outPoint.txHash === right.code_cell_dep.out_point.tx_hash
    && left.codeCellDep.outPoint.index === right.code_cell_dep.out_point.index
    && left.codeCellDep.depType === right.code_cell_dep.dep_type;
}

/** Bind a receipt/value pair returned by the successful ProtocolBundle checker. */
export function bindCheckedExactScriptHandle(
  artifact: ProtocolBundleArtifactBinding,
  checked: { exact_handle_receipt: ExactScriptHandleReceipt; exact_handle: ExactScriptHandleValue; exact_handle_hash: string },
): BoundExactScriptHandle {
  const receipt = checked.exact_handle_receipt;
  const value = checked.exact_handle;
  const handleHash = checked.exact_handle_hash;
  if (typeof artifact.verifiedBundleId !== "string") {
    throw new Error("generated artifact has no verified bundle identity for an exact Script handle");
  }
  const expectedClass = artifact.scriptRole === "spawned-verifier" ? "verifier" : "script";
  const expectedPolicy = artifact.deployment.script.hashType === "type"
    ? "type-hash-exact-code-cell"
    : "data-hash-exact-artifact";
  if (!receipt || receipt.schema !== EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA || receipt.version !== EXACT_SCRIPT_HANDLE_RECEIPT_VERSION
    || receipt.class !== expectedClass || receipt.script_role !== artifact.scriptRole
    || receipt.package_coordinate !== artifact.packageCoordinate || receipt.lock_node_id !== artifact.lockNodeId
    || receipt.entry.kind !== artifact.entry.kind || receipt.entry.name !== artifact.entry.name
    || receipt.interface_hash !== artifact.interfaceHash
    || receipt.typed_semantics_hash !== artifact.typedSemanticsHash
    || receipt.artifact_hash !== artifact.artifactHash
    || receipt.target_profile_hash !== artifact.targetProfileHash
    || receipt.runtime_abi_hash !== artifact.runtimeAbiHash
    || receipt.verified_bundle_id !== artifact.verifiedBundleId
    || receipt.code_identity_policy !== expectedPolicy || receipt.upgrade_policy !== "exact-artifact"
    || !exactDeploymentMatches(artifact, receipt)) {
    throw new Error("checked exact Script handle receipt does not match generated artifact/deployment identity");
  }
  if (expectedPolicy === "data-hash-exact-artifact"
    && artifact.deployment.script.codeHash !== `0x${artifact.artifactHash}`) {
    throw new Error("exact data-hash Script handle does not identify the generated artifact bytes");
  }
  if (!value || value.schema !== EXACT_SCRIPT_HANDLE_VALUE_SCHEMA || value.version !== "#);
    source.push_str(&(crate::script_handle::EXACT_SCRIPT_HANDLE_VALUE_VERSION).to_string());
    source.push_str(
        r#"
    || value.encoding !== EXACT_SCRIPT_HANDLE_ENCODING || !/^0x[0-9a-f]{"#,
    );
    source.push_str(&(crate::script_handle::EXACT_SCRIPT_HANDLE_BYTES * 2).to_string());
    source.push_str(
        r#"}$/.test(value.encoded)) {
    throw new Error("checked exact Script handle value has an invalid schema or fixed-width encoding");
  }
  if (!/^[0-9a-f]{64}$/.test(handleHash)) {
    throw new Error("checked exact Script handle has an invalid full-value CKB Blake2b-256 hash");
  }
  return Object.freeze({ artifact, receipt: Object.freeze(receipt), value: Object.freeze(value), handleHash });
}

export interface ProtocolBundleClosedRoleInput {
  schema: typeof PROTOCOL_CLOSED_ROLE_SCHEMA;
  role_id: string;
  kind: "cell" | "witness";
  schema_identity: ProtocolBundleRoleSchemaIdentity;
  provider: { artifact: string; claim: string };
  consumers: readonly { artifact: string; claim: string }[];
  correspondence: "exact-physical-binding";
}

export function bindClosedProtocolRole(options: {
  roleId: string;
  kind: "cell" | "witness";
  schemaIdentity: ProtocolBundleRoleSchemaIdentity;
  provider: { artifact: ProtocolBundleArtifactBinding; claim: string };
  consumers: readonly { artifact: ProtocolBundleArtifactBinding; claim: string }[];
}): ProtocolBundleClosedRoleInput {
  if (!options.roleId || !options.provider.claim || options.consumers.length < 1) {
    throw new Error("closed ProtocolBundle role requires an id, provider claim, and at least one consumer");
  }
  if (!options.schemaIdentity.type_name || !/^[0-9a-f]{64}$/.test(options.schemaIdentity.schema_hash)) {
    throw new Error("closed ProtocolBundle role schema identity is invalid");
  }
  const participants = [options.provider, ...options.consumers];
  const participantIds = participants.map((participant) => participant.artifact.id);
  if (new Set(participantIds).size !== participantIds.length || options.consumers.some((consumer) => !consumer.claim)) {
    throw new Error("closed ProtocolBundle role participants and claims must be distinct and non-empty");
  }
  for (const participant of participants) {
    if (participant.artifact.schema !== PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA) {
      throw new Error(`ProtocolBundle artifact '${participant.artifact.id}' has an invalid binding schema`);
    }
    const exposesSchema = participant.artifact.schemaContracts.some((candidate) =>
      candidate.type_name === options.schemaIdentity.type_name
      && candidate.schema_hash === options.schemaIdentity.schema_hash);
    if (!exposesSchema) {
      throw new Error(`ProtocolBundle artifact '${participant.artifact.id}' does not expose the closed-role schema identity`);
    }
  }
  return Object.freeze({
    schema: PROTOCOL_CLOSED_ROLE_SCHEMA,
    role_id: options.roleId,
    kind: options.kind,
    schema_identity: Object.freeze({ ...options.schemaIdentity }),
    provider: Object.freeze({ artifact: options.provider.artifact.id, claim: options.provider.claim }),
    consumers: Object.freeze(options.consumers.map((consumer) =>
      Object.freeze({ artifact: consumer.artifact.id, claim: consumer.claim }))),
    correspondence: "exact-physical-binding" as const,
  });
}

export interface CheckedProtocolBundle {
  schema: typeof PROTOCOL_BUNDLE_REPORT_SCHEMA | typeof PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA;
  status: "ok";
  bundle_hash: string;
  bundle: unknown;
  evidence: unknown;
}

/** Compare generated source contracts; actual Cell reconstruction remains the
 * native checker and materialization adapter's responsibility. */
function assertExactRoleCatalog(checked: CheckedProtocolBundle, artifact: ProtocolBundleArtifactBinding): void {
  if (artifact.exactRoleContracts === undefined) return;
  const bundle = checked.bundle as { schema?: string; artifacts?: readonly Record<string, unknown>[] };
  const evidence = checked.evidence as { exact_role_selections?: Readonly<Record<string, unknown>> };
  const candidates = bundle.artifacts?.filter((item) => item.id === artifact.id) ?? [];
  if (checked.schema !== PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA || bundle.schema !== PROTOCOL_BUNDLE_SELECTION_SCHEMA
    || candidates.length !== 1 || !sameRoleContract(candidates[0].exact_role_contracts, artifact.exactRoleContracts)
    || !Array.isArray(evidence.exact_role_selections?.[artifact.id])) {
    throw new Error("checked exact-role catalog differs from the generated builder or lacks v2 evidence");
  }
}

function sameRoleContract(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (left === null || right === null || typeof left !== "object" || typeof right !== "object") return false;
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left) && Array.isArray(right) && left.length === right.length
      && left.every((value, index) => sameRoleContract(value, right[index]));
  }
  const l = left as Record<string, unknown>;
  const r = right as Record<string, unknown>;
  const keys = Object.keys(l);
  return keys.length === Object.keys(r).length
    && keys.every((key) => Object.prototype.hasOwnProperty.call(r, key) && sameRoleContract(l[key], r[key]));
}

export function exactScriptHandleFromCheckedBundle(
  checked: CheckedProtocolBundle,
  artifact: ProtocolBundleArtifactBinding,
): BoundExactScriptHandle {
  assertExactRoleCatalog(checked, artifact);
  const bundle = checked.bundle as { artifacts?: readonly Record<string, unknown>[] };
  const candidate = bundle.artifacts?.find((item) => item.id === artifact.id);
  if (!candidate) {
    throw new Error(`successful ProtocolBundle report has no exact handle for artifact '${artifact.id}'`);
  }
  return bindCheckedExactScriptHandle(artifact, candidate as unknown as {
    exact_handle_receipt: ExactScriptHandleReceipt;
    exact_handle: ExactScriptHandleValue;
    exact_handle_hash: string;
  });
}

export interface BoundDeploymentLineHandle {
  artifact: ProtocolBundleArtifactBinding;
  admission: DeploymentLineAdmissionEvidence;
  exact: BoundExactScriptHandle;
  handleBytes: HexString;
  admissionCellDepIndex: number;
  codeCellDepIndex: number;
}

/** Extract the current checked CSLINv1 value and its exact CellDep positions. */
export function deploymentLineHandleFromCheckedBundle(
  checked: CheckedProtocolBundle,
  artifact: ProtocolBundleArtifactBinding,
): BoundDeploymentLineHandle {
  if (artifact.targetProfile !== "ckb-type-hash" || artifact.deployment.script.hashType !== "type") {
    throw new Error("deployment line handles require a checked ckb-type-hash artifact");
  }
  const bundle = checked.bundle as { deployment_lines?: readonly DeploymentLineAdmissionEvidence[] };
  const admission = bundle.deployment_lines?.find((item) => item.artifact === artifact.id);
  if (!admission || admission.schema !== DEPLOYMENT_LINE_ADMISSION_EVIDENCE_SCHEMA || admission.version !== "#,
    );
    source.push_str(&(crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_EVIDENCE_VERSION).to_string());
    source.push_str(
        r#"
    || admission.receipt.schema !== DEPLOYMENT_LINE_RECEIPT_SCHEMA || admission.receipt.version !== DEPLOYMENT_LINE_RECEIPT_VERSION
    || admission.receipt.status !== "active" || admission.receipt.script_role !== artifact.scriptRole
    || admission.handle.schema !== DEPLOYMENT_LINE_VALUE_SCHEMA || admission.handle.version !== "#,
    );
    source.push_str(&(crate::deployment_line_handle::DEPLOYMENT_LINE_VALUE_VERSION).to_string());
    source.push_str(
        r#"
    || admission.handle.encoding !== DEPLOYMENT_LINE_HANDLE_ENCODING
    || !/^0x[0-9a-f]{"#,
    );
    source.push_str(&(crate::deployment_line_handle::DEPLOYMENT_LINE_HANDLE_BYTES * 2).to_string());
    source.push_str(r#"}$/.test(admission.handle.encoded)
    || !Number.isSafeInteger(admission.admission_cell_dep_index) || admission.admission_cell_dep_index < 0
    || !Number.isSafeInteger(admission.code_cell_dep_index) || admission.code_cell_dep_index < 0
    || admission.admission_cell_dep_index === admission.code_cell_dep_index) {
    throw new Error(`successful ProtocolBundle report has no active canonical deployment line for artifact '${artifact.id}'`);
  }
  const exact = exactScriptHandleFromCheckedBundle(checked, artifact);
  if (admission.receipt.current_exact_handle_hash !== exact.handleHash
    || admission.receipt.current_exact_receipt.verified_bundle_id !== exact.receipt.verified_bundle_id
    || !exactDeploymentMatches(artifact, admission.receipt.current_exact_receipt)) {
    throw new Error(`deployment line for artifact '${artifact.id}' does not bind its checked exact version`);
  }
  return Object.freeze({
    artifact,
    admission: Object.freeze(admission),
    exact,
    handleBytes: admission.handle.encoded,
    admissionCellDepIndex: admission.admission_cell_dep_index,
    codeCellDepIndex: admission.code_cell_dep_index,
  });
}

/** Build the exact evidence payload consumed by `cellc tx validate`. */
export function deploymentLineHandleEvidence(
  binding: BoundDeploymentLineHandle,
  source: { location: "input" | "output" | "cell_dep"; index: number },
  witness: { index: number; field: "input_type" },
): {
  handle: HexString;
  source: { location: "input" | "output" | "cell_dep"; index: number };
  admission: { index: number };
  code: { index: number };
  witness: { index: number; field: "input_type" };
} {
  if (!Number.isSafeInteger(source.index) || source.index < 0 || !Number.isSafeInteger(witness.index) || witness.index < 0) {
    throw new Error("deployment line runtime source and witness indexes must be non-negative safe integers");
  }
  return Object.freeze({
    handle: binding.handleBytes,
    source: Object.freeze({ ...source }),
    admission: Object.freeze({ index: binding.admissionCellDepIndex }),
    code: Object.freeze({ index: binding.codeCellDepIndex }),
    witness: Object.freeze({ ...witness }),
  });
}

export interface ProtocolBundleStage<S extends string> {
  state: S;
  bundleHash: string;
  rawTransactionHash: string;
  transaction: unknown;
  evidence: unknown;
}

export type MaterializedProtocolBundle = ProtocolBundleStage<"MaterializedProtocolBundleTx">;
export type LiveResolvedProtocolBundle = ProtocolBundleStage<"LiveResolvedProtocolBundleTx">;
export type DependencyResolvedProtocolBundle = ProtocolBundleStage<"LiveDependenciesResolvedProtocolBundleTx">;
export type ReadyToSignProtocolBundle = ProtocolBundleStage<"ReadyToSignProtocolBundleTx">;
export type SignedProtocolBundle = ProtocolBundleStage<"SignedProtocolBundleTx">;
export type SignedDryRunProtocolBundle = ProtocolBundleStage<"SignedDryRunProtocolBundleTx">;
export type TxPoolAcceptedProtocolBundle = ProtocolBundleStage<"TxPoolAcceptedProtocolBundleTx">;
export type SubmittedProtocolBundle = ProtocolBundleStage<"SubmittedProtocolBundleTx">;
export type ConfirmedProtocolBundle = ProtocolBundleStage<"ConfirmedProtocolBundleTx">;

export interface ProtocolBundleConfirmationPolicy {
  requiredConfirmations: number;
  maxAttempts: number;
  pollingIntervalMs: number;
}

export interface ProtocolBundleSigningRequest {
  state: "ProtocolBundleSigningRequest";
  bundleHash: string;
  rawTransactionHash: string;
  unsignedTransaction: unknown;
  privateKeysIncluded: false;
}

export interface ProtocolBundleRuntime {
  checkBundle(input: unknown, artifacts: readonly ProtocolBundleArtifactBinding[]): Promise<CheckedProtocolBundle>;
  materializeBundle(checked: CheckedProtocolBundle): Promise<MaterializedProtocolBundle>;
  resolveLiveInputs(materialized: MaterializedProtocolBundle): Promise<LiveResolvedProtocolBundle>;
  resolveLiveDependencies(
    materialized: MaterializedProtocolBundle,
    live: LiveResolvedProtocolBundle,
  ): Promise<DependencyResolvedProtocolBundle>;
  readyToSign(
    materialized: MaterializedProtocolBundle,
    live: LiveResolvedProtocolBundle,
    dependencies: DependencyResolvedProtocolBundle,
  ): Promise<ReadyToSignProtocolBundle>;
  bindSignedTransaction(
    prepared: ReadyToSignProtocolBundle,
    signedTransaction: unknown,
  ): Promise<SignedProtocolBundle>;
  dryRunSigned(signed: SignedProtocolBundle): Promise<SignedDryRunProtocolBundle>;
  testTxPool(signed: SignedProtocolBundle, dryRun: SignedDryRunProtocolBundle): Promise<TxPoolAcceptedProtocolBundle>;
  submit(signed: SignedProtocolBundle, accepted: TxPoolAcceptedProtocolBundle): Promise<SubmittedProtocolBundle>;
  confirm(
    submitted: SubmittedProtocolBundle,
    policy: ProtocolBundleConfirmationPolicy,
  ): Promise<ConfirmedProtocolBundle>;
}

function assertProtocolBundleStage<S extends string>(value: ProtocolBundleStage<S>, state: S, previous?: ProtocolBundleStage<string>): void {
  if (!value || value.state !== state || !value.bundleHash || !value.rawTransactionHash) {
    throw new Error(`ProtocolBundle runtime returned invalid ${state} evidence`);
  }
  if (previous && (value.bundleHash !== previous.bundleHash || value.rawTransactionHash !== previous.rawTransactionHash)) {
    throw new Error(`ProtocolBundle runtime changed transaction identity at ${state}`);
  }
}

export function createProtocolBundleClient(runtime: ProtocolBundleRuntime) {
  return {
    async prepare(input: unknown, artifacts: readonly ProtocolBundleArtifactBinding[]) {
      if (artifacts.length < 2) throw new Error("ProtocolBundle requires at least two artifacts");
      if (new Set(artifacts.map((artifact) => artifact.id)).size !== artifacts.length) {
        throw new Error("ProtocolBundle artifact ids must be unique");
      }
      if (artifacts.some((artifact) => artifact.schema !== PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA)) {
        throw new Error("ProtocolBundle artifact binding schema mismatch");
      }
      const checked = await runtime.checkBundle(input, artifacts);
      if (!checked || (checked.schema !== PROTOCOL_BUNDLE_REPORT_SCHEMA && checked.schema !== PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA) || checked.status !== "ok" || !checked.bundle_hash) {
        throw new Error("ProtocolBundle offline check did not return a successful canonical report");
      }
      for (const artifact of artifacts) assertExactRoleCatalog(checked, artifact);
      const materialized = await runtime.materializeBundle(checked);
      assertProtocolBundleStage(materialized, "MaterializedProtocolBundleTx");
      if (materialized.bundleHash !== checked.bundle_hash) throw new Error("ProtocolBundle materialization changed bundle identity");
      const live = await runtime.resolveLiveInputs(materialized);
      assertProtocolBundleStage(live, "LiveResolvedProtocolBundleTx", materialized);
      const dependencies = await runtime.resolveLiveDependencies(materialized, live);
      assertProtocolBundleStage(dependencies, "LiveDependenciesResolvedProtocolBundleTx", live);
      const prepared = await runtime.readyToSign(materialized, live, dependencies);
      assertProtocolBundleStage(prepared, "ReadyToSignProtocolBundleTx", dependencies);
      return { checked, materialized, live, dependencies, prepared } as const;
    },
    signingRequest(prepared: ReadyToSignProtocolBundle): ProtocolBundleSigningRequest {
      assertProtocolBundleStage(prepared, "ReadyToSignProtocolBundleTx");
      return Object.freeze({
        state: "ProtocolBundleSigningRequest" as const,
        bundleHash: prepared.bundleHash,
        rawTransactionHash: prepared.rawTransactionHash,
        unsignedTransaction: prepared.transaction,
        privateKeysIncluded: false as const,
      });
    },
    async resumeSigned(prepared: ReadyToSignProtocolBundle, signedTransaction: unknown) {
      assertProtocolBundleStage(prepared, "ReadyToSignProtocolBundleTx");
      const signed = await runtime.bindSignedTransaction(prepared, signedTransaction);
      assertProtocolBundleStage(signed, "SignedProtocolBundleTx", prepared);
      return signed;
    },
    async acceptSigned(signed: SignedProtocolBundle) {
      assertProtocolBundleStage(signed, "SignedProtocolBundleTx");
      const dryRun = await runtime.dryRunSigned(signed);
      assertProtocolBundleStage(dryRun, "SignedDryRunProtocolBundleTx", signed);
      const accepted = await runtime.testTxPool(signed, dryRun);
      assertProtocolBundleStage(accepted, "TxPoolAcceptedProtocolBundleTx", dryRun);
      return { dryRun, accepted } as const;
    },
    async submit(signed: SignedProtocolBundle, accepted: TxPoolAcceptedProtocolBundle) {
      assertProtocolBundleStage(signed, "SignedProtocolBundleTx");
      assertProtocolBundleStage(accepted, "TxPoolAcceptedProtocolBundleTx", signed);
      const submitted = await runtime.submit(signed, accepted);
      assertProtocolBundleStage(submitted, "SubmittedProtocolBundleTx", accepted);
      return submitted;
    },
    async confirm(submitted: SubmittedProtocolBundle, policy: ProtocolBundleConfirmationPolicy) {
      assertProtocolBundleStage(submitted, "SubmittedProtocolBundleTx");
      if (!Number.isInteger(policy.requiredConfirmations) || policy.requiredConfirmations < 1
        || !Number.isInteger(policy.maxAttempts) || policy.maxAttempts < 1
        || !Number.isInteger(policy.pollingIntervalMs) || policy.pollingIntervalMs < 0) {
        throw new Error("ProtocolBundle confirmation policy must use bounded non-negative integers and at least one confirmation/attempt");
      }
      const confirmed = await runtime.confirm(submitted, policy);
      assertProtocolBundleStage(confirmed, "ConfirmedProtocolBundleTx", submitted);
      return confirmed;
    },
  };
}

"#);
    source
}

#[cfg(test)]
mod tests {
    use super::typescript_protocol_bundle_api;

    #[test]
    fn generated_protocol_bundle_api_uses_native_wire_constants() {
        let source = typescript_protocol_bundle_api();
        for (name, value) in [
            ("PROTOCOL_BUNDLE_SELECTION_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_SELECTION_SCHEMA),
            ("PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_SELECTION_REPORT_SCHEMA),
            ("PROTOCOL_BUNDLE_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_SCHEMA),
            ("PROTOCOL_BUNDLE_REPORT_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_REPORT_SCHEMA),
            ("PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA", crate::protocol_bundle::PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA),
            ("PROTOCOL_CLOSED_ROLE_SCHEMA", crate::protocol_bundle::PROTOCOL_CLOSED_ROLE_SCHEMA),
            (
                "PROTOCOL_BUNDLE_BOUNDED_OUTPUT_PLAN_EVIDENCE_SCHEMA",
                crate::protocol_bundle::PROTOCOL_BUNDLE_BOUNDED_OUTPUT_PLAN_EVIDENCE_SCHEMA,
            ),
            ("EXACT_SCRIPT_HANDLE_VALUE_SCHEMA", crate::script_handle::EXACT_SCRIPT_HANDLE_VALUE_SCHEMA),
            ("EXACT_SCRIPT_HANDLE_ENCODING", crate::script_handle::EXACT_SCRIPT_HANDLE_ENCODING),
            ("DEPLOYMENT_LINE_VALUE_SCHEMA", crate::deployment_line_handle::DEPLOYMENT_LINE_VALUE_SCHEMA),
            ("DEPLOYMENT_LINE_HANDLE_ENCODING", crate::deployment_line_handle::DEPLOYMENT_LINE_HANDLE_ENCODING),
            ("DEPLOYMENT_LINE_ADMISSION_EVIDENCE_SCHEMA", crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_EVIDENCE_SCHEMA),
            (
                "DEPLOYMENT_LINE_ADMISSION_TRANSITION_SCHEMA",
                crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_TRANSITION_SCHEMA,
            ),
            ("EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA", crate::script_handle::EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA),
            ("DEPLOYMENT_LINE_RECEIPT_SCHEMA", crate::deployment_line_handle::DEPLOYMENT_LINE_RECEIPT_SCHEMA),
        ] {
            assert!(source.contains(&format!("export const {name} = {value:?} as const;")), "{name}");
        }
        for (name, version) in [
            ("EXACT_SCRIPT_HANDLE_RECEIPT_VERSION", crate::script_handle::EXACT_SCRIPT_HANDLE_RECEIPT_VERSION),
            ("DEPLOYMENT_LINE_RECEIPT_VERSION", crate::deployment_line_handle::DEPLOYMENT_LINE_RECEIPT_VERSION),
        ] {
            assert!(source.contains(&format!("export const {name} = {version} as const;")), "{name}");
        }
        for (name, version) in [
            ("ProtocolBundleBoundedOutputPlanEvidence", crate::protocol_bundle::PROTOCOL_BUNDLE_BOUNDED_OUTPUT_PLAN_EVIDENCE_VERSION),
            ("ExactScriptHandleValue", crate::script_handle::EXACT_SCRIPT_HANDLE_VALUE_VERSION),
            ("DeploymentLineHandleValue", crate::deployment_line_handle::DEPLOYMENT_LINE_VALUE_VERSION),
            ("DeploymentLineAdmissionEvidence", crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_EVIDENCE_VERSION),
            ("DeploymentLineAdmissionTransition", crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_TRANSITION_VERSION),
        ] {
            let declaration = source.split_once(&format!("export interface {name} {{")).unwrap().1.split_once("\n}").unwrap().0;
            assert!(declaration.lines().any(|line| line == format!("  version: {version};")), "{name}");
        }
        for (field, version) in [
            ("value.version", crate::script_handle::EXACT_SCRIPT_HANDLE_VALUE_VERSION),
            ("admission.version", crate::deployment_line_handle::DEPLOYMENT_LINE_ADMISSION_EVIDENCE_VERSION),
            ("admission.handle.version", crate::deployment_line_handle::DEPLOYMENT_LINE_VALUE_VERSION),
        ] {
            assert!(source.contains(&format!("{field} !== {version}")), "{field}");
        }
        for bytes in [crate::script_handle::EXACT_SCRIPT_HANDLE_BYTES, crate::deployment_line_handle::DEPLOYMENT_LINE_HANDLE_BYTES] {
            assert!(source.contains(&format!("^0x[0-9a-f]{{{}}}$", bytes * 2)));
        }
    }

    #[test]
    fn generated_protocol_bundle_api_preserves_the_characterized_bytes() {
        // Public generated source before moving wire literals to native owners.
        // A deliberate API change must review and refresh this byte contract.
        let source = typescript_protocol_bundle_api();
        assert_eq!(source.len(), 39_479);
        assert_eq!(
            hex::encode(crate::ckb_blake2b256(source.as_bytes())),
            "64f8cc07bb54b47a53cce7e079b4ea11c6c867e7d6be92861fb889cf4d621c38"
        );
    }
}
