//! Host byte consistency for a direct data2 Type group and complete witnesses.
//! Supplied inputs are not authenticated resolution, liveness or signatures.
use super::{molecule, CheckedDirectCodeDependency, SuppliedDependencyCell};
use crate::fixed_policy_receipt::CheckedFixedPolicyReceipt;
use crate::{
    canonical_bytes, canonical_hash, ckb_blake2b256, hex_encode, CheckerBudgets, CheckerError, CheckerRejectionCode,
    EntryDispatchContract,
};
use serde::Serialize;
use std::collections::BTreeMap;

/// Untrusted input Cell snapshots, checked by exact raw input OutPoint.
#[derive(Debug)]
pub struct SuppliedInputCell<'a> {
    pub out_point: [u8; 36],
    pub output: &'a [u8],
    pub data: &'a [u8],
}
/// All actual inputs for this operation. No caller-provided group indices.
#[derive(Debug)]
pub struct SuppliedTypeGroupTransaction<'a> {
    pub full_transaction: &'a [u8],
    pub dependencies: &'a [SuppliedDependencyCell<'a>],
    pub inputs: &'a [SuppliedInputCell<'a>],
}
#[derive(Debug)]
pub struct CheckedDirectTypeGroup {
    dependency: CheckedDirectCodeDependency,
    record: Record,
    identity: String,
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    profile: &'static str,
    fixed_receipt: String,
    checked_dependency: String,
    full_transaction_hash: String,
    full_transaction_bytes: usize,
    selected_script_hash: String,
    supplied_inputs: Vec<InputFingerprint>,
    group_inputs: Vec<usize>,
    group_outputs: Vec<usize>,
    witness_source: &'static str,
    witness_index: usize,
    selected_tag: u32,
    policy_bundle_hash: String,
    policy_bundle_bytes: usize,
    consensus_claimed: bool,
    vm_execution_claimed: bool,
    authorization_claimed: bool,
    signatures_verified: bool,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
struct InputFingerprint {
    out_point: String,
    output_hash: String,
    output_bytes: usize,
    data_hash: String,
    data_bytes: usize,
}
impl CheckedDirectTypeGroup {
    pub fn dependency(&self) -> &CheckedDirectCodeDependency {
        &self.dependency
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        canonical_bytes(&self.record)
    }
    pub fn group_inputs(&self) -> &[usize] {
        &self.record.group_inputs
    }
    pub fn group_outputs(&self) -> &[usize] {
        &self.record.group_outputs
    }
    pub fn witness_index(&self) -> usize {
        self.record.witness_index
    }
    pub fn selected_tag(&self) -> u32 {
        self.record.selected_tag
    }
    /// Freeze all full transaction bytes, including opaque Lock/extra witnesses,
    /// and every supplied input/dependency snapshot. Signature validity and
    /// live resolution remain outside this proof; signing mutations need a new
    /// checked snapshot, never permission to reuse stale materialization bytes.
    pub fn check_unchanged_inputs(
        &self,
        supplied: &SuppliedTypeGroupTransaction<'_>,
        budgets: &CheckerBudgets,
    ) -> Result<(), CheckerError> {
        preflight(supplied, budgets)?;
        if supplied.full_transaction.len() != self.record.full_transaction_bytes
            || hash(supplied.full_transaction) != self.record.full_transaction_hash
            || fingerprints(supplied.inputs) != self.record.supplied_inputs
        {
            return Err(invalid("full transaction/witnesses or supplied input Cells changed; check the new complete snapshot"));
        }
        let parts = molecule::fields(supplied.full_transaction, 2)?;
        self.dependency.check_unchanged_inputs(parts[0], supplied.dependencies, budgets)
    }
}
fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("direct Type group: {message}"))
}
fn hash(bytes: &[u8]) -> String {
    hex_encode(&ckb_blake2b256(bytes))
}
fn preflight(supplied: &SuppliedTypeGroupTransaction<'_>, budgets: &CheckerBudgets) -> Result<(), CheckerError> {
    if supplied.inputs.len() > molecule::MAX_CELLS || supplied.dependencies.is_empty() || supplied.dependencies.len() > 64 {
        return Err(invalid("supplied counts exceed <=256 inputs and 1..=64 direct deps; use the bounded profile"));
    }
    let mut total = 0usize;
    let mut add = |size: usize, limit: u64| {
        total = total.checked_add(size).ok_or_else(|| invalid("input byte sum overflow"))?;
        if size > 4 * 1024 * 1024 || size as u64 > limit || total > 16 * 1024 * 1024 {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2400BudgetExceeded,
                "direct Type group per-file/caller or shared 16 MiB budget exceeded",
            ));
        }
        Ok(())
    };
    add(supplied.full_transaction.len(), budgets.record_bytes)?;
    for cell in supplied.dependencies {
        add(cell.out_point.len(), budgets.record_bytes)?;
        add(cell.output.len(), budgets.record_bytes)?;
        add(cell.data.len(), budgets.artifact_bytes)?;
    }
    for cell in supplied.inputs {
        add(cell.out_point.len(), budgets.record_bytes)?;
        add(cell.output.len(), budgets.record_bytes)?;
        add(cell.data.len(), budgets.artifact_bytes)?;
    }
    Ok(())
}
fn fingerprints(inputs: &[SuppliedInputCell<'_>]) -> Vec<InputFingerprint> {
    inputs
        .iter()
        .map(|cell| InputFingerprint {
            out_point: hex_encode(&cell.out_point),
            output_hash: hash(cell.output),
            output_bytes: cell.output.len(),
            data_hash: hash(cell.data),
            data_bytes: cell.data.len(),
        })
        .collect()
}
fn policy_request<'a>(bundle: &'a [u8], selected_hash: &str) -> Result<(u32, &'a [u8]), CheckerError> {
    if bundle.len() > 4076 {
        return Err(invalid("policy bundle exceeds 4076 bytes"));
    }
    let vector =
        bundle.strip_prefix(b"CSPOLv1\0").ok_or_else(|| invalid("selected input_type needs the canonical CSPOLv1 envelope"))?;
    let records = molecule::dynamic(vector, 8)?;
    if records.is_empty() {
        return Err(invalid("policy bundle must contain 1..=8 records"));
    }
    let mut previous: Option<(u8, &[u8])> = None;
    let mut selected = None;
    for record in records {
        let fields = molecule::fields(record, 4)?;
        if fields[0].len() != 1 || fields[0][0] > 1 || fields[1].len() != 32 || fields[2].len() != 4 {
            return Err(invalid("policy record role/hash/tag widths are invalid"));
        }
        let key = (fields[0][0], fields[1]);
        if previous.is_some_and(|old| old >= key) {
            return Err(invalid("duplicate or unordered policy role/full Script hash"));
        }
        previous = Some(key);
        let args = molecule::fixed(fields[3], 1, 4076)?;
        if !args.is_empty() && !args.starts_with(b"CSARGv1\0") {
            return Err(invalid("policy record args have invalid CSARGv1 magic"));
        }
        if key.0 == 1 && hex_encode(key.1) == selected_hash {
            selected = Some((u32::from_le_bytes(fields[2].try_into().expect("checked tag width")), args));
        }
    }
    selected.ok_or_else(|| invalid("selected Type/full Script hash is absent from the group witness"))
}
/// Consume the actual checked dependency and bind a private finite Type-policy
/// receipt with matching target identity to a complete canonical transaction
/// plus every input Cell snapshot. The dependency record stores target identity;
/// this new proof separately stores the actual fixed-receipt identity.
/// All groups are derived from complete Type Script hashes; no supplied index,
/// code-hash-only match, Lock occurrence or peer role can stand in for this group.
/// The initial direct data2 profile checks the selected CSPOL tag, cardinality
/// and fixed scalar args against the privately checked machine codec. Other
/// records are structurally checked, never granted peer execution/admission.
pub fn check_direct_type_group(
    receipt: &CheckedFixedPolicyReceipt,
    dependency: CheckedDirectCodeDependency,
    supplied: &SuppliedTypeGroupTransaction<'_>,
    budgets: &CheckerBudgets,
) -> Result<CheckedDirectTypeGroup, CheckerError> {
    preflight(supplied, budgets)?;
    if receipt.target_origin().identity() != dependency.target_identity() {
        return Err(invalid("dependency belongs to a different actual target receipt; bind the exact selected receipt"));
    }
    let entry = receipt.entry_contract();
    let EntryDispatchContract::PolicyWitnessV1(contract) = &entry.dispatch else {
        return Err(invalid("requires the checked Type-policy dispatch"));
    };
    if entry.script_role != "type"
        || entry.entry_payload_abi != crate::POLICY_PAYLOAD_ABI
        || entry.witness_placement_abi != crate::POLICY_PLACEMENT_ABI
        || entry.witness_placement_field != "input_type"
        || entry.witness_placement_source != crate::POLICY_WITNESS_SOURCE
    {
        return Err(invalid("unsupported selected role or witness placement; use the checked Type-policy profile"));
    }
    let parts = molecule::fields(supplied.full_transaction, 2)?;
    dependency.check_unchanged_inputs(parts[0], supplied.dependencies, budgets)?;
    let raw = molecule::transaction(parts[0])?;
    let witnesses = molecule::dynamic(parts[1], molecule::MAX_CELLS)?;
    // Every Bytes item, including opaque extra witnesses, must be canonical.
    let witnesses = witnesses.into_iter().map(|w| molecule::fixed(w, 1, 4 * 1024 * 1024)).collect::<Result<Vec<_>, _>>()?;
    if raw.inputs.len() / 44 != supplied.inputs.len() {
        return Err(invalid("supplied input Cells do not exactly cover raw input OutPoints"));
    }
    let selected_hash = receipt.target_origin().origin().selected_script_hash();
    let mut cells = BTreeMap::new();
    for cell in supplied.inputs {
        let output = molecule::cell(cell.output)?;
        if cells.insert(cell.out_point, output).is_some() {
            return Err(invalid("duplicate supplied input OutPoint"));
        }
    }
    let mut group_inputs = Vec::new();
    for (index, input) in raw.inputs.chunks_exact(44).enumerate() {
        let point: [u8; 36] = input[8..].try_into().expect("checked input width");
        let output = cells.get(&point).ok_or_else(|| invalid("missing referenced input Cell or extra supplied input"))?;
        if output.type_script.as_ref().is_some_and(|script| hash(script.bytes) == selected_hash) {
            group_inputs.push(index);
        }
    }
    let group_outputs = raw
        .cells
        .iter()
        .enumerate()
        .filter_map(|(index, output)| {
            output.type_script.as_ref().is_some_and(|script| hash(script.bytes) == selected_hash).then_some(index)
        })
        .collect::<Vec<_>>();
    let (witness_source, witness_index) = if let Some(&index) = group_inputs.first() {
        ("group-input[0]", index)
    } else if let Some(&index) = group_outputs.first() {
        ("group-output[0]", index)
    } else {
        return Err(invalid(
            "selected complete Type Script has no input or output group; Lock/code-hash-only matches cannot substitute",
        ));
    };
    let witness = *witnesses.get(witness_index).ok_or_else(|| invalid("first selected group witness is missing"))?;
    if witness.len() > contract.max_witness_bytes as usize || witness.len() > 4096 {
        return Err(invalid("selected WitnessArgs exceeds the checked policy limit"));
    }
    let witness_fields = molecule::fields(witness, 3)?;
    for field in &witness_fields {
        if !field.is_empty() {
            molecule::fixed(field, 1, 4096)?;
        }
    }
    if witness_fields[1].is_empty() {
        return Err(invalid("selected WitnessArgs.input_type is absent"));
    }
    let bundle = molecule::fixed(witness_fields[1], 1, 4076)?;
    let (tag, args) = policy_request(bundle, selected_hash)?;
    let variant = contract
        .variants
        .iter()
        .find(|variant| variant.tag == tag)
        .ok_or_else(|| invalid("selected tag is outside the checked artifact dispatch"))?;
    if group_inputs.len() != variant.input_count as usize || group_outputs.len() != variant.output_count as usize {
        return Err(invalid("selected Type group cardinality differs from the checked action; bind every group Cell"));
    }
    receipt.target_origin().origin().codec().parameters().check_payload(tag, args)?;
    let record = Record {
        schema: "cellscript-direct-type-group-v1",
        profile: "data2-direct-supplied-type-policy-group-v1",
        fixed_receipt: receipt.identity().into(),
        checked_dependency: dependency.identity().into(),
        full_transaction_hash: hash(supplied.full_transaction),
        full_transaction_bytes: supplied.full_transaction.len(),
        selected_script_hash: selected_hash.into(),
        supplied_inputs: fingerprints(supplied.inputs),
        group_inputs,
        group_outputs,
        witness_source,
        witness_index,
        selected_tag: tag,
        policy_bundle_hash: hash(bundle),
        policy_bundle_bytes: bundle.len(),
        consensus_claimed: false,
        vm_execution_claimed: false,
        authorization_claimed: false,
        signatures_verified: false,
    };
    let identity = canonical_hash("cellscript-direct-type-group-id-v1", &record)?;
    Ok(CheckedDirectTypeGroup { dependency, record, identity })
}
