//! Source-bound schema reviews for a transactional package upgrade.

use super::{
    compile_environment_scope, confined_lock_path, find_manifest_root, inspection_build_unit, locked_build_info,
    optional_compile_evidence, package_roots, relative_lock_path, resolution_options, review_environments, RootCandidate,
    UpgradeDiagnostic, UpgradeOptions, UpgradePlan, UpgradeSelection,
};
use crate::error::{CompileError, Result};
use crate::package::{DependencyScope, Lockfile, PackageManager};
use crate::schema_acknowledgement::{
    bind_schema_review_context, build_optional_schema_change_plans, verify_schema_acknowledgement, SchemaAcknowledgementReceipt,
    SchemaChangePlan, SchemaReviewContext,
};
use crate::CompileResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpgradeSchemaAcknowledgement {
    pub subject: String,
    pub environment: Option<String>,
    pub receipt: SchemaAcknowledgementReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpgradeSourceBaseline {
    pub member: String,
    pub root: String,
    pub source_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpgradeSchemaReview {
    pub subject: String,
    pub environment: Option<String>,
    pub plan: SchemaChangePlan,
    pub acknowledgement: Option<SchemaAcknowledgementReceipt>,
}

pub(super) fn source_baseline(candidate: &RootCandidate, options: &UpgradeOptions) -> Result<UpgradeSourceBaseline> {
    let path = options.schema_baselines.get(&candidate.member).unwrap_or(&candidate.root);
    let root = std::fs::canonicalize(find_manifest_root(path)?)?;
    let source_hash = crate::package::registry::compute_source_hash(&root)?;
    let expected = candidate.old.package.source_hash.as_deref().ok_or_else(|| {
        schema_error(format!(
            "member '{}' old lock has no source baseline hash; a schema review cannot establish its old source",
            candidate.member
        ))
    })?;
    if source_hash != expected {
        return Err(schema_error(format!(
            "member '{}' old source baseline does not match its authoritative Cell.lock; preserve the old package and use --schema-baseline {}=PATH",
            candidate.member, candidate.member
        )));
    }
    let bytes = std::fs::read(root.join("Cell.lock"))?;
    if bytes != candidate.old_content.as_bytes() {
        return Err(schema_error(format!(
            "member '{}' source baseline Cell.lock differs from the authoritative old graph",
            candidate.member
        )));
    }
    Ok(UpgradeSourceBaseline { member: candidate.member.clone(), root: root.to_string_lossy().replace('\\', "/"), source_hash })
}

pub(super) fn reviews(
    member: &str,
    environment: &Option<String>,
    old: Option<&CompileResult>,
    new: Option<&CompileResult>,
    old_graph_hash: &str,
    new_graph_hash: &str,
    acknowledgements: &[UpgradeSchemaAcknowledgement],
) -> Result<Vec<UpgradeSchemaReview>> {
    build_optional_schema_change_plans(old, new)?
        .into_iter()
        .map(|plan| {
            let plan = bind_schema_review_context(
                plan,
                SchemaReviewContext {
                    subject: member.to_string(),
                    environment: environment.clone(),
                    old_graph_hash: old_graph_hash.to_string(),
                    new_graph_hash: new_graph_hash.to_string(),
                },
            )?;
            let matching = acknowledgements
                .iter()
                .filter(|supplied| {
                    supplied.subject == member
                        && &supplied.environment == environment
                        && supplied.receipt.module == plan.module
                        && supplied.receipt.selector == plan.selector
                })
                .collect::<Vec<_>>();
            let acknowledgement = match matching.as_slice() {
                [] => None,
                [supplied] => {
                    verify_schema_acknowledgement(&plan, &supplied.receipt)?;
                    Some(supplied.receipt.clone())
                }
                _ => {
                    return Err(schema_error(format!(
                        "member '{member}' has duplicate schema acknowledgements for action '{}'",
                        plan.selector.action
                    )))
                }
            };
            Ok(UpgradeSchemaReview { subject: member.to_string(), environment: environment.clone(), plan, acknowledgement })
        })
        .collect()
}

pub(super) fn review_diagnostics(
    reviews: &[UpgradeSchemaReview],
    diagnostics: &mut Vec<UpgradeDiagnostic>,
    migration_artifacts: &mut BTreeSet<String>,
) {
    for review in reviews {
        for blocker in &review.plan.blockers {
            diagnostics.push(UpgradeDiagnostic {
                code: blocker.code.clone(),
                severity: "error".to_string(),
                member: Some(review.subject.clone()),
                message: format!(
                    "action '{}' replace {} -> {}: {}",
                    review.plan.selector.action, review.plan.selector.before, review.plan.selector.after, blocker.message
                ),
                acknowledgeable: false,
            });
        }
        if review.plan.state_migration_required {
            migration_artifacts.insert(format!("{}:state-migration-plan", review.subject));
        }
        if review.plan.requires_acknowledgement && review.acknowledgement.is_none() {
            diagnostics.push(UpgradeDiagnostic {
                code: "UPG3201".to_string(), severity: "error".to_string(), member: Some(review.subject.clone()),
                message: format!("schema review for action '{}' replace {} -> {} requires a verified schema acknowledgement; use schema-ack --upgrade-plan and regenerate this plan with --schema-ack",
                    review.plan.selector.action, review.plan.selector.before, review.plan.selector.after),
                acknowledgeable: false,
            });
        }
    }
}

pub(super) fn validate_supplied_acknowledgements(
    supplied: &[UpgradeSchemaAcknowledgement],
    reviews: &[UpgradeSchemaReview],
) -> Result<()> {
    for supplied in supplied {
        if !reviews.iter().any(|review| {
            review.subject == supplied.subject
                && review.environment == supplied.environment
                && review.acknowledgement.as_ref() == Some(&supplied.receipt)
        }) {
            return Err(schema_error(format!(
                "schema acknowledgement for member '{}' is stale, unused, or belongs to a different environment/relation",
                supplied.subject
            )));
        }
    }
    Ok(())
}

/// Recompile the exact planned old/new graphs before any lock is written.
/// Plan hashes are integrity checks, not authority to invent schema coverage.
pub(super) fn revalidate(plan: &UpgradePlan, root: &Path) -> Result<()> {
    let expected_locks = package_roots(root)?
        .into_iter()
        .filter(|package| package.join("Cell.lock").exists())
        .map(|package| relative_lock_path(root, &package))
        .collect::<Result<BTreeSet<_>>>()?;
    let actual_locks = plan.locks.iter().map(|transaction| transaction.lock_path.clone()).collect::<BTreeSet<_>>();
    if expected_locks != actual_locks
        || plan.locks.len() != actual_locks.len()
        || plan.reverse_dependents.iter().any(|impact| !plan.locks.iter().any(|transaction| transaction.member == impact.member))
    {
        return Err(schema_error("upgrade plan package graph coverage differs from the current workspace"));
    }
    let mut expected_reviews = Vec::new();
    let mut dependency_reviews = super::dependency_review::DependencyReviewState::default();
    let supplied = plan
        .schema_reviews
        .iter()
        .filter_map(|review| {
            review.acknowledgement.clone().map(|receipt| UpgradeSchemaAcknowledgement {
                subject: review.subject.clone(),
                environment: review.environment.clone(),
                receipt,
            })
        })
        .collect::<Vec<_>>();
    if plan.source_baselines.len() != plan.locks.len() {
        return Err(schema_error("upgrade plan does not bind exactly one old source baseline per lock"));
    }
    let mut seen_baselines = BTreeSet::new();
    for transaction in &plan.locks {
        let path = confined_lock_path(root, &transaction.lock_path)?;
        let package_root = path.parent().unwrap();
        let old_lock: Lockfile = toml::from_str(&transaction.old_lock_content)?;
        let new_lock: Lockfile = toml::from_str(&transaction.new_lock_content)?;
        if serde_json::to_value(&old_lock)? != serde_json::to_value(&transaction.old_lock)?
            || serde_json::to_value(&new_lock)? != serde_json::to_value(&transaction.new_lock)?
        {
            return Err(schema_error("upgrade plan embedded lock records differ from their bound lockfile contents"));
        }
        if PackageManager::new(package_root).read_manifest()?.package.name != transaction.member {
            return Err(schema_error("upgrade plan lock member does not match the current package identity"));
        }
        let baseline = plan
            .source_baselines
            .iter()
            .find(|baseline| baseline.member == transaction.member)
            .ok_or_else(|| schema_error(format!("member '{}' has no old source baseline", transaction.member)))?;
        if !seen_baselines.insert(baseline.member.clone()) {
            return Err(schema_error("upgrade plan repeats a source baseline member"));
        }
        let baseline_root = std::fs::canonicalize(&baseline.root)?;
        if crate::package::registry::compute_source_hash(&baseline_root)? != baseline.source_hash
            || transaction.old_lock.package.source_hash.as_deref() != Some(&baseline.source_hash)
            || std::fs::read(baseline_root.join("Cell.lock"))? != transaction.old_lock_content.as_bytes()
        {
            return Err(schema_error(format!("member '{}' old source baseline or graph changed after review", transaction.member)));
        }
        if crate::package::registry::compute_source_hash(package_root)?
            != transaction.new_lock.package.source_hash.clone().unwrap_or_default()
        {
            return Err(schema_error(format!(
                "member '{}' candidate source changed after the upgrade plan was reviewed",
                transaction.member
            )));
        }
        let impacts = plan.reverse_dependents.iter().filter(|impact| impact.member == transaction.member).collect::<Vec<_>>();
        if impacts.is_empty() {
            return Err(schema_error(format!("member '{}' has no reverse-dependent compilation coverage", transaction.member)));
        }
        let manifest = PackageManager::new(package_root).read_manifest()?;
        let options = options_from_selection(&plan.selection)?;
        let old_manifest = PackageManager::new(&baseline_root).read_manifest()?;
        let scopes = review_environments(Some(&old_manifest), &transaction.old_lock, &manifest)
            .into_iter()
            .map(|scope| (scope.name.clone(), scope))
            .collect::<std::collections::BTreeMap<_, _>>();
        let expected_environments = scopes.keys().cloned().collect::<BTreeSet<_>>();
        let actual_environments = impacts.iter().map(|impact| impact.environment.clone()).collect::<BTreeSet<_>>();
        if actual_environments != expected_environments || impacts.len() != actual_environments.len() {
            return Err(schema_error(format!(
                "member '{}' reverse-dependent environment union coverage is incomplete or duplicated",
                transaction.member
            )));
        }
        let mut primary_build = None;
        for impact in impacts {
            let resolution = resolution_options(&options, impact.environment.clone());
            let scope = &scopes[&impact.environment];
            if scope.transition != impact.environment_transition {
                return Err(schema_error("upgrade plan environment transition does not match the actual old/new manifests"));
            }
            let mut requested_inspection = resolution.clone();
            requested_inspection.offline = plan.selection.offline;
            let old_unit = scope
                .old_present
                .then(|| inspection_build_unit(&baseline_root, &transaction.old_lock, &requested_inspection))
                .transpose()?;
            let new_unit = scope
                .new_present
                .then(|| inspection_build_unit(package_root, &transaction.new_lock, &requested_inspection))
                .transpose()?;
            if old_unit.as_ref().map(|unit| &unit.id) != impact.old_build_unit_id.as_ref()
                || new_unit.as_ref().map(|unit| &unit.id) != impact.new_build_unit_id.as_ref()
                || stable_unit(old_unit.as_ref())? != stable_unit(impact.old_build_unit.as_ref())?
                || stable_unit(new_unit.as_ref())? != stable_unit(impact.new_build_unit.as_ref())?
            {
                return Err(schema_error("upgrade plan build-unit inputs differ from the final bound old/new lock graphs"));
            }
            let old = compile_environment_scope(&baseline_root, &transaction.old_lock, &resolution, scope.old_present)?;
            let new = compile_environment_scope(package_root, &transaction.new_lock, &resolution, scope.new_present)?;
            if primary_build.is_none()
                && let Some(compiled) = &new
            {
                primary_build = Some((locked_build_info(&compiled.metadata)?, compiled.metadata.source_hash.clone()));
            }
            dependency_reviews.collect(
                package_root,
                &baseline_root,
                scope.old_present.then_some(&transaction.old_lock),
                scope.new_present.then_some(&transaction.new_lock),
                &resolution,
                &transaction.member,
                &supplied,
            )?;
            if optional_compile_evidence(old.as_ref(), "absent-new-environment") != impact.old
                || optional_compile_evidence(new.as_ref(), "absent-retired-environment") != impact.new
            {
                return Err(schema_error(format!("member '{}' compiled evidence changed after upgrade review", transaction.member)));
            }
            expected_reviews.extend(reviews(
                &transaction.member,
                &impact.environment,
                old.as_ref(),
                new.as_ref(),
                &graph_hash(&transaction.old_lock, package_root)?,
                &graph_hash(&transaction.new_lock, package_root)?,
                &supplied,
            )?);
        }
        if let Some((build, source_hash)) = primary_build {
            if serde_json::to_value(transaction.new_lock.package_build.as_ref())? != serde_json::to_value(Some(&build))?
                || transaction.new_lock.package.compiler_source_hash != source_hash
            {
                return Err(schema_error("upgrade plan derived package build evidence differs from the actual candidate compilation"));
            }
        } else if transaction.new_lock.package_build.is_some() {
            return Err(schema_error("upgrade plan invents build evidence for an absent candidate compilation"));
        }
    }
    let (dependency_impacts, dependency_owned_reviews) = dependency_reviews.finish();
    if dependency_impacts != plan.dependency_schema_impacts {
        return Err(schema_error("upgrade plan dependency-owned compilation/schema coverage differs from the actual locked graphs"));
    }
    expected_reviews.extend(dependency_owned_reviews);
    expected_reviews.sort_by(|left, right| {
        (&left.subject, &left.environment, &left.plan.selector).cmp(&(&right.subject, &right.environment, &right.plan.selector))
    });
    if expected_reviews != plan.schema_reviews {
        return Err(schema_error("upgrade plan schema review coverage differs from the actual old/new package graphs"));
    }
    let mut diagnostics = Vec::new();
    review_diagnostics(&expected_reviews, &mut diagnostics, &mut BTreeSet::new());
    if !diagnostics.is_empty() {
        return Err(schema_error("upgrade plan has missing schema acknowledgements or blocking field treatments"));
    }
    Ok(())
}

fn options_from_selection(selection: &UpgradeSelection) -> Result<UpgradeOptions> {
    Ok(UpgradeOptions {
        scope: match selection.scope.as_str() {
            "runtime" => DependencyScope::Runtime,
            "test" => DependencyScope::Test,
            _ => return Err(schema_error("upgrade plan has an unsupported dependency scope")),
        },
        features: selection.features.iter().cloned().collect(),
        all_features: selection.all_features,
        no_default_features: !selection.default_features,
        environment: selection.environment.clone(),
        // Apply materializes exact locked sources and must never select versions
        // or reach the network merely to replay a reviewed schema comparison.
        offline: true,
        ..UpgradeOptions::default()
    })
}

fn schema_error(message: impl Into<String>) -> CompileError {
    CompileError::without_span(message.into()).with_code(crate::error::DiagnosticCode::E2802)
}

/// Build evidence is an output of planning, not source-selection authority.
/// Bind the source/manifest/selection graph while omitting derived build caches.
pub(super) fn graph_hash(lock: &Lockfile, authority_root: &Path) -> Result<String> {
    let mut graph = lock.clone();
    graph.package_build = None;
    graph.package.compiler_source_hash = None;
    for dependency in graph.dependencies.values_mut() {
        dependency.build = None;
    }
    let authority_root = std::fs::canonicalize(authority_root)?.to_string_lossy().replace('\\', "/");
    Ok(super::sha256(&serde_json::to_vec(&serde_json::json!({
        "domain": "cellscript-upgrade-root-source-graph-v2", "authority_root": authority_root, "graph": graph,
    }))?))
}

fn stable_unit(unit: Option<&crate::package::inspection::BuildUnit>) -> Result<serde_json::Value> {
    let mut value = serde_json::to_value(unit)?;
    if let Some(cache) = value.get_mut("cache").and_then(serde_json::Value::as_object_mut) {
        // Cache presence is an observation; its source/key inputs are binding.
        cache.remove("status");
        cache.remove("rebuild_reason");
    }
    Ok(value)
}
