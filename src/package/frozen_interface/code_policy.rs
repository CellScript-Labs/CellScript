//! Bind a declared finite policy snapshot to every actual private code receipt.
//! This is host binding evidence, not immutable root authorization or admission.
use super::source_receipt::{source_package_version, SourcePackageVersion};
use super::{checker_error, invalid, FrozenCodeCandidate, ResolvedSourceCatalog};
use crate::error::Result;
use cellscript_artifact_checker::open_handle_policy::{
    verify_selection, AuthorizationSet, CodeHashType, HandleClass, Hash, MemberStatus, PolicyMembership, ScriptRole, SelectionMode,
};
use cellscript_artifact_checker::{canonical_bytes, canonical_hash, hex_encode};
use serde::Serialize;
mod dependency;
pub use dependency::FrozenSourceCodeDependency;
mod type_group;
pub use type_group::FrozenSourceCodeTypeGroup;

#[derive(Debug)]
pub struct FrozenCodePolicy {
    sources: ResolvedSourceCatalog,
    policy: AuthorizationSet,
    candidate_indices: Vec<usize>,
    identity: String,
    source_version_floor: Option<SourcePackageVersion>,
}
#[derive(Serialize)]
struct Record<'a> {
    schema: &'static str,
    source_catalog: &'a str,
    policy_root: String,
    candidate_indices: &'a [usize],
    #[serde(skip_serializing_if = "Option::is_none")]
    source_version_floor: Option<&'a SourcePackageVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_receipts: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_rule: Option<&'static str>,
}
/// A host selection under this declared snapshot, never an authorized root or
/// source handle. The actual candidate and complete membership remain private.
#[derive(Debug)]
pub struct FrozenCodePolicySelection<'a> {
    policy: &'a FrozenCodePolicy,
    membership: PolicyMembership,
    candidate_index: usize,
}
impl FrozenCodePolicySelection<'_> {
    pub fn membership(&self) -> &PolicyMembership {
        &self.membership
    }
    pub fn candidate(&self) -> &FrozenCodeCandidate {
        &self.policy.sources.catalog().candidates()[self.candidate_index]
    }
    /// Recheck source ownership and exact receipt inputs before using a later
    /// materialization. Source/input byte ceilings remain separate operations.
    /// No final transaction/dependency, consensus or root authority is inferred.
    pub fn check_unchanged_inputs(
        &self,
        bundle: [&[u8]; 4],
        raw_transaction: &[u8],
        output_index: u32,
        selected_script: &[u8],
    ) -> Result<()> {
        self.candidate()
            .receipt()
            .check_unchanged_inputs(bundle, raw_transaction, output_index, selected_script)
            .map_err(checker_error)?;
        self.policy.sources.check_unchanged_sources()
    }
}
impl FrozenCodePolicy {
    pub fn sources(&self) -> &ResolvedSourceCatalog {
        &self.sources
    }
    pub fn policy(&self) -> &AuthorizationSet {
        &self.policy
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record()).map_err(checker_error)
    }
    /// Verify under this bound snapshot's root, then recheck actual sources.
    /// The caller must separately authorize this root from committed code/args
    /// or controlled state. A witness cannot authorize its own root.
    pub fn check_selection(&self, bytes: &[u8]) -> Result<FrozenCodePolicySelection<'_>> {
        let membership = verify_selection(&self.policy.root(), bytes).map_err(|error| invalid(error.to_string()))?;
        let candidate_index = *self
            .candidate_indices
            .get(membership.index() as usize)
            .ok_or_else(|| invalid("code policy selection is outside checked candidate bindings"))?;
        self.sources.check_unchanged_sources()?;
        Ok(FrozenCodePolicySelection { policy: self, membership, candidate_index })
    }
    fn record(&self) -> Record<'_> {
        Record {
            schema: if self.source_version_floor.is_some() {
                "cellscript-frozen-code-policy-bindings-v2"
            } else {
                "cellscript-frozen-code-policy-bindings-v1"
            },
            source_catalog: self.sources.identity(),
            policy_root: hex_encode(&self.policy.root()),
            candidate_indices: &self.candidate_indices,
            source_version_floor: self.source_version_floor.as_ref(),
            source_receipts: self
                .source_version_floor
                .as_ref()
                .map(|_| self.sources.catalog().candidates().iter().map(|candidate| candidate.source_receipt().identity()).collect()),
            version_rule: self.source_version_floor.as_ref().map(|_| "source-coordinate-exact-floor-compatible-stable-major-v1"),
        }
    }
}
fn hash32(text: &str) -> Result<Hash> {
    let text = text.strip_prefix("0x").unwrap_or(text);
    if text.len() != 64 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid("code policy requires an actual checked 32-byte identity"));
    }
    let mut hash = [0; 32];
    for (slot, pair) in hash.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        let nibble = |byte: u8| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => unreachable!("validated hex byte"),
        };
        *slot = nibble(pair[0]) * 16 + nibble(pair[1]);
    }
    Ok(hash)
}
/// Consume actual private source/catalog proofs and a separately declared wire
/// snapshot. Check every header/member binding, including unselected members.
/// Status, admission sequences and floors are application snapshot choices,
/// not authenticated Registry/version facts. Only data2 Type-policy bindings
/// are supported: Type-hash history needs separate evidence and fails closed.
/// This neither authorizes the resulting root nor completes H1/H2 admission.
pub fn freeze_code_policy(sources: ResolvedSourceCatalog, policy: AuthorizationSet) -> Result<FrozenCodePolicy> {
    freeze_policy(sources, policy, BindingProfile::Finite)
}
/// Bind native source receipts and resolver-owned required identity in addition
/// to the existing finite artifact fields. Compatible selectable versions must
/// be stable, on the baseline source coordinate/edition and major, and at or
/// above its SemVer precedence. Exact mode pins the complete source receipt
/// and also enforces the baseline precedence floor on that selected receipt.
/// Source versions are manifest facts, not authenticated publisher releases.
/// This remains host evidence; immutable root authority and H1/H2 are required.
pub fn freeze_source_code_policy(sources: ResolvedSourceCatalog, policy: AuthorizationSet) -> Result<FrozenSourceCodePolicy> {
    freeze_policy(sources, policy, BindingProfile::SourceVersions).map(|inner| FrozenSourceCodePolicy { inner })
}
/// A distinct native source/version proof. Artifact-only policies cannot be
/// supplied where this proof is required; no public conversion or deserialize.
/// This is still host evidence, not a nominal language handle or root authority.
///
/// ```compile_fail
/// use cellscript::package::frozen_interface::{FrozenCodePolicy, FrozenSourceCodePolicy};
/// fn needs_source(_: FrozenSourceCodePolicy) {}
/// fn artifact_only(policy: FrozenCodePolicy) { needs_source(policy); }
/// ```
#[derive(Debug)]
pub struct FrozenSourceCodePolicy {
    inner: FrozenCodePolicy,
}
impl FrozenSourceCodePolicy {
    pub fn sources(&self) -> &ResolvedSourceCatalog {
        self.inner.sources()
    }
    pub fn policy(&self) -> &AuthorizationSet {
        self.inner.policy()
    }
    pub fn identity(&self) -> &str {
        self.inner.identity()
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.inner.canonical_bytes()
    }
    pub fn check_selection(&self, bytes: &[u8]) -> Result<FrozenSourceCodePolicySelection<'_>> {
        self.inner.check_selection(bytes).map(|inner| FrozenSourceCodePolicySelection { inner })
    }
}
/// A source/version-checked selection, separately typed from finite selection.
#[derive(Debug)]
pub struct FrozenSourceCodePolicySelection<'a> {
    inner: FrozenCodePolicySelection<'a>,
}
impl FrozenSourceCodePolicySelection<'_> {
    pub fn membership(&self) -> &PolicyMembership {
        self.inner.membership()
    }
    pub fn candidate(&self) -> &FrozenCodeCandidate {
        self.inner.candidate()
    }
    pub fn check_unchanged_inputs(&self, bundle: [&[u8]; 4], raw: &[u8], index: u32, script: &[u8]) -> Result<()> {
        self.inner.check_unchanged_inputs(bundle, raw, index, script)
    }
}
#[derive(Clone, Copy)]
enum BindingProfile {
    Finite,
    SourceVersions,
}
fn freeze_policy(sources: ResolvedSourceCatalog, policy: AuthorizationSet, profile: BindingProfile) -> Result<FrozenCodePolicy> {
    let candidates = sources.catalog().candidates();
    if policy.members().len() != candidates.len() {
        return Err(invalid("code policy must bind exactly every checked catalog candidate"));
    }
    let header = policy.header();
    if header.class != HandleClass::Script || header.role != ScriptRole::Type {
        return Err(invalid("code policy bindings require the checked finite Type-policy role"));
    }
    let required = sources.catalog().required();
    let runtime = required.projection().runtime_contract();
    if runtime.target_profile != "ckb" || candidates.iter().any(|candidate| candidate.origin().selected_hash_type() != 4) {
        return Err(invalid("code policy bindings require data2; Type-hash history lacks this binding profile"));
    }
    let target = canonical_hash("cellscript-code-policy-target-id-v1", &runtime.target_profile).map_err(checker_error)?;
    let abi = canonical_hash("cellscript-code-policy-runtime-id-v1", runtime).map_err(checker_error)?;
    let required_identity = match profile {
        BindingProfile::Finite => required.projection().identity(),
        BindingProfile::SourceVersions => sources.source_owner_identity(),
    };
    let source_floor = match profile {
        BindingProfile::Finite => None,
        BindingProfile::SourceVersions => Some(source_package_version(required)?),
    };
    if header.required_interface != hash32(required_identity)?
        || header.network_genesis != hash32(&required.context.network_genesis)?
        || header.target_profile != hash32(&target)?
        || header.runtime_abi != hash32(&abi)?
    {
        return Err(invalid("code policy header differs from actual required API, pinned network or checked runtime/target"));
    }
    let mut candidate_indices = Vec::new();
    for member in policy.members() {
        let index = candidates
            .iter()
            .position(|candidate| {
                let receipt = match profile {
                    BindingProfile::Finite => candidate.receipt().identity(),
                    BindingProfile::SourceVersions => candidate.source_receipt().identity(),
                };
                hash32(receipt).is_ok_and(|identity| identity == member.receipt)
            })
            .ok_or_else(|| invalid("code policy member has no actual checked catalog receipt"))?;
        if candidate_indices.contains(&index) {
            return Err(invalid("code policy repeats a checked catalog candidate"));
        }
        let candidate = &candidates[index];
        let origin = candidate.origin();
        if member.hash_type != CodeHashType::Data2
            || member.interface != hash32(candidate.module().projection().identity())?
            || member.artifact != hash32(origin.artifact_hash())?
            || member.script != hash32(origin.selected_script_hash())?
            || member.code_hash != hash32(origin.artifact_hash())?
            || member.code_tx_hash != hash32(origin.transaction_hash())?
            || member.code_output_index != origin.output_index()
            || member.deployment_sequence != 0
            || member.deployment_line != [0; 32]
            || member.history_tip != [0; 32]
        {
            return Err(invalid("code policy member differs from its actual checked API, artifact, complete Script or deployment"));
        }
        if let Some((floor, version)) = &source_floor {
            let actual = candidate.source_receipt();
            let package = actual.package();
            if package.name != floor.name || package.namespace != floor.namespace || package.edition != floor.edition {
                return Err(invalid(
                    "source code policy candidate differs from the required source coordinate/edition; use a matching baseline",
                ));
            }
            let selectable = member.status == MemberStatus::Active
                && member.admission_sequence >= header.minimum_admission_sequence
                && member.admission_sequence <= header.sequence
                && (header.mode == SelectionMode::Compatible || member.receipt == header.exact_receipt);
            if selectable && actual.package_version().cmp_precedence(version).is_lt() {
                return Err(invalid("source code policy rejects a selectable version downgrade; use a receipt at or above the baseline or explicitly pin an older baseline"));
            }
            if header.mode == SelectionMode::Compatible
                && selectable
                && (!version.pre.is_empty()
                    || !actual.package_version().pre.is_empty()
                    || actual.package_version().major != version.major)
            {
                return Err(invalid("source code policy rejects a selectable prerelease or major change in compatible mode; pin an exact receipt or use a stable matching major"));
            }
        }
        candidate_indices.push(index);
    }
    sources.check_unchanged_sources()?;
    let mut checked = FrozenCodePolicy {
        sources,
        policy,
        candidate_indices,
        identity: String::new(),
        source_version_floor: source_floor.map(|(package, _)| package),
    };
    let domain = match profile {
        BindingProfile::Finite => "cellscript-frozen-code-policy-bindings-id-v1",
        BindingProfile::SourceVersions => "cellscript-frozen-code-policy-bindings-id-v2",
    };
    checked.identity = canonical_hash(domain, &checked.record()).map_err(checker_error)?;
    Ok(checked)
}
