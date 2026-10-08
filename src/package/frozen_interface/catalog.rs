//! Combine owned native source snapshots with all-bundle API evidence.
//! Stable package families, complete codecs and deployment admission remain
//! distinct prerequisites; source snapshots must not masquerade as those.
use super::{checker_error, invalid, FrozenPackageModule};
use crate::error::Result;
use cellscript_artifact_checker::{canonical_bytes, canonical_hash, interface, CheckerBudgets};
use serde::Serialize;

#[derive(Debug)]
pub struct FrozenModuleCatalog {
    required: FrozenPackageModule,
    candidates: Vec<FrozenPackageModule>,
    evidence: interface::CheckedModuleCatalog,
    record: Record,
    identity: String,
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    checked_catalog: String,
    required_source_context: String,
    candidate_source_contexts: Vec<String>,
}
impl FrozenModuleCatalog {
    pub fn required(&self) -> &FrozenPackageModule {
        &self.required
    }
    pub fn candidates(&self) -> &[FrozenPackageModule] {
        &self.candidates
    }
    pub fn evidence(&self) -> &interface::CheckedModuleCatalog {
        &self.evidence
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record).map_err(checker_error)
    }
}

/// Consume actual native snapshots, never exported JSON or caller labels.
/// The selected source environments must agree. This is host context, not a
/// fictional on-chain genesis check. All actual four-file tuples are checked
/// under the shared 16 MiB ceiling before fresh all-bundle inspection. Native
/// compilation and its per-module source preflight have already occurred.
pub fn freeze_module_catalog(
    required: FrozenPackageModule,
    candidates: Vec<FrozenPackageModule>,
    budgets: &CheckerBudgets,
) -> Result<FrozenModuleCatalog> {
    if candidates.is_empty() || candidates.len() > cellscript_artifact_checker::open_handle_policy::MAX_MEMBERS {
        return Err(invalid("frozen module catalog requires 1..=32 actual source snapshots"));
    }
    for candidate in &candidates {
        if candidate.context.chain_id != required.context.chain_id
            || candidate.context.network_genesis != required.context.network_genesis
        {
            return Err(invalid("frozen module catalog has conflicting pinned chain identities"));
        }
    }
    let bundles = candidates.iter().map(FrozenPackageModule::bundle).collect::<Vec<_>>();
    let evidence = interface::check_module_catalog(required.bundle(), &bundles, budgets).map_err(checker_error)?;
    let record = Record {
        schema: "cellscript-frozen-module-catalog-v1",
        checked_catalog: evidence.identity().into(),
        required_source_context: required.context_identity().into(),
        candidate_source_contexts: candidates.iter().map(|candidate| candidate.context_identity().into()).collect(),
    };
    let identity = canonical_hash("cellscript-frozen-module-catalog-id-v1", &record).map_err(checker_error)?;
    Ok(FrozenModuleCatalog { required, candidates, evidence, record, identity })
}
