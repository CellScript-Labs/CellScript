//! Focused review receipts for `same except` schema evolution.
//!
//! A receipt is off-chain review evidence. It binds one old/new schema pair
//! and one canonical authoring relation, but it does not authorize a package,
//! deployment, or on-chain migration. Newly added fields must be assigned
//! explicitly before an acknowledgement can be created; implicit preservation
//! remains a blocking migration decision.

mod checked;
pub(crate) use checked::SchemaReviewCompilation;

use crate::ast::{ActionDef, Item, ReplaceDataTreatment, ReplaceRelation, Stmt};
use crate::error::{CompileError, Result};
use crate::CompileResult;
use cellscript_artifact_checker::canonical_hash;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_CHANGE_PLAN_SCHEMA: &str = "cellscript-schema-change-plan-v2";
pub const SCHEMA_ACKNOWLEDGEMENT_SCHEMA: &str = "cellscript-schema-acknowledgement-v2";
const RELATION_IDENTITY_DOMAIN: &str = "cellscript-authoring-relation-identity-v2";
const PLAN_HASH_DOMAIN: &str = "cellscript-schema-change-plan-hash-v2";
const ACKNOWLEDGEMENT_HASH_DOMAIN: &str = "cellscript-schema-acknowledgement-hash-v2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct SchemaRelationSelector {
    pub action: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaFieldPolicy {
    pub treatment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
}

/// The exact package-graph comparison that consumes a review. Standalone
/// source comparisons omit this; upgrade receipts bind it explicitly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaReviewContext {
    pub subject: String,
    pub environment: Option<String>,
    pub old_graph_hash: String,
    pub new_graph_hash: String,
}

/// One source occurrence in a relation family. Paths use structural statement
/// and expression child indices, so formatting does not renumber branches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaRelationOccurrence {
    pub path: Vec<usize>,
    pub fields: BTreeMap<String, SchemaFieldPolicy>,
    pub lock: String,
    pub capacity: String,
    pub type_script: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaFieldChange {
    pub field: String,
    pub classification: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_type: Option<String>,
    pub old_type_identity: Option<String>,
    pub new_type_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_policy: Option<SchemaFieldPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_policy: Option<SchemaFieldPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaAcknowledgementBlocker {
    pub code: String,
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaChangePlan {
    pub schema: String,
    pub version: u32,
    pub module: String,
    pub old_module: Option<String>,
    pub new_module: Option<String>,
    pub selector: SchemaRelationSelector,
    pub context: Option<SchemaReviewContext>,
    pub resource_type: String,
    pub old_resource_type: Option<String>,
    pub new_resource_type: Option<String>,
    pub old_interface_hash: String,
    pub new_interface_hash: String,
    pub old_source_identity: Option<String>,
    pub new_source_identity: Option<String>,
    pub old_schema_identity: String,
    pub new_schema_identity: String,
    pub old_relation_identity: String,
    pub new_relation_identity: String,
    pub old_occurrences: Vec<SchemaRelationOccurrence>,
    pub new_occurrences: Vec<SchemaRelationOccurrence>,
    pub old_action_source: Option<String>,
    pub new_action_source: Option<String>,
    pub requires_acknowledgement: bool,
    pub state_migration_required: bool,
    pub field_changes: Vec<SchemaFieldChange>,
    pub blockers: Vec<SchemaAcknowledgementBlocker>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaAcknowledgementReceipt {
    pub schema: String,
    pub version: u32,
    pub plan_hash: String,
    pub module: String,
    pub old_module: Option<String>,
    pub new_module: Option<String>,
    pub selector: SchemaRelationSelector,
    pub context: Option<SchemaReviewContext>,
    pub resource_type: String,
    pub old_resource_type: Option<String>,
    pub new_resource_type: Option<String>,
    pub old_source_identity: Option<String>,
    pub new_source_identity: Option<String>,
    pub old_schema_identity: String,
    pub new_schema_identity: String,
    pub old_relation_identity: String,
    pub new_relation_identity: String,
    pub old_occurrences: Vec<SchemaRelationOccurrence>,
    pub new_occurrences: Vec<SchemaRelationOccurrence>,
    pub review_policy: String,
    pub reviewer: String,
    pub rationale: String,
    pub field_changes: Vec<SchemaFieldChange>,
    pub acknowledgement_hash: String,
}

#[derive(Debug, Clone, Serialize)]
struct RelationIdentity<'a> {
    action_source: &'a Option<String>,
    before: &'a str,
    after: &'a str,
    occurrences: &'a [SchemaRelationOccurrence],
}

#[derive(Debug, Clone, Serialize)]
struct PlanHashPayload<'a> {
    schema: &'a str,
    version: u32,
    module: &'a str,
    old_module: &'a Option<String>,
    new_module: &'a Option<String>,
    selector: &'a SchemaRelationSelector,
    context: &'a Option<SchemaReviewContext>,
    resource_type: &'a str,
    old_resource_type: &'a Option<String>,
    new_resource_type: &'a Option<String>,
    old_interface_hash: &'a str,
    new_interface_hash: &'a str,
    old_source_identity: &'a Option<String>,
    new_source_identity: &'a Option<String>,
    old_schema_identity: &'a str,
    new_schema_identity: &'a str,
    old_relation_identity: &'a str,
    new_relation_identity: &'a str,
    old_occurrences: &'a [SchemaRelationOccurrence],
    new_occurrences: &'a [SchemaRelationOccurrence],
    old_action_source: &'a Option<String>,
    new_action_source: &'a Option<String>,
    requires_acknowledgement: bool,
    state_migration_required: bool,
    field_changes: &'a [SchemaFieldChange],
    blockers: &'a [SchemaAcknowledgementBlocker],
}

#[derive(Debug, Clone, Serialize)]
struct AcknowledgementHashPayload<'a> {
    schema: &'a str,
    version: u32,
    plan_hash: &'a str,
    module: &'a str,
    old_module: &'a Option<String>,
    new_module: &'a Option<String>,
    selector: &'a SchemaRelationSelector,
    context: &'a Option<SchemaReviewContext>,
    resource_type: &'a str,
    old_resource_type: &'a Option<String>,
    new_resource_type: &'a Option<String>,
    old_source_identity: &'a Option<String>,
    new_source_identity: &'a Option<String>,
    old_schema_identity: &'a str,
    new_schema_identity: &'a str,
    old_relation_identity: &'a str,
    new_relation_identity: &'a str,
    old_occurrences: &'a [SchemaRelationOccurrence],
    new_occurrences: &'a [SchemaRelationOccurrence],
    review_policy: &'a str,
    reviewer: &'a str,
    rationale: &'a str,
    field_changes: &'a [SchemaFieldChange],
}

pub fn build_schema_change_plan(
    old: &CompileResult,
    new: &CompileResult,
    selector: SchemaRelationSelector,
) -> Result<SchemaChangePlan> {
    build_optional_schema_change_plan(Some(old), Some(new), selector)
}

pub(crate) fn build_optional_schema_change_plan(
    old: Option<&CompileResult>,
    new: Option<&CompileResult>,
    selector: SchemaRelationSelector,
) -> Result<SchemaChangePlan> {
    if let (Some(old), Some(new)) = (old, new)
        && old.metadata.module != new.metadata.module
    {
        return Err(CompileError::without_span("schema acknowledgement compares one exact module identity"));
    }
    let module =
        old.or(new).ok_or_else(|| CompileError::without_span("schema comparison has no package candidate"))?.metadata.module.clone();
    let old_catalog = old.map(compilation_catalog).transpose()?;
    let new_catalog = new.map(compilation_catalog).transpose()?;
    let old_action = old.and_then(|result| find_action(&result.ast, &selector.action));
    let new_action = new.and_then(|result| find_action(&result.ast, &selector.action));
    let old_relations = old_action.map(|action| find_relations(action, &selector)).unwrap_or_default();
    let new_relations = new_action.map(|action| find_relations(action, &selector)).unwrap_or_default();
    if old_relations.is_empty() && new_relations.is_empty() {
        return Err(CompileError::without_span("neither candidate contains the selected replace relation"));
    }
    let old_role_type =
        (!old_relations.is_empty()).then(|| relation_resource_type(old_catalog.unwrap(), &selector, "old")).transpose()?;
    let new_role_type =
        (!new_relations.is_empty()).then(|| relation_resource_type(new_catalog.unwrap(), &selector, "new")).transpose()?;
    let resource_type = new_role_type.as_ref().or(old_role_type.as_ref()).unwrap().clone();
    let old_resource_type = if let Some(name) = old_role_type {
        Some(name)
    } else {
        old_catalog.zip(new_catalog).map(|(old, new)| old.equivalent_schema(new, &resource_type)).transpose()?.flatten()
    };
    let new_resource_type = if let Some(name) = new_role_type {
        Some(name)
    } else {
        new_catalog.zip(old_catalog).map(|(new, old)| new.equivalent_schema(old, &resource_type)).transpose()?.flatten()
    };
    let old_schema = old.zip(old_resource_type.as_deref()).and_then(|(result, resource)| typed_schema(result, resource));
    let new_schema = new.zip(new_resource_type.as_deref()).and_then(|(result, resource)| typed_schema(result, resource));
    if (old_resource_type.is_some() && old_schema.is_none()) || (new_resource_type.is_some() && new_schema.is_none()) {
        return Err(CompileError::without_span("schema review relation has no concrete checked Cell schema evidence"));
    }
    let old_schema_identity = hash_optional_schema_identity(old_catalog, old_resource_type.as_deref())?;
    let new_schema_identity = hash_optional_schema_identity(new_catalog, new_resource_type.as_deref())?;
    let old_source_identity = old_catalog.map(|catalog| catalog.source_identity.clone());
    let new_source_identity = new_catalog.map(|catalog| catalog.source_identity.clone());
    let old_occurrences = relation_occurrences(&old_relations, old_schema)?;
    let new_occurrences = relation_occurrences(&new_relations, new_schema)?;
    let old_policies = summarized_policies(&old_occurrences);
    let new_policies = summarized_policies(&new_occurrences);
    let old_action_source = old.map(|result| canonical_action(old_action, &result.ast)).transpose()?.flatten();
    let new_action_source = new.map(|result| canonical_action(new_action, &result.ast)).transpose()?.flatten();
    let old_relation_identity = hash_relation_identity(&selector, &old_occurrences, &old_action_source)?;
    let new_relation_identity = hash_relation_identity(&selector, &new_occurrences, &new_action_source)?;
    let old_types = old_catalog
        .zip(old_resource_type.as_deref())
        .map(|(catalog, resource)| catalog.field_types(resource))
        .transpose()?
        .unwrap_or_default();
    let new_types = new_catalog
        .zip(new_resource_type.as_deref())
        .map(|(catalog, resource)| catalog.field_types(resource))
        .transpose()?
        .unwrap_or_default();
    let names = old_types.keys().chain(new_types.keys()).cloned().collect::<BTreeSet<_>>();
    let mut field_changes = Vec::new();
    let mut blockers = Vec::new();
    for name in names {
        let old_type = old_types.get(&name);
        let new_type = new_types.get(&name);
        let root_field = old_type.or(new_type).expect("field-name union contains a bound field").root_field.as_str();
        let old_policy = old_policies.get(root_field);
        let new_policy = new_policies.get(root_field);
        let classification = match (old_type, new_type) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            (Some(old), Some(new)) if old.identity != new.identity => "type-changed",
            _ if old_policy != new_policy => "policy-changed",
            _ => continue,
        };
        if old_schema.is_some()
            && classification == "added"
            && new_occurrences
                .iter()
                .any(|occurrence| occurrence.fields.get(root_field).is_some_and(|policy| policy.treatment == "implicit-preserve"))
        {
            blockers.push(SchemaAcknowledgementBlocker {
                code: "SACK1001".to_string(),
                field: name.to_string(),
                message: format!(
                    "new field '{name}' is implicitly preserved by `same except`; assign it explicitly before acknowledging the schema change"
                ),
            });
        }
        field_changes.push(SchemaFieldChange {
            field: name.to_string(),
            classification: classification.to_string(),
            old_type: old_type.map(|field| field.source_type.clone()),
            new_type: new_type.map(|field| field.source_type.clone()),
            old_type_identity: old_type.map(|field| field.identity.clone()),
            new_type_identity: new_type.map(|field| field.identity.clone()),
            old_policy: old_policy.cloned(),
            new_policy: new_policy.cloned(),
        });
    }
    blockers.sort_by(|left, right| left.field.cmp(&right.field).then(left.code.cmp(&right.code)));
    let requires_acknowledgement = old_schema.is_some() && old_schema_identity != new_schema_identity;
    let state_migration_required = requires_acknowledgement;
    let mut plan = SchemaChangePlan {
        schema: SCHEMA_CHANGE_PLAN_SCHEMA.to_string(),
        version: 2,
        module,
        old_module: old.map(|result| result.metadata.module.clone()),
        new_module: new.map(|result| result.metadata.module.clone()),
        selector,
        context: None,
        resource_type: resource_type.clone(),
        old_resource_type,
        new_resource_type,
        old_interface_hash: old.map(|result| result.metadata.interface_hash.clone()).unwrap_or_else(|| "absent".to_string()),
        new_interface_hash: new.map(|result| result.metadata.interface_hash.clone()).unwrap_or_else(|| "absent".to_string()),
        old_source_identity,
        new_source_identity,
        old_schema_identity,
        new_schema_identity,
        old_relation_identity,
        new_relation_identity,
        old_occurrences,
        new_occurrences,
        old_action_source,
        new_action_source,
        requires_acknowledgement,
        state_migration_required,
        field_changes,
        blockers,
        plan_hash: String::new(),
    };
    plan.plan_hash = plan_hash(&plan)?;
    Ok(plan)
}

pub(crate) fn bind_schema_review_context(mut plan: SchemaChangePlan, context: SchemaReviewContext) -> Result<SchemaChangePlan> {
    validate_plan(&plan)?;
    plan.context = Some(context);
    plan.plan_hash = plan_hash(&plan)?;
    Ok(plan)
}

pub fn acknowledge_schema_change(
    plan: &SchemaChangePlan,
    reviewer: impl Into<String>,
    rationale: impl Into<String>,
) -> Result<SchemaAcknowledgementReceipt> {
    validate_plan(plan)?;
    if !plan.requires_acknowledgement {
        return Err(CompileError::without_span("schema-change plan does not require an acknowledgement"));
    }
    if !plan.blockers.is_empty() {
        return Err(CompileError::without_span(format!(
            "schema-change plan has blocking field treatments: {}",
            plan.blockers.iter().map(|blocker| format!("{} {}", blocker.code, blocker.field)).collect::<Vec<_>>().join(", ")
        )));
    }
    let reviewer = reviewer.into().trim().to_string();
    let rationale = rationale.into().trim().to_string();
    if reviewer.is_empty() || rationale.is_empty() {
        return Err(CompileError::without_span("schema acknowledgement requires a non-empty reviewer and rationale"));
    }
    let mut receipt = SchemaAcknowledgementReceipt {
        schema: SCHEMA_ACKNOWLEDGEMENT_SCHEMA.to_string(),
        version: 2,
        plan_hash: plan.plan_hash.clone(),
        module: plan.module.clone(),
        old_module: plan.old_module.clone(),
        new_module: plan.new_module.clone(),
        selector: plan.selector.clone(),
        context: plan.context.clone(),
        resource_type: plan.resource_type.clone(),
        old_resource_type: plan.old_resource_type.clone(),
        new_resource_type: plan.new_resource_type.clone(),
        old_source_identity: plan.old_source_identity.clone(),
        new_source_identity: plan.new_source_identity.clone(),
        old_schema_identity: plan.old_schema_identity.clone(),
        new_schema_identity: plan.new_schema_identity.clone(),
        old_relation_identity: plan.old_relation_identity.clone(),
        new_relation_identity: plan.new_relation_identity.clone(),
        old_occurrences: plan.old_occurrences.clone(),
        new_occurrences: plan.new_occurrences.clone(),
        review_policy: "reviewed-explicit-field-treatment-delta-v2".to_string(),
        reviewer,
        rationale,
        field_changes: plan.field_changes.clone(),
        acknowledgement_hash: String::new(),
    };
    receipt.acknowledgement_hash = acknowledgement_hash(&receipt)?;
    Ok(receipt)
}

pub fn verify_schema_acknowledgement(plan: &SchemaChangePlan, receipt: &SchemaAcknowledgementReceipt) -> Result<()> {
    validate_plan(plan)?;
    if !plan.requires_acknowledgement || !plan.blockers.is_empty() {
        return Err(CompileError::without_span("current schema-change plan is not eligible for acknowledgement"));
    }
    if receipt.schema != SCHEMA_ACKNOWLEDGEMENT_SCHEMA
        || receipt.version != 2
        || receipt.review_policy != "reviewed-explicit-field-treatment-delta-v2"
    {
        return Err(CompileError::without_span("unsupported schema acknowledgement schema/version/review policy"));
    }
    if receipt.reviewer.trim().is_empty() || receipt.rationale.trim().is_empty() {
        return Err(CompileError::without_span("schema acknowledgement reviewer and rationale must remain non-empty"));
    }
    let expected_hash = acknowledgement_hash(receipt)?;
    if receipt.acknowledgement_hash != expected_hash {
        return Err(CompileError::without_span("schema acknowledgement hash does not match its contents"));
    }
    if receipt.plan_hash != plan.plan_hash
        || receipt.module != plan.module
        || receipt.old_module != plan.old_module
        || receipt.new_module != plan.new_module
        || receipt.selector != plan.selector
        || receipt.context != plan.context
        || receipt.resource_type != plan.resource_type
        || receipt.old_resource_type != plan.old_resource_type
        || receipt.new_resource_type != plan.new_resource_type
        || receipt.old_source_identity != plan.old_source_identity
        || receipt.new_source_identity != plan.new_source_identity
        || receipt.old_schema_identity != plan.old_schema_identity
        || receipt.new_schema_identity != plan.new_schema_identity
        || receipt.old_relation_identity != plan.old_relation_identity
        || receipt.new_relation_identity != plan.new_relation_identity
        || receipt.field_changes != plan.field_changes
        || receipt.old_occurrences != plan.old_occurrences
        || receipt.new_occurrences != plan.new_occurrences
    {
        return Err(CompileError::without_span("schema acknowledgement is stale or belongs to a different schema/relation change"));
    }
    Ok(())
}

pub fn validate_plan(plan: &SchemaChangePlan) -> Result<()> {
    if plan.schema != SCHEMA_CHANGE_PLAN_SCHEMA || plan.version != 2 {
        return Err(CompileError::without_span("unsupported schema-change plan schema/version"));
    }
    if plan.module.is_empty()
        || plan.selector.action.is_empty()
        || plan.selector.before.is_empty()
        || plan.selector.after.is_empty()
        || plan.resource_type.is_empty()
    {
        return Err(CompileError::without_span("schema-change plan contains an empty identity field"));
    }
    if plan.requires_acknowledgement != (plan.old_resource_type.is_some() && plan.old_schema_identity != plan.new_schema_identity)
        || plan.state_migration_required != plan.requires_acknowledgement
    {
        return Err(CompileError::without_span("schema-change plan flags do not match the bound schema identities"));
    }
    let expected = plan_hash(plan)?;
    if plan.plan_hash != expected {
        return Err(CompileError::without_span("schema-change plan hash does not match its contents"));
    }
    Ok(())
}

/// Enumerate relation families without discarding branch occurrences.
pub fn schema_relation_selectors(module: &crate::ast::Module) -> BTreeSet<SchemaRelationSelector> {
    let mut selectors = BTreeSet::new();
    for item in &module.items {
        let Item::Action(action) = item else { continue };
        let mut relations = Vec::new();
        collect_relations(&action.body, &mut relations);
        for (relation, _) in relations {
            selectors.insert(SchemaRelationSelector {
                action: action.name.clone(),
                before: relation.before.clone(),
                after: relation.after.clone(),
            });
        }
    }
    selectors
}

/// Compare the union of old/new same-except families. Removal and renaming
/// retain the previous expansion; every branch remains bound by the receipt.
pub fn build_schema_change_plans(old: &CompileResult, new: &CompileResult) -> Result<Vec<SchemaChangePlan>> {
    build_optional_schema_change_plans(Some(old), Some(new))
}

pub(crate) fn build_optional_schema_change_plans(
    old: Option<&CompileResult>,
    new: Option<&CompileResult>,
) -> Result<Vec<SchemaChangePlan>> {
    if let (Some(old), Some(new)) = (old, new)
        && old.metadata.module != new.metadata.module
    {
        // A module rename does not establish nominal ownership continuity.
        // Review the old endpoint's retirement and the new endpoint's first
        // schema independently, while binding both complete source candidates.
        let mut plans = build_optional_schema_change_plans(Some(old), None)?;
        plans.extend(build_optional_schema_change_plans(None, Some(new))?);
        for plan in &mut plans {
            plan.old_module = Some(old.metadata.module.clone());
            plan.new_module = Some(new.metadata.module.clone());
            plan.old_source_identity = Some(compilation_catalog(old)?.source_identity.clone());
            plan.new_source_identity = Some(compilation_catalog(new)?.source_identity.clone());
            plan.plan_hash = plan_hash(plan)?;
        }
        return Ok(plans);
    }
    let old_selectors = old.map(|result| schema_relation_selectors(&result.ast)).unwrap_or_default();
    let new_selectors = new.map(|result| schema_relation_selectors(&result.ast)).unwrap_or_default();
    let mut plans = Vec::new();
    for selector in old_selectors.union(&new_selectors) {
        let is_same_except = |result: &CompileResult| {
            find_action(&result.ast, &selector.action).is_some_and(|action| {
                find_relations(action, selector)
                    .iter()
                    .any(|(relation, _)| matches!(&relation.data, ReplaceDataTreatment::SameExcept(_)))
            })
        };
        if old.is_some_and(is_same_except) || new.is_some_and(is_same_except) {
            plans.push(build_optional_schema_change_plan(old, new, selector.clone())?);
        }
    }
    Ok(plans)
}

fn find_action<'a>(module: &'a crate::ast::Module, name: &str) -> Option<&'a ActionDef> {
    module.items.iter().find_map(|item| match item {
        Item::Action(action) if action.name == name => Some(action),
        _ => None,
    })
}

fn find_relations<'a>(action: &'a ActionDef, selector: &SchemaRelationSelector) -> Vec<(&'a ReplaceRelation, Vec<usize>)> {
    let mut relations = Vec::new();
    collect_relations(&action.body, &mut relations);
    relations.into_iter().filter(|(relation, _)| relation.before == selector.before && relation.after == selector.after).collect()
}

fn collect_relations<'a>(statements: &'a [Stmt], out: &mut Vec<(&'a ReplaceRelation, Vec<usize>)>) {
    for (statement_index, statement) in statements.iter().enumerate() {
        for (relation_index, relation) in crate::ast::statement_replace_relations(statement).into_iter().enumerate() {
            out.push((relation, vec![statement_index, relation_index]));
        }
    }
}

fn canonical_action(action: Option<&ActionDef>, module: &crate::ast::Module) -> Result<Option<String>> {
    action
        .map(|action| {
            let mut action = action.clone();
            action.doc_comment = None;
            let mut module = module.clone();
            module.items = vec![Item::Action(action)];
            module.interface_templates.clear();
            crate::fmt::format_default(&module)
        })
        .transpose()
}

fn compilation_catalog(result: &CompileResult) -> Result<&SchemaReviewCompilation> {
    result.schema_review.as_ref().ok_or_else(|| {
        CompileError::without_span(
            "schema review requires the checked compilation/source catalog; recompile both candidates without the incremental cache",
        )
    })
}

fn relation_resource_type(catalog: &SchemaReviewCompilation, selector: &SchemaRelationSelector, label: &str) -> Result<String> {
    let before = catalog
        .roles
        .get(&(selector.action.clone(), selector.before.clone()))
        .ok_or_else(|| CompileError::without_span(format!("{label} relation predecessor has no checked Cell binding")))?;
    let after = catalog
        .roles
        .get(&(selector.action.clone(), selector.after.clone()))
        .ok_or_else(|| CompileError::without_span(format!("{label} relation successor has no checked Cell binding")))?;
    if before != after || !catalog.has_cell_schema(before) {
        return Err(CompileError::without_span(format!("{label} relation does not bind one concrete checked Cell schema")));
    }
    Ok(before.clone())
}

fn typed_schema<'a>(result: &'a CompileResult, resource_type: &str) -> Option<&'a cellscript_artifact_checker::TypedSemanticType> {
    result
        .metadata
        .typed_semantics
        .types
        .iter()
        .find(|schema| schema.name == resource_type && matches!(schema.kind.as_str(), "resource" | "shared" | "receipt"))
}

fn hash_optional_schema_identity(catalog: Option<&SchemaReviewCompilation>, resource_type: Option<&str>) -> Result<String> {
    match catalog.zip(resource_type) {
        Some((catalog, resource)) => catalog.schema_identity(resource),
        None => canonical_hash("cellscript-absent-authoring-schema-v2", &())
            .map_err(|error| CompileError::without_span(format!("failed to hash absent schema identity: {error}"))),
    }
}

fn relation_policies(
    relation: Option<&ReplaceRelation>,
    schema: Option<&cellscript_artifact_checker::TypedSemanticType>,
) -> Result<BTreeMap<String, SchemaFieldPolicy>> {
    let (Some(relation), Some(schema)) = (relation, schema) else { return Ok(BTreeMap::new()) };
    let mut policies = BTreeMap::new();
    match &relation.data {
        ReplaceDataTreatment::SameExcept(assignments) => {
            let assigned = assignments.iter().map(|(field, value)| (field.as_str(), value)).collect::<BTreeMap<_, _>>();
            for field in &schema.fields {
                let policy = if let Some(value) = assigned.get(field.name.as_str()) {
                    SchemaFieldPolicy { treatment: "assign".to_string(), expression: Some(crate::fmt::format_expression(value)) }
                } else {
                    SchemaFieldPolicy { treatment: "implicit-preserve".to_string(), expression: None }
                };
                policies.insert(field.name.clone(), policy);
            }
        }
        ReplaceDataTreatment::Fields(fields) => {
            for field in fields {
                let (name, policy) = match field {
                    crate::ast::ReplaceFieldTreatment::Same(name) => {
                        (name, SchemaFieldPolicy { treatment: "preserve".to_string(), expression: None })
                    }
                    crate::ast::ReplaceFieldTreatment::Assign(name, value) => (
                        name,
                        SchemaFieldPolicy { treatment: "assign".to_string(), expression: Some(crate::fmt::format_expression(value)) },
                    ),
                };
                policies.insert(name.clone(), policy);
            }
        }
    }
    Ok(policies)
}

fn relation_occurrences(
    relations: &[(&ReplaceRelation, Vec<usize>)],
    schema: Option<&cellscript_artifact_checker::TypedSemanticType>,
) -> Result<Vec<SchemaRelationOccurrence>> {
    relations
        .iter()
        .map(|(relation, path)| {
            let lock = match &relation.lock {
                crate::ast::ReplaceLockTreatment::Same => "same".to_string(),
                crate::ast::ReplaceLockTreatment::Exact(value) => format!("exact({})", crate::fmt::format_expression(value)),
                crate::ast::ReplaceLockTreatment::ExactHash(value) => format!("exact_hash({})", crate::fmt::format_expression(value)),
            };
            Ok(SchemaRelationOccurrence {
                path: path.clone(),
                fields: relation_policies(Some(relation), schema)?,
                lock,
                capacity: "same".to_string(),
                type_script: "same".to_string(),
            })
        })
        .collect()
}

fn summarized_policies(occurrences: &[SchemaRelationOccurrence]) -> BTreeMap<String, SchemaFieldPolicy> {
    let mut policies = BTreeMap::new();
    for occurrence in occurrences {
        for (name, policy) in &occurrence.fields {
            let current = policies.entry(name.clone()).or_insert_with(|| policy.clone());
            if current != policy {
                *current = SchemaFieldPolicy { treatment: "per-occurrence".to_string(), expression: None };
            }
        }
    }
    policies
}

fn hash_relation_identity(
    selector: &SchemaRelationSelector,
    occurrences: &[SchemaRelationOccurrence],
    action_source: &Option<String>,
) -> Result<String> {
    canonical_hash(
        RELATION_IDENTITY_DOMAIN,
        &RelationIdentity { action_source, before: &selector.before, after: &selector.after, occurrences },
    )
    .map_err(|error| CompileError::without_span(format!("failed to hash relation identity: {error}")))
}

fn plan_hash(plan: &SchemaChangePlan) -> Result<String> {
    canonical_hash(
        PLAN_HASH_DOMAIN,
        &PlanHashPayload {
            schema: &plan.schema,
            version: plan.version,
            module: &plan.module,
            old_module: &plan.old_module,
            new_module: &plan.new_module,
            selector: &plan.selector,
            context: &plan.context,
            resource_type: &plan.resource_type,
            old_resource_type: &plan.old_resource_type,
            new_resource_type: &plan.new_resource_type,
            old_interface_hash: &plan.old_interface_hash,
            new_interface_hash: &plan.new_interface_hash,
            old_source_identity: &plan.old_source_identity,
            new_source_identity: &plan.new_source_identity,
            old_schema_identity: &plan.old_schema_identity,
            new_schema_identity: &plan.new_schema_identity,
            old_relation_identity: &plan.old_relation_identity,
            new_relation_identity: &plan.new_relation_identity,
            old_occurrences: &plan.old_occurrences,
            new_occurrences: &plan.new_occurrences,
            old_action_source: &plan.old_action_source,
            new_action_source: &plan.new_action_source,
            requires_acknowledgement: plan.requires_acknowledgement,
            state_migration_required: plan.state_migration_required,
            field_changes: &plan.field_changes,
            blockers: &plan.blockers,
        },
    )
    .map_err(|error| CompileError::without_span(format!("failed to hash schema-change plan: {error}")))
}

fn acknowledgement_hash(receipt: &SchemaAcknowledgementReceipt) -> Result<String> {
    canonical_hash(
        ACKNOWLEDGEMENT_HASH_DOMAIN,
        &AcknowledgementHashPayload {
            schema: &receipt.schema,
            version: receipt.version,
            plan_hash: &receipt.plan_hash,
            module: &receipt.module,
            old_module: &receipt.old_module,
            new_module: &receipt.new_module,
            selector: &receipt.selector,
            context: &receipt.context,
            resource_type: &receipt.resource_type,
            old_resource_type: &receipt.old_resource_type,
            new_resource_type: &receipt.new_resource_type,
            old_source_identity: &receipt.old_source_identity,
            new_source_identity: &receipt.new_source_identity,
            old_schema_identity: &receipt.old_schema_identity,
            new_schema_identity: &receipt.new_schema_identity,
            old_relation_identity: &receipt.old_relation_identity,
            new_relation_identity: &receipt.new_relation_identity,
            old_occurrences: &receipt.old_occurrences,
            new_occurrences: &receipt.new_occurrences,
            review_policy: &receipt.review_policy,
            reviewer: &receipt.reviewer,
            rationale: &receipt.rationale,
            field_changes: &receipt.field_changes,
        },
    )
    .map_err(|error| CompileError::without_span(format!("failed to hash schema acknowledgement: {error}")))
}
