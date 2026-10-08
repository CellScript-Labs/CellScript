//! Bind direct data2 code dependencies to supplied canonical Cell snapshots.
//! Supplied snapshots are not authenticated node/consensus or VM observations.
use super::{molecule, CheckedTargetCodeCellOrigin};
use crate::{canonical_bytes, canonical_hash, ckb_blake2b256, hex_encode, CheckerBudgets, CheckerError, CheckerRejectionCode};
use serde::Serialize;
use std::collections::BTreeSet;

/// Untrusted host inputs. Only the checked factory can produce dependency proof.
#[derive(Debug)]
pub struct SuppliedDependencyCell<'a> {
    pub out_point: [u8; 36],
    pub output: &'a [u8],
    pub data: &'a [u8],
}
#[derive(Debug)]
pub struct CheckedDirectCodeDependency {
    record: Record,
    identity: String,
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    profile: &'static str,
    target_origin: String,
    transaction_hash: String,
    raw_transaction_bytes: usize,
    raw_dependency_index: usize,
    supplied_cell_index: usize,
    supplied_cells: Vec<CellFingerprint>,
    consensus_claimed: bool,
    vm_execution_claimed: bool,
    authorization_claimed: bool,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
struct CellFingerprint {
    out_point: String,
    output_hash: String,
    output_bytes: usize,
    data_hash: String,
    data_bytes: usize,
}
impl CheckedDirectCodeDependency {
    pub(super) fn target_identity(&self) -> &str {
        &self.record.target_origin
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        canonical_bytes(&self.record)
    }
    pub fn transaction_hash(&self) -> &str {
        &self.record.transaction_hash
    }
    pub fn raw_dependency_index(&self) -> usize {
        self.record.raw_dependency_index
    }
    /// Position in the supplied snapshot list, not a proven VM/syscall index.
    pub fn supplied_cell_index(&self) -> usize {
        self.record.supplied_cell_index
    }
    /// Recheck exactly all original final raw/snapshot inputs, including order.
    /// Bounds precede hashing. This does not freeze witnesses or signatures.
    pub fn check_unchanged_inputs(
        &self,
        raw: &[u8],
        cells: &[SuppliedDependencyCell<'_>],
        budgets: &CheckerBudgets,
    ) -> Result<(), CheckerError> {
        preflight(raw, cells, budgets)?;
        if raw.len() != self.record.raw_transaction_bytes
            || hex_encode(&ckb_blake2b256(raw)) != self.record.transaction_hash
            || fingerprints(cells) != self.record.supplied_cells
        {
            return Err(invalid("final raw transaction or supplied Cell snapshots changed; check and bind the new inputs"));
        }
        Ok(())
    }
}
fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("direct code dependency: {message}"))
}
fn preflight(raw: &[u8], cells: &[SuppliedDependencyCell<'_>], budgets: &CheckerBudgets) -> Result<(), CheckerError> {
    if cells.is_empty() || cells.len() > 64 {
        return Err(invalid("supplied Cell count must be 1..=64; use a bounded direct dependency set"));
    }
    let mut total = 0usize;
    let mut add = |bytes: usize, limit: u64| {
        total = total.checked_add(bytes).ok_or_else(|| invalid("input size overflow"))?;
        if bytes > 4 * 1024 * 1024 || bytes as u64 > limit || total > 16 * 1024 * 1024 {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2400BudgetExceeded,
                "direct code dependency shared 16 MiB or per-file/caller byte budget exceeded",
            ));
        }
        Ok(())
    };
    add(raw.len(), budgets.record_bytes)?;
    for cell in cells {
        add(cell.out_point.len(), budgets.record_bytes)?;
        add(cell.output.len(), budgets.record_bytes)?;
        add(cell.data.len(), budgets.artifact_bytes)?;
    }
    Ok(())
}
fn fingerprints(cells: &[SuppliedDependencyCell<'_>]) -> Vec<CellFingerprint> {
    cells
        .iter()
        .map(|cell| CellFingerprint {
            out_point: hex_encode(&cell.out_point),
            output_hash: hex_encode(&ckb_blake2b256(cell.output)),
            output_bytes: cell.output.len(),
            data_hash: hex_encode(&ckb_blake2b256(cell.data)),
            data_bytes: cell.data.len(),
        })
        .collect()
}
/// Check an actual private target/byte-origin token and every canonical final
/// raw/snapshot input. This initial profile permits only direct code CellDeps
/// and data2, rejects dep groups/Type history, and requires exact set coverage
/// plus a unique selected data hash. Supplied list order is explicitly separate
/// from raw order; no VM resolved-index or live-chain claim is inferred.
/// Each input is <=4 MiB, all inputs <=16 MiB, <=64 supplied/raw deps before
/// parsing. Existing bounded RawTransaction/Script validation also applies.
pub fn check_direct_code_dependency(
    target: &CheckedTargetCodeCellOrigin,
    raw: &[u8],
    cells: &[SuppliedDependencyCell<'_>],
    budgets: &CheckerBudgets,
) -> Result<CheckedDirectCodeDependency, CheckerError> {
    preflight(raw, cells, budgets)?;
    let origin = target.origin();
    if origin.selected_hash_type() != 4 || target.runtime_contract().target_profile != "ckb" {
        return Err(invalid("only checked data2 targets belong to this direct profile; Type history needs separate evidence"));
    }
    let tx = molecule::transaction(raw)?;
    if tx.deps.chunks_exact(37).any(|dep| dep[36] != 0) {
        return Err(invalid("dep groups are outside this direct profile; provide checked expansion evidence separately"));
    }
    if tx.deps.len() / 37 != cells.len() {
        return Err(invalid("supplied Cells do not cover exactly every raw direct dependency; supply each exact raw OutPoint"));
    }
    let mut outpoints = BTreeSet::new();
    let mut selected = None;
    for (index, cell) in cells.iter().enumerate() {
        if !outpoints.insert(cell.out_point) || !tx.deps.chunks_exact(37).any(|dep| dep[..36] == cell.out_point) {
            return Err(invalid("duplicate or unreferenced supplied Cell; bind each raw OutPoint exactly once"));
        }
        let output = molecule::cell(cell.output)?;
        if hex_encode(&ckb_blake2b256(cell.data)) == origin.artifact_hash() {
            if selected.is_some() {
                return Err(invalid(
                    "selected data hash is not unique across all supplied dependencies; keep exactly one selected code Cell",
                ));
            }
            let tx_hash = hex_encode(&cell.out_point[..32]);
            let output_index = u32::from_le_bytes(cell.out_point[32..].try_into().expect("fixed OutPoint index"));
            if tx_hash != origin.transaction_hash() || output_index != origin.output_index() {
                return Err(invalid("matching code bytes came from a different OutPoint than the checked deployment; use the checked creation OutPoint"));
            }
            if output.capacity != origin.record.capacity
                || hex_encode(&ckb_blake2b256(output.lock.bytes)) != origin.record.lock_script_hash
                || output.type_script.as_ref().map(|script| hex_encode(&ckb_blake2b256(script.bytes)))
                    != origin.record.type_script_hash
            {
                return Err(invalid(
                    "selected supplied CellOutput differs from its checked creation output; supply the original created output",
                ));
            }
            selected = Some(index);
        }
    }
    let supplied_cell_index = selected.ok_or_else(|| {
        invalid("selected checked code bytes are absent from supplied dependencies; add the actual selected code Cell")
    })?;
    let raw_dependency_index = tx
        .deps
        .chunks_exact(37)
        .position(|dep| dep[..36] == cells[supplied_cell_index].out_point)
        .ok_or_else(|| invalid("selected deployment has no raw direct code dependency; add its exact direct CellDep"))?;
    let record = Record {
        schema: "cellscript-direct-code-dependency-v1",
        profile: "data2-direct-host-supplied-cells-v1",
        target_origin: target.identity().into(),
        transaction_hash: hex_encode(&ckb_blake2b256(raw)),
        raw_transaction_bytes: raw.len(),
        raw_dependency_index,
        supplied_cell_index,
        supplied_cells: fingerprints(cells),
        consensus_claimed: false,
        vm_execution_claimed: false,
        authorization_claimed: false,
    };
    let identity = canonical_hash("cellscript-direct-code-dependency-id-v1", &record)?;
    Ok(CheckedDirectCodeDependency { record, identity })
}
