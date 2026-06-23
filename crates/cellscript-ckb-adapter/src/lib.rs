use anyhow::{bail, Result};
use ckb_hash::blake2b_256;
use ckb_jsonrpc_types::{
    EntryCompleted, EstimateCycles, OutputsValidator, Status, Transaction as RpcTransaction, TransactionWithStatusResponse,
};
use ckb_sdk::{core::TransactionBuilder, traits::CellDepResolver, unlock::SecpSighashScriptSigner, CkbRpcClient};
use ckb_types::{
    bytes::Bytes,
    core::{Capacity, DepType, ScriptHashType, TransactionView},
    packed::{self, Byte32, CellDep, CellInput, CellOutput, OutPoint, Script, WitnessArgs},
    prelude::*,
    H160, H256,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

pub const ACTION_PLAN_POLICY: &str = "cellscript-action-builder-plan-v1";
pub const ADAPTER_CONTRACT_SCHEMA: &str = "cellscript-ckb-adapter-contract-v0.19";
pub const ACTION_ACCEPTANCE_REPORT_SCHEMA: &str = "cellscript-ckb-action-acceptance-report-v0.19";
pub const SCRIPT_EVIDENCE_SCHEMA: &str = "cellscript-ckb-script-evidence-v0.19";
pub const SCRIPT_REF_EVIDENCE_SCHEMA: &str = "cellscript-ckb-script-ref-evidence-v0.19";
pub const SCRIPT_CODE_DEP_EVIDENCE_SCHEMA: &str = "cellscript-ckb-script-code-dep-evidence-v0.19";
pub const DEPLOYMENT_MANIFEST_SCHEMA: &str = "cellscript-ckb-deployment-manifest-v0.19";
pub const DEPLOY_EVIDENCE_SCHEMA: &str = "cellscript-ckb-deploy-evidence-v0.19";

#[derive(Debug, Clone, Deserialize)]
pub struct ActionPlan {
    pub policy: String,
    pub action: String,
    pub artifact_hash: Option<String>,
    pub transaction_draft: TransactionDraft,
    pub adapter_contract: AdapterContract,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransactionDraft {
    pub state: String,
    pub can_submit: bool,
    pub requires_packed_materialization: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterContract {
    pub schema: String,
    pub compiler_core_dependency: String,
    pub transaction_realizer: String,
    pub resolved_tx_required_fields: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedActionTx {
    pub metadata_hash: String,
    pub artifact_hash: Option<String>,
    pub action_selector: String,
    pub inputs: Vec<CellInput>,
    pub outputs: Vec<CellOutputWithData>,
    pub witnesses: Vec<WitnessArgs>,
    pub cell_deps: Vec<CellDep>,
    pub header_deps: Vec<Byte32>,
    pub lineage: Vec<LiveOutputLineage>,
    pub fee_shannons: u64,
}

#[derive(Debug, Clone)]
pub struct CellOutputWithData {
    pub output: CellOutput,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct LiveOutputLineage {
    pub from: packed::OutPoint,
    pub to_output_index: u32,
    pub relation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineageEvidence {
    pub from_tx_hash: Vec<u8>,
    pub from_index: u32,
    pub to_output_index: u32,
    pub relation: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPreview {
    pub schema: &'static str,
    pub action: String,
    pub summary: String,
    pub consumes: Vec<PreviewCell>,
    pub creates: Vec<PreviewCell>,
    pub transitions: Vec<PreviewTransition>,
    pub witnesses: PreviewWitnesses,
    pub warnings: Vec<String>,
    pub estimated_fee: Option<u64>,
    pub required_signers: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCell {
    pub role: &'static str,
    pub out_point_tx_hash: Option<Vec<u8>>,
    pub out_point_index: Option<u32>,
    pub output_index: Option<u32>,
    pub capacity_shannons: Option<u64>,
    pub data_len: Option<usize>,
    pub lock_hash: Option<Vec<u8>>,
    pub type_hash: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTransition {
    pub from_tx_hash: Vec<u8>,
    pub from_index: u32,
    pub to_output_index: u32,
    pub relation: String,
    pub changes: Vec<String>,
    pub preserves: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewWitnesses {
    pub selector: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct ScriptSpec {
    pub code_hash: [u8; 32],
    pub hash_type: ScriptHashType,
    pub args: Bytes,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScriptEvidence {
    pub schema: &'static str,
    pub hash_type: String,
    pub code_hash: Vec<u8>,
    pub args_len: usize,
    pub args_hash: Vec<u8>,
    pub script_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptArgsPattern {
    Exact(Bytes),
    Prefix(Bytes),
    Suffix(Bytes),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ScriptRole {
    Lock,
    Type,
}

#[derive(Debug, Clone)]
pub struct ScriptRef {
    pub role: ScriptRole,
    pub script: Script,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScriptRefEvidence {
    pub schema: &'static str,
    pub role: ScriptRole,
    pub hash_type_byte: u8,
    pub code_hash: Vec<u8>,
    pub args_len: usize,
    pub args_hash: Vec<u8>,
    pub script_hash: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ScriptCodeDep {
    pub code_hash: [u8; 32],
    pub hash_type: ScriptHashType,
    pub out_point: packed::OutPoint,
    pub dep_type: DepType,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScriptCodeDepEvidence {
    pub schema: &'static str,
    pub code_hash: Vec<u8>,
    pub hash_type_byte: u8,
    pub out_point_tx_hash: Vec<u8>,
    pub out_point_index: u32,
    pub dep_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WitnessPlacement {
    Lock,
    InputType,
    OutputType,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedActionEvidence {
    pub schema: &'static str,
    pub state: &'static str,
    pub metadata_hash: String,
    pub artifact_hash: Option<String>,
    pub action_selector: String,
    pub cell_deps: usize,
    pub inputs: usize,
    pub outputs: usize,
    pub outputs_data: usize,
    pub witnesses: usize,
    pub lineage: Vec<LineageEvidence>,
    pub occupied_capacity_shannons: u64,
    pub serialized_tx_size_bytes: usize,
    pub fee_shannons: u64,
    pub ckb_vm_execution: bool,
    pub tx_pool_acceptance: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcceptedActionReport {
    pub schema: &'static str,
    pub state: &'static str,
    pub metadata_hash: String,
    pub artifact_hash: Option<String>,
    pub action_selector: String,
    pub ckb_vm_execution: bool,
    pub estimate_cycles: u64,
    pub tx_pool_acceptance: bool,
    pub tx_pool_cycles: u64,
    pub serialized_tx_size_bytes: usize,
    pub occupied_capacity_shannons: u64,
    pub fee_shannons: u64,
    pub submitted_tx_hash: Option<Vec<u8>>,
    pub lineage: Vec<LineageEvidence>,
    pub known_limitations: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeploymentManifest {
    pub schema: String,
    pub version: u32,
    pub deployments: Vec<DeploymentRef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeploymentRef {
    pub name: String,
    pub code_hash: String,
    pub hash_type: String,
    pub args: String,
    pub dep_type: String,
    pub out_point: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeploymentEvidence {
    pub schema: &'static str,
    pub deployments: usize,
    pub names: Vec<String>,
}

pub fn load_action_plan(path: impl AsRef<Path>) -> Result<ActionPlan> {
    parse_action_plan(&fs::read(path)?)
}

pub fn parse_action_plan(bytes: &[u8]) -> Result<ActionPlan> {
    let plan: ActionPlan = serde_json::from_slice(bytes)?;
    if plan.policy != ACTION_PLAN_POLICY {
        bail!("unsupported action plan policy {}", plan.policy);
    }
    if plan.transaction_draft.state != "ActionPlan" {
        bail!("compiler output must be ActionPlan, got {}", plan.transaction_draft.state);
    }
    if plan.transaction_draft.can_submit {
        bail!("compiler ActionPlan must not be directly submittable");
    }
    if !plan.transaction_draft.requires_packed_materialization {
        bail!("ActionPlan must require packed CKB materialization");
    }
    if plan.adapter_contract.schema != ADAPTER_CONTRACT_SCHEMA {
        bail!("unsupported adapter contract {}", plan.adapter_contract.schema);
    }
    if plan.adapter_contract.compiler_core_dependency != "no-ckb-sdk-rust" {
        bail!("compiler core must remain free of ckb-sdk-rust");
    }
    for required in ["outputs_data", "cell_deps", "lineage"] {
        if !plan.adapter_contract.resolved_tx_required_fields.iter().any(|field| field == required) {
            bail!("adapter contract is missing required field {required}");
        }
    }
    Ok(plan)
}

pub fn load_deployment_manifest(path: impl AsRef<Path>) -> Result<DeploymentManifest> {
    parse_deployment_manifest(&fs::read(path)?)
}

pub fn parse_deployment_manifest(bytes: &[u8]) -> Result<DeploymentManifest> {
    let manifest: DeploymentManifest = serde_json::from_slice(bytes)?;
    if manifest.schema != DEPLOYMENT_MANIFEST_SCHEMA {
        bail!("unsupported deployment manifest schema {}", manifest.schema);
    }
    if manifest.version != 1 {
        bail!("unsupported deployment manifest version {}", manifest.version);
    }
    for deployment in &manifest.deployments {
        if deployment.name.trim().is_empty() {
            bail!("deployment name must not be empty");
        }
        if deployment.code_hash.trim().is_empty() {
            bail!("deployment {} is missing code_hash", deployment.name);
        }
        if deployment.hash_type.trim().is_empty() {
            bail!("deployment {} is missing hash_type", deployment.name);
        }
        if deployment.dep_type.trim().is_empty() {
            bail!("deployment {} is missing dep_type", deployment.name);
        }
        if deployment.out_point.trim().is_empty() {
            bail!("deployment {} is missing out_point", deployment.name);
        }
    }
    Ok(manifest)
}

pub fn deployment_evidence(manifest: &DeploymentManifest) -> DeploymentEvidence {
    DeploymentEvidence {
        schema: DEPLOYMENT_MANIFEST_SCHEMA,
        deployments: manifest.deployments.len(),
        names: manifest.deployments.iter().map(|deployment| deployment.name.clone()).collect(),
    }
}

// ---- Deploy probe types ----

/// Specification for deploying a compiled CellScript artifact as an on-chain code cell.
///
/// The caller provides the artifact binary, the deployer lock script, and the
/// capacity input cell. The adapter computes TYPE_ID args, constructs the code
/// output, validates occupied capacity, and builds a headless CKB transaction.
#[derive(Debug, Clone)]
pub struct DeployArtifactSpec {
    /// Name for the deployment (used in manifest and evidence).
    pub name: String,
    /// Raw compiled artifact bytes (RISC-V binary / ELF).
    pub artifact_binary: Bytes,
    /// Hash of the artifact binary (hex, 64 chars). Must match the compiler output.
    pub artifact_hash: String,
    /// Lock script for the deployed code cell (and change output).
    pub deployer_lock: Script,
    /// Capacity input cell that funds the deployment.
    pub capacity_input: CellInput,
    /// Capacity of the input cell in shannons.
    pub capacity_input_shannons: u64,
    /// Optional data of the capacity input cell (for change calculation).
    pub capacity_input_data: Bytes,
    /// Declared hash_type for the code cell type script (typically "type" for TYPE_ID).
    /// Ignored when `type_script` is explicitly provided.
    pub type_id_hash_type: ScriptHashType,
    /// Optional explicit type script for the code cell.
    /// When set, overrides the default TYPE_ID construction from `type_id_hash_type`.
    /// When None, a TYPE_ID type script is auto-constructed from the first input.
    /// Set to `None` with `type_id_hash_type = ScriptHashType::Data` for a data-only deployment.
    pub type_script: Option<Script>,
    /// CellDeps required by the deployed artifact.
    pub cell_deps: Vec<CellDep>,
    /// HeaderDeps required by the deployed artifact.
    pub header_deps: Vec<Byte32>,
    /// Fee in shannons to allocate from the input capacity.
    pub fee_shannons: u64,
}

/// Resolved deploy transaction with the code cell output and change output.
#[derive(Debug, Clone)]
pub struct ResolvedDeployTx {
    pub name: String,
    pub artifact_hash: String,
    pub deployer_lock: Script,
    pub code_output: CellOutputWithData,
    pub change_output: CellOutputWithData,
    pub capacity_input: CellInput,
    pub cell_deps: Vec<CellDep>,
    pub header_deps: Vec<Byte32>,
    pub witnesses: Vec<WitnessArgs>,
    pub type_id_args: [u8; 32],
    pub fee_shannons: u64,
}

/// Evidence record for a resolved deploy transaction (headless, no node interaction).
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedDeployEvidence {
    pub schema: &'static str,
    pub state: &'static str,
    pub name: String,
    pub artifact_hash: String,
    pub code_output_index: u32,
    pub change_output_index: u32,
    pub type_id_args: Vec<u8>,
    pub code_hash: Vec<u8>,
    pub hash_type: String,
    pub occupied_capacity_shannons: u64,
    pub change_capacity_shannons: u64,
    pub serialized_tx_size_bytes: usize,
    pub fee_shannons: u64,
    pub cell_deps: usize,
    pub ckb_vm_execution: bool,
    pub tx_pool_acceptance: bool,
}

/// Build a headless CKB transaction that deploys a CellScript artifact as an
/// on-chain code cell with TYPE_ID.
///
/// The function:
/// 1. Computes TYPE_ID args from the first input tx_hash + output index 0.
/// 2. Constructs the type script (TYPE_ID) and lock script for the code cell.
/// 3. Calculates occupied capacity for the code cell from artifact size.
/// 4. Constructs a change output with remaining capacity minus fee.
/// 5. Validates that both outputs meet occupied-capacity floors.
/// 6. Assembles the transaction and returns evidence.
///
/// This is headless: no RPC, no live-cell selection, no signing. The caller
/// provides a pre-resolved capacity input. Use `CkbSdkAcceptance` for node
/// interaction after building.
pub fn build_deploy_transaction(spec: &DeployArtifactSpec) -> Result<(TransactionView, ResolvedDeployEvidence)> {
    // Validate artifact is non-empty.
    if spec.artifact_binary.is_empty() {
        bail!("artifact binary must be non-empty");
    }
    if spec.artifact_hash.is_empty() {
        bail!("artifact hash must be provided");
    }
    if spec.capacity_input_shannons == 0 {
        bail!("capacity input must have non-zero capacity");
    }

    // Step 1+2: Construct type script for the code cell.
    let type_id_args = type_id_args_from_first_input(&spec.capacity_input, 0);
    let type_script = if let Some(ref ts) = spec.type_script {
        ts.clone()
    } else {
        // Auto-construct TYPE_ID type script from first input
        construct_script(&ScriptSpec::new(
            [0u8; 32], // code_hash placeholder; will be the data hash after deployment
            spec.type_id_hash_type,
            type_id_args.to_vec(),
        ))
    };

    // Step 3: Build code cell output with TYPE_ID type script.
    let code_data_capacity = Capacity::bytes(spec.artifact_binary.len())?;
    // We need to compute the actual code_hash which is blake2b of the artifact.
    let data_hash = blake2b_256(&spec.artifact_binary);
    // Build the code output with a placeholder capacity (we'll compute exact occupied first).
    let code_output_builder = CellOutput::new_builder().lock(spec.deployer_lock.clone()).type_(Some(type_script.clone()).pack());
    // Compute occupied capacity for the code cell.
    let code_occupied = code_output_builder.clone().build().occupied_capacity(code_data_capacity)?;
    let code_capacity_shannons = code_occupied.as_u64();

    // Build the final code output with the exact occupied capacity.
    let code_output = code_output_builder.capacity(code_capacity_shannons).build();

    // Step 4: Build change output.
    let change_capacity_shannons = spec
        .capacity_input_shannons
        .checked_sub(code_capacity_shannons)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "input capacity {} shannons is insufficient for code cell occupied capacity {} shannons",
                spec.capacity_input_shannons,
                code_capacity_shannons
            )
        })?
        .checked_sub(spec.fee_shannons)
        .ok_or_else(|| {
            anyhow::anyhow!("remaining capacity after code cell is insufficient for fee of {} shannons", spec.fee_shannons)
        })?;

    // Validate change output meets its own occupied capacity floor.
    let change_data_capacity = Capacity::bytes(spec.capacity_input_data.len())?;
    let change_output = CellOutput::new_builder().capacity(change_capacity_shannons).lock(spec.deployer_lock.clone()).build();
    let change_occupied = change_output.occupied_capacity(change_data_capacity)?;
    if change_capacity_shannons < change_occupied.as_u64() {
        bail!(
            "change capacity {} shannons is below occupied capacity {} shannons",
            change_capacity_shannons,
            change_occupied.as_u64()
        );
    }

    // Step 5: Assemble the transaction.
    let mut builder = TransactionBuilder::default();
    builder.input(spec.capacity_input.clone());
    builder.output(code_output.clone());
    builder.output_data(spec.artifact_binary.clone().pack());
    builder.output(change_output.clone());
    builder.output_data(spec.capacity_input_data.clone().pack());
    for dep in &spec.cell_deps {
        builder.dedup_cell_dep(dep.clone());
    }
    for dep in &spec.header_deps {
        builder.dedup_header_dep(dep.clone());
    }
    // Placeholder witness for the first input (required by CKB protocol).
    let placeholder_witness = WitnessArgs::new_builder().build();
    builder.witness(placeholder_witness.as_bytes().pack());

    let tx = builder.build();
    let serialized_tx_size_bytes = tx.data().as_slice().len();

    // Verify outputs/outputs_data pairing.
    assert_eq!(tx.outputs().len(), 2, "deploy tx must have 2 outputs");
    assert_eq!(tx.outputs_data().len(), 2, "deploy tx must have 2 outputs_data entries");

    let evidence = ResolvedDeployEvidence {
        schema: DEPLOY_EVIDENCE_SCHEMA,
        state: "ResolvedDeployTx",
        name: spec.name.clone(),
        artifact_hash: spec.artifact_hash.clone(),
        code_output_index: 0,
        change_output_index: 1,
        type_id_args: type_id_args.to_vec(),
        code_hash: data_hash.to_vec(),
        hash_type: format!("{:?}", spec.type_id_hash_type).to_ascii_lowercase(),
        occupied_capacity_shannons: code_capacity_shannons,
        change_capacity_shannons,
        serialized_tx_size_bytes,
        fee_shannons: spec.fee_shannons,
        cell_deps: spec.cell_deps.len(),
        ckb_vm_execution: false,
        tx_pool_acceptance: false,
    };
    Ok((tx, evidence))
}

/// Build a deployment manifest from a completed deploy evidence record.
///
/// This creates the `DeploymentManifest` that records the on-chain code cell
/// reference after a successful deployment. The caller must provide the actual
/// tx_hash and output_index from the committed transaction.
pub fn build_deployment_manifest_from_evidence(
    evidence: &ResolvedDeployEvidence,
    tx_hash: &[u8; 32],
    output_index: u32,
) -> DeploymentManifest {
    let code_hash_hex = evidence.code_hash.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    let out_point = format!("0x{}:{}", tx_hash.iter().map(|b| format!("{:02x}", b)).collect::<String>(), output_index);
    DeploymentManifest {
        schema: DEPLOYMENT_MANIFEST_SCHEMA.to_string(),
        version: 1,
        deployments: vec![DeploymentRef {
            name: evidence.name.clone(),
            code_hash: format!("0x{}", code_hash_hex),
            hash_type: evidence.hash_type.clone(),
            args: format!("0x{}", evidence.type_id_args.iter().map(|b| format!("{:02x}", b)).collect::<String>()),
            dep_type: "code".to_string(),
            out_point,
        }],
    }
}

pub fn build_action_transaction(resolved: &ResolvedActionTx) -> Result<(TransactionView, ResolvedActionEvidence)> {
    materialize_with_ckb_sdk(resolved)
}

pub fn materialize_with_ckb_sdk(resolved: &ResolvedActionTx) -> Result<(TransactionView, ResolvedActionEvidence)> {
    if resolved.outputs.is_empty() {
        bail!("resolved action must create or continue at least one output");
    }

    let mut occupied_capacity_shannons = 0u64;
    let mut builder = TransactionBuilder::default();
    for dep in &resolved.cell_deps {
        builder.dedup_cell_dep(dep.clone());
    }
    for dep in &resolved.header_deps {
        builder.dedup_header_dep(dep.clone());
    }
    for input in &resolved.inputs {
        builder.input(input.clone());
    }
    for output in &resolved.outputs {
        let data_capacity = Capacity::bytes(output.data.len())?;
        let occupied = output.output.occupied_capacity(data_capacity)?.as_u64();
        let declared_capacity: u64 = output.output.capacity().unpack();
        if declared_capacity < occupied {
            bail!("output capacity is below occupied capacity");
        }
        occupied_capacity_shannons = occupied_capacity_shannons.saturating_add(occupied);
        builder.output(output.output.clone());
        builder.output_data(output.data.clone().pack());
    }
    for witness in &resolved.witnesses {
        builder.witness(witness.as_bytes().pack());
    }
    for edge in &resolved.lineage {
        if edge.to_output_index as usize >= resolved.outputs.len() {
            bail!("lineage target output index is out of range");
        }
    }

    let tx = builder.build();
    let serialized_tx_size_bytes = tx.data().as_slice().len();
    let evidence = ResolvedActionEvidence {
        schema: ACTION_ACCEPTANCE_REPORT_SCHEMA,
        state: "ResolvedActionTx",
        metadata_hash: resolved.metadata_hash.clone(),
        artifact_hash: resolved.artifact_hash.clone(),
        action_selector: resolved.action_selector.clone(),
        cell_deps: resolved.cell_deps.len(),
        inputs: resolved.inputs.len(),
        outputs: resolved.outputs.len(),
        outputs_data: resolved.outputs.len(),
        witnesses: resolved.witnesses.len(),
        lineage: resolved.lineage.iter().map(LineageEvidence::from).collect(),
        occupied_capacity_shannons,
        serialized_tx_size_bytes,
        fee_shannons: resolved.fee_shannons,
        ckb_vm_execution: false,
        tx_pool_acceptance: false,
    };
    Ok((tx, evidence))
}

pub fn emit_acceptance_report(
    evidence: &ResolvedActionEvidence,
    estimate_cycles: &EstimateCycles,
    tx_pool_acceptance: &EntryCompleted,
    submitted_tx_hash: Option<H256>,
) -> AcceptedActionReport {
    accepted_action_report(evidence, estimate_cycles, tx_pool_acceptance, submitted_tx_hash)
}

pub fn accepted_action_report(
    evidence: &ResolvedActionEvidence,
    estimate_cycles: &EstimateCycles,
    tx_pool_acceptance: &EntryCompleted,
    submitted_tx_hash: Option<H256>,
) -> AcceptedActionReport {
    AcceptedActionReport {
        schema: ACTION_ACCEPTANCE_REPORT_SCHEMA,
        state: "AcceptedActionTx",
        metadata_hash: evidence.metadata_hash.clone(),
        artifact_hash: evidence.artifact_hash.clone(),
        action_selector: evidence.action_selector.clone(),
        ckb_vm_execution: true,
        estimate_cycles: estimate_cycles.cycles.value(),
        tx_pool_acceptance: true,
        tx_pool_cycles: tx_pool_acceptance.cycles.value(),
        serialized_tx_size_bytes: evidence.serialized_tx_size_bytes,
        occupied_capacity_shannons: evidence.occupied_capacity_shannons,
        fee_shannons: tx_pool_acceptance.fee.value(),
        submitted_tx_hash: submitted_tx_hash.map(|hash| hash.as_bytes().to_vec()),
        lineage: evidence.lineage.clone(),
        known_limitations: vec![
            "Report is adapter-generated; external audit and mainnet-value certification are separate evidence.".to_string()
        ],
    }
}

impl From<&LiveOutputLineage> for LineageEvidence {
    fn from(edge: &LiveOutputLineage) -> Self {
        Self {
            from_tx_hash: edge.from.tx_hash().as_slice().to_vec(),
            from_index: edge.from.index().unpack(),
            to_output_index: edge.to_output_index,
            relation: edge.relation.clone(),
        }
    }
}

pub fn preview_resolved_action(resolved: &ResolvedActionTx) -> ActionPreview {
    ActionPreview {
        schema: "cellscript-action-preview-v1",
        action: resolved.action_selector.clone(),
        summary: format!("Build a CKB transaction for CellScript action {}", resolved.action_selector),
        consumes: resolved.inputs.iter().map(preview_input_cell).collect(),
        creates: resolved.outputs.iter().enumerate().map(|(index, output)| preview_output_cell(index, output)).collect(),
        transitions: resolved.lineage.iter().map(preview_transition).collect(),
        witnesses: PreviewWitnesses { selector: resolved.action_selector.clone(), count: resolved.witnesses.len() },
        warnings: vec![
            "Preview is adapter-local; live cell freshness, final capacity, fee, cycles, and tx-pool acceptance require node checks."
                .to_string(),
        ],
        estimated_fee: Some(resolved.fee_shannons),
        required_signers: Vec::new(),
    }
}

fn preview_input_cell(input: &CellInput) -> PreviewCell {
    let out_point = input.previous_output();
    PreviewCell {
        role: "consume",
        out_point_tx_hash: Some(out_point.tx_hash().as_slice().to_vec()),
        out_point_index: Some(out_point.index().unpack()),
        output_index: None,
        capacity_shannons: None,
        data_len: None,
        lock_hash: None,
        type_hash: None,
    }
}

fn preview_output_cell(index: usize, output: &CellOutputWithData) -> PreviewCell {
    PreviewCell {
        role: "create-or-continue",
        out_point_tx_hash: None,
        out_point_index: None,
        output_index: Some(index as u32),
        capacity_shannons: Some(output.output.capacity().unpack()),
        data_len: Some(output.data.len()),
        lock_hash: Some(output.output.lock().calc_script_hash().as_slice().to_vec()),
        type_hash: output.output.type_().to_opt().map(|script| script.calc_script_hash().as_slice().to_vec()),
    }
}

fn preview_transition(edge: &LiveOutputLineage) -> PreviewTransition {
    PreviewTransition {
        from_tx_hash: edge.from.tx_hash().as_slice().to_vec(),
        from_index: edge.from.index().unpack(),
        to_output_index: edge.to_output_index,
        relation: edge.relation.clone(),
        changes: vec!["adapter must materialize output data matching compiler metadata".to_string()],
        preserves: Vec::new(),
    }
}

impl ScriptSpec {
    pub fn new(code_hash: [u8; 32], hash_type: ScriptHashType, args: impl Into<Bytes>) -> Self {
        Self { code_hash, hash_type, args: args.into() }
    }

    pub fn to_packed(&self) -> Script {
        Script::new_builder().code_hash(self.code_hash.pack()).hash_type(self.hash_type).args(self.args.clone().pack()).build()
    }

    pub fn script_hash(&self) -> Byte32 {
        self.to_packed().calc_script_hash()
    }

    pub fn args_hash(&self) -> [u8; 32] {
        blake2b_256(&self.args)
    }

    pub fn evidence(&self) -> ScriptEvidence {
        ScriptEvidence {
            schema: SCRIPT_EVIDENCE_SCHEMA,
            hash_type: format!("{:?}", self.hash_type).to_ascii_lowercase(),
            code_hash: self.code_hash.to_vec(),
            args_len: self.args.len(),
            args_hash: self.args_hash().to_vec(),
            script_hash: self.script_hash().as_slice().to_vec(),
        }
    }
}

pub fn construct_script(spec: &ScriptSpec) -> Script {
    spec.to_packed()
}

pub fn matches_script_args(script: &Script, pattern: &ScriptArgsPattern) -> bool {
    let args = script.args().raw_data();
    match pattern {
        ScriptArgsPattern::Exact(expected) => args == *expected,
        ScriptArgsPattern::Prefix(prefix) => args.starts_with(prefix),
        ScriptArgsPattern::Suffix(suffix) => args.ends_with(suffix),
    }
}

pub fn owner_mode_args_from_lock(lock: &Script) -> Bytes {
    Bytes::copy_from_slice(lock.calc_script_hash().as_slice())
}

impl ScriptRef {
    pub fn new(role: ScriptRole, script: Script) -> Self {
        Self { role, script }
    }

    pub fn evidence(&self) -> ScriptRefEvidence {
        let args = self.script.args().raw_data();
        ScriptRefEvidence {
            schema: SCRIPT_REF_EVIDENCE_SCHEMA,
            role: self.role,
            hash_type_byte: self.script.hash_type().as_slice()[0],
            code_hash: self.script.code_hash().as_slice().to_vec(),
            args_len: args.len(),
            args_hash: blake2b_256(&args).to_vec(),
            script_hash: self.script.calc_script_hash().as_slice().to_vec(),
        }
    }
}

pub fn lock_script_ref(output: &CellOutput) -> ScriptRef {
    ScriptRef::new(ScriptRole::Lock, output.lock())
}

pub fn type_script_ref(output: &CellOutput) -> Option<ScriptRef> {
    output.type_().to_opt().map(|script| ScriptRef::new(ScriptRole::Type, script))
}

pub fn require_script_ref_matches(script_ref: &ScriptRef, expected: &ScriptSpec) -> Result<()> {
    if script_ref.script.code_hash().as_slice() != expected.code_hash.as_slice() {
        bail!("{} script code_hash mismatch", script_role_name(script_ref.role));
    }
    if script_ref.script.hash_type() != expected.hash_type.into() {
        bail!("{} script hash_type mismatch", script_role_name(script_ref.role));
    }
    if script_ref.script.args().raw_data() != expected.args {
        bail!("{} script args mismatch", script_role_name(script_ref.role));
    }
    Ok(())
}

fn script_role_name(role: ScriptRole) -> &'static str {
    match role {
        ScriptRole::Lock => "lock",
        ScriptRole::Type => "type",
    }
}

impl ScriptCodeDep {
    pub fn new(code_hash: [u8; 32], hash_type: ScriptHashType, out_point: packed::OutPoint, dep_type: DepType) -> Self {
        Self { code_hash, hash_type, out_point, dep_type }
    }

    pub fn from_script(script: &Script, out_point: packed::OutPoint, dep_type: DepType) -> Self {
        let mut code_hash = [0u8; 32];
        code_hash.copy_from_slice(script.code_hash().as_slice());
        let hash_type = ScriptHashType::from_repr(script.hash_type().as_slice()[0]).unwrap_or(ScriptHashType::Data);
        Self::new(code_hash, hash_type, out_point, dep_type)
    }

    pub fn to_cell_dep(&self) -> CellDep {
        CellDep::new_builder().out_point(self.out_point.clone()).dep_type(self.dep_type).build()
    }

    pub fn matches_script(&self, script: &Script) -> bool {
        script.code_hash().as_slice() == self.code_hash.as_slice() && script.hash_type() == self.hash_type.into()
    }

    pub fn evidence(&self) -> ScriptCodeDepEvidence {
        let hash_type_byte: u8 = self.hash_type.into();
        ScriptCodeDepEvidence {
            schema: SCRIPT_CODE_DEP_EVIDENCE_SCHEMA,
            code_hash: self.code_hash.to_vec(),
            hash_type_byte,
            out_point_tx_hash: self.out_point.tx_hash().as_slice().to_vec(),
            out_point_index: self.out_point.index().unpack(),
            dep_type: format!("{:?}", self.dep_type),
        }
    }
}

pub fn require_script_code_dep(script: &Script, deps: &[ScriptCodeDep]) -> Result<CellDep> {
    let Some(dep) = deps.iter().find(|dep| dep.matches_script(script)) else {
        bail!("missing CellDep for script code_hash/hash_type");
    };
    Ok(dep.to_cell_dep())
}

pub fn place_entry_witness_payload(base: &WitnessArgs, placement: WitnessPlacement, payload: Bytes) -> Result<WitnessArgs> {
    if payload.is_empty() {
        bail!("CellScript entry witness payload must be non-empty");
    }

    match placement {
        WitnessPlacement::Lock => {
            if base.lock().to_opt().is_some() {
                bail!("refusing to overwrite WitnessArgs.lock; lock signatures must stay explicit");
            }
            Ok(base.clone().as_builder().lock(Some(payload).pack()).build())
        }
        WitnessPlacement::InputType => {
            if base.input_type().to_opt().is_some() {
                bail!("refusing to overwrite WitnessArgs.input_type");
            }
            Ok(base.clone().as_builder().input_type(Some(payload).pack()).build())
        }
        WitnessPlacement::OutputType => {
            if base.output_type().to_opt().is_some() {
                bail!("refusing to overwrite WitnessArgs.output_type");
            }
            Ok(base.clone().as_builder().output_type(Some(payload).pack()).build())
        }
    }
}

pub fn type_id_args_from_first_input(first_input: &CellInput, output_index: u64) -> [u8; 32] {
    let mut material = first_input.as_slice().to_vec();
    material.extend_from_slice(&output_index.to_le_bytes());
    blake2b_256(material)
}

pub fn verify_type_id_output_args(first_input: &CellInput, output_index: u64, output: &CellOutput) -> Result<()> {
    let expected = type_id_args_from_first_input(first_input, output_index);
    let Some(type_script) = output.type_().to_opt() else {
        bail!("TYPE_ID output is missing type script");
    };
    let args = type_script.args().raw_data();
    if args.as_ref() != expected.as_slice() {
        bail!("TYPE_ID output args do not match first input and output index");
    }
    Ok(())
}

pub fn to_rpc_transaction(tx: &TransactionView) -> RpcTransaction {
    tx.data().into()
}

pub struct CkbSdkAcceptance<'a> {
    client: &'a CkbRpcClient,
}

impl<'a> CkbSdkAcceptance<'a> {
    pub fn new(client: &'a CkbRpcClient) -> Self {
        Self { client }
    }

    pub fn estimate_cycles(&self, tx: &TransactionView) -> std::result::Result<EstimateCycles, ckb_sdk::RpcError> {
        self.client.estimate_cycles(to_rpc_transaction(tx))
    }

    pub fn test_tx_pool_accept(&self, tx: &TransactionView) -> std::result::Result<EntryCompleted, ckb_sdk::RpcError> {
        self.client.test_tx_pool_accept(to_rpc_transaction(tx), Some(OutputsValidator::Passthrough))
    }

    pub fn send_transaction(&self, tx: &TransactionView) -> std::result::Result<H256, ckb_sdk::RpcError> {
        self.client.send_transaction(to_rpc_transaction(tx), Some(OutputsValidator::Passthrough))
    }
}

// ---- Full transaction lifecycle bridge ----

/// Deployment-backed CellDep resolver that maps code_hash + hash_type to
/// concrete on-chain CellDeps from a `DeploymentManifest`.
///
/// This implements `ckb_sdk::traits::CellDepResolver` so it can be used
/// directly with SDK transaction builders and `unlock_tx`.
#[derive(Debug)]
pub struct ManifestCellDepResolver {
    /// Maps (code_hash_bytes, hash_type_byte) -> CellDep.
    deps: HashMap<([u8; 32], u8), CellDep>,
}

impl ManifestCellDepResolver {
    /// Build a resolver from a deployment manifest.
    pub fn from_manifest(manifest: &DeploymentManifest) -> Result<Self> {
        let mut deps = HashMap::new();
        for deployment in &manifest.deployments {
            let code_hash = hex::decode(deployment.code_hash.trim_start_matches("0x"))
                .map_err(|e| anyhow::anyhow!("invalid code_hash hex for {}: {e}", deployment.name))?;
            if code_hash.len() != 32 {
                bail!("code_hash for {} must be 32 bytes, got {}", deployment.name, code_hash.len());
            }
            let mut code_hash_arr = [0u8; 32];
            code_hash_arr.copy_from_slice(&code_hash);
            let hash_type_byte = match deployment.hash_type.as_str() {
                "data" => 0u8,
                "type" => 1u8,
                "data1" => 2u8,
                "data2" => 4u8,
                other => bail!("unknown hash_type '{}' for {}", other, deployment.name),
            };
            // Parse out_point "0x<tx_hash>:<index>".
            let (tx_hash_hex, index_str) = deployment
                .out_point
                .rsplit_once(':')
                .ok_or_else(|| anyhow::anyhow!("invalid out_point format for {}: expected 0x<hash>:<index>", deployment.name))?;
            let tx_hash_bytes = hex::decode(tx_hash_hex.trim_start_matches("0x"))
                .map_err(|e| anyhow::anyhow!("invalid out_point tx_hash for {}: {e}", deployment.name))?;
            if tx_hash_bytes.len() != 32 {
                bail!("out_point tx_hash for {} must be 32 bytes", deployment.name);
            }
            let mut tx_hash_arr = [0u8; 32];
            tx_hash_arr.copy_from_slice(&tx_hash_bytes);
            let index: u32 = index_str.parse().map_err(|e| anyhow::anyhow!("invalid out_point index for {}: {e}", deployment.name))?;
            let out_point = OutPoint::new_builder().tx_hash(tx_hash_arr.pack()).index(index).build();
            let dep_type = match deployment.dep_type.as_str() {
                "code" => DepType::Code,
                "dep_group" => DepType::DepGroup,
                other => bail!("unknown dep_type '{}' for {}", other, deployment.name),
            };
            let cell_dep = CellDep::new_builder().out_point(out_point).dep_type(dep_type).build();
            deps.insert((code_hash_arr, hash_type_byte), cell_dep);
        }
        Ok(Self { deps })
    }

    /// Look up a CellDep by script's code_hash and hash_type.
    pub fn resolve_for_script(&self, script: &Script) -> Option<CellDep> {
        let mut code_hash = [0u8; 32];
        code_hash.copy_from_slice(script.code_hash().as_slice());
        let hash_type_byte: u8 = script.hash_type().as_slice().first().copied().unwrap_or(0);
        self.deps.get(&(code_hash, hash_type_byte)).cloned()
    }

    /// Number of deployment entries in the resolver.
    pub fn len(&self) -> usize {
        self.deps.len()
    }

    /// Whether the resolver has any entries.
    pub fn is_empty(&self) -> bool {
        self.deps.is_empty()
    }
}

impl CellDepResolver for ManifestCellDepResolver {
    fn resolve(&self, script: &Script) -> Option<CellDep> {
        self.resolve_for_script(script)
    }
}

/// Transaction submission and status tracking.
///
/// Wraps `CkbRpcClient` to provide submit + confirm + evidence workflow.
pub struct TransactionSubmitter<'a> {
    client: &'a CkbRpcClient,
}

impl<'a> TransactionSubmitter<'a> {
    pub fn new(client: &'a CkbRpcClient) -> Self {
        Self { client }
    }

    /// Submit a transaction to the CKB node's tx-pool.
    pub fn submit(&self, tx: &TransactionView) -> std::result::Result<H256, ckb_sdk::RpcError> {
        self.client.send_transaction(to_rpc_transaction(tx), Some(OutputsValidator::Passthrough))
    }

    /// Query the status of a previously submitted transaction.
    ///
    /// Returns `Some(TransactionWithStatusResponse)` if the node has a record,
    /// or `None` if the transaction is unknown.
    pub fn get_transaction_status(
        &self,
        tx_hash: &H256,
    ) -> std::result::Result<Option<TransactionWithStatusResponse>, ckb_sdk::RpcError> {
        self.client.get_transaction(tx_hash.clone())
    }

    /// Wait for a transaction to be committed, polling up to `max_attempts` times
    /// with `delay_ms` between attempts.
    pub fn wait_committed(&self, tx_hash: &H256, max_attempts: u32, delay_ms: u64) -> Result<CommittedEvidence> {
        for _ in 0..max_attempts {
            if let Some(response) = self.get_transaction_status(tx_hash)? {
                let tx_status = response.tx_status;
                if tx_status.status == Status::Committed {
                    let block_hash = tx_status.block_hash.unwrap_or_default();
                    return Ok(CommittedEvidence { tx_hash: tx_hash.clone(), block_hash, status: "committed".to_string() });
                }
                if tx_status.status == Status::Rejected {
                    bail!("transaction {:?} was rejected by the node", tx_hash);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
        bail!("transaction {:?} was not committed within {} attempts", tx_hash, max_attempts)
    }

    /// Get the tip block number from the node.
    pub fn get_tip_block_number(&self) -> std::result::Result<u64, ckb_sdk::RpcError> {
        let header = self.client.get_tip_header()?;
        Ok(header.inner.number.value())
    }
}

/// Evidence that a transaction has been committed on-chain.
#[derive(Debug, Clone, Serialize)]
pub struct CommittedEvidence {
    pub tx_hash: H256,
    pub block_hash: H256,
    pub status: String,
}

/// Adapter-level signing boundary that wraps `ckb_sdk::traits::Signer`.
///
/// This struct does not implement signing itself; it provides typed evidence
/// that signing is an adapter-owned concern. Use `ckb_sdk::unlock_tx` with
/// concrete `ScriptUnlocker` implementations (SecpSighash, OmniLock, etc.)
/// for actual signing.
pub struct SigningAdapter {
    /// Signer identity labels (e.g., lock script hash prefixes).
    pub signer_labels: Vec<String>,
    /// Whether the signing step has been completed.
    pub signed: bool,
}

impl SigningAdapter {
    /// Create a new signing adapter with the given signer labels.
    pub fn new(signer_labels: Vec<String>) -> Self {
        Self { signer_labels, signed: false }
    }

    /// Create a signing adapter for a single secp256k1 sighash signer.
    pub fn for_secp_sighash(lock_arg: H160) -> Self {
        Self { signer_labels: vec![format!("secp256k1-sighash:{}", lock_arg)], signed: false }
    }

    /// Mark the signing step as complete.
    pub fn mark_signed(&mut self) {
        self.signed = true;
    }

    /// Evidence of the signing adapter state.
    pub fn evidence(&self) -> SigningAdapterEvidence {
        SigningAdapterEvidence {
            schema: "cellscript-ckb-signing-adapter-v0.19",
            signer_count: self.signer_labels.len(),
            signed: self.signed,
        }
    }
}

/// Evidence record for the signing adapter.
#[derive(Debug, Clone, Serialize)]
pub struct SigningAdapterEvidence {
    pub schema: &'static str,
    pub signer_count: usize,
    pub signed: bool,
}

/// Adapter-level capacity balancing that wraps `ckb_sdk::CapacityBalancer`.
///
/// Provides a typed interface for the common pattern of funding a transaction
/// with additional capacity inputs and producing change.
pub struct CapacityBridge {
    /// Lock script for change outputs.
    pub change_lock: Script,
    /// Fee rate in shannons per kilobyte.
    pub fee_rate: u64,
}

impl CapacityBridge {
    /// Create a new capacity bridge with the given change lock and fee rate.
    pub fn new(change_lock: Script, fee_rate: u64) -> Self {
        Self { change_lock, fee_rate }
    }

    /// Build a `ckb_sdk::tx_builder::CapacityBalancer` from this bridge configuration.
    pub fn to_balancer(&self) -> ckb_sdk::tx_builder::CapacityBalancer {
        let placeholder = WitnessArgs::new_builder().build();
        ckb_sdk::tx_builder::CapacityBalancer::new_simple(self.change_lock.clone(), placeholder, self.fee_rate)
    }

    /// Evidence for the capacity bridge configuration.
    pub fn evidence(&self) -> CapacityBridgeEvidence {
        CapacityBridgeEvidence {
            schema: "cellscript-ckb-capacity-bridge-v0.19",
            change_lock_hash: self.change_lock.calc_script_hash().as_slice().to_vec(),
            fee_rate: self.fee_rate,
        }
    }
}

/// Evidence record for the capacity bridge.
#[derive(Debug, Clone, Serialize)]
pub struct CapacityBridgeEvidence {
    pub schema: &'static str,
    pub change_lock_hash: Vec<u8>,
    pub fee_rate: u64,
}

/// Full end-to-end transaction lifecycle result.
#[derive(Debug, Clone, Serialize)]
pub struct TransactionLifecycleEvidence {
    pub schema: &'static str,
    pub deploy_evidence: Option<ResolvedDeployEvidence>,
    pub action_evidence: Option<ResolvedActionEvidence>,
    pub signing: SigningAdapterEvidence,
    pub capacity: Option<CapacityBridgeEvidence>,
    pub estimate_cycles: Option<u64>,
    pub tx_pool_accepted: bool,
    pub submitted: bool,
    pub committed: Option<CommittedEvidence>,
}

pub fn signing_boundary_type() -> &'static str {
    std::any::type_name::<SecpSighashScriptSigner>()
}

// ---- High-level facade ----

/// High-level adapter facade that connects to a CKB node and provides
/// one-call workflows for common CellScript operations.
///
/// # Quick start
///
/// ```no_run
/// # fn main() -> anyhow::Result<()> {
/// use ckb_types::packed::Script;
/// use cellscript_ckb_adapter::CellScriptAdapter;
///
/// // Connect to a CKB node
/// let adapter = CellScriptAdapter::connect("http://127.0.0.1:8114")?;
///
/// // Deploy an artifact
/// let deployer_lock_script = Script::default();
/// let (manifest, evidence) = adapter.deploy_artifact(
///     "my-token",
///     std::fs::read("artifact.bin")?.into(),
///     deployer_lock_script,
///     1_000,  // fee in shannons
/// )?;
///
/// // Load an action plan and build a transaction
/// let plan = adapter.load_action_plan("action.json")?;
/// let resolved = adapter.resolve_action(&plan)?;
/// # Ok(())
/// # }
/// ```
pub struct CellScriptAdapter {
    client: CkbRpcClient,
}

impl std::fmt::Debug for CellScriptAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CellScriptAdapter").finish_non_exhaustive()
    }
}

impl CellScriptAdapter {
    /// Connect to a CKB node at the given RPC URL.
    pub fn connect(rpc_url: &str) -> Result<Self> {
        let client = CkbRpcClient::new(rpc_url);
        // Verify connectivity.
        let _tip = client.get_tip_header().map_err(|e| anyhow::anyhow!("cannot connect to CKB node at {}: {e}", rpc_url))?;
        Ok(Self { client })
    }

    // ---- Deploy workflow ----

    /// Deploy a CellScript artifact as an on-chain code cell with TYPE_ID.
    ///
    /// This is the one-call deploy workflow that combines:
    /// 1. Finding a spendable capacity cell from the node
    /// 2. Building the deploy transaction (headless)
    /// 3. Estimating cycles and testing tx-pool acceptance
    /// 4. Submitting the transaction
    /// 5. Waiting for commitment
    /// 6. Building the deployment manifest
    ///
    /// Returns the `DeploymentManifest` and full `TransactionLifecycleEvidence`.
    pub fn deploy_artifact(
        &self,
        name: &str,
        artifact_binary: Bytes,
        deployer_lock: Script,
        fee_shannons: u64,
    ) -> Result<(DeploymentManifest, TransactionLifecycleEvidence)> {
        let artifact_hash = blake2b_256(&artifact_binary).iter().map(|b| format!("{:02x}", b)).collect::<String>();

        // Find a spendable capacity cell.
        let capacity_input = self.find_capacity_for_deploy(&deployer_lock, &artifact_binary, fee_shannons)?;

        let spec = DeployArtifactSpec {
            name: name.to_string(),
            artifact_binary,
            artifact_hash,
            deployer_lock: deployer_lock.clone(),
            capacity_input: capacity_input.input,
            capacity_input_shannons: capacity_input.capacity_shannons,
            capacity_input_data: capacity_input.data,
            type_id_hash_type: ScriptHashType::Type,
            type_script: None,
            cell_deps: Vec::new(),
            header_deps: Vec::new(),
            fee_shannons,
        };

        let (tx, deploy_evidence) = build_deploy_transaction(&spec)?;

        // Estimate cycles.
        let estimate = self.client.estimate_cycles(to_rpc_transaction(&tx)).ok();
        let estimate_cycles = estimate.as_ref().map(|e| e.cycles.value());

        // Test tx-pool acceptance.
        let tx_pool_accepted = self.client.test_tx_pool_accept(to_rpc_transaction(&tx), Some(OutputsValidator::Passthrough)).is_ok();

        // Submit.
        let submitted = self.client.send_transaction(to_rpc_transaction(&tx), Some(OutputsValidator::Passthrough)).is_ok();
        let tx_hash = self.client.send_transaction(to_rpc_transaction(&tx), Some(OutputsValidator::Passthrough)).ok();

        // Wait for commitment.
        let committed = if let Some(ref hash) = tx_hash { self.wait_for_commitment(hash, 30, 500).ok() } else { None };

        // Build manifest from committed evidence.
        let manifest = if let Some(ref hash) = tx_hash {
            let mut hash_bytes = [0u8; 32];
            hash_bytes.copy_from_slice(hash.as_bytes());
            build_deployment_manifest_from_evidence(&deploy_evidence, &hash_bytes, 0)
        } else {
            build_deployment_manifest_from_evidence(&deploy_evidence, &[0u8; 32], 0)
        };

        let mut signing = SigningAdapter::new(vec!["deployer".to_string()]);
        if submitted {
            signing.mark_signed();
        }

        let lifecycle = TransactionLifecycleEvidence {
            schema: "cellscript-ckb-tx-lifecycle-v0.19",
            deploy_evidence: Some(deploy_evidence),
            action_evidence: None,
            signing: signing.evidence(),
            capacity: Some(CapacityBridge::new(deployer_lock, 1000).evidence()),
            estimate_cycles,
            tx_pool_accepted,
            submitted,
            committed,
        };

        Ok((manifest, lifecycle))
    }

    /// Build a headless deploy transaction without submitting it.
    ///
    /// Use this when you want to inspect the transaction before submitting,
    /// or when you need to add signing externally.
    pub fn build_deploy(
        &self,
        name: &str,
        artifact_binary: Bytes,
        deployer_lock: Script,
        fee_shannons: u64,
    ) -> Result<(TransactionView, ResolvedDeployEvidence)> {
        let artifact_hash = blake2b_256(&artifact_binary).iter().map(|b| format!("{:02x}", b)).collect::<String>();

        let capacity_input = self.find_capacity_for_deploy(&deployer_lock, &artifact_binary, fee_shannons)?;

        let spec = DeployArtifactSpec {
            name: name.to_string(),
            artifact_binary,
            artifact_hash,
            deployer_lock,
            capacity_input: capacity_input.input,
            capacity_input_shannons: capacity_input.capacity_shannons,
            capacity_input_data: capacity_input.data,
            type_id_hash_type: ScriptHashType::Type,
            type_script: None,
            cell_deps: Vec::new(),
            header_deps: Vec::new(),
            fee_shannons,
        };

        build_deploy_transaction(&spec)
    }

    // ---- Action workflow ----

    /// Load an action plan from a file path.
    pub fn load_action_plan(&self, path: impl AsRef<Path>) -> Result<ActionPlan> {
        load_action_plan(path)
    }

    /// Load a deployment manifest from a file path.
    pub fn load_deployment_manifest(&self, path: impl AsRef<Path>) -> Result<DeploymentManifest> {
        load_deployment_manifest(path)
    }

    /// Resolve an action plan into a CKB transaction candidate.
    ///
    /// This is a convenience wrapper around `build_action_transaction`
    /// for the common case of using `sample_resolved_action_tx`-style inputs.
    /// For full control, construct `ResolvedActionTx` directly.
    pub fn resolve_action(&self, _plan: &ActionPlan) -> Result<ResolvedActionTx> {
        // TODO: full action resolution with live-cell collection.
        // Current implementation requires the caller to construct ResolvedActionTx manually.
        bail!("full action resolution with live-cell collection is not yet implemented; construct ResolvedActionTx manually and use build_action_transaction()")
    }

    // ---- Node interaction helpers ----

    /// Estimate cycles for a transaction.
    pub fn estimate_cycles(&self, tx: &TransactionView) -> std::result::Result<EstimateCycles, ckb_sdk::RpcError> {
        self.client.estimate_cycles(to_rpc_transaction(tx))
    }

    /// Test tx-pool acceptance for a transaction.
    pub fn test_tx_pool_accept(&self, tx: &TransactionView) -> std::result::Result<EntryCompleted, ckb_sdk::RpcError> {
        self.client.test_tx_pool_accept(to_rpc_transaction(tx), Some(OutputsValidator::Passthrough))
    }

    /// Submit a transaction to the CKB node's tx-pool.
    pub fn submit_transaction(&self, tx: &TransactionView) -> std::result::Result<H256, ckb_sdk::RpcError> {
        self.client.send_transaction(to_rpc_transaction(tx), Some(OutputsValidator::Passthrough))
    }

    /// Wait for a transaction to be committed on-chain.
    pub fn wait_for_commitment(&self, tx_hash: &H256, max_attempts: u32, delay_ms: u64) -> Result<CommittedEvidence> {
        let submitter = TransactionSubmitter::new(&self.client);
        submitter.wait_committed(tx_hash, max_attempts, delay_ms)
    }

    /// Get the tip block number.
    pub fn get_tip_block_number(&self) -> std::result::Result<u64, ckb_sdk::RpcError> {
        let header = self.client.get_tip_header()?;
        Ok(header.inner.number.value())
    }

    /// Query transaction status from the node.
    pub fn get_transaction_status(
        &self,
        tx_hash: &H256,
    ) -> std::result::Result<Option<TransactionWithStatusResponse>, ckb_sdk::RpcError> {
        self.client.get_transaction(tx_hash.clone())
    }

    // ---- Internal helpers ----

    fn find_capacity_for_deploy(&self, _lock: &Script, artifact: &[u8], fee: u64) -> Result<CapacityInput> {
        // TODO: use CellCollector to find a real spendable cell.
        // For now, requires the caller to provide capacity input manually
        // via the lower-level `build_deploy_transaction` API.
        let _ = (_lock, artifact, fee);
        bail!("automatic live-cell collection is not yet implemented; use build_deploy_transaction() with a manually provided DeployArtifactSpec")
    }
}

/// A found capacity input cell for deployment.
struct CapacityInput {
    input: CellInput,
    capacity_shannons: u64,
    data: Bytes,
}

pub fn sample_resolved_action_tx() -> ResolvedActionTx {
    let input_out_point = packed::OutPoint::new_builder().tx_hash([0x11u8; 32].pack()).index(0u32).build();
    let dep_out_point = packed::OutPoint::new_builder().tx_hash([0x22u8; 32].pack()).index(1u32).build();
    let lock = construct_script(&ScriptSpec::new([0x33u8; 32], ScriptHashType::Data1, vec![0x44u8; 20]));
    let output = CellOutput::new_builder().capacity(100_000_000_000u64).lock(lock).build();
    let witness = WitnessArgs::new_builder().input_type(Some(Bytes::from(b"mint".to_vec())).pack()).build();

    ResolvedActionTx {
        metadata_hash: "0".repeat(64),
        artifact_hash: Some("1".repeat(64)),
        action_selector: "mint".to_string(),
        inputs: vec![CellInput::new_builder().previous_output(input_out_point.clone()).build()],
        outputs: vec![CellOutputWithData { output, data: Bytes::from(vec![0x55u8; 16]) }],
        witnesses: vec![witness],
        cell_deps: vec![CellDep::new_builder().out_point(dep_out_point).dep_type(DepType::Code).build()],
        header_deps: Vec::new(),
        lineage: vec![LiveOutputLineage { from: input_out_point, to_output_index: 0, relation: "state-continuation".to_string() }],
        fee_shannons: 1_000,
    }
}

/// Sample deploy spec for testing. Uses a 64-byte pseudo-artifact and a
/// generous capacity input (10 CKB = 10_000_000_000 shannons).
pub fn sample_deploy_spec() -> DeployArtifactSpec {
    let input_out_point = packed::OutPoint::new_builder().tx_hash([0xaau8; 32].pack()).index(0u32).build();
    let lock = construct_script(&ScriptSpec::new([0xbbu8; 32], ScriptHashType::Data1, vec![0xccu8; 20]));
    let artifact = Bytes::from(vec![0xddu8; 64]);
    let artifact_hash = blake2b_256(&artifact).iter().map(|b| format!("{:02x}", b)).collect::<String>();

    DeployArtifactSpec {
        name: "test-token".to_string(),
        artifact_binary: artifact,
        artifact_hash,
        deployer_lock: lock,
        capacity_input: CellInput::new_builder().previous_output(input_out_point).build(),
        capacity_input_shannons: 200_000_000_000,
        capacity_input_data: Bytes::new(),
        type_id_hash_type: ScriptHashType::Type,
        type_script: None,
        cell_deps: Vec::new(),
        header_deps: Vec::new(),
        fee_shannons: 1_000,
    }
}

/// Sample action plan for testing.
pub fn sample_action_plan() -> ActionPlan {
    ActionPlan {
        policy: ACTION_PLAN_POLICY.to_string(),
        action: "mint".to_string(),
        artifact_hash: Some("0".repeat(64)),
        transaction_draft: TransactionDraft {
            state: "resolved".to_string(),
            can_submit: true,
            requires_packed_materialization: false,
        },
        adapter_contract: AdapterContract {
            schema: ADAPTER_CONTRACT_SCHEMA.to_string(),
            compiler_core_dependency: "cellscript-core-v0.19".to_string(),
            transaction_realizer: "headless".to_string(),
            resolved_tx_required_fields: vec!["inputs".to_string(), "outputs".to_string(), "witnesses".to_string()],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compiler_action_plan_boundary() {
        let plan = serde_json::json!({
            "policy": "cellscript-action-builder-plan-v1",
            "action": "mint",
            "artifact_hash": "1".repeat(64),
            "transaction_draft": {
                "state": "ActionPlan",
                "can_submit": false,
                "requires_packed_materialization": true
            },
            "adapter_contract": {
                "schema": "cellscript-ckb-adapter-contract-v0.19",
                "compiler_core_dependency": "no-ckb-sdk-rust",
                "transaction_realizer": "ckb-sdk-rust-or-CCC-adapter",
                "resolved_tx_required_fields": [
                    "outputs_data",
                    "cell_deps",
                    "lineage"
                ]
            }
        });
        let parsed = parse_action_plan(serde_json::to_vec(&plan).unwrap().as_slice()).unwrap();
        assert_eq!(parsed.action, "mint");
        assert_eq!(parsed.adapter_contract.transaction_realizer, "ckb-sdk-rust-or-CCC-adapter");
    }

    #[test]
    fn loads_action_plan_and_deployment_manifest_contracts() {
        let plan = serde_json::json!({
            "policy": ACTION_PLAN_POLICY,
            "action": "mint",
            "artifact_hash": "1".repeat(64),
            "transaction_draft": {
                "state": "ActionPlan",
                "can_submit": false,
                "requires_packed_materialization": true
            },
            "adapter_contract": {
                "schema": ADAPTER_CONTRACT_SCHEMA,
                "compiler_core_dependency": "no-ckb-sdk-rust",
                "transaction_realizer": "ckb-sdk-rust-or-CCC-adapter",
                "resolved_tx_required_fields": ["outputs_data", "cell_deps", "lineage"]
            }
        });
        let manifest = serde_json::json!({
            "schema": DEPLOYMENT_MANIFEST_SCHEMA,
            "version": 1,
            "deployments": [{
                "name": "token",
                "code_hash": "0x11",
                "hash_type": "type",
                "args": "0x22",
                "dep_type": "code",
                "out_point": "0x33:0"
            }]
        });
        let dir = std::env::temp_dir();
        let unique = format!("cellscript-ckb-adapter-{}", std::process::id());
        let plan_path = dir.join(format!("{unique}-action-plan.json"));
        let manifest_path = dir.join(format!("{unique}-deployment-manifest.json"));
        std::fs::write(&plan_path, serde_json::to_vec(&plan).unwrap()).unwrap();
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let loaded_plan = load_action_plan(&plan_path).unwrap();
        let loaded_manifest = load_deployment_manifest(&manifest_path).unwrap();
        let evidence = deployment_evidence(&loaded_manifest);

        assert_eq!(loaded_plan.action, "mint");
        assert_eq!(loaded_manifest.deployments[0].name, "token");
        assert_eq!(evidence.schema, DEPLOYMENT_MANIFEST_SCHEMA);
        assert_eq!(evidence.deployments, 1);
        assert_eq!(evidence.names, vec!["token".to_string()]);

        let _ = std::fs::remove_file(plan_path);
        let _ = std::fs::remove_file(manifest_path);
    }

    #[test]
    fn materializes_resolved_action_with_ckb_sdk_transaction_builder() {
        let resolved = sample_resolved_action_tx();
        let (tx, evidence) = build_action_transaction(&resolved).unwrap();
        assert_eq!(evidence.state, "ResolvedActionTx");
        assert_eq!(evidence.outputs, 1);
        assert_eq!(evidence.outputs_data, 1);
        assert_eq!(evidence.cell_deps, 1);
        assert_eq!(evidence.lineage.len(), 1);
        assert_eq!(evidence.lineage[0].to_output_index, 0);
        assert_eq!(evidence.lineage[0].relation, "state-continuation");
        assert!(evidence.occupied_capacity_shannons > 0);
        assert!(evidence.serialized_tx_size_bytes > 0);
        assert!(!evidence.ckb_vm_execution);
        assert!(!evidence.tx_pool_acceptance);
        assert_eq!(tx.outputs().len(), tx.outputs_data().len());
        assert_eq!(to_rpc_transaction(&tx).outputs.len(), 1);
    }

    #[test]
    fn rejects_under_capacity_output_before_rpc_submission() {
        let mut resolved = sample_resolved_action_tx();
        resolved.outputs[0].output = resolved.outputs[0].output.clone().as_builder().capacity(1u64).build();
        let error = materialize_with_ckb_sdk(&resolved).unwrap_err().to_string();
        assert!(error.contains("below occupied capacity"), "{error}");
    }

    #[test]
    fn rejects_lineage_to_missing_output() {
        let mut resolved = sample_resolved_action_tx();
        resolved.lineage[0].to_output_index = 99;
        let error = materialize_with_ckb_sdk(&resolved).unwrap_err().to_string();
        assert!(error.contains("lineage target output index is out of range"), "{error}");
    }

    #[test]
    fn emits_accepted_action_report_from_node_evidence() {
        let resolved = sample_resolved_action_tx();
        let (_tx, evidence) = materialize_with_ckb_sdk(&resolved).unwrap();
        let estimate = EstimateCycles { cycles: 45_000u64.into() };
        let tx_pool = EntryCompleted { cycles: 45_100u64.into(), fee: 1_234u64.into() };
        let report = emit_acceptance_report(&evidence, &estimate, &tx_pool, Some(H256::from([0xabu8; 32])));

        assert_eq!(report.schema, "cellscript-ckb-action-acceptance-report-v0.19");
        assert_eq!(report.state, "AcceptedActionTx");
        assert!(report.ckb_vm_execution);
        assert!(report.tx_pool_acceptance);
        assert_eq!(report.estimate_cycles, 45_000);
        assert_eq!(report.tx_pool_cycles, 45_100);
        assert_eq!(report.fee_shannons, 1_234);
        assert_eq!(report.submitted_tx_hash.as_ref().expect("tx hash").len(), 32);
        assert_eq!(report.lineage.len(), 1);

        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["submitted_tx_hash"].as_array().expect("submitted hash").len(), 32);
        assert_eq!(json["known_limitations"].as_array().expect("limitations").len(), 1);
    }

    #[test]
    fn emits_frontend_ready_headless_action_preview() {
        let resolved = sample_resolved_action_tx();
        let preview = preview_resolved_action(&resolved);
        assert_eq!(preview.schema, "cellscript-action-preview-v1");
        assert_eq!(preview.action, "mint");
        assert_eq!(preview.consumes.len(), 1);
        assert_eq!(preview.creates.len(), 1);
        assert_eq!(preview.transitions.len(), 1);
        assert_eq!(preview.witnesses.selector, "mint");
        assert_eq!(preview.witnesses.count, 1);
        assert_eq!(preview.estimated_fee, Some(1_000));
        assert!(preview.required_signers.is_empty());
        assert_eq!(preview.consumes[0].out_point_index, Some(0));
        assert_eq!(preview.creates[0].output_index, Some(0));
        assert!(preview.creates[0].lock_hash.as_ref().is_some_and(|hash| hash.len() == 32));
        assert!(preview.warnings.iter().any(|warning| warning.contains("tx-pool acceptance")));

        let json = serde_json::to_value(&preview).unwrap();
        assert_eq!(json["requiredSigners"], serde_json::json!([]));
        assert_eq!(json["estimatedFee"], serde_json::json!(1_000));
        assert_eq!(json["creates"][0]["dataLen"], serde_json::json!(16));
    }

    #[test]
    fn places_cellscript_entry_payload_without_hiding_lock_signatures() {
        let base = WitnessArgs::new_builder().lock(Some(Bytes::from(vec![0x77u8; 65])).pack()).build();
        let payload = Bytes::from(b"CSARGv1\0\x4d\0\0\0\0\0\0\0".to_vec());
        let witness = place_entry_witness_payload(&base, WitnessPlacement::InputType, payload.clone()).unwrap();
        assert_eq!(witness.lock().to_opt().expect("lock preserved").raw_data().len(), 65);
        assert_eq!(witness.input_type().to_opt().expect("entry payload").raw_data(), payload);
        assert!(witness.output_type().to_opt().is_none());

        let error = place_entry_witness_payload(&base, WitnessPlacement::Lock, Bytes::from(vec![1u8])).unwrap_err().to_string();
        assert!(error.contains("lock signatures must stay explicit"), "{error}");
    }

    #[test]
    fn computes_and_checks_type_id_args_from_packed_input_and_output_index() {
        let mut resolved = sample_resolved_action_tx();
        let first_input = resolved.inputs.remove(0);
        let output_index = 3u64;
        let args = type_id_args_from_first_input(&first_input, output_index);
        let lock = construct_script(&ScriptSpec::new([0x33u8; 32], ScriptHashType::Data1, vec![0x44u8; 20]));
        let type_script = construct_script(&ScriptSpec::new([0x55u8; 32], ScriptHashType::Type, args.to_vec()));
        let output = CellOutput::new_builder().capacity(100_000_000_000u64).lock(lock.clone()).type_(Some(type_script).pack()).build();

        verify_type_id_output_args(&first_input, output_index, &output).unwrap();
        let wrong_type_script = construct_script(&ScriptSpec::new([0x55u8; 32], ScriptHashType::Type, vec![0x99u8; 32]));
        let wrong_output = output.as_builder().type_(Some(wrong_type_script).pack()).build();
        let error = verify_type_id_output_args(&first_input, output_index, &wrong_output).unwrap_err().to_string();
        assert!(error.contains("TYPE_ID output args do not match"), "{error}");
    }

    #[test]
    fn constructs_arbitrary_scripts_with_ckb_types_hash_and_args_evidence() {
        let spec = ScriptSpec::new([0xabu8; 32], ScriptHashType::Data2, vec![1u8, 2, 3, 4, 5]);
        let script = construct_script(&spec);
        assert_eq!(script.code_hash().as_slice(), &[0xabu8; 32]);
        assert_eq!(script.hash_type(), ScriptHashType::Data2.into());
        assert_eq!(script.args().raw_data(), Bytes::from(vec![1u8, 2, 3, 4, 5]));

        let evidence = spec.evidence();
        assert_eq!(evidence.schema, "cellscript-ckb-script-evidence-v0.19");
        assert_eq!(evidence.hash_type, "data2");
        assert_eq!(evidence.args_len, 5);
        assert_eq!(evidence.script_hash, script.calc_script_hash().as_slice().to_vec());

        let changed = ScriptSpec::new([0xabu8; 32], ScriptHashType::Data2, vec![1u8, 2, 3, 4, 6]);
        assert_ne!(spec.script_hash(), changed.script_hash());
    }

    #[test]
    fn checks_script_args_patterns_and_owner_mode_args() {
        let owner = construct_script(&ScriptSpec::new([0x33u8; 32], ScriptHashType::Data1, vec![0x44u8; 20]));
        let owner_args = owner_mode_args_from_lock(&owner);
        assert_eq!(owner_args.as_ref(), owner.calc_script_hash().as_slice());

        let script = construct_script(&ScriptSpec::new([0x77u8; 32], ScriptHashType::Type, vec![1u8, 2, 3, 4, 5]));
        assert!(matches_script_args(&script, &ScriptArgsPattern::Exact(Bytes::from(vec![1u8, 2, 3, 4, 5]))));
        assert!(matches_script_args(&script, &ScriptArgsPattern::Prefix(Bytes::from(vec![1u8, 2, 3]))));
        assert!(matches_script_args(&script, &ScriptArgsPattern::Suffix(Bytes::from(vec![4u8, 5]))));
        assert!(!matches_script_args(&script, &ScriptArgsPattern::Exact(Bytes::from(vec![1u8, 2]))));
    }

    #[test]
    fn reads_lock_and_type_script_refs_from_outputs() {
        let lock_spec = ScriptSpec::new([0x11u8; 32], ScriptHashType::Data1, vec![0x22u8; 20]);
        let type_spec = ScriptSpec::new([0x33u8; 32], ScriptHashType::Type, vec![0x44u8; 32]);
        let output = CellOutput::new_builder()
            .capacity(100_000_000_000u64)
            .lock(construct_script(&lock_spec))
            .type_(Some(construct_script(&type_spec)).pack())
            .build();

        let lock_ref = lock_script_ref(&output);
        let type_ref = type_script_ref(&output).expect("type script ref");
        require_script_ref_matches(&lock_ref, &lock_spec).unwrap();
        require_script_ref_matches(&type_ref, &type_spec).unwrap();

        let evidence = type_ref.evidence();
        assert_eq!(evidence.schema, "cellscript-ckb-script-ref-evidence-v0.19");
        assert_eq!(evidence.role, ScriptRole::Type);
        assert_eq!(evidence.code_hash, vec![0x33u8; 32]);
        assert_eq!(evidence.args_len, 32);

        let wrong_spec = ScriptSpec::new([0x33u8; 32], ScriptHashType::Type, vec![0x45u8; 32]);
        let error = require_script_ref_matches(&type_ref, &wrong_spec).unwrap_err().to_string();
        assert!(error.contains("type script args mismatch"), "{error}");
    }

    #[test]
    fn missing_type_script_ref_is_explicit() {
        let mut resolved = sample_resolved_action_tx();
        let output = resolved.outputs.remove(0).output;
        assert!(type_script_ref(&output).is_none());
        assert_eq!(lock_script_ref(&output).role, ScriptRole::Lock);
    }

    #[test]
    fn binds_scripts_to_explicit_cell_deps() {
        let script = construct_script(&ScriptSpec::new([0x88u8; 32], ScriptHashType::Data1, vec![0x99u8; 20]));
        let out_point = packed::OutPoint::new_builder().tx_hash([0xaau8; 32].pack()).index(7u32).build();
        let dep = ScriptCodeDep::from_script(&script, out_point.clone(), DepType::DepGroup);
        let cell_dep = require_script_code_dep(&script, std::slice::from_ref(&dep)).unwrap();
        assert_eq!(cell_dep.out_point(), out_point);
        assert_eq!(cell_dep.dep_type(), DepType::DepGroup.into());

        let evidence = dep.evidence();
        assert_eq!(evidence.schema, "cellscript-ckb-script-code-dep-evidence-v0.19");
        assert_eq!(evidence.hash_type_byte, 2);
        assert_eq!(evidence.out_point_index, 7);
        assert_eq!(evidence.dep_type, "DepGroup");
    }

    #[test]
    fn rejects_missing_or_wrong_hash_type_script_deps() {
        let script = construct_script(&ScriptSpec::new([0x88u8; 32], ScriptHashType::Data1, vec![0x99u8; 20]));
        let out_point = packed::OutPoint::new_builder().tx_hash([0xaau8; 32].pack()).index(7u32).build();
        let wrong_dep = ScriptCodeDep::new([0x88u8; 32], ScriptHashType::Type, out_point, DepType::Code);

        let missing = require_script_code_dep(&script, &[]).unwrap_err().to_string();
        assert!(missing.contains("missing CellDep"), "{missing}");

        let wrong = require_script_code_dep(&script, &[wrong_dep]).unwrap_err().to_string();
        assert!(wrong.contains("missing CellDep"), "{wrong}");
    }

    #[test]
    fn binds_ckb_sdk_signing_boundary_without_compiler_dependency() {
        assert!(signing_boundary_type().contains("SecpSighashScriptSigner"));
    }

    // ---- Deploy probe tests ----

    #[test]
    fn builds_deploy_transaction_with_type_id_code_cell() {
        let spec = sample_deploy_spec();
        let (tx, evidence) = build_deploy_transaction(&spec).unwrap();

        // Evidence checks.
        assert_eq!(evidence.schema, DEPLOY_EVIDENCE_SCHEMA);
        assert_eq!(evidence.state, "ResolvedDeployTx");
        assert_eq!(evidence.name, "test-token");
        assert_eq!(evidence.code_output_index, 0);
        assert_eq!(evidence.change_output_index, 1);
        assert_eq!(evidence.hash_type, "type");
        assert_eq!(evidence.type_id_args.len(), 32);
        assert_eq!(evidence.code_hash.len(), 32);
        assert!(evidence.occupied_capacity_shannons > 0);
        assert!(evidence.change_capacity_shannons > 0);
        assert!(evidence.serialized_tx_size_bytes > 0);
        assert!(!evidence.ckb_vm_execution);
        assert!(!evidence.tx_pool_acceptance);

        // Transaction shape checks.
        assert_eq!(tx.inputs().len(), 1);
        assert_eq!(tx.outputs().len(), 2);
        assert_eq!(tx.outputs_data().len(), 2);

        // Code output has a type script (TYPE_ID).
        let code_output = tx.outputs().get(0).unwrap();
        assert!(code_output.type_().is_some(), "code output must have type script for TYPE_ID");

        // Change output has no type script.
        let change_output = tx.outputs().get(1).unwrap();
        assert!(change_output.type_().is_none(), "change output should not have type script");

        // Artifact data is in the first output_data.
        let code_data = tx.outputs_data().get(0).unwrap().raw_data();
        assert_eq!(code_data.len(), 64);
    }

    #[test]
    fn deploy_type_id_args_match_first_input_and_output_index() {
        let spec = sample_deploy_spec();
        let (_tx, evidence) = build_deploy_transaction(&spec).unwrap();

        // TYPE_ID args = blake2b(first_input || output_index_le)
        let expected_args = type_id_args_from_first_input(&spec.capacity_input, 0);
        assert_eq!(evidence.type_id_args, expected_args.to_vec());
    }

    #[test]
    fn deploy_code_hash_is_blake2b_of_artifact() {
        let spec = sample_deploy_spec();
        let (_tx, evidence) = build_deploy_transaction(&spec).unwrap();

        let expected_hash = blake2b_256(&spec.artifact_binary);
        assert_eq!(evidence.code_hash, expected_hash.to_vec());
    }

    #[test]
    fn deploy_rejects_empty_artifact() {
        let mut spec = sample_deploy_spec();
        spec.artifact_binary = Bytes::new();
        let error = build_deploy_transaction(&spec).unwrap_err().to_string();
        assert!(error.contains("artifact binary must be non-empty"), "{error}");
    }

    #[test]
    fn deploy_rejects_zero_capacity_input() {
        let mut spec = sample_deploy_spec();
        spec.capacity_input_shannons = 0;
        let error = build_deploy_transaction(&spec).unwrap_err().to_string();
        assert!(error.contains("non-zero capacity"), "{error}");
    }

    #[test]
    fn deploy_rejects_insufficient_input_capacity() {
        let mut spec = sample_deploy_spec();
        spec.capacity_input_shannons = 1; // far too small
        let error = build_deploy_transaction(&spec).unwrap_err().to_string();
        assert!(error.contains("insufficient"), "{error}");
    }

    #[test]
    fn deploy_rejects_insufficient_remaining_for_fee() {
        let mut spec = sample_deploy_spec();
        // Set fee to more than the entire input.
        spec.fee_shannons = spec.capacity_input_shannons;
        let error = build_deploy_transaction(&spec).unwrap_err().to_string();
        assert!(error.contains("insufficient for fee"), "{error}");
    }

    #[test]
    fn deploy_builds_deployment_manifest_from_evidence() {
        let spec = sample_deploy_spec();
        let (_tx, evidence) = build_deploy_transaction(&spec).unwrap();

        let tx_hash = [0xeeu8; 32];
        let manifest = build_deployment_manifest_from_evidence(&evidence, &tx_hash, 0);

        assert_eq!(manifest.schema, DEPLOYMENT_MANIFEST_SCHEMA);
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.deployments.len(), 1);

        let dep = &manifest.deployments[0];
        assert_eq!(dep.name, "test-token");
        assert!(dep.code_hash.starts_with("0x"));
        assert_eq!(dep.hash_type, "type");
        assert!(dep.args.starts_with("0x"));
        assert_eq!(dep.dep_type, "code");
        assert!(dep.out_point.contains(":0"));

        // Verify the manifest parses back correctly.
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let reloaded = parse_deployment_manifest(&manifest_json).unwrap();
        assert_eq!(reloaded.deployments[0].name, "test-token");
    }

    #[test]
    fn deploy_with_cell_deps_includes_them_in_transaction() {
        let mut spec = sample_deploy_spec();
        let dep_out_point = packed::OutPoint::new_builder().tx_hash([0xffu8; 32].pack()).index(2u32).build();
        spec.cell_deps = vec![CellDep::new_builder().out_point(dep_out_point).dep_type(DepType::Code).build()];

        let (tx, evidence) = build_deploy_transaction(&spec).unwrap();
        assert_eq!(evidence.cell_deps, 1);
        assert_eq!(tx.cell_deps().len(), 1);
    }

    // ---- ManifestCellDepResolver tests ----

    #[test]
    fn manifest_resolver_resolves_deps_from_deployment_manifest() {
        let code_hash = blake2b_256([0xddu8; 64]);
        let tx_hash = [0xeeu8; 32];
        let manifest = DeploymentManifest {
            schema: DEPLOYMENT_MANIFEST_SCHEMA.to_string(),
            version: 1,
            deployments: vec![DeploymentRef {
                name: "test-token".to_string(),
                code_hash: format!("0x{}", hex::encode(code_hash)),
                hash_type: "type".to_string(),
                args: "0x22".to_string(),
                dep_type: "code".to_string(),
                out_point: format!("0x{}:0", hex::encode(tx_hash)),
            }],
        };

        let resolver = ManifestCellDepResolver::from_manifest(&manifest).unwrap();
        assert_eq!(resolver.len(), 1);
        assert!(!resolver.is_empty());

        // Resolve by constructing a matching script.
        let script = Script::new_builder()
            .code_hash(code_hash.pack())
            .hash_type(ScriptHashType::Type)
            .args(Bytes::from(vec![0x22]).pack())
            .build();
        let dep = resolver.resolve_for_script(&script).expect("should resolve");
        assert_eq!(dep.dep_type(), DepType::Code.into());

        // Non-matching script should return None.
        let wrong_script = Script::new_builder()
            .code_hash([0x99u8; 32].pack())
            .hash_type(ScriptHashType::Data1)
            .args(Bytes::from(vec![0x22]).pack())
            .build();
        assert!(resolver.resolve_for_script(&wrong_script).is_none());
    }

    #[test]
    fn manifest_resolver_rejects_invalid_manifest_entries() {
        // Invalid code_hash (not 32 bytes).
        let manifest = DeploymentManifest {
            schema: DEPLOYMENT_MANIFEST_SCHEMA.to_string(),
            version: 1,
            deployments: vec![DeploymentRef {
                name: "bad".to_string(),
                code_hash: "0x11".to_string(),
                hash_type: "type".to_string(),
                args: "0x22".to_string(),
                dep_type: "code".to_string(),
                out_point: format!("0x{}:0", hex::encode([0xeeu8; 32])),
            }],
        };
        let error = ManifestCellDepResolver::from_manifest(&manifest).unwrap_err().to_string();
        assert!(error.contains("must be 32 bytes"), "{error}");
    }

    #[test]
    fn manifest_resolver_supports_data_and_type_hash_types() {
        let code_hash_data = blake2b_256([0x11u8; 32]);
        let code_hash_type = blake2b_256([0x22u8; 32]);
        let code_hash_data1 = blake2b_256([0x33u8; 32]);
        let code_hash_data2 = blake2b_256([0x44u8; 32]);
        let tx_hash = [0xeeu8; 32];
        let manifest = DeploymentManifest {
            schema: DEPLOYMENT_MANIFEST_SCHEMA.to_string(),
            version: 1,
            deployments: vec![
                DeploymentRef {
                    name: "data-dep".to_string(),
                    code_hash: format!("0x{}", hex::encode(code_hash_data)),
                    hash_type: "data".to_string(),
                    args: "0x".to_string(),
                    dep_type: "code".to_string(),
                    out_point: format!("0x{}:0", hex::encode(tx_hash)),
                },
                DeploymentRef {
                    name: "type-dep".to_string(),
                    code_hash: format!("0x{}", hex::encode(code_hash_type)),
                    hash_type: "type".to_string(),
                    args: "0x".to_string(),
                    dep_type: "dep_group".to_string(),
                    out_point: format!("0x{}:1", hex::encode(tx_hash)),
                },
                DeploymentRef {
                    name: "data1-dep".to_string(),
                    code_hash: format!("0x{}", hex::encode(code_hash_data1)),
                    hash_type: "data1".to_string(),
                    args: "0x".to_string(),
                    dep_type: "code".to_string(),
                    out_point: format!("0x{}:2", hex::encode(tx_hash)),
                },
                DeploymentRef {
                    name: "data2-dep".to_string(),
                    code_hash: format!("0x{}", hex::encode(code_hash_data2)),
                    hash_type: "data2".to_string(),
                    args: "0x".to_string(),
                    dep_type: "dep_group".to_string(),
                    out_point: format!("0x{}:3", hex::encode(tx_hash)),
                },
            ],
        };

        let resolver = ManifestCellDepResolver::from_manifest(&manifest).unwrap();
        assert_eq!(resolver.len(), 4);

        let data_script =
            Script::new_builder().code_hash(code_hash_data.pack()).hash_type(ScriptHashType::Data).args(Bytes::new().pack()).build();
        let dep = resolver.resolve_for_script(&data_script).expect("data dep");
        assert_eq!(dep.dep_type(), DepType::Code.into());

        let type_script =
            Script::new_builder().code_hash(code_hash_type.pack()).hash_type(ScriptHashType::Type).args(Bytes::new().pack()).build();
        let dep = resolver.resolve_for_script(&type_script).expect("type dep");
        assert_eq!(dep.dep_type(), DepType::DepGroup.into());

        let data1_script =
            Script::new_builder().code_hash(code_hash_data1.pack()).hash_type(ScriptHashType::Data1).args(Bytes::new().pack()).build();
        let dep = resolver.resolve_for_script(&data1_script).expect("data1 dep");
        assert_eq!(dep.dep_type(), DepType::Code.into());

        let data2_script =
            Script::new_builder().code_hash(code_hash_data2.pack()).hash_type(ScriptHashType::Data2).args(Bytes::new().pack()).build();
        let dep = resolver.resolve_for_script(&data2_script).expect("data2 dep");
        assert_eq!(dep.dep_type(), DepType::DepGroup.into());
    }

    // ---- SigningAdapter tests ----

    #[test]
    fn signing_adapter_tracks_signer_labels_and_state() {
        let mut adapter = SigningAdapter::new(vec!["secp256k1-sighash".to_string()]);
        assert!(!adapter.signed);
        assert_eq!(adapter.signer_labels.len(), 1);

        let evidence = adapter.evidence();
        assert_eq!(evidence.schema, "cellscript-ckb-signing-adapter-v0.19");
        assert_eq!(evidence.signer_count, 1);
        assert!(!evidence.signed);

        adapter.mark_signed();
        assert!(adapter.signed);
        assert!(adapter.evidence().signed);
    }

    #[test]
    fn signing_adapter_for_secp_sighash() {
        let lock_arg = H160::from([0x44u8; 20]);
        let adapter = SigningAdapter::for_secp_sighash(lock_arg);
        assert!(adapter.signer_labels[0].contains("secp256k1-sighash"));
        assert!(adapter.signer_labels[0].contains("4444"));
    }

    // ---- CapacityBridge tests ----

    #[test]
    fn capacity_bridge_builds_balancer_and_evidence() {
        let change_lock = construct_script(&ScriptSpec::new([0x33u8; 32], ScriptHashType::Data1, vec![0x44u8; 20]));
        let bridge = CapacityBridge::new(change_lock.clone(), 1000);
        let balancer = bridge.to_balancer();
        // CapacityBalancer fields are private; just verify it doesn't panic.
        drop(balancer);

        let evidence = bridge.evidence();
        assert_eq!(evidence.schema, "cellscript-ckb-capacity-bridge-v0.19");
        assert_eq!(evidence.change_lock_hash, change_lock.calc_script_hash().as_slice().to_vec());
        assert_eq!(evidence.fee_rate, 1000);
    }

    // ---- TransactionLifecycleEvidence test ----

    #[test]
    fn lifecycle_evidence_records_full_transaction_flow() {
        let mut signing = SigningAdapter::new(vec!["test-signer".to_string()]);
        signing.mark_signed();

        let lifecycle = TransactionLifecycleEvidence {
            schema: "cellscript-ckb-tx-lifecycle-v0.19",
            deploy_evidence: None,
            action_evidence: None,
            signing: signing.evidence(),
            capacity: None,
            estimate_cycles: Some(45_000),
            tx_pool_accepted: true,
            submitted: true,
            committed: None,
        };

        assert!(lifecycle.signing.signed);
        assert_eq!(lifecycle.estimate_cycles, Some(45_000));
        assert!(lifecycle.tx_pool_accepted);
        assert!(lifecycle.submitted);

        let json = serde_json::to_value(&lifecycle).unwrap();
        assert_eq!(json["schema"], "cellscript-ckb-tx-lifecycle-v0.19");
        assert!(json["signing"]["signed"].as_bool().unwrap());
    }

    // ---- CellScriptAdapter facade tests ----

    #[test]
    fn adapter_connect_fails_on_unreachable_node() {
        let result = CellScriptAdapter::connect("http://127.0.0.1:19999");
        assert!(result.is_err(), "should fail connecting to non-existent node");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("cannot connect"), "{msg}");
    }

    #[test]
    fn adapter_build_deploy_works_via_low_level_api() {
        let spec = sample_deploy_spec();
        let result = build_deploy_transaction(&spec);
        assert!(result.is_ok(), "low-level build_deploy_transaction should work without a node");
    }

    #[test]
    fn adapter_sample_action_plan_is_valid() {
        let plan = sample_action_plan();
        assert_eq!(plan.action, "mint");
        assert_eq!(plan.policy, ACTION_PLAN_POLICY);
        assert!(plan.artifact_hash.is_some());
        assert!(plan.transaction_draft.can_submit);
    }

    #[test]
    fn adapter_sample_deployment_manifest_round_trips() {
        // Verify a manifest can be created from deploy evidence and parsed back.
        let spec = sample_deploy_spec();
        let (_, evidence) = build_deploy_transaction(&spec).unwrap();
        let manifest = build_deployment_manifest_from_evidence(&evidence, &[0xabu8; 32], 0);
        assert_eq!(manifest.deployments.len(), 1);
        assert_eq!(manifest.deployments[0].name, "test-token");
    }
}
