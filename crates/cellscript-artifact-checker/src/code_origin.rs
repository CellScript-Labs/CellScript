//! Recompute a code Cell's creation identity from actual canonical transaction
//! bytes and an independently checked ELF bundle. This is a byte-origin binding,
//! not deployment/history admission, network observation or Cell liveness.
use crate::external_codec::{check_fixed_external_codec, CheckedFixedExternalCodec};
use crate::interface::ModuleBundle;
use crate::{CheckerBudgets, CheckerError, CheckerRejectionCode};
use serde::Serialize;
mod molecule;

#[derive(Debug)]
pub struct CheckedCodeCellOrigin {
    codec: CheckedFixedExternalCodec,
    record: Record,
    identity: String,
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    external_codec: String,
    transaction_hash: String,
    output_index: u32,
    artifact_hash: String,
    capacity: u64,
    creation_input_outpoints: Vec<String>,
    lock_script_hash: String,
    type_script_hash: Option<String>,
    selected_script_hash: String,
    selected_code_hash: String,
    selected_hash_type: u8,
    selected_args: String,
}
impl CheckedCodeCellOrigin {
    pub fn codec(&self) -> &CheckedFixedExternalCodec {
        &self.codec
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        crate::canonical_bytes(&self.record)
    }
    pub fn transaction_hash(&self) -> &str {
        &self.record.transaction_hash
    }
    pub fn output_index(&self) -> u32 {
        self.record.output_index
    }
    pub fn artifact_hash(&self) -> &str {
        &self.record.artifact_hash
    }
    pub fn type_script_hash(&self) -> Option<&str> {
        self.record.type_script_hash.as_deref()
    }
    pub fn selected_script_hash(&self) -> &str {
        &self.record.selected_script_hash
    }
    pub fn selected_hash_type(&self) -> u8 {
        self.record.selected_hash_type
    }
}
fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("code Cell origin: {}", message.into()))
}
/// All actual four-file bytes, raw deployment-transaction bytes and the complete
/// selected Script are preflighted under 4 MiB/file and a shared 16 MiB total
/// before any parser. The finite RawTransaction profile has <=256 inputs/outputs,
/// <=64 raw/header deps, <=4096-byte Scripts and version zero. Every unselected
/// output's complete Script/data structure is checked too.
///
/// Data modes bind the selected code hash to the exact ELF; Type mode binds the
/// selected code hash to the actual output Type Script. Type replacement history,
/// authenticated policy authorization and on-chain liveness are separate checks.
/// A synthetic canonical transaction cannot establish any of those claims.
pub fn check_code_cell_origin(
    bundle: ModuleBundle<'_>,
    raw_transaction: &[u8],
    output_index: u32,
    selected_script: &[u8],
    budgets: &CheckerBudgets,
) -> Result<CheckedCodeCellOrigin, CheckerError> {
    let mut total = 0usize;
    for (bytes, limit) in [
        (bundle[0], budgets.artifact_bytes),
        (bundle[1], budgets.record_bytes),
        (bundle[2], budgets.record_bytes),
        (bundle[3], budgets.source_map_bytes),
        (raw_transaction, budgets.record_bytes),
        (selected_script, budgets.record_bytes),
    ] {
        total = total.checked_add(bytes.len()).ok_or_else(|| invalid("input size overflow"))?;
        if bytes.len() as u64 > limit || bytes.len() > 4 * 1024 * 1024 || total > 16 * 1024 * 1024 {
            return Err(CheckerError::new(CheckerRejectionCode::V2400BudgetExceeded, "code Cell origin input budget exceeded"));
        }
    }
    let transaction = molecule::transaction(raw_transaction)?;
    let index = usize::try_from(output_index).map_err(|_| invalid("code output index overflow"))?;
    let output = transaction.cells.get(index).ok_or_else(|| invalid("code output index is absent"))?;
    let data = transaction.data.get(index).ok_or_else(|| invalid("code output data is absent"))?;
    if *data != bundle[0] {
        return Err(invalid("selected code output bytes differ from actual ELF"));
    }
    let selected = molecule::script(selected_script)?;
    let artifact = crate::ckb_blake2b256(bundle[0]);
    let type_hash = output.type_script.as_ref().map(|script| crate::ckb_blake2b256(script.bytes));
    let expected = match selected.hash_type {
        0 | 2 | 4 => artifact,
        1 => type_hash.ok_or_else(|| invalid("Type-hash selection has no actual code Cell Type Script"))?,
        _ => return Err(invalid("unsupported code hash type")),
    };
    if selected.code_hash != expected {
        return Err(invalid("selected code identity differs from actual code output"));
    }
    let codec = check_fixed_external_codec(bundle, budgets)?;
    let record = Record {
        schema: "cellscript-code-cell-origin-v1",
        external_codec: codec.identity().into(),
        transaction_hash: crate::hex_encode(&crate::ckb_blake2b256(raw_transaction)),
        output_index,
        artifact_hash: crate::hex_encode(&artifact),
        capacity: output.capacity,
        creation_input_outpoints: transaction.inputs.chunks_exact(44).map(|input| crate::hex_encode(&input[8..])).collect(),
        lock_script_hash: crate::hex_encode(&crate::ckb_blake2b256(output.lock.bytes)),
        type_script_hash: type_hash.map(|hash| crate::hex_encode(&hash)),
        selected_script_hash: crate::hex_encode(&crate::ckb_blake2b256(selected.bytes)),
        selected_code_hash: crate::hex_encode(selected.code_hash),
        selected_hash_type: selected.hash_type,
        selected_args: crate::hex_encode(selected.args),
    };
    let identity = crate::canonical_hash("cellscript-code-cell-origin-id-v1", &record)?;
    Ok(CheckedCodeCellOrigin { codec, record, identity })
}
