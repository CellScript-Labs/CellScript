use cellscript::artifact::{compile_artifact_metadata, ArtifactDeclaration, EntryCellBindings, EntryCoverage};
use cellscript::policy_explanation::{explain, PolicyCategory, PolicyTreatment, RelationPolicies};
use cellscript::{CompileMetadata, CompileOptions, CURRENT_EDITION};

#[path = "support/fixed_action.rs"]
mod fixed_action;

fn component<'a>(relation: &'a RelationPolicies, component: &str) -> &'a PolicyTreatment {
    relation.treatments.iter().find(|policy| policy.component == component).unwrap()
}

fn ordinary(source: &str) -> CompileMetadata {
    cellscript::compile_metadata(source, CURRENT_EDITION, None).unwrap()
}

fn fixed(source: &str) -> CompileMetadata {
    fixed_action::compile_fixture(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() })
        .unwrap()
        .metadata
}

#[test]
fn fresh_and_pool_views_follow_the_selected_coordinates_and_membership() {
    for (source, inputs) in [
        (include_str!("syntax_combo/authoring/absolute-fresh.cell"), 0),
        (include_str!("syntax_combo/authoring/absolute-pool.cell"), 1),
    ] {
        for mode in ["ordinary", "fixed", "manual"] {
            let metadata = if mode == "ordinary" {
                ordinary(source)
            } else {
                let mut declaration = ArtifactDeclaration::single_type_group("token", "verify", "Token");
                if mode == "manual" {
                    declaration.bindings = EntryCellBindings::TransactionAbsolute;
                    declaration.coverage = EntryCoverage::ActionDefined;
                    declaration.layout = None;
                }
                compile_artifact_metadata(source, CompileOptions::default(), declaration).unwrap()
            };
            let summary = explain(&metadata, "action:verify").unwrap();
            assert!(summary.selected);
            assert_eq!(summary.group_counts, if mode == "fixed" { Some((inputs, 1)) } else { None });
            let relation = summary.relations.iter().find(|relation| relation.output.is_some()).unwrap();
            assert_eq!(relation.output.as_ref().unwrap().selector, if mode == "fixed" { "group-output[0]" } else { "output[0]" });
            assert_eq!(
                component(relation, "type-script").category,
                if mode == "fixed" { PolicyCategory::DerivedAndEnforced } else { PolicyCategory::UnconstrainedByRelation }
            );
            assert_eq!(component(relation, "capacity").category, PolicyCategory::UnconstrainedByRelation);
            assert_eq!(component(relation, "data").category, PolicyCategory::Explicit);
            let expanded = cellscript::semantic_expansion::render(&metadata).unwrap();
            assert!(
                expanded.contains("policy capacity category=unconstrained-by-this-relation evidence=not-established-by-relation"),
                "{expanded}"
            );
            let hover = summary.hover(Some("after"));
            assert!(hover.contains("no predecessor/successor capacity equality"), "{hover}");
            assert!(hover.contains(&relation.output.as_ref().unwrap().selector), "{hover}");
            assert_eq!(
                summary.coverage,
                match mode {
                    "fixed" => "fixed-group",
                    "manual" => "action-defined",
                    _ => "not-declared",
                }
            );
        }
    }
}

#[test]
fn complete_replacement_retirement_and_fixed_kernel_keep_distinct_capacity_policies() {
    let metadata = fixed(include_str!("syntax_combo/authoring/branch-continue-retire.cell"));
    let summary = explain(&metadata, "action:continue_one").unwrap();
    assert_eq!(summary.group_counts, Some((2, 1)));
    for relation in &summary.relations {
        assert_eq!(
            component(relation, "capacity").category,
            if relation.output.is_some() { PolicyCategory::Explicit } else { PolicyCategory::NotApplicable }
        );
        if relation.output.is_none() {
            assert_eq!(component(relation, "identity").category, PolicyCategory::Explicit);
            assert!(component(relation, "identity").detail.contains("absence="));
        }
    }
    let metadata = fixed(include_str!("syntax_combo/authoring/kernel-group-successor.cell"));
    let entry = &metadata.typed_semantics.foundation.entry_contract.exact_entry;
    let summary = explain(&metadata, entry).unwrap();
    assert_eq!(summary.group_counts, Some((1, 1)));
    assert_eq!(component(&summary.relations[0], "capacity").category, PolicyCategory::UnconstrainedByRelation);
    assert_eq!(component(&summary.relations[0], "type-script").category, PolicyCategory::DerivedAndEnforced);
}

#[test]
fn advanced_relations_keep_their_builder_evidence_boundary() {
    for source in [
        include_str!("syntax_combo/authoring/branch-unique-retire.cell"),
        include_str!("syntax_combo/authoring/receipt-claim-kernel.cell"),
        include_str!("syntax_combo/authoring/settle-kernel.cell"),
    ] {
        let metadata = ordinary(source);
        let summary = explain(&metadata, &metadata.typed_semantics.foundation.entry_contract.exact_entry).unwrap();
        let relation = summary.relations.iter().find(|relation| relation.output.is_some()).unwrap();
        assert_eq!(relation.enforcement, "builder-evidence-required");
        assert_eq!(component(relation, "capacity").category, PolicyCategory::UnconstrainedByRelation);
        assert_eq!(component(relation, "type-script").category, PolicyCategory::UnconstrainedByRelation);
        assert!(summary.hover(None).contains("builder-evidence-required"));
    }
}

#[test]
fn creation_identity_does_not_conflate_local_script_checks_and_type_id_builder_evidence() {
    for (policy, expected, evidence) in [
        ("field(id)", PolicyCategory::UnconstrainedByRelation, "checked-runtime"),
        ("singleton_type", PolicyCategory::Explicit, "checked-runtime"),
        ("ckb_type_id", PolicyCategory::Explicit, "builder-evidence-required"),
    ] {
        let type_id = if policy == "ckb_type_id" { "#[type_id(\"policies::Token\")]" } else { "" };
        let source = format!("module policies; {type_id} resource Token has store,create identity({policy}) {{id:u64,}} action mint(witness owner:Address,output next:Token) {{fresh next {{data {{id=7,}};lock=exact(owner);}}}}");
        let metadata = ordinary(&source);
        let summary = explain(&metadata, "action:mint").unwrap();
        let script = component(&summary.relations[0], "type-script");
        assert_eq!(script.category, expected, "{policy}: {script:?}");
        assert_eq!(script.evidence, evidence, "{policy}: {script:?}");
        assert_eq!(component(&summary.relations[0], "identity").category, PolicyCategory::Explicit);
    }
}

#[test]
fn declaration_floors_remain_separate_from_successor_capacity_equality() {
    let source = include_str!("syntax_combo/authoring/absolute-fresh.cell")
        .replace("has store, create, consume {", "has store, create, consume with_capacity_floor(100_000_000_000) {");
    let metadata = ordinary(&source);
    let summary = explain(&metadata, "action:verify").unwrap();
    let capacity = component(&summary.relations[0], "capacity");
    assert_eq!(capacity.category, PolicyCategory::UnconstrainedByRelation);
    assert!(capacity.detail.contains("declaration floor=100000000000 shannons"), "{}", capacity.detail);

    let report = cellscript::compile_metadata_with_diagnostics(&source, CURRENT_EDITION, None);
    assert!(!report.has_errors(), "{:?}", report.diagnostics);
    assert!(report.metadata.is_some());
    let sources = [cellscript::InMemorySource { path: "main.cell".into(), source: source.clone(), role: None }];
    let report = cellscript::compile_sources_metadata_with_diagnostics(&sources, "main.cell", CURRENT_EDITION, None);
    assert!(!report.has_errors(), "{:?}", report.diagnostics);
    assert!(report.metadata.is_some());

    // The actual artifact must retain the independently validated duplicate
    // declaration summary; allowing metadata-only analysis cannot remove it.
    let mut artifact = fixed(&source);
    assert_eq!(artifact.constraints.ckb.as_ref().unwrap().declared_capacity_floors[0].shannons, 100_000_000_000);
    artifact.constraints.ckb = None;
    let error = cellscript::validate_compile_metadata(&artifact, cellscript::ArtifactFormat::RiscvElf).unwrap_err();
    assert!(error.message.contains("missing constraints.ckb.declared_capacity_floors"), "{error}");

    let mut invalid_floor = metadata;
    invalid_floor.types.iter_mut().find(|ty| ty.name == "Token").unwrap().capacity_floor_shannons = Some(0);
    let error = cellscript::validate_compile_metadata(&invalid_floor, cellscript::ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(error.message.contains("zero capacity_floor_shannons"), "{error}");
}

#[test]
fn retirement_explanation_follows_the_actual_absence_scan_domain() {
    let source = include_str!("syntax_combo/authoring/branch-continue-retire.cell");
    for (metadata, scan) in [(ordinary(source), "transaction output"), (fixed(source), "group-output")] {
        let summary = explain(&metadata, "action:continue_one").unwrap();
        for relation in summary.relations.iter().filter(|relation| relation.output.is_none()) {
            let identity = component(relation, "identity");
            assert!(identity.detail.contains(&format!("search={scan}")), "{}", identity.detail);
            assert!(identity.detail.contains("matching input Type Script hash"), "{}", identity.detail);
        }
    }
}

#[test]
fn bounded_group_handles_retain_scan_coordinates_without_scalar_role_bindings() {
    let source = "module bounded_policy; resource Token has store,consume {amount:u64,} action verify(input inputs:BoundedCellSet<Token,2>)->u64 {consume_each token in inputs {require token.amount>0;} return 0;}";
    let metadata = ordinary(source);
    let summary = explain(&metadata, "action:verify").unwrap();
    assert_eq!(summary.coordinates, "group-relative");
    assert_eq!(summary.group_counts, None);
    let role = summary.roles.iter().find(|role| role.binding == "inputs").unwrap();
    assert_eq!(role.source_scope, "group-relative");
    assert_eq!(role.membership, "current-type-group");
    assert!(role.selector.contains("canonical group order"));
    assert!(!role.selector.contains("[0]"));
}

#[test]
fn bounded_output_plans_do_not_invent_scalar_successor_policies() {
    let metadata = ordinary(include_str!("fixtures/bounded_output_plan.cell"));
    let summary = explain(&metadata, "action:verify").unwrap();
    assert_eq!(summary.group_counts, None);
    assert!(summary.relations.is_empty());
    assert!(summary.roles.is_empty());
    let plan = metadata.runtime.collection_instantiations.iter().find_map(|row| row.bounded_output_plan.as_ref()).unwrap();
    assert_eq!(plan.capacity_floor_shannons, 10_000_000_000);
    assert!(summary.hover(None).contains("No lifecycle relation claims are recorded"));
}

#[test]
fn unrecognized_type_script_labels_cannot_acquire_explicit_policy_authority() {
    let mut metadata = ordinary(include_str!("syntax_combo/authoring/absolute-fresh.cell"));
    metadata.typed_semantics.foundation.dispositions[0].envelope.type_script = "unrecognized-producer-label".into();
    let summary = explain(&metadata, "action:verify").unwrap();
    assert_eq!(component(&summary.relations[0], "type-script").category, PolicyCategory::UnconstrainedByRelation);
}

#[test]
fn policy_rendering_rejects_unbound_group_claims() {
    let mut metadata = ordinary(include_str!("syntax_combo/authoring/absolute-fresh.cell"));
    metadata.typed_semantics.foundation.dispositions[0].envelope.type_script = "checked-type-group-member".into();
    let error = cellscript::semantic_expansion::render(&metadata).unwrap_err();
    assert!(error.message.contains("Type group authority"), "{error}");
}

#[test]
fn lock_policy_views_keep_authorization_and_successor_policies_separate() {
    let source = include_str!("syntax_combo/authoring/lock-artifact.cell");
    let metadata =
        compile_artifact_metadata(source, CompileOptions::default(), ArtifactDeclaration::single_lock_group("vault", "unlock"))
            .unwrap();
    let summary = explain(&metadata, "lock:unlock").unwrap();
    assert_eq!(summary.coverage, "authorization-only");
    assert_eq!(summary.group_counts, None);
    let relation = &summary.relations[0];
    assert_eq!(component(relation, "lock").category, PolicyCategory::Explicit);
    for field in ["data", "type-script", "capacity"] {
        assert_eq!(component(relation, field).category, PolicyCategory::NotApplicable);
    }
    assert_eq!(relation.input.as_ref().unwrap().membership, "current-lock-group");
}
