//! Bind an actual byte-origin token to its independently checked target profile.
//! Profile compatibility does not prove chain activation or Type history.
use super::{invalid, CheckedCodeCellOrigin};
use crate::interface::InterfaceRuntimeContract;
use crate::CheckerError;
use serde::Serialize;

#[derive(Debug)]
pub struct CheckedTargetCodeCellOrigin {
    origin: CheckedCodeCellOrigin,
    record: Record,
    identity: String,
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    code_origin: String,
    runtime: InterfaceRuntimeContract,
    deployment_hash_type: &'static str,
}
impl CheckedTargetCodeCellOrigin {
    pub fn origin(&self) -> &CheckedCodeCellOrigin {
        &self.origin
    }
    pub fn runtime_contract(&self) -> &InterfaceRuntimeContract {
        &self.record.runtime
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        crate::canonical_bytes(&self.record)
    }
}
/// Consume actual private byte-origin/codec evidence, never exported receipts.
/// Its independently inspected bundle already enforces VM2/rv64imac_zbb and
/// the profile's singleton deployment hash type. No new bytes are parsed here.
/// A legal Script hash type still rejects if it selects another target profile.
/// Type-hash VM2 activation, authorized replacement history, live dependencies
/// and immutable policy roots require separate evidence.
pub fn check_code_cell_target(origin: CheckedCodeCellOrigin) -> Result<CheckedTargetCodeCellOrigin, CheckerError> {
    let runtime = origin.codec().parameters().module_projection().runtime_contract().clone();
    let deployment_hash_type = crate::checker::ckb_deployment_hash_type(&runtime.target_profile)
        .ok_or_else(|| invalid("unsupported independently checked deployment target"))?;
    let required = match deployment_hash_type {
        "data2" => 4,
        "type" => 1,
        _ => return Err(invalid("unsupported independently checked deployment hash type")),
    };
    if origin.selected_hash_type() != required {
        return Err(invalid("selected Script hash type differs from independently checked target profile"));
    }
    let record =
        Record { schema: "cellscript-code-cell-target-v1", code_origin: origin.identity().into(), runtime, deployment_hash_type };
    let identity = crate::canonical_hash("cellscript-code-cell-target-id-v1", &record)?;
    Ok(CheckedTargetCodeCellOrigin { origin, record, identity })
}
