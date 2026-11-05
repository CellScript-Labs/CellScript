use super::*;
use cellscript_artifact_checker::effect_facts::{
    ContextAccess as Access, ContextSource as Source, EffectFacts, LifecycleOperation as Life, ProcessOperation as Process,
};
const ORIGINAL:&str="module effect_facts\npublic action verify() { verification let amount = ckb::cell_capacity(source::input(0)) require amount > 0 }";
#[test]
fn inferred_effects_bind_the_original_reproduction_and_optimization_boundary() {
    for source in [ORIGINAL,"module effect_facts\npublic action verify() { verification if false { let amount = ckb::cell_capacity(source::input(0)) require amount > 0 } require true }"]{
        let mut interface=None;
        for diagnostics in [cellscript::CompilerDiagnostics::default(),cellscript::CompilerDiagnostics::without_optimizations()]{
            let compiled=compile(source,CompileOptions{source_contracts:true,diagnostics,target:Some("riscv64-elf".into()),..Default::default()}).unwrap();
            let expected=EffectFacts::read(Source::Input,Access::Capacity);
            assert_eq!(compiled.metadata.actions[0].effects,expected);
            let actual=&compiled.metadata.public_interface;if let Some(expected)=&interface{assert_eq!(actual,expected);}interface=Some(actual.clone());
            let fixture=Fixture::from_result(compiled);
            let checked=cellscript_artifact_checker::interface::inspect_bundle(&fixture.artifact,&serde_json::to_vec(&fixture.metadata).unwrap(),&serde_json::to_vec(&fixture.record).unwrap(),&serde_json::to_vec(&fixture.source_map).unwrap(),&CheckerBudgets::default()).unwrap();
            assert_eq!(checked.declared().callables[0].effects,Some(expected.clone()));
            assert_eq!(checked.effective().entries.iter().find(|entry|entry.name=="verify").unwrap().effects,expected);
        }
    }
}
#[test]
fn rebound_broader_effects_do_not_pass_as_inferred_facts() {
    let valid = Fixture::from_source("module facts\naction verify() { verification require true }");
    for broader in [
        EffectFacts::read(Source::Input, Access::Capacity),
        EffectFacts::lifecycle(Life::Create),
        EffectFacts::lifecycle(Life::Destroy),
        EffectFacts::process(Process::Exec),
    ] {
        let mut changed = valid.clone();
        for entry in &mut changed.record.typed_semantics.entries {
            entry.effects = broader.clone();
        }
        changed.metadata["actions"][0]["effects"] = serde_json::to_value(broader).unwrap();
        changed.rebind_typed_semantics();
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{error:?}");
        assert!(error.message.contains("independently reconstructed effect facts"), "{error:?}");
    }
}
#[test]
fn affected_old_metadata_generations_and_retired_fields_reject() {
    let valid = Fixture::from_source(ORIGINAL);
    for field in ["effect", "effect_class", "declared_effect", "declared_effect_class", "effect_declared", "inferred_effect_class"] {
        let mut changed = valid.clone();
        changed.metadata["actions"][0][field] = "Pure".into();
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }
    for generation in [69, 70, 71, 72, 73, 74] {
        let mut old = valid.clone();
        old.metadata["metadata_schema_version"] = generation.into();
        assert_code(&old, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }
}
#[test]
fn unknown_source_profiles_reject_after_complete_sidecar_rebinding() {
    for (source, edition, unsupported) in [
        (ORIGINAL, cellscript::CURRENT_EDITION, "unknown-source-profile"),
        (
            include_str!("../syntax_combo/seeds/edition-2027-type-script.cell"),
            CURRENT_EDITION,
            "cellscript-source-semantics-2027-unsupported",
        ),
    ] {
        let mut old = Fixture::from_result(
            compile(source, CompileOptions { edition, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        old.record.compatibility_profile.source_semantics = unsupported.into();
        old.metadata["compatibility_profile"]["source_semantics"] = unsupported.into();
        old.record.compatibility_profile_hash =
            canonical_hash("cellscript-compatibility-profile-identity-v1", &old.record.compatibility_profile).unwrap();
        old.metadata["compatibility_profile_hash"] = old.record.compatibility_profile_hash.clone().into();
        old.rebind_sidecars();
        assert_code(&old, CheckerRejectionCode::V2403UnsupportedSchema);
    }
}

#[test]
fn unknown_edition_rejects_after_profile_and_sidecar_hashes_are_rebound() {
    let valid = Fixture::from_source(ORIGINAL);
    for semantics in [cellscript_artifact_checker::SOURCE_SEMANTICS_2027, "cellscript-source-semantics-2099-unknown"] {
        let mut unknown = valid.clone();
        unknown.record.edition = "2099".into();
        let profile = &mut unknown.record.compatibility_profile;
        profile.edition = "2099".into();
        profile.id = profile.id.replace("semantics-2027", "semantics-2099");
        profile.source_semantics = semantics.into();
        unknown.metadata["edition"] = "2099".into();
        unknown.metadata["compatibility_profile"] = serde_json::to_value(&profile).unwrap();
        unknown.record.compatibility_profile_hash = canonical_hash("cellscript-compatibility-profile-identity-v1", &profile).unwrap();
        unknown.metadata["compatibility_profile_hash"] = unknown.record.compatibility_profile_hash.clone().into();
        unknown.rebind_sidecars();
        let error = check_bundle_values(
            &unknown.artifact,
            &unknown.metadata,
            &unknown.record,
            &unknown.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2403UnsupportedSchema, "{error:?}");
        assert!(error.message.contains("Edition 2027"), "{error:?}");
    }
}

#[test]
fn retired_bare_spawn_helper_rejects_after_sidecar_rebinding() {
    let mut changed = Fixture::from_source("module retired_spawn\npublic action verify() -> u64 { verification return process_id() }");
    let mut replaced = 0;
    for entry in &mut changed.record.typed_semantics.entries {
        for operation in entry.blocks.iter_mut().flat_map(|block| &mut block.operations) {
            if let Some(call) = &mut operation.call
                && call.target == "__ckb_process_id"
            {
                call.target = "__ckb_spawn".into();
                replaced += 1;
            }
        }
    }
    assert!(replaced > 0);
    changed.rebind_typed_semantics();
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{error:?}");
    assert!(error.message.contains("unregistered runtime effect helper"), "{error:?}");
}
