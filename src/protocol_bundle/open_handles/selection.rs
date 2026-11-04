//! Typed composition keeps old exact-instance evidence separate from the new
//! per-instance receipts while sharing transaction/group validation.
use super::*;
use crate::package::frozen_interface::{
    CheckedSourceCodeReceipt, FrozenCodeCandidate, FrozenInstanceDependency, FrozenInstanceGroup, FrozenInstancePolicy,
    FrozenInstanceSelection, FrozenSourceCodeDependency, FrozenSourceCodePolicySelection, FrozenSourceCodeTypeGroup,
};
use cellscript_artifact_checker::code_origin::{
    CheckedDirectCodeDependency, CheckedDirectTypeGroup, ResolvedInstanceAdmission, SuppliedTypeGroupTransaction,
};
use cellscript_artifact_checker::fixed_policy_receipt::CheckedFixedPolicyReceipt;
use cellscript_artifact_checker::open_handle_policy::PolicyMembership;

#[derive(Clone, Copy)]
pub(super) enum AdmissionPolicy<'a> {
    Open(&'a FrozenSourceCodePolicy),
    Instance(&'a FrozenInstancePolicy),
}
impl<'a> AdmissionPolicy<'a> {
    pub(super) fn implementation(self) -> &'a FrozenSourceCodePolicy {
        match self {
            Self::Open(policy) => policy,
            Self::Instance(policy) => policy.implementation(),
        }
    }
    pub(super) fn is_instance(self) -> bool {
        matches!(self, Self::Instance(_))
    }
    pub(super) fn select(self, consumer: &[u8], wire: &[u8], budgets: &CheckerBudgets) -> Result<AdmissionSelection<'a>> {
        match self {
            Self::Open(policy) => Ok(AdmissionSelection::Open(Box::new(policy.check_selection(wire)?))),
            Self::Instance(policy) => Ok(AdmissionSelection::Instance(Box::new(policy.check_selection(consumer, wire, budgets)?))),
        }
    }
}

pub(super) enum AdmissionSelection<'a> {
    Open(Box<FrozenSourceCodePolicySelection<'a>>),
    Instance(Box<FrozenInstanceSelection<'a>>),
}
impl<'a> AdmissionSelection<'a> {
    pub(super) fn candidate(&self) -> &FrozenCodeCandidate {
        match self {
            Self::Open(value) => value.candidate(),
            Self::Instance(value) => value.implementation().candidate(),
        }
    }
    pub(super) fn receipt(&self) -> &CheckedFixedPolicyReceipt {
        match self {
            Self::Open(value) => value.candidate().receipt(),
            Self::Instance(value) => value.receipt(),
        }
    }
    pub(super) fn membership(&self) -> &PolicyMembership {
        match self {
            Self::Open(value) => value.membership(),
            Self::Instance(value) => value.membership().implementation(),
        }
    }
    pub(super) fn instance_record(&self, policy: AdmissionPolicy<'_>, consumer: &[u8]) -> Result<Option<ResolvedInstanceAdmission>> {
        match (self, policy) {
            (Self::Open(_), AdmissionPolicy::Open(_)) => Ok(None),
            (Self::Instance(selected), AdmissionPolicy::Instance(policy)) => Ok(Some(ResolvedInstanceAdmission {
                schema: PROTOCOL_INSTANCE_ADMISSION_SCHEMA.into(),
                source_policy: policy.identity().into(),
                selection_identity: selected.identity().into(),
                implementation_receipt: selected.implementation().candidate().source_receipt().identity().into(),
                args_schema: hex_encode(&policy.policy().args_schema().map_err(|error| invalid(&error.to_string()))?),
                policy_root: format!("0x{}", hex_encode(&policy.policy().root().map_err(|error| invalid(&error.to_string()))?)),
                consumer_args: format!("0x{}", hex_encode(consumer)),
                instance_mode: match policy.policy().mode {
                    cellscript_artifact_checker::instance_policy::InstanceMode::Exact => "exact",
                    cellscript_artifact_checker::instance_policy::InstanceMode::Constrained => "constrained",
                }
                .into(),
            })),
            _ => Err(invalid("instance selection and policy kinds differ")),
        }
    }
    pub(super) fn check_direct_dependency(
        self,
        raw: &[u8],
        dependencies: &[SuppliedDependencyCell<'_>],
        budgets: &CheckerBudgets,
    ) -> Result<AdmissionDependency<'a>> {
        match self {
            Self::Open(value) => {
                Ok(AdmissionDependency::Open(Box::new((*value).check_direct_dependency(raw, dependencies, budgets)?)))
            }
            Self::Instance(value) => {
                Ok(AdmissionDependency::Instance(Box::new((*value).check_direct_dependency(raw, dependencies, budgets)?)))
            }
        }
    }
}

pub(super) enum AdmissionDependency<'a> {
    Open(Box<FrozenSourceCodeDependency<'a>>),
    Instance(Box<FrozenInstanceDependency<'a>>),
}
impl<'a> AdmissionDependency<'a> {
    pub(super) fn dependency(&self) -> &CheckedDirectCodeDependency {
        match self {
            Self::Open(value) => value.dependency(),
            Self::Instance(value) => value.dependency(),
        }
    }
    pub(super) fn source_receipt(&self) -> &CheckedSourceCodeReceipt {
        match self {
            Self::Open(value) => value.selection().candidate().source_receipt(),
            Self::Instance(value) => value.selection().source_receipt(),
        }
    }
    pub(super) fn receipt(&self) -> &CheckedFixedPolicyReceipt {
        match self {
            Self::Open(value) => value.selection().candidate().receipt(),
            Self::Instance(value) => value.selection().receipt(),
        }
    }
    pub(super) fn check_script_group(
        self,
        inputs: &SuppliedTypeGroupTransaction<'_>,
        budgets: &CheckerBudgets,
    ) -> Result<AdmissionGroup<'a>> {
        match self {
            Self::Open(value) => Ok(AdmissionGroup::Open(Box::new((*value).check_script_group(inputs, budgets)?))),
            Self::Instance(value) => Ok(AdmissionGroup::Instance(Box::new((*value).check_script_group(inputs, budgets)?))),
        }
    }
}

pub(super) enum AdmissionGroup<'a> {
    Open(Box<FrozenSourceCodeTypeGroup<'a>>),
    Instance(Box<FrozenInstanceGroup<'a>>),
}
impl AdmissionGroup<'_> {
    pub(super) fn group(&self) -> &CheckedDirectTypeGroup {
        match self {
            Self::Open(value) => value.group(),
            Self::Instance(value) => value.group(),
        }
    }
}
