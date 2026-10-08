//! Bind all finite external codec/deployment byte origins to actual native
//! frozen source snapshots. No immutable authorization, live deployment or
//! resolver-owned source handle is introduced by this host prerequisite.
use super::source_receipt::{bind_source_receipt, CheckedSourceCodeReceipt};
use super::{checker_error, invalid, FrozenPackageModule};
use crate::error::Result;
use cellscript_artifact_checker::code_origin::{CheckedCodeCellOrigin, CheckedTargetCodeCellOrigin};
use cellscript_artifact_checker::external_codec::{check_fixed_external_codec, CheckedFixedExternalCodec};
use cellscript_artifact_checker::fixed_policy_receipt::{check_fixed_policy_receipt, CheckedFixedPolicyReceipt};
use cellscript_artifact_checker::{canonical_bytes, canonical_hash, interface, CheckerBudgets};
use serde::Serialize;
use std::collections::BTreeSet;

/// Source snapshots are already native-compiled. Deployment input remains raw
/// bytes, never supplied receipt/proof records or compatibility booleans.
pub struct CodeCandidateInput {
    pub module: FrozenPackageModule,
    pub raw_transaction: Vec<u8>,
    pub output_index: u32,
    pub selected_script: Vec<u8>,
}
#[derive(Debug)]
pub struct FrozenCodeCandidate {
    module: FrozenPackageModule,
    receipt: CheckedFixedPolicyReceipt,
    source_receipt: CheckedSourceCodeReceipt,
    raw_transaction: Vec<u8>,
    selected_script: Vec<u8>,
}
impl FrozenCodeCandidate {
    pub fn module(&self) -> &FrozenPackageModule {
        &self.module
    }
    pub fn origin(&self) -> &CheckedCodeCellOrigin {
        self.receipt.target_origin().origin()
    }
    pub fn target_origin(&self) -> &CheckedTargetCodeCellOrigin {
        self.receipt.target_origin()
    }
    pub fn receipt(&self) -> &CheckedFixedPolicyReceipt {
        &self.receipt
    }
    pub fn source_receipt(&self) -> &CheckedSourceCodeReceipt {
        &self.source_receipt
    }
    pub fn raw_transaction(&self) -> &[u8] {
        &self.raw_transaction
    }
    pub fn selected_script(&self) -> &[u8] {
        &self.selected_script
    }
}
#[derive(Debug)]
pub struct FrozenCodeCatalog {
    required: FrozenPackageModule,
    required_codec: CheckedFixedExternalCodec,
    candidates: Vec<FrozenCodeCandidate>,
    modules: interface::CheckedModuleCatalog,
    record: Record,
    identity: String,
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    checked_modules: String,
    required_source_context: String,
    required_external_codec: String,
    candidate_source_contexts: Vec<String>,
    candidate_code_origins: Vec<String>,
    candidate_target_origins: Vec<String>,
    candidate_receipts: Vec<String>,
    candidate_source_receipts: Vec<String>,
}
impl FrozenCodeCatalog {
    pub fn required(&self) -> &FrozenPackageModule {
        &self.required
    }
    pub fn required_codec(&self) -> &CheckedFixedExternalCodec {
        &self.required_codec
    }
    pub fn candidates(&self) -> &[FrozenCodeCandidate] {
        &self.candidates
    }
    pub fn module_evidence(&self) -> &interface::CheckedModuleCatalog {
        &self.modules
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record).map_err(checker_error)
    }
}
/// All required/candidate four-file bundles and all raw transaction/Script
/// bytes share 16 MiB before any fresh catalog/codec/deployment parser. This
/// does not preflight the earlier independent source compilation retroactively.
/// All 1..=32 candidates must match the pinned chain identity and required
/// directional module contract, satisfy the finite external profile and bind
/// a complete finite checked receipt with exact code output/target selection.
/// Duplicated concrete deployments reject;
/// identical code/OutPoint with different exact Script args remains distinct.
/// A failure anywhere returns no partially checked catalog.
pub fn freeze_code_catalog(
    required: FrozenPackageModule,
    inputs: Vec<CodeCandidateInput>,
    budgets: &CheckerBudgets,
) -> Result<FrozenCodeCatalog> {
    if inputs.is_empty() || inputs.len() > cellscript_artifact_checker::open_handle_policy::MAX_MEMBERS {
        return Err(invalid("frozen code catalog requires 1..=32 actual candidates"));
    }
    let mut total = 0usize;
    let mut preflight = |bytes: &[u8], limit: u64| -> Result<()> {
        total = total.checked_add(bytes.len()).ok_or_else(|| invalid("frozen code catalog byte overflow"))?;
        if bytes.len() as u64 > limit || bytes.len() > 4 * 1024 * 1024 || total > 16 * 1024 * 1024 {
            return Err(invalid("frozen code catalog exceeds shared 16 MiB or per-file/caller byte budgets"));
        }
        Ok(())
    };
    for module in std::iter::once(&required).chain(inputs.iter().map(|input| &input.module)) {
        for (bytes, limit) in module.bundle().into_iter().zip([
            budgets.artifact_bytes,
            budgets.record_bytes,
            budgets.record_bytes,
            budgets.source_map_bytes,
        ]) {
            preflight(bytes, limit)?;
        }
    }
    for input in &inputs {
        preflight(&input.raw_transaction, budgets.record_bytes)?;
        preflight(&input.selected_script, budgets.record_bytes)?;
        if input.module.context.chain_id != required.context.chain_id
            || input.module.context.network_genesis != required.context.network_genesis
        {
            return Err(invalid("frozen code catalog has conflicting pinned chain identities"));
        }
    }
    let bundles = inputs.iter().map(|input| input.module.bundle()).collect::<Vec<_>>();
    let modules = interface::check_module_catalog(required.bundle(), &bundles, budgets).map_err(checker_error)?;
    let required_codec = check_fixed_external_codec(required.bundle(), budgets).map_err(checker_error)?;
    let mut candidates = Vec::new();
    let mut deployments = BTreeSet::new();
    for input in inputs {
        let receipt = check_fixed_policy_receipt(
            input.module.bundle(),
            &input.raw_transaction,
            input.output_index,
            &input.selected_script,
            budgets,
        )
        .map_err(checker_error)?;
        let origin = receipt.target_origin().origin();
        if !deployments.insert((origin.transaction_hash().to_owned(), origin.output_index(), origin.selected_script_hash().to_owned()))
        {
            return Err(invalid("frozen code catalog has a duplicate concrete Script/code deployment"));
        }
        let source_receipt = bind_source_receipt(&input.module, &receipt)?;
        candidates.push(FrozenCodeCandidate {
            module: input.module,
            receipt,
            source_receipt,
            raw_transaction: input.raw_transaction,
            selected_script: input.selected_script,
        });
    }
    let record = Record {
        schema: "cellscript-frozen-code-catalog-v4",
        checked_modules: modules.identity().into(),
        required_source_context: required.context_identity().into(),
        required_external_codec: required_codec.identity().into(),
        candidate_source_contexts: candidates.iter().map(|candidate| candidate.module.context_identity().into()).collect(),
        candidate_code_origins: candidates.iter().map(|candidate| candidate.origin().identity().into()).collect(),
        candidate_target_origins: candidates.iter().map(|candidate| candidate.target_origin().identity().into()).collect(),
        candidate_receipts: candidates.iter().map(|candidate| candidate.receipt().identity().into()).collect(),
        candidate_source_receipts: candidates.iter().map(|candidate| candidate.source_receipt().identity().into()).collect(),
    };
    let identity = canonical_hash("cellscript-frozen-code-catalog-id-v4", &record).map_err(checker_error)?;
    Ok(FrozenCodeCatalog { required, required_codec, candidates, modules, record, identity })
}
