//! Couple source/version selection to actual final raw dependency byte checks.
//! No consensus resolution, signatures, peer execution or root authority.
use super::FrozenSourceCodePolicySelection;
use crate::error::Result;
use crate::package::frozen_interface::checker_error;
use cellscript_artifact_checker::code_origin::{check_direct_code_dependency, CheckedDirectCodeDependency, SuppliedDependencyCell};
use cellscript_artifact_checker::{canonical_bytes, canonical_hash, CheckerBudgets};
use serde::Serialize;

#[derive(Debug)]
pub struct FrozenSourceCodeDependency<'a> {
    pub(super) selection: FrozenSourceCodePolicySelection<'a>,
    pub(super) dependency: CheckedDirectCodeDependency,
    identity: String,
}
#[derive(Serialize)]
struct Record<'a> {
    schema: &'static str,
    source_policy: &'a str,
    selected_source_receipt: &'a str,
    checked_dependency: &'a str,
}
impl FrozenSourceCodeDependency<'_> {
    pub fn selection(&self) -> &FrozenSourceCodePolicySelection<'_> {
        &self.selection
    }
    pub fn dependency(&self) -> &CheckedDirectCodeDependency {
        &self.dependency
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record()).map_err(checker_error)
    }
    /// Original final raw/supplied snapshots and current consumer source closure
    /// must remain unchanged. Witness/signature freezing is a separate boundary.
    pub fn check_unchanged_inputs(&self, raw: &[u8], cells: &[SuppliedDependencyCell<'_>], budgets: &CheckerBudgets) -> Result<()> {
        self.dependency.check_unchanged_inputs(raw, cells, budgets).map_err(checker_error)?;
        self.selection.inner.policy.sources.check_unchanged_sources()
    }
    fn record(&self) -> Record<'_> {
        Record {
            schema: "cellscript-frozen-source-code-dependency-v1",
            source_policy: self.selection.inner.policy.identity(),
            selected_source_receipt: self.selection.candidate().source_receipt().identity(),
            checked_dependency: self.dependency.identity(),
        }
    }
}
impl<'a> FrozenSourceCodePolicySelection<'a> {
    /// Consume a source/version-checked selection, then bind actual final raw
    /// direct deps to every supplied Cell snapshot. Pure checker bounds/parsing
    /// precede the separate consumer source recheck. Only data2/direct deps are
    /// supported. Supplied cells are not authenticated VM/consensus resolution;
    /// raw tx excludes witnesses/signatures. This does not authorize the root.
    ///
    /// ```compile_fail
    /// use cellscript::package::frozen_interface::FrozenCodePolicySelection;
    /// use cellscript_artifact_checker::code_origin::SuppliedDependencyCell;
    /// fn artifact_only(s: FrozenCodePolicySelection<'_>, raw: &[u8], cells: &[SuppliedDependencyCell<'_>]) {
    ///     s.check_direct_dependency(raw, cells, &Default::default());
    /// }
    /// ```
    pub fn check_direct_dependency(
        self,
        raw: &[u8],
        cells: &[SuppliedDependencyCell<'_>],
        budgets: &CheckerBudgets,
    ) -> Result<FrozenSourceCodeDependency<'a>> {
        let dependency =
            check_direct_code_dependency(self.candidate().receipt().target_origin(), raw, cells, budgets).map_err(checker_error)?;
        self.inner.policy.sources.check_unchanged_sources()?;
        let mut checked = FrozenSourceCodeDependency { selection: self, dependency, identity: String::new() };
        checked.identity =
            canonical_hash("cellscript-frozen-source-code-dependency-id-v1", &checked.record()).map_err(checker_error)?;
        Ok(checked)
    }
}
