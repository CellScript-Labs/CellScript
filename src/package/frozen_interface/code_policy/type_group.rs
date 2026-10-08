//! Native source/version composition with a complete host Type-group snapshot.
use super::{FrozenSourceCodeDependency, FrozenSourceCodePolicySelection};
use crate::error::Result;
use crate::package::frozen_interface::checker_error;
use cellscript_artifact_checker::code_origin::{check_direct_type_group, CheckedDirectTypeGroup, SuppliedTypeGroupTransaction};
use cellscript_artifact_checker::{canonical_bytes, canonical_hash, CheckerBudgets};
use serde::Serialize;

/// A private host source/version/Type-group byte proof. This cannot be created
/// from artifact-only selections or exported JSON. It authorizes no policy root
/// and grants no authenticated resolution, signature validity or peer execution.
#[derive(Debug)]
pub struct FrozenSourceCodeTypeGroup<'a> {
    selection: FrozenSourceCodePolicySelection<'a>,
    group: CheckedDirectTypeGroup,
    identity: String,
}
#[derive(Serialize)]
struct Record<'a> {
    schema: &'static str,
    source_policy: &'a str,
    selected_source_receipt: &'a str,
    checked_type_group: &'a str,
}
impl FrozenSourceCodeTypeGroup<'_> {
    pub fn selection(&self) -> &FrozenSourceCodePolicySelection<'_> {
        &self.selection
    }
    pub fn group(&self) -> &CheckedDirectTypeGroup {
        &self.group
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record()).map_err(checker_error)
    }
    /// Recheck the full transaction (including opaque signature/extra bytes),
    /// every supplied Cell and current consumer sources. A signature mutation
    /// invalidates this snapshot; this does not verify signature correctness.
    pub fn check_unchanged_inputs(&self, inputs: &SuppliedTypeGroupTransaction<'_>, budgets: &CheckerBudgets) -> Result<()> {
        self.group.check_unchanged_inputs(inputs, budgets).map_err(checker_error)?;
        self.selection.inner.policy.sources.check_unchanged_sources()
    }
    fn record(&self) -> Record<'_> {
        Record {
            schema: "cellscript-frozen-source-code-type-group-v1",
            source_policy: self.selection.inner.policy.identity(),
            selected_source_receipt: self.selection.candidate().source_receipt().identity(),
            checked_type_group: self.group.identity(),
        }
    }
}
impl<'a> FrozenSourceCodeDependency<'a> {
    /// Consume the actual source/version/dependency proof and bind its exact
    /// candidate to a canonical complete Type-policy transaction snapshot.
    /// Independent bounded byte checks precede the separate consumer-source
    /// recheck. This remains a host prerequisite for #28, not open admission.
    ///
    /// ```compile_fail
    /// use cellscript::package::frozen_interface::FrozenCodePolicySelection;
    /// use cellscript_artifact_checker::code_origin::SuppliedTypeGroupTransaction;
    /// fn artifact_only(s: FrozenCodePolicySelection<'_>, tx: &SuppliedTypeGroupTransaction<'_>) {
    ///     s.check_type_group(tx, &Default::default());
    /// }
    /// ```
    pub fn check_type_group(
        self,
        inputs: &SuppliedTypeGroupTransaction<'_>,
        budgets: &CheckerBudgets,
    ) -> Result<FrozenSourceCodeTypeGroup<'a>> {
        let group =
            check_direct_type_group(self.selection.candidate().receipt(), self.dependency, inputs, budgets).map_err(checker_error)?;
        self.selection.inner.policy.sources.check_unchanged_sources()?;
        let mut checked = FrozenSourceCodeTypeGroup { selection: self.selection, group, identity: String::new() };
        checked.identity =
            canonical_hash("cellscript-frozen-source-code-type-group-id-v1", &checked.record()).map_err(checker_error)?;
        Ok(checked)
    }
}
