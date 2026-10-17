//! Concrete ProtocolBundle/CKB byte parity. Input Cells are host snapshots;
//! signatures, liveness and consensus remain separate adapter/VM obligations.
use super::{invalid, molecule, SuppliedInputCell};
use crate::{ckb_blake2b256, hex_encode, CheckerBudgets, CheckerError, CheckerRejectionCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Serialized binding evidence only. Deserialization never yields private
/// source or compatibility admission; native constructors retain those proofs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedOpenHandle {
    pub schema: String,
    pub consumer: String,
    pub parameter: String,
    pub participant: String,
    pub interface_module: String,
    pub source_policy: String,
    pub selected_receipt: String,
    pub authorization_root: String,
    pub selection: String,
    pub witness_index: u32,
    pub witness_field: String,
    pub code_dependency_index: u32,
    pub selected_script_hash: String,
    pub selected_artifact_hash: String,
    pub checked_dependency: String,
    pub checked_type_group: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_invocation: Option<super::OpenVerifierInvocation>,
    pub raw_transaction_hash: String,
    pub serialized_transaction_hash: String,
    pub peer_input_indices: Vec<u32>,
    pub peer_output_indices: Vec<u32>,
    pub claim_boundaries: OpenHandleClaimBoundaries,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<OpenParticipantObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<ResolvedInstanceAdmission>,
}

/// Evidence of the separately versioned instance policy. Old selections keep
/// `instance` absent and retain their exact-instance meaning and byte format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedInstanceAdmission {
    pub schema: String,
    pub source_policy: String,
    pub selection_identity: String,
    pub implementation_receipt: String,
    pub args_schema: String,
    pub policy_root: String,
    pub consumer_args: String,
    pub instance_mode: String,
}

/// Assign responsibility for a claim, never assert that pending execution or
/// consensus has occurred. Offline admission cannot upgrade these boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenHandleClaimBoundary {
    CheckedByCurrentScript,
    RequiredPeerScriptGroup,
    CheckedByBundleDryRun,
    GuaranteedOnlyByCkbConsensus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenHandleClaimBoundaries {
    pub membership_and_committed_root: OpenHandleClaimBoundary,
    pub selected_code_dependency: OpenHandleClaimBoundary,
    pub observed_script: Option<OpenHandleClaimBoundary>,
    pub participant_predicates_and_data: OpenHandleClaimBoundary,
    pub complete_transaction_execution: OpenHandleClaimBoundary,
    pub live_consensus_acceptance: OpenHandleClaimBoundary,
    pub foreign_observation_grants_lifecycle_authority: bool,
}
impl OpenHandleClaimBoundaries {
    pub fn for_verifier() -> Self {
        let mut claims = Self::for_observation(false);
        claims.participant_predicates_and_data = OpenHandleClaimBoundary::CheckedByBundleDryRun;
        claims
    }
    pub fn for_observation(observed: bool) -> Self {
        use OpenHandleClaimBoundary::*;
        Self {
            membership_and_committed_root: CheckedByCurrentScript,
            selected_code_dependency: CheckedByCurrentScript,
            observed_script: observed.then_some(CheckedByCurrentScript),
            participant_predicates_and_data: RequiredPeerScriptGroup,
            complete_transaction_execution: CheckedByBundleDryRun,
            live_consensus_acceptance: GuaranteedOnlyByCkbConsensus,
            foreign_observation_grants_lifecycle_authority: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenCellSource {
    Input,
    Output,
    GroupInput,
    GroupOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenParticipantObservation {
    pub source: OpenCellSource,
    pub index: u32,
    pub transaction_index: u32,
    pub type_name: String,
    pub schema_hash: String,
}

#[derive(Debug)]
pub struct CheckedProtocolTransaction<'a> {
    full: &'a [u8],
    raw: &'a [u8],
    inputs: Vec<&'a [u8]>,
    outputs: Vec<&'a [u8]>,
    witnesses: Vec<&'a [u8]>,
}
impl<'a> CheckedProtocolTransaction<'a> {
    pub fn raw_transaction(&self) -> &'a [u8] {
        self.raw
    }
    pub fn full_transaction(&self) -> &'a [u8] {
        self.full
    }
    pub fn raw_hash(&self) -> String {
        hex_encode(&ckb_blake2b256(self.raw))
    }
    pub fn full_hash(&self) -> String {
        hex_encode(&ckb_blake2b256(self.full))
    }
    /// Derive complete Script groups from every checked input/output snapshot.
    /// Lock outputs are observations only and never an executing Lock group.
    pub fn group_indices(&self, script: &[u8], lock: bool) -> Result<(Vec<usize>, Vec<usize>), CheckerError> {
        molecule::script(script)?;
        let matching = |cells: &[&[u8]]| -> Result<Vec<usize>, CheckerError> {
            let mut indices = Vec::new();
            for (index, bytes) in cells.iter().enumerate() {
                let cell = molecule::cell(bytes)?;
                let actual = if lock { Some(cell.lock) } else { cell.type_script };
                if actual.is_some_and(|actual| actual.bytes == script) {
                    indices.push(index);
                }
            }
            Ok(indices)
        };
        Ok((matching(&self.inputs)?, if lock { Vec::new() } else { matching(&self.outputs)? }))
    }
    pub fn witness_field(&self, index: usize, field: usize) -> Result<&'a [u8], CheckerError> {
        if field > 2 {
            return Err(invalid("unknown WitnessArgs field"));
        }
        let witness = self.witnesses.get(index).ok_or_else(|| invalid("missing selected witness"))?;
        let fields = molecule::fields(witness, 3)?;
        if fields[field].is_empty() {
            return Ok(&[]);
        }
        molecule::fixed(fields[field], 1, 4 * 1024 * 1024)
    }
}
fn bytes_equal(value: &Value, bytes: &[u8]) -> bool {
    value.as_str().is_some_and(|text| text.strip_prefix("0x").is_some_and(|hex| hex == hex_encode(bytes)))
}
fn script_equal(value: &Value, bytes: &molecule::Script<'_>) -> bool {
    let kind = match bytes.hash_type {
        0 => "data",
        1 => "type",
        2 => "data1",
        4 => "data2",
        _ => return false,
    };
    bytes_equal(&value["code_hash"], bytes.code_hash) && value["hash_type"] == kind && bytes_equal(&value["args"], bytes.args)
}
fn cell_equal(value: &Value, output: &[u8], data: &[u8]) -> Result<bool, CheckerError> {
    let cell = molecule::cell(output)?;
    Ok(value["capacity"].as_u64() == Some(cell.capacity)
        && script_equal(&value["lock"], &cell.lock)
        && match cell.type_script {
            Some(script) => script_equal(&value["type"], &script),
            None => value["type"].is_null(),
        }
        && bytes_equal(&value["data"], data))
}
fn array<'a>(value: &'a Value, field: &str, max: usize) -> Result<&'a [Value], CheckerError> {
    let items = value[field].as_array().ok_or_else(|| invalid("concrete transaction requires all array fields"))?;
    if items.len() > max {
        return Err(invalid("concrete transaction count exceeds the bounded profile"));
    }
    Ok(items)
}
fn point_equal(value: &Value, bytes: &[u8]) -> bool {
    bytes.len() == 36
        && bytes_equal(&value["tx_hash"], &bytes[..32])
        && value["index"].as_u64() == Some(u64::from(u32::from_le_bytes(bytes[32..].try_into().expect("OutPoint width"))))
}
/// Compare every concrete skeleton field against canonical complete transaction
/// bytes and all supplied input Cells, before open receipt admission. There is
/// no second transaction serializer and no supplied Script-group index.
/// At most 256 inputs/outputs/witnesses, 64 deps/headers, 4 MiB per byte input,
/// and 16 MiB aggregate; the JSON caller must also bound its parser input.
pub fn check_protocol_transaction<'a>(
    full: &'a [u8],
    inputs: &'a [SuppliedInputCell<'a>],
    skeleton: &Value,
    budgets: &CheckerBudgets,
) -> Result<CheckedProtocolTransaction<'a>, CheckerError> {
    if inputs.len() > 256 {
        return Err(invalid("transaction input snapshot count exceeds 256"));
    }
    let mut total = 0usize;
    for bytes in std::iter::once(full).chain(inputs.iter().flat_map(|c| [c.out_point.as_slice(), c.output, c.data])) {
        total = total.checked_add(bytes.len()).ok_or_else(|| invalid("transaction byte sum overflow"))?;
        if bytes.len() > 4 * 1024 * 1024 || bytes.len() as u64 > budgets.record_bytes || total > 16 * 1024 * 1024 {
            return Err(CheckerError::new(CheckerRejectionCode::V2400BudgetExceeded, "concrete transaction exceeds byte budgets"));
        }
    }
    let input_slots = array(skeleton, "inputs", 256)?;
    let output_slots = array(skeleton, "outputs", 256)?;
    let dep_slots = array(skeleton, "cell_deps", 64)?;
    let header_slots = array(skeleton, "header_deps", 64)?;
    let witness_slots = array(skeleton, "witnesses", 256)?;
    let parts = molecule::fields(full, 2)?;
    let raw = molecule::transaction(parts[0])?;
    let raw_fields = molecule::fields(parts[0], 6)?;
    let outputs = molecule::dynamic(raw_fields[4], 256)?;
    let headers = molecule::fixed(raw_fields[2], 32, 64)?;
    let witnesses = molecule::dynamic(parts[1], 256)?
        .into_iter()
        .map(|bytes| molecule::fixed(bytes, 1, 4 * 1024 * 1024))
        .collect::<Result<Vec<_>, _>>()?;
    if skeleton["version"] != 0
        || input_slots.len() != inputs.len()
        || raw.inputs.len() / 44 != inputs.len()
        || output_slots.len() != outputs.len()
        || dep_slots.len() != raw.deps.len() / 37
        || header_slots.len() != headers.len() / 32
        || witness_slots.len() != witnesses.len()
    {
        return Err(invalid("transaction and concrete skeleton cardinalities differ"));
    }
    let mut by_point = BTreeMap::new();
    for cell in inputs {
        if by_point.insert(cell.out_point, cell).is_some() {
            return Err(invalid("duplicate input snapshot"));
        }
    }
    let mut ordered_inputs = Vec::new();
    for (slot, input) in input_slots.iter().zip(raw.inputs.chunks_exact(44)) {
        let point: [u8; 36] = input[8..].try_into().expect("CellInput width");
        let cell = by_point.remove(&point).ok_or_else(|| invalid("missing or repeated input snapshot"))?;
        let since = if slot["since"].is_null() {
            0
        } else {
            slot["since"].as_u64().ok_or_else(|| invalid("input since must be an unsigned integer"))?
        };
        if !point_equal(&slot["out_point"], &point)
            || since != u64::from_le_bytes(input[..8].try_into().expect("since width"))
            || !cell_equal(slot, cell.output, cell.data)?
        {
            return Err(invalid("input bytes or snapshot differ from skeleton"));
        }
        ordered_inputs.push(cell.output);
    }
    for ((slot, output), data) in output_slots.iter().zip(&outputs).zip(&raw.data) {
        if !cell_equal(slot, output, data)? {
            return Err(invalid("output bytes differ from skeleton"));
        }
    }
    for (slot, dep) in dep_slots.iter().zip(raw.deps.chunks_exact(37)) {
        if !point_equal(&slot["out_point"], &dep[..36]) || slot["dep_type"] != if dep[36] == 0 { "code" } else { "dep_group" } {
            return Err(invalid("CellDep bytes differ from skeleton"));
        }
    }
    for (slot, header) in header_slots.iter().zip(headers.chunks_exact(32)) {
        if !bytes_equal(slot, header) {
            return Err(invalid("HeaderDep bytes differ from skeleton"));
        }
    }
    for (slot, witness) in witness_slots.iter().zip(&witnesses) {
        let fields = molecule::fields(witness, 3)?;
        for (name, field) in ["lock", "input_type", "output_type"].into_iter().zip(fields) {
            let data_key = format!("{name}_bytes");
            if field.is_empty() {
                if !slot[name].is_null() || !slot[&data_key].is_null() {
                    return Err(invalid("absent witness field was substituted"));
                }
            } else {
                let data = molecule::fixed(field, 1, 4 * 1024 * 1024)?;
                if !bytes_equal(&slot[&data_key], data) || !bytes_equal(&slot[name], &ckb_blake2b256(data)) {
                    return Err(invalid("witness bytes or commitment differ from skeleton"));
                }
            }
        }
    }
    Ok(CheckedProtocolTransaction { full, raw: parts[0], inputs: ordered_inputs, outputs, witnesses })
}
