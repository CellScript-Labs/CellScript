use super::{effective_check_args, BuildArgs, CheckArgs};
use crate::error::Result;
use crate::package::PolicyConfig;
use crate::{ArtifactFormat, ExecutableSurfacePolicy, TargetProfile};

pub(super) fn validate_build_entry_selection(args: &BuildArgs) -> Result<()> {
    let selected = [args.artifact.is_some(), args.entry_action.is_some(), args.entry_lock.is_some()];
    if selected.into_iter().filter(|selected| *selected).count() > 1 {
        return Err(crate::error::CompileError::without_span("--artifact, --entry-action, and --entry-lock are mutually exclusive"));
    }
    Ok(())
}

pub(super) fn effective_build_check_args(args: &BuildArgs) -> Result<CheckArgs> {
    effective_check_args(CheckArgs {
        all_targets: false,
        artifact: args.artifact.clone(),
        target_profile: args.target_profile.clone(),
        features: args.features.clone(),
        all_features: args.all_features,
        no_default_features: args.no_default_features,
        locked: args.locked,
        frozen: args.frozen,
        offline: args.offline,
        environment: args.environment.clone(),
        json: false,
        production: args.production,
        deny_fail_closed: args.deny_fail_closed,
        deny_ckb_runtime: args.deny_ckb_runtime,
        deny_runtime_obligations: args.deny_runtime_obligations,
        primitive_compat: args.primitive_compat.clone(),
        package: None,
        workspace: false,
    })
}

pub(super) fn merge_check_policy(args: &mut CheckArgs, policy: &PolicyConfig) {
    args.production |= policy.production;
    args.deny_fail_closed |= policy.deny_fail_closed;
    args.deny_ckb_runtime |= policy.deny_ckb_runtime;
    args.deny_runtime_obligations |= policy.deny_runtime_obligations;
}

pub(super) fn validate_expected_metadata_hash(field: &str, actual: Option<&str>, expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err(crate::error::CompileError::without_span(format!(
            "{} expectation must be a 64-character lowercase CKB Blake2b hex digest, got '{}'",
            field, expected
        )));
    }
    match actual {
        Some(actual) if actual.eq_ignore_ascii_case(expected) => Ok(()),
        Some(actual) => Err(crate::error::CompileError::without_span(format!(
            "metadata {} '{}' does not match expected '{}'",
            field, actual, expected
        ))),
        None => Err(crate::error::CompileError::without_span(format!(
            "metadata is missing {} required by expectation '{}'",
            field, expected
        ))),
    }
}

pub(super) fn validate_expected_target_profile(actual: &str, expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let expected_profile = TargetProfile::from_name(expected)?;
    if actual == expected_profile.name() {
        return Ok(());
    }

    Err(crate::error::CompileError::without_span(format!(
        "metadata target_profile '{}' does not match expected '{}'",
        actual,
        expected_profile.name()
    )))
}

pub(super) fn validate_check_policy(metadata: &crate::CompileMetadata, args: &CheckArgs) -> Result<()> {
    let mut violations = Vec::new();

    if args.primitive_compat.as_deref() == Some("0.16") {
        if let Err(error) = crate::proof_plan::soundness::validate_metadata(metadata, true) {
            violations.push(error.into_message());
        }
    } else if matches!(args.primitive_compat.as_deref(), Some("0.17" | "0.18")) {
        if let Err(error) = crate::validate_primitive_strict_017_metadata(metadata) {
            violations.push(error.into_message());
        }
    } else if metadata.runtime.proof_plan_soundness.status == "failed" {
        violations.push(format!("ProofPlan soundness failed: {} issue(s)", metadata.runtime.proof_plan_soundness.issue_count));
    }

    if args.production || args.deny_fail_closed {
        if !metadata.constraints.failures.is_empty() {
            violations.push(format!("constraints failures: {}", metadata.constraints.failures.join(", ")));
        }

        if !metadata.runtime.fail_closed_runtime_features.is_empty() {
            violations.push(format!("fail-closed runtime features: {}", metadata.runtime.fail_closed_runtime_features.join(", ")));
        }

        let fail_closed_obligations = metadata
            .runtime
            .verifier_obligations
            .iter()
            .filter(|obligation| obligation.status == "fail-closed")
            .map(|obligation| format!("{}:{} ({})", obligation.scope, obligation.feature, obligation.category))
            .collect::<Vec<_>>();
        if !fail_closed_obligations.is_empty() {
            violations.push(format!("fail-closed verifier obligations: {}", fail_closed_obligations.join(", ")));
        }
    }

    if args.production {
        let blocking_consensus_gaps = metadata
            .runtime
            .proof_plan
            .iter()
            .filter(|plan| crate::proof_plan_has_blocking_consensus_gap(plan))
            .map(|plan| format!("{}:{} ({})", plan.origin, plan.feature, plan.codegen_coverage_status))
            .collect::<Vec<_>>();
        if !blocking_consensus_gaps.is_empty() {
            violations.push(format!(
                "unresolved consensus ProofPlan gaps cannot support production artifacts: {}",
                blocking_consensus_gaps.join(", ")
            ));
        }
    }

    if args.deny_ckb_runtime && metadata.runtime.ckb_runtime_required {
        violations.push(format!("CKB runtime features: {}", metadata.runtime.ckb_runtime_features.join(", ")));
    }

    if args.deny_runtime_obligations {
        let runtime_required_obligations = metadata
            .runtime
            .verifier_obligations
            .iter()
            .filter(|obligation| obligation.status == "runtime-required")
            .map(|obligation| format!("{}:{} ({})", obligation.scope, obligation.feature, obligation.category))
            .collect::<Vec<_>>();
        if !runtime_required_obligations.is_empty() {
            violations.push(format!("runtime-required verifier obligations: {}", runtime_required_obligations.join(", ")));
        }

        let runtime_required_proof_plan = metadata
            .runtime
            .proof_plan
            .iter()
            .filter(|plan| plan.status == "runtime-required" && !plan.on_chain_checked)
            .map(|plan| format!("{}:{} ({})", plan.origin, plan.feature, plan.codegen_coverage_status))
            .collect::<Vec<_>>();
        if !runtime_required_proof_plan.is_empty() {
            violations.push(format!("runtime-required ProofPlan gaps: {}", runtime_required_proof_plan.join(", ")));
        }

        let transaction_invariants = transaction_invariant_checked_subcondition_summaries(metadata);
        if !transaction_invariants.is_empty() {
            violations.push(format!(
                "runtime-required transaction invariants with checked subconditions: {}",
                transaction_invariants.join(", ")
            ));
        }

        let transaction_runtime_inputs = transaction_runtime_input_requirement_summaries_by_status(metadata, "runtime-required");
        if !transaction_runtime_inputs.is_empty() {
            violations
                .push(format!("runtime-required transaction runtime input requirements: {}", transaction_runtime_inputs.join(", ")));
        }

        let transaction_runtime_input_blockers = transaction_runtime_input_blocker_summaries_by_status(metadata, "runtime-required");
        if !transaction_runtime_input_blockers.is_empty() {
            violations.push(format!(
                "runtime-required transaction runtime input blockers: {}",
                transaction_runtime_input_blockers.join(", ")
            ));
        }

        let transaction_runtime_input_blocker_classes =
            transaction_runtime_input_blocker_class_summaries_by_status(metadata, "runtime-required");
        if !transaction_runtime_input_blocker_classes.is_empty() {
            violations.push(format!(
                "runtime-required transaction runtime input blocker classes: {}",
                transaction_runtime_input_blocker_classes.join(", ")
            ));
        }

        let runtime_required_pool_invariants = pool_invariant_family_summaries(metadata, "runtime-required");
        if !runtime_required_pool_invariants.is_empty() {
            violations.push(format!("runtime-required Pool invariant families: {}", runtime_required_pool_invariants.join(", ")));
        }

        let runtime_required_pool_blocker_classes = pool_invariant_family_blocker_class_summaries(metadata, "runtime-required");
        if !runtime_required_pool_blocker_classes.is_empty() {
            violations.push(format!(
                "runtime-required Pool invariant blocker classes: {}",
                runtime_required_pool_blocker_classes.join(", ")
            ));
        }

        let pool_runtime_inputs = pool_runtime_input_requirement_summaries(metadata);
        if !pool_runtime_inputs.is_empty() {
            violations.push(format!("runtime-required Pool runtime input requirements: {}", pool_runtime_inputs.join(", ")));
        }
    }

    if violations.is_empty() {
        return Ok(());
    }

    Err(crate::error::CompileError::without_span(format!("check policy failed:\n  - {}", violations.join("\n  - "))))
}

pub(super) fn executable_surface_policy(args: &CheckArgs) -> ExecutableSurfacePolicy {
    if args.production || args.deny_fail_closed {
        ExecutableSurfacePolicy::DenyFailClosed
    } else {
        ExecutableSurfacePolicy::AllowFailClosed
    }
}

pub(super) fn target_profile_policy_violations(
    metadata: &crate::CompileMetadata,
    artifact_format: ArtifactFormat,
    profile: TargetProfile,
) -> Vec<String> {
    match profile {
        TargetProfile::Ckb => ckb_target_profile_policy_violations(metadata, artifact_format),
        TargetProfile::CkbTypeHash => ckb_target_profile_policy_violations(metadata, artifact_format),
    }
}

fn ckb_target_profile_policy_violations(_metadata: &crate::CompileMetadata, _artifact_format: ArtifactFormat) -> Vec<String> {
    Vec::new()
}

pub(super) fn runtime_required_obligation_count(metadata: &crate::CompileMetadata) -> usize {
    metadata.runtime.verifier_obligations.iter().filter(|obligation| obligation.status == "runtime-required").count()
}

pub(super) fn fail_closed_obligation_count(metadata: &crate::CompileMetadata) -> usize {
    metadata.runtime.verifier_obligations.iter().filter(|obligation| obligation.status == "fail-closed").count()
}

pub(super) fn runtime_required_transaction_invariant_count(metadata: &crate::CompileMetadata) -> usize {
    metadata
        .runtime
        .verifier_obligations
        .iter()
        .filter(|obligation| obligation.category == "transaction-invariant" && obligation.status == "runtime-required")
        .count()
}

pub(super) fn runtime_required_transaction_invariant_checked_subcondition_count(metadata: &crate::CompileMetadata) -> usize {
    metadata
        .runtime
        .verifier_obligations
        .iter()
        .filter(|obligation| obligation.category == "transaction-invariant" && obligation.status == "runtime-required")
        .map(|obligation| checked_runtime_subconditions(&obligation.detail).len())
        .sum()
}

pub(super) fn transaction_invariant_checked_subcondition_summaries(metadata: &crate::CompileMetadata) -> Vec<String> {
    metadata
        .runtime
        .verifier_obligations
        .iter()
        .filter(|obligation| obligation.category == "transaction-invariant" && obligation.status == "runtime-required")
        .filter_map(|obligation| {
            let subconditions = checked_runtime_subconditions(&obligation.detail);
            if subconditions.is_empty() {
                None
            } else {
                Some(format!("{}:{} checked=[{}]", obligation.scope, obligation.feature, subconditions.join(",")))
            }
        })
        .collect()
}

pub(super) fn transaction_runtime_input_requirement_count(metadata: &crate::CompileMetadata) -> usize {
    metadata.runtime.transaction_runtime_input_requirements.len()
}

pub(super) fn transaction_runtime_input_requirement_count_by_status(metadata: &crate::CompileMetadata, status: &str) -> usize {
    metadata.runtime.transaction_runtime_input_requirements.iter().filter(|requirement| requirement.status == status).count()
}

pub(super) fn transaction_runtime_input_requirement_summaries(metadata: &crate::CompileMetadata) -> Vec<String> {
    metadata.runtime.transaction_runtime_input_requirements.iter().map(transaction_runtime_input_requirement_summary).collect()
}

pub(super) fn transaction_runtime_input_requirement_summaries_by_status(
    metadata: &crate::CompileMetadata,
    status: &str,
) -> Vec<String> {
    metadata
        .runtime
        .transaction_runtime_input_requirements
        .iter()
        .filter(|requirement| requirement.status == status)
        .map(transaction_runtime_input_requirement_summary)
        .collect()
}

pub(super) fn transaction_runtime_input_blocker_count_by_status(metadata: &crate::CompileMetadata, status: &str) -> usize {
    transaction_runtime_input_blocker_summaries_by_status(metadata, status).len()
}

pub(super) fn transaction_runtime_input_blocker_summaries_by_status(metadata: &crate::CompileMetadata, status: &str) -> Vec<String> {
    metadata
        .runtime
        .transaction_runtime_input_requirements
        .iter()
        .filter(|requirement| requirement.status == status)
        .filter_map(|requirement| {
            requirement.blocker.as_deref().map(|blocker| {
                let blocker_class = requirement
                    .blocker_class
                    .as_deref()
                    .map(|blocker_class| format!(" blocker_class={}", blocker_class))
                    .unwrap_or_default();
                format!("{}:{}:{} blocker={}{}", requirement.scope, requirement.feature, requirement.component, blocker, blocker_class)
            })
        })
        .collect()
}

pub(super) fn transaction_runtime_input_blocker_class_count_by_status(metadata: &crate::CompileMetadata, status: &str) -> usize {
    transaction_runtime_input_blocker_class_summaries_by_status(metadata, status).len()
}

pub(super) fn transaction_runtime_input_blocker_class_summaries_by_status(
    metadata: &crate::CompileMetadata,
    status: &str,
) -> Vec<String> {
    metadata
        .runtime
        .transaction_runtime_input_requirements
        .iter()
        .filter(|requirement| requirement.status == status)
        .filter_map(|requirement| {
            requirement.blocker_class.as_deref().map(|blocker_class| {
                format!("{}:{}:{} blocker_class={}", requirement.scope, requirement.feature, requirement.component, blocker_class)
            })
        })
        .collect()
}

fn transaction_runtime_input_requirement_summary(requirement: &crate::TransactionRuntimeInputRequirementMetadata) -> String {
    let field = requirement.field.as_deref().map(|field| format!(".{}", field)).unwrap_or_default();
    let bytes = requirement.byte_len.map(|byte_len| format!("[{}]", byte_len)).unwrap_or_default();
    let blocker = requirement.blocker.as_deref().map(|blocker| format!(" blocker={}", blocker)).unwrap_or_default();
    let blocker_class = requirement.blocker_class.as_deref().map(|class| format!(" blocker_class={}", class)).unwrap_or_default();
    format!(
        "{}:{}:{}={}:{}{}:{}{} ({}){}{}",
        requirement.scope,
        requirement.feature,
        requirement.component,
        requirement.source,
        requirement.binding,
        field,
        requirement.abi,
        bytes,
        requirement.status,
        blocker,
        blocker_class
    )
}

fn checked_runtime_subconditions(detail: &str) -> Vec<String> {
    detail
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .filter_map(|part| part.trim().strip_suffix("=checked-runtime"))
        .map(|name| name.trim_matches(|ch: char| ch == '`' || ch == '.' || ch == ':').to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

pub(super) fn checked_pool_invariant_family_count(metadata: &crate::CompileMetadata) -> usize {
    pool_invariant_family_summaries(metadata, "checked-runtime").len()
}

pub(super) fn runtime_required_pool_invariant_family_count(metadata: &crate::CompileMetadata) -> usize {
    pool_invariant_family_summaries(metadata, "runtime-required").len()
}

pub(super) fn pool_runtime_input_requirement_count(metadata: &crate::CompileMetadata) -> usize {
    metadata.runtime.pool_primitives.iter().map(|primitive| primitive.runtime_input_requirements.len()).sum()
}

pub(super) fn pool_runtime_input_requirement_summaries(metadata: &crate::CompileMetadata) -> Vec<String> {
    metadata
        .runtime
        .pool_primitives
        .iter()
        .flat_map(|primitive| {
            primitive.runtime_input_requirements.iter().map(move |requirement| {
                let field = requirement.field.as_deref().map(|field| format!(".{}", field)).unwrap_or_default();
                let blocker = requirement.blocker.as_deref().map(|blocker| format!(" blocker={}", blocker)).unwrap_or_default();
                let blocker_class =
                    requirement.blocker_class.as_deref().map(|class| format!(" blocker_class={}", class)).unwrap_or_default();
                format!(
                    "{}:{}:{}={}#{}:{}{}:{}[{}]{}{}",
                    primitive.scope,
                    primitive.feature,
                    requirement.component,
                    requirement.source,
                    requirement.index,
                    requirement.binding,
                    field,
                    requirement.abi,
                    requirement.byte_len,
                    blocker,
                    blocker_class
                )
            })
        })
        .collect()
}

fn pool_invariant_family_summaries(metadata: &crate::CompileMetadata, status: &str) -> Vec<String> {
    metadata
        .runtime
        .pool_primitives
        .iter()
        .flat_map(|primitive| {
            primitive.invariant_families.iter().filter(move |family| family.status == status).map(move |family| {
                let blocker = family.blocker.as_deref().map(|blocker| format!(" blocker={}", blocker)).unwrap_or_default();
                let blocker_class =
                    family.blocker_class.as_deref().map(|class| format!(" blocker_class={}", class)).unwrap_or_default();
                format!("{}:{}:{} ({}){}{}", primitive.scope, primitive.feature, family.name, family.source, blocker, blocker_class)
            })
        })
        .collect()
}

pub(super) fn pool_invariant_family_blocker_class_count(metadata: &crate::CompileMetadata, status: &str) -> usize {
    pool_invariant_family_blocker_class_summaries(metadata, status).len()
}

pub(super) fn pool_invariant_family_blocker_class_summaries(metadata: &crate::CompileMetadata, status: &str) -> Vec<String> {
    metadata
        .runtime
        .pool_primitives
        .iter()
        .flat_map(|primitive| {
            primitive.invariant_families.iter().filter(move |family| family.status == status).filter_map(move |family| {
                family.blocker_class.as_deref().map(|blocker_class| {
                    format!("{}:{}:{} blocker_class={}", primitive.scope, primitive.feature, family.name, blocker_class)
                })
            })
        })
        .collect()
}
