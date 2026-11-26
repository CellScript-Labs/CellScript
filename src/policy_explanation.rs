//! Human policy views of resolved typed records. This projection is not a hash
//! input or an additional verifier. Category describes the policy's authority;
//! its evidence boundary is displayed separately and is never promoted here.

use crate::error::{CompileError, Result};
use crate::CompileMetadata;
use cellscript_artifact_checker::{
    CellBindingMembership, CellBindingRole, CellBindingSource, CellDisposition, InputDisposition, OutputOrigin,
    SuccessorExecutionContract, TypedSemanticCellBinding,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyCategory {
    Explicit,
    DerivedAndEnforced,
    UnconstrainedByRelation,
    NotApplicable,
}

impl PolicyCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::DerivedAndEnforced => "derived-and-enforced",
            Self::UnconstrainedByRelation => "unconstrained-by-this-relation",
            Self::NotApplicable => "not-applicable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleCoordinate {
    pub role_id: String,
    pub binding: String,
    pub selector: String,
    pub source_scope: String,
    pub membership: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyTreatment {
    pub component: &'static str,
    pub category: PolicyCategory,
    pub evidence: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationPolicies {
    pub id: String,
    pub input: Option<RoleCoordinate>,
    pub output: Option<RoleCoordinate>,
    pub enforcement: String,
    pub treatments: Vec<PolicyTreatment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryPolicies {
    pub entry_id: String,
    pub selected: bool,
    pub coverage: String,
    pub coordinates: String,
    pub group_counts: Option<(usize, usize)>,
    pub roles: Vec<RoleCoordinate>,
    pub relations: Vec<RelationPolicies>,
}

pub fn explain(metadata: &CompileMetadata, entry_id: &str) -> Result<EntryPolicies> {
    let typed = &metadata.typed_semantics;
    let entry = typed
        .entries
        .iter()
        .find(|entry| entry.id == entry_id)
        .ok_or_else(|| CompileError::without_span(format!("policy explanation has no resolved entry '{entry_id}'")))?;
    let contract = &typed.foundation.entry_contract;
    let selected = contract.exact_entry == entry_id
        || matches!(&contract.dispatch, cellscript_artifact_checker::EntryDispatchContract::PolicyWitnessV1(policy)
            if policy.variants.iter().any(|variant| variant.entry_id == entry_id));
    let cells = selected.then_some(contract.cells.as_ref()).flatten();
    let coverage = if !selected { "unselected-entry" } else { cells.map_or("not-declared", |cells| cells.coverage.as_str()) };
    let mut coordinates = cells.map_or_else(
        || {
            entry
                .cell_bindings
                .iter()
                .map(TypedSemanticCellBinding::source_scope)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(",")
        },
        |cells| cells.bindings.as_str().into(),
    );
    if coordinates.is_empty() {
        coordinates = "none".into();
    }
    let group_counts = cells.filter(|cells| cells.coverage == cellscript_artifact_checker::EntryCoverage::FixedGroup).map(|_| {
        let count = |role, source| {
            entry
                .cell_bindings
                .iter()
                .filter(|binding| binding.role == role && binding.source == source)
                .map(|binding| binding.ordinal)
                .collect::<BTreeSet<_>>()
                .len()
        };
        (count(CellBindingRole::Input, CellBindingSource::GroupInput), count(CellBindingRole::Output, CellBindingSource::GroupOutput))
    });
    let roles = typed
        .foundation
        .roles
        .iter()
        .filter(|role| role.entry_id == entry_id)
        .map(|role| {
            let binding = entry.cell_bindings.iter().find(|binding| binding.role_id(entry_id) == role.role_id);
            if let Some(binding) = binding
                && (binding.selector() != role.selector || binding.binding != role.binding)
            {
                return Err(CompileError::without_span(format!("policy role '{}' disagrees with its physical binding", role.role_id)));
            }
            Ok(RoleCoordinate {
                role_id: role.role_id.clone(),
                binding: role.binding.clone(),
                selector: role.selector.clone(),
                source_scope: binding.map_or_else(|| role.source.clone(), |binding| binding.source_scope().into()),
                membership: binding.map_or_else(|| role.script_identity_policy.clone(), |binding| binding.membership_policy().into()),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if cells.is_none() && coordinates == "none" && !roles.is_empty() {
        coordinates =
            roles.iter().map(|role| role.source_scope.as_str()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(",");
    }
    let mut relations = Vec::new();
    for disposition in typed.foundation.dispositions.iter().filter(|record| record.entry_id == entry_id) {
        let lookup = |id: Option<&String>| -> Result<Option<RoleCoordinate>> {
            id.map(|id| {
                roles.iter().find(|role| &role.role_id == id).cloned().ok_or_else(|| {
                    CompileError::without_span(format!("policy relation '{}' has an unresolved role '{id}'", disposition.id))
                })
            })
            .transpose()
        };
        let input = lookup(disposition.input_role.as_ref())?;
        let output = lookup(disposition.output_role.as_ref())?;
        let role_id = disposition.input_role.as_ref().or(disposition.output_role.as_ref());
        let binding = entry.cell_bindings.iter().find(|binding| Some(&binding.role_id(entry_id)) == role_id);
        let member = binding.is_some_and(|binding| binding.membership == CellBindingMembership::CurrentTypeGroup);
        if disposition.envelope.type_script == "checked-type-group-member" && !member {
            return Err(CompileError::without_span(format!(
                "policy relation '{}' claims Type group authority without a matching binding",
                disposition.id
            )));
        }
        relations.push(RelationPolicies {
            id: disposition.id.clone(),
            treatments: treatments(metadata, disposition, binding),
            input,
            output,
            enforcement: disposition.enforcement.clone(),
        });
    }
    Ok(EntryPolicies { entry_id: entry_id.into(), selected, coverage: coverage.into(), coordinates, group_counts, roles, relations })
}

fn treatments(
    metadata: &CompileMetadata,
    record: &CellDisposition,
    binding: Option<&TypedSemanticCellBinding>,
) -> Vec<PolicyTreatment> {
    use PolicyCategory::{DerivedAndEnforced as Derived, Explicit, NotApplicable, UnconstrainedByRelation as Free};
    let member = binding.is_some_and(|binding| binding.membership == CellBindingMembership::CurrentTypeGroup);
    let envelope = &record.envelope;
    let pooled = matches!(record.input, Some(InputDisposition::Pooled { .. }))
        || matches!(record.output, Some(OutputOrigin::PoolResult { .. }));
    let retired = matches!(record.input, Some(InputDisposition::Retired { .. }));
    let authorization = matches!(record.input, Some(InputDisposition::AuthorizationOnly { .. }));
    let no_successor = record.output_role.is_none() && !pooled;
    let mut policies = Vec::new();
    let mut add = |component, category, detail: String, evidence: &str| {
        policies.push(PolicyTreatment { component, category, detail, evidence: evidence.into() })
    };
    add(
        "data",
        if no_successor { NotApplicable } else { Explicit },
        if no_successor {
            "no successor data relation; input decoding and local predicates remain separate".into()
        } else {
            envelope.data_fields.iter().map(|field| format!("{}={}", field.field, field.treatment)).collect::<Vec<_>>().join(", ")
        },
        &record.enforcement,
    );
    match (&record.input, &record.output) {
        (Some(InputDisposition::Retired { absence_policy }), _) => {
            let scan = binding.map_or("unresolved output search domain", |binding| {
                if binding.source == CellBindingSource::GroupInput {
                    "group-output"
                } else {
                    "transaction output"
                }
            });
            add(
                "identity",
                Explicit,
                format!("absence={absence_policy}; search={scan} with matching input Type Script hash; not a successor policy"),
                &record.enforcement,
            )
        }
        _ if pooled => add(
            "identity/accounting",
            if record.enforcement == "checked-runtime" { Derived } else { Explicit },
            format!("{}; named quantity accounting does not establish individual object identity", envelope.logical_identity),
            &record.enforcement,
        ),
        (_, Some(OutputOrigin::Fresh { identity_policy })) if identity_policy != "none" => {
            let boundary = if record.lifecycle_execution.is_some() {
                "local identity check; global uniqueness needs builder/indexer or TYPE_ID evidence"
            } else {
                "declared identity policy; retain the recorded evidence boundary"
            };
            add("identity", Explicit, format!("declared {identity_policy}; {boundary}"), &record.enforcement);
        }
        _ if envelope.logical_identity.starts_with("preserve-declared:")
            || envelope.logical_identity.starts_with("receipt-claim-result:") =>
        {
            add("identity", Explicit, envelope.logical_identity.clone(), &record.enforcement);
        }
        _ => add("identity", Free, "business-object identity is not established by this relation".into(), "not-established"),
    }
    let lock = envelope.lock_script.as_str();
    add(
        "lock",
        if authorization {
            Explicit
        } else if no_successor {
            NotApplicable
        } else if matches!(lock, "unspecified" | "unconstrained-by-this-relation") {
            Free
        } else {
            Explicit
        },
        if authorization {
            "authorization belongs to the selected Lock entry and its Boolean condition; no signer is inferred".into()
        } else {
            lock.into()
        },
        &record.enforcement,
    );
    let type_script = envelope.type_script.as_str();
    let group_kernel =
        record.execution.as_ref().is_some_and(|execution| execution.contract == SuccessorExecutionContract::FixedGroupKernel);
    let type_category = if no_successor {
        NotApplicable
    } else if matches!(type_script, "checked-type-group-member" | "current-type-group") || member && group_kernel {
        Derived
    } else if matches!(
        type_script,
        "preserve-and-check"
            | "preserve"
            | "set-and-check"
            | "declared-preserve-type-hash"
            | "identity-preserve-current-type-hash"
            | "identity-type-hash-present-builder-plan-required"
    ) {
        Explicit
    } else {
        Free
    };
    let type_evidence = if type_script == "identity-type-hash-present-builder-plan-required" {
        "builder-evidence-required"
    } else if member && type_category == Derived {
        "checked-current-type-group"
    } else {
        &record.enforcement
    };
    add(
        "type-script",
        type_category,
        if retired {
            "no successor Type Script; the explicit absence rule is shown under identity".into()
        } else {
            type_script.into()
        },
        type_evidence,
    );
    let capacity = envelope.capacity.as_str();
    let capacity_category = if no_successor || pooled && record.output_role.is_none() {
        NotApplicable
    } else if matches!(capacity, "preserve-and-check" | "at-least-declared-floor-checked-runtime") {
        Explicit
    } else {
        Free
    };
    let mut capacity_detail = if capacity_category == Free {
        format!("{capacity}; no predecessor/successor capacity equality supplied by this relation")
    } else {
        capacity.into()
    };
    if let Some(role) =
        record.output_role.as_ref().and_then(|id| metadata.typed_semantics.foundation.roles.iter().find(|role| &role.role_id == id))
        && let Some(floor) = metadata.types.iter().find(|ty| ty.name == role.ty).and_then(|ty| ty.capacity_floor_shannons)
    {
        capacity_detail.push_str(&format!("; declaration floor={floor} shannons has separate verifier obligations"));
    }
    add(
        "capacity",
        capacity_category,
        capacity_detail,
        if capacity_category == Free { "not-established-by-relation" } else { &record.enforcement },
    );
    let arity = if record.enforcement == "checked-runtime"
        && (record.execution.is_some() || record.lifecycle_execution.is_some() || record.kernel_execution.is_some())
    {
        Derived
    } else {
        Explicit
    };
    add(
        "cardinality",
        arity,
        format!("{}; local relation arity, separate from entry/group coverage", envelope.cardinality),
        &record.enforcement,
    );
    policies
}

impl RelationPolicies {
    pub fn text_lines(&self) -> Vec<String> {
        self.treatments
            .iter()
            .map(|policy| {
                format!(
                    "policy {} category={} evidence={} value={}",
                    policy.component,
                    policy.category.as_str(),
                    policy.evidence,
                    policy.detail
                )
            })
            .collect()
    }
}

impl EntryPolicies {
    pub fn hover(&self, binding: Option<&str>) -> String {
        let mut text = format!(
            "\n\n**Resolved Cell policy**\n\nEntry `{}`; selected `{}`; coordinates `{}`; coverage `{}`.",
            self.entry_id, self.selected, self.coordinates, self.coverage
        );
        if let Some((inputs, outputs)) = self.group_counts {
            text.push_str(&format!(" Exact group bounds: {inputs} inputs, {outputs} outputs."));
        }
        for role in self.roles.iter().filter(|role| binding.is_none_or(|binding| role.binding == binding)) {
            text.push_str(&format!(
                "\n\nRole {} at {}; scope {}; membership {}.",
                code_span(&role.binding),
                code_span(&role.selector),
                code_span(&role.source_scope),
                code_span(&role.membership)
            ));
        }
        let relations = self
            .relations
            .iter()
            .filter(|record| {
                binding.is_none_or(|binding| record.input.iter().chain(&record.output).any(|role| role.binding == binding))
            })
            .collect::<Vec<_>>();
        if relations.is_empty() {
            text.push_str("\n\nNo lifecycle relation claims are recorded for this role/entry.");
        }
        for relation in relations.iter().take(16) {
            text.push_str(&format!("\n\nRelation {}; evidence {}.", code_span(&relation.id), code_span(&relation.enforcement)));
            for policy in &relation.treatments {
                let detail = if policy.detail.chars().count() > 512 {
                    policy.detail.chars().take(512).collect::<String>() + "… (full value in cellc expand)"
                } else {
                    policy.detail.clone()
                };
                text.push_str(&format!(
                    "\n\n- {}: {}; evidence {}; {}",
                    policy.component,
                    code_span(policy.category.as_str()),
                    code_span(&policy.evidence),
                    code_span(&detail)
                ));
            }
        }
        if relations.len() > 16 {
            text.push_str(&format!("\n\n{} more relation occurrences are available in `cellc expand`.", relations.len() - 16));
        }
        text
    }
}

fn code_span(value: &str) -> String {
    let delimiter = "`".repeat(value.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
    format!("{delimiter} {} {delimiter}", value.replace(['\n', '\r'], " "))
}
