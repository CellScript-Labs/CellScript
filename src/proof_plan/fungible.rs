//! The explicitly selected fungible profile specializes the source domain
//! template to owner-or-conservation. Its evidence never asserts unconditional
//! equality for owner-authorized issuance or destruction.
use super::{EvidenceTier, ProofPlanDiagnosticMetadata, ProofPlanMetadata, ProofPlanSourceSpanMetadata};
use crate::{ir, FungibleTypeGroupEntryMetadata};

pub(super) fn plans(invariant: &ir::IrInvariant, profile: &FungibleTypeGroupEntryMetadata) -> Vec<ProofPlanMetadata> {
    let feature = format!("{}:owner-or-conservation:{}.{}", profile.contract, profile.type_name, profile.field);
    let relation = format!(
        "owner_authorized || (nonempty_group_inputs && nonempty_group_outputs && checked_u128_sum(group_inputs<{}>.{})==checked_u128_sum(group_outputs<{}>.{}))",
        profile.type_name, profile.field, profile.type_name, profile.field,
    );
    let detail = format!(
        "explicitly selected {} profile uses invariant '{}' as its resolved structural domain template; owner authorization permits issuance/destruction, while non-owner execution requires nonempty input/output groups and checked u128 conservation; unconditional source equality is not the selected profile contract",
        profile.contract, invariant.name,
    );
    let summary = ProofPlanMetadata {
        name: invariant.name.clone(),
        origin: format!("invariant:{}", invariant.name),
        category: "selected-profile-invariant".into(),
        feature: feature.clone(),
        evidence_tier: EvidenceTier::CheckedRuntime,
        source_span: Some(ProofPlanSourceSpanMetadata {
            start: invariant.span.start,
            end: invariant.span.end,
            line: invariant.span.line,
            column: invariant.span.column,
        }),
        trigger: "type_group".into(),
        scope: "group".into(),
        reads: vec![
            "group_input".into(),
            "group_output".into(),
            "input_lock_hash".into(),
            "input_type_hash".into(),
            "script_args".into(),
        ],
        coverage: vec![
            format!("selected_profile:{}", profile.contract),
            format!("runtime_helper_checked:{}", profile.runtime_helper),
            format!("source_template:{}", super::aggregate_feature_label(&invariant.aggregates[0])),
            "source_template_role:structural-domain-not-unconditional-equality".into(),
            "codec:exactly-16-little-endian-bytes".into(),
            "owner_mode:authorized-issuance-and-destruction".into(),
            "non_owner_mode:nonempty-groups-and-checked-u128-conservation".into(),
        ],
        input_output_relation_checks: vec![relation],
        group_cardinality: "complete-current-type-group; non-owner inputs>=1, outputs>=1; owner empty sides allowed".into(),
        identity_lifecycle_policy: "selected owner authority and complete current Type Script group".into(),
        preserved_fields: Vec::new(),
        witness_fields: Vec::new(),
        lock_args_fields: Vec::new(),
        on_chain_checked: true,
        on_chain_checked_obligations: vec![format!("selected-profile-invariant:{feature}=checked-runtime")],
        builder_assumptions: Vec::new(),
        codegen_coverage_status: "covered".into(),
        status: "checked-runtime".into(),
        detail: detail.clone(),
        diagnostics: vec![ProofPlanDiagnosticMetadata { severity: "info".into(), message: detail }],
    };
    // The profile's admission requires exactly one aggregate. Retain that
    // source occurrence without presenting its equality as an unconditional
    // checked proposition.
    let aggregate = &invariant.aggregates[0];
    let mut selected_aggregate = summary.clone();
    selected_aggregate.name = format!("{}#aggregate0", invariant.name);
    selected_aggregate.origin = format!("invariant:{}#aggregate:0", invariant.name);
    selected_aggregate.category = "selected-profile-aggregate".into();
    selected_aggregate.source_span = Some(ProofPlanSourceSpanMetadata {
        start: aggregate.span.start,
        end: aggregate.span.end,
        line: aggregate.span.line,
        column: aggregate.span.column,
    });
    selected_aggregate.on_chain_checked_obligations = vec![format!("selected-profile-aggregate:{feature}=checked-runtime")];
    vec![summary, selected_aggregate]
}
