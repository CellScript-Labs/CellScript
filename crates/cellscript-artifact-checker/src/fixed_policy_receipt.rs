//! Independently checked receipts for the finite public Type-policy profile.
//! No network, version/status policy, Type history or authorization is inferred.
use crate::code_origin::{check_code_cell_origin, check_code_cell_target, CheckedTargetCodeCellOrigin};
use crate::interface::{inspect_bundle, ModuleBundle, PackageInterface};
use crate::{ArtifactEntryContract, CheckerBudgets, CheckerError, CheckerRejectionCode, CheckerReport};
use serde::Serialize;

#[derive(Debug)]
pub struct CheckedFixedPolicyReceipt {
    target: CheckedTargetCodeCellOrigin,
    record: Record,
    identity: String,
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    profile: &'static str,
    declared_interface: PackageInterface,
    entry_contract: ArtifactEntryContract,
    module_contract: String,
    external_codec: String,
    code_origin: String,
    target_selection: String,
    bundle_byte_hashes: [String; 4],
    bundle_byte_lengths: [usize; 4],
    artifact_report: CheckerReport,
}
impl CheckedFixedPolicyReceipt {
    pub fn target_origin(&self) -> &CheckedTargetCodeCellOrigin {
        &self.target
    }
    pub fn declared_interface(&self) -> &PackageInterface {
        &self.record.declared_interface
    }
    pub fn entry_contract(&self) -> &ArtifactEntryContract {
        &self.record.entry_contract
    }
    pub fn artifact_report(&self) -> &CheckerReport {
        &self.record.artifact_report
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        crate::canonical_bytes(&self.record)
    }
    /// Directional matching of these privately checked finite API/codec proofs.
    /// Different exact deployment/receipt identities may satisfy one baseline.
    /// This does not authorize either candidate or assert behavioral equivalence.
    pub fn check_required_contracts(&self, candidate: &Self) -> Result<(), CheckerError> {
        self.target
            .origin()
            .codec()
            .parameters()
            .module_projection()
            .check_required_contracts(candidate.target.origin().codec().parameters().module_projection())
    }
    /// Compare actual immutable inputs, not a supplied receipt record. This is
    /// byte substitution detection, not a new check under new semantic budgets.
    /// Apply fixed input ceilings before hashing; no additional parser is used.
    pub fn check_unchanged_inputs(
        &self,
        bundle: ModuleBundle<'_>,
        raw_transaction: &[u8],
        output_index: u32,
        selected_script: &[u8],
    ) -> Result<(), CheckerError> {
        let mut total = 0usize;
        for bytes in bundle.into_iter().chain([raw_transaction, selected_script]) {
            total = total.checked_add(bytes.len()).ok_or_else(|| invalid("input length overflow"))?;
            if bytes.len() > 4 * 1024 * 1024 || total > 16 * 1024 * 1024 {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2400BudgetExceeded,
                    "fixed policy receipt input budget exceeded",
                ));
            }
        }
        if byte_hashes(bundle) != self.record.bundle_byte_hashes
            || bundle.map(<[u8]>::len) != self.record.bundle_byte_lengths
            || crate::hex_encode(&crate::ckb_blake2b256(raw_transaction)) != self.target.origin().transaction_hash()
            || output_index != self.target.origin().output_index()
            || crate::hex_encode(&crate::ckb_blake2b256(selected_script)) != self.target.origin().selected_script_hash()
        {
            return Err(invalid("actual bundle, raw transaction, code index or complete Script changed; check the new inputs"));
        }
        Ok(())
    }
}
fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("fixed policy receipt: {message}"))
}
fn byte_hashes(bundle: ModuleBundle<'_>) -> [String; 4] {
    bundle.map(|bytes| crate::hex_encode(&crate::ckb_blake2b256(bytes)))
}
/// The existing six-input origin preflight enforces 4 MiB/file, shared 16 MiB
/// and caller byte budgets before any parsing. The receipt only certifies the
/// finite unit-result/scalar/flat-unsigned-Cell public Type-policy profile.
/// Generic declarations remain explicitly non-executable; public constants
/// reject because their values are not independently proven by this profile.
/// Historical exact handles and broader bundle inspection remain unchanged.
pub fn check_fixed_policy_receipt(
    bundle: ModuleBundle<'_>,
    raw_transaction: &[u8],
    output_index: u32,
    selected_script: &[u8],
    budgets: &CheckerBudgets,
) -> Result<CheckedFixedPolicyReceipt, CheckerError> {
    let target = check_code_cell_target(check_code_cell_origin(bundle, raw_transaction, output_index, selected_script, budgets)?)?;
    let inspection = inspect_bundle(bundle[0], bundle[1], bundle[2], bundle[3], budgets)?;
    if !inspection.declared().constants.is_empty() {
        return Err(invalid("public constant values lack this finite receipt profile"));
    }
    let entry_contract = &inspection.effective().foundation.entry_contract;
    if entry_contract.script_role != "type" {
        return Err(invalid("requires the independently checked Type-policy role; Lock/verifier receipts need another profile"));
    }
    let codec = target.origin().codec();
    let record = Record {
        schema: "cellscript-fixed-policy-interface-receipt-v1",
        profile: "policy-unit-scalars-nested-unsigned-cell-v1",
        declared_interface: inspection.declared().clone(),
        entry_contract: entry_contract.clone(),
        module_contract: codec.parameters().module_projection().identity().into(),
        external_codec: codec.identity().into(),
        code_origin: target.origin().identity().into(),
        target_selection: target.identity().into(),
        bundle_byte_hashes: byte_hashes(bundle),
        bundle_byte_lengths: bundle.map(<[u8]>::len),
        artifact_report: inspection.report().clone(),
    };
    let identity = crate::canonical_hash("cellscript-fixed-policy-interface-receipt-id-v1", &record)?;
    Ok(CheckedFixedPolicyReceipt { target, record, identity })
}
