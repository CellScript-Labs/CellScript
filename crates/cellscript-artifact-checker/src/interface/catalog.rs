//! All-bundle API coherence evidence, distinct from policy/deployment admission.
use super::{project_bundle, CheckedModuleProjection};
use crate::{canonical_bytes, canonical_hash, CheckerBudgets, CheckerError, CheckerRejectionCode};
use serde::Serialize;

pub type ModuleBundle<'a> = [&'a [u8]; 4];
const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_ALL_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub struct CheckedModuleCatalog {
    required: CheckedModuleProjection,
    candidates: Vec<CheckedModuleProjection>,
    record: Record,
    identity: String,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Fingerprint {
    module_contract: String,
    artifact: String,
    metadata: String,
    lowering: String,
    source_map: String,
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    required: Fingerprint,
    candidates: Vec<Fingerprint>,
}
impl CheckedModuleCatalog {
    pub fn required(&self) -> &CheckedModuleProjection {
        &self.required
    }
    pub fn candidates(&self) -> &[CheckedModuleProjection] {
        &self.candidates
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        canonical_bytes(&self.record)
    }
    /// Freeze every exact four-file tuple, including unselected candidates.
    /// This is not transaction/receipt authorization or policy membership.
    pub fn check_unchanged_inputs(&self, required: ModuleBundle<'_>, candidates: &[ModuleBundle<'_>]) -> Result<(), CheckerError> {
        preflight(&required, candidates)?;
        if candidates.len() != self.candidates.len() || fingerprint(required, self.required.identity()) != self.record.required {
            return Err(invalid("required bundle or candidate count changed"));
        }
        for (index, bundle) in candidates.iter().enumerate() {
            if fingerprint(*bundle, self.candidates[index].identity()) != self.record.candidates[index] {
                return Err(invalid("a catalog candidate was substituted or reordered"));
            }
        }
        Ok(())
    }
}
fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, format!("checked module catalog: {message}"))
}
fn preflight(required: &ModuleBundle<'_>, candidates: &[ModuleBundle<'_>]) -> Result<(), CheckerError> {
    if candidates.is_empty() || candidates.len() > crate::open_handle_policy::MAX_MEMBERS {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2400BudgetExceeded,
            "checked module catalog requires 1..=32 actual candidates",
        ));
    }
    let mut total = 0usize;
    for bundle in std::iter::once(required).chain(candidates.iter()) {
        for bytes in bundle {
            total = total.checked_add(bytes.len()).ok_or_else(|| invalid("aggregate input size overflow"))?;
            if bytes.len() > MAX_FILE_BYTES || total > MAX_ALL_BYTES {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2400BudgetExceeded,
                    "checked module catalog exceeds 4 MiB/file or 16 MiB/all bundles",
                ));
            }
        }
    }
    Ok(())
}
fn fingerprint(bundle: ModuleBundle<'_>, module_contract: &str) -> Fingerprint {
    let hash = |bytes| crate::hex_encode(&crate::ckb_blake2b256(bytes));
    Fingerprint {
        module_contract: module_contract.into(),
        artifact: hash(bundle[0]),
        metadata: hash(bundle[1]),
        lowering: hash(bundle[2]),
        source_map: hash(bundle[3]),
    }
}
/// Preflight the required bundle and every candidate together before parsing
/// any input. Check all actual bundles; a valid selected candidate never excuses
/// an invalid unselected candidate. No partial checked value escapes on error.
/// This evidence covers module API coherence only, not complete codecs,
/// package ownership, status/history/deployment receipts or open admission.
pub fn check_module_catalog(
    required: ModuleBundle<'_>,
    candidates: &[ModuleBundle<'_>],
    budgets: &CheckerBudgets,
) -> Result<CheckedModuleCatalog, CheckerError> {
    preflight(&required, candidates)?;
    let inspect = |bundle: ModuleBundle<'_>| project_bundle(bundle[0], bundle[1], bundle[2], bundle[3], budgets);
    let required_projection = inspect(required)?;
    let mut candidate_projections = Vec::with_capacity(candidates.len());
    let mut fingerprints = Vec::with_capacity(candidates.len());
    for bundle in candidates {
        let candidate = inspect(*bundle)?;
        required_projection.check_required_contracts(&candidate)?;
        fingerprints.push(fingerprint(*bundle, candidate.identity()));
        candidate_projections.push(candidate);
    }
    let record = Record {
        schema: "cellscript-checked-module-catalog-v1",
        required: fingerprint(required, required_projection.identity()),
        candidates: fingerprints,
    };
    let identity = canonical_hash("cellscript-checked-module-catalog-id-v1", &record)?;
    Ok(CheckedModuleCatalog { required: required_projection, candidates: candidate_projections, record, identity })
}
