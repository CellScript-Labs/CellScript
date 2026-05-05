use anyhow::{bail, Result};
use ckb_hash::blake2b_256;
use ckb_jsonrpc_types::{EntryCompleted, EstimateCycles, OutputsValidator, Transaction as RpcTransaction};
use ckb_sdk::{core::TransactionBuilder, unlock::SecpSighashScriptSigner, CkbRpcClient};
use ckb_types::{
    bytes::Bytes,
    core::{Capacity, DepType, ScriptHashType, TransactionView},
    packed::{self, Byte32, CellDep, CellInput, CellOutput, Script, WitnessArgs},
    prelude::*,
    H256,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const ACTION_PLAN_POLICY: &str = "cellscript-action-builder-plan-v1";
pub const ADAPTER_CONTRACT_SCHEMA: &str = "cellscript-ckb-adapter-contract-v0.19";
pub const ACTION_ACCEPTANCE_REPORT_SCHEMA: &str = "cellscript-ckb-action-acceptance-report-v0.19";
pub const SCRIPT_EVIDENCE_SCHEMA: &str = "cellscript-ckb-script-evidence-v0.19";
pub const SCRIPT_REF_EVIDENCE_SCHEMA: &str = "cellscript-ckb-script-ref-evidence-v0.19";
pub const SCRIPT_CODE_DEP_EVIDENCE_SCHEMA: &str = "cellscript-ckb-script-code-dep-evidence-v0.19";
pub const DEPLOYMENT_MANIFEST_SCHEMA: &str = "cellscript-ckb-deployment-manifest-v0.19";

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

pub fn signing_boundary_type() -> &'static str {
    std::any::type_name::<SecpSighashScriptSigner>()
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
}
