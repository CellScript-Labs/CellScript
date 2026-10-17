//! Private source/version evidence composed with independently checked instance
//! constraints and actual deployment bytes. Host proofs remain separate from
//! authenticated consumer args, machine enforcement, signatures and execution.
use super::source_receipt::bind_source_receipt;
use super::{checker_error, invalid, CheckedSourceCodeReceipt, FrozenSourceCodePolicy, FrozenSourceCodePolicySelection};
use crate::error::Result;
use cellscript_artifact_checker::code_origin::{
    check_direct_code_dependency_with_history, check_direct_script_group, CheckedDirectCodeDependency, CheckedDirectTypeGroup,
    CheckedTypeCodeHistory, SuppliedDependencyCell, SuppliedTypeGroupTransaction,
};
use cellscript_artifact_checker::fixed_policy_receipt::{check_fixed_policy_receipt, CheckedFixedPolicyReceipt};
use cellscript_artifact_checker::instance_policy::{self, InstanceMembership, InstanceMode, InstancePolicy};
use cellscript_artifact_checker::open_handle_policy::ScriptRole;
use cellscript_artifact_checker::{canonical_bytes, canonical_hash, ckb_blake2b256, hex_encode, CheckerBudgets};
use serde::Serialize;

#[derive(Debug)]
pub struct FrozenInstancePolicy {
    implementation: FrozenSourceCodePolicy,
    policy: InstancePolicy,
    identity: String,
}

#[derive(Serialize)]
struct PolicyRecord<'a> {
    schema: &'static str,
    source_policy: &'a str,
    instance_policy_root: String,
    args_schema: String,
}

pub fn freeze_instance_policy(implementation: FrozenSourceCodePolicy, policy: InstancePolicy) -> Result<FrozenInstancePolicy> {
    policy.encode().map_err(|error| invalid(error.to_string()))?;
    if policy.implementation_root != implementation.policy().root() {
        return Err(invalid("instance policy does not bind the actual private source policy"));
    }
    if policy.mode == InstanceMode::Constrained && implementation.policy().header().role == ScriptRole::SpawnedVerifier {
        return Err(invalid("parameterless spawned verifiers have no independent instance args context; keep exact admission"));
    }
    implementation.sources().check_unchanged_sources()?;
    let mut frozen = FrozenInstancePolicy { implementation, policy, identity: String::new() };
    frozen.identity = canonical_hash("cellscript-frozen-instance-policy-id-v1", &frozen.record()?).map_err(checker_error)?;
    Ok(frozen)
}

impl FrozenInstancePolicy {
    pub fn implementation(&self) -> &FrozenSourceCodePolicy {
        &self.implementation
    }
    pub fn policy(&self) -> &InstancePolicy {
        &self.policy
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record()?).map_err(checker_error)
    }
    fn record(&self) -> Result<PolicyRecord<'_>> {
        Ok(PolicyRecord {
            schema: "cellscript-frozen-instance-policy-v1",
            source_policy: self.implementation.identity(),
            instance_policy_root: hex_encode(&self.policy.root().map_err(|error| invalid(error.to_string()))?),
            args_schema: hex_encode(&self.policy.args_schema().map_err(|error| invalid(error.to_string()))?),
        })
    }

    /// The consumer byte string is a supplied host input. ProtocolBundle and
    /// the runtime must separately authenticate its actual input/current Script
    /// origin. No public `verified` flag can construct this private selection.
    pub fn check_selection<'a>(
        &'a self,
        consumer_args: &[u8],
        selection: &[u8],
        budgets: &CheckerBudgets,
    ) -> Result<FrozenInstanceSelection<'a>> {
        let membership = instance_policy::verify_selection(consumer_args, selection).map_err(|error| invalid(error.to_string()))?;
        if membership.policy() != &self.policy {
            return Err(invalid("selection carries a different instance policy"));
        }
        let implementation =
            self.implementation.check_selection(&selection[instance_policy::IMPLEMENTATION_OFFSET..instance_policy::ARGS_OFFSET])?;
        let candidate = implementation.candidate();
        let receipt = check_fixed_policy_receipt(
            candidate.module().bundle(),
            candidate.raw_transaction(),
            candidate.origin().output_index(),
            membership.script(),
            budgets,
        )
        .map_err(checker_error)?;
        candidate.receipt().check_required_contracts(&receipt).map_err(checker_error)?;
        let history = implementation
            .type_history()
            .map(|history| history.bind_instance(candidate.target_origin(), receipt.target_origin()))
            .transpose()
            .map_err(checker_error)?;
        let source_receipt = bind_source_receipt(candidate.module(), &receipt)?;
        let record = SelectionRecord {
            schema: "cellscript-frozen-instance-selection-v1",
            policy: self.identity.clone(),
            implementation_source_receipt: candidate.source_receipt().identity().into(),
            instance_source_receipt: source_receipt.identity().into(),
            instance_receipt: receipt.identity().into(),
            consumer_args_hash: hex_encode(&ckb_blake2b256(consumer_args)),
            selection_hash: hex_encode(&ckb_blake2b256(selection)),
            script_hash: hex_encode(&membership.script_hash()),
        };
        self.implementation.sources().check_unchanged_sources()?;
        let identity = canonical_hash("cellscript-frozen-instance-selection-id-v1", &record).map_err(checker_error)?;
        Ok(FrozenInstanceSelection { policy: self, implementation, membership, receipt, source_receipt, history, record, identity })
    }
}

#[derive(Debug, Serialize)]
struct SelectionRecord {
    schema: &'static str,
    policy: String,
    implementation_source_receipt: String,
    instance_source_receipt: String,
    instance_receipt: String,
    consumer_args_hash: String,
    selection_hash: String,
    script_hash: String,
}
#[derive(Debug)]
pub struct FrozenInstanceSelection<'a> {
    policy: &'a FrozenInstancePolicy,
    implementation: FrozenSourceCodePolicySelection<'a>,
    membership: InstanceMembership,
    receipt: CheckedFixedPolicyReceipt,
    source_receipt: CheckedSourceCodeReceipt,
    history: Option<CheckedTypeCodeHistory>,
    record: SelectionRecord,
    identity: String,
}
impl<'a> FrozenInstanceSelection<'a> {
    pub fn implementation(&self) -> &FrozenSourceCodePolicySelection<'a> {
        &self.implementation
    }
    pub fn membership(&self) -> &InstanceMembership {
        &self.membership
    }
    pub fn receipt(&self) -> &CheckedFixedPolicyReceipt {
        &self.receipt
    }
    pub fn source_receipt(&self) -> &CheckedSourceCodeReceipt {
        &self.source_receipt
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record).map_err(checker_error)
    }

    pub fn check_unchanged_selection(&self, consumer_args: &[u8], selection: &[u8]) -> Result<()> {
        if consumer_args.len() != instance_policy::CONSUMER_ARGS_BYTES
            || selection.len() != instance_policy::SELECTION_BYTES
            || hex_encode(&ckb_blake2b256(consumer_args)) != self.record.consumer_args_hash
            || hex_encode(&ckb_blake2b256(selection)) != self.record.selection_hash
        {
            return Err(invalid("instance consumer args or selection changed after admission"));
        }
        self.policy.implementation.sources().check_unchanged_sources()
    }

    pub fn check_direct_dependency(
        self,
        raw: &[u8],
        cells: &[SuppliedDependencyCell<'_>],
        budgets: &CheckerBudgets,
    ) -> Result<FrozenInstanceDependency<'a>> {
        let dependency =
            check_direct_code_dependency_with_history(self.receipt.target_origin(), self.history.as_ref(), raw, cells, budgets)
                .map_err(checker_error)?;
        self.policy.implementation.sources().check_unchanged_sources()?;
        let record = BindingRecord {
            schema: "cellscript-frozen-instance-dependency-v1",
            policy: self.policy.identity.clone(),
            selection: self.identity.clone(),
            checked_transaction: dependency.identity().into(),
        };
        let identity = canonical_hash("cellscript-frozen-instance-dependency-id-v1", &record).map_err(checker_error)?;
        Ok(FrozenInstanceDependency { selection: self, dependency, record, identity })
    }
}

#[derive(Debug, Serialize)]
struct BindingRecord {
    schema: &'static str,
    policy: String,
    selection: String,
    checked_transaction: String,
}

#[derive(Debug)]
pub struct FrozenInstanceDependency<'a> {
    selection: FrozenInstanceSelection<'a>,
    dependency: CheckedDirectCodeDependency,
    record: BindingRecord,
    identity: String,
}
impl<'a> FrozenInstanceDependency<'a> {
    pub fn selection(&self) -> &FrozenInstanceSelection<'a> {
        &self.selection
    }
    pub fn dependency(&self) -> &CheckedDirectCodeDependency {
        &self.dependency
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record).map_err(checker_error)
    }
    pub fn check_unchanged_inputs(&self, raw: &[u8], cells: &[SuppliedDependencyCell<'_>], budgets: &CheckerBudgets) -> Result<()> {
        self.dependency.check_unchanged_inputs(raw, cells, budgets).map_err(checker_error)?;
        self.selection.policy.implementation.sources().check_unchanged_sources()
    }
    pub fn check_script_group(
        self,
        inputs: &SuppliedTypeGroupTransaction<'_>,
        budgets: &CheckerBudgets,
    ) -> Result<FrozenInstanceGroup<'a>> {
        if self.selection.membership.implementation().header().role == ScriptRole::SpawnedVerifier {
            return Err(invalid("a spawned verifier has no independent Script group"));
        }
        let group = check_direct_script_group(&self.selection.receipt, self.dependency, inputs, budgets).map_err(checker_error)?;
        self.selection.policy.implementation.sources().check_unchanged_sources()?;
        let record = BindingRecord {
            schema: "cellscript-frozen-instance-group-v1",
            policy: self.selection.policy.identity.clone(),
            selection: self.selection.identity.clone(),
            checked_transaction: group.identity().into(),
        };
        let identity = canonical_hash("cellscript-frozen-instance-group-id-v1", &record).map_err(checker_error)?;
        Ok(FrozenInstanceGroup { selection: self.selection, group, record, identity })
    }
}

#[derive(Debug)]
pub struct FrozenInstanceGroup<'a> {
    selection: FrozenInstanceSelection<'a>,
    group: CheckedDirectTypeGroup,
    record: BindingRecord,
    identity: String,
}
impl<'a> FrozenInstanceGroup<'a> {
    pub fn selection(&self) -> &FrozenInstanceSelection<'a> {
        &self.selection
    }
    pub fn group(&self) -> &CheckedDirectTypeGroup {
        &self.group
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record).map_err(checker_error)
    }
    pub fn check_unchanged_inputs(&self, inputs: &SuppliedTypeGroupTransaction<'_>, budgets: &CheckerBudgets) -> Result<()> {
        self.group.check_unchanged_inputs(inputs, budgets).map_err(checker_error)?;
        self.selection.policy.implementation.sources().check_unchanged_sources()
    }
}
