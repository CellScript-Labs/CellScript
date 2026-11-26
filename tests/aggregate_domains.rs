//! Aggregate helper evidence must refer to the same resolved field domain.
use cellscript::{CompileOptions, CompileResult, CompilerDiagnostics, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[allow(dead_code)]
#[path = "support/ckb_script_runner.rs"]
mod ckb_script_runner;

fn source(schema: &str, assertion: &str) -> String {
    source_with_actions(schema, assertion, "action verify() { xudt::require_group_amount_conserved(); }")
}

fn source_with_actions(schema: &str, assertion: &str, actions: &str) -> String {
    format!(
        "module aggregate_domains;\n{schema}\n\
         invariant supply {{ trigger: type_group; scope: group;\n\
         reads: group_inputs<Token>.amount, group_outputs<Token>.amount;\n\
         {assertion}\n}}\n\
         {actions}"
    )
}

fn options(diagnostics: CompilerDiagnostics) -> CompileOptions {
    CompileOptions { target: Some("riscv64-elf".into()), source_contracts: true, diagnostics, ..Default::default() }
}

fn compile(source: &str, diagnostics: CompilerDiagnostics) -> CompileResult {
    let result =
        cellscript::compile_with_executable_surface_policy(source, options(diagnostics), ExecutableSurfacePolicy::AllowFailClosed)
            .unwrap();
    result.validate().unwrap();
    result
}

fn aggregate(result: &CompileResult) -> &cellscript::ProofPlanMetadata {
    result.metadata.runtime.proof_plan.iter().find(|plan| plan.category == "aggregate-invariant").unwrap()
}

fn raw_amount(low: u64, high: u64) -> Bytes {
    let mut bytes = low.to_le_bytes().to_vec();
    bytes.extend(high.to_le_bytes());
    Bytes::from(bytes)
}

fn execute(result: &CompileResult, first_output_high: u64, args: &[EntryWitnessArg]) -> i64 {
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 2, 2);
    fixture.current_type_script_input_indices = vec![0, 1];
    fixture.inputs[0].data = raw_amount(u64::MAX, 0);
    fixture.inputs[1].data = raw_amount(1, 0);
    fixture.outputs[0].data = raw_amount(0, first_output_high);
    fixture.outputs[1].data = raw_amount(0, 0);
    let entry = result.metadata.typed_semantics.foundation.entry_contract.exact_entry.strip_prefix("action:").unwrap();
    let payload = result.metadata.actions.iter().find(|action| action.name == entry).unwrap().entry_witness_args(args).unwrap();
    fixture.witnesses =
        vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes(), Bytes::new()];
    ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code
}

#[test]
fn raw_u128_group_helper_does_not_prove_a_u64_field_beside_other_bytes() {
    let source = source("resource Token has store { amount: u64, symbol: [u8; 8], }", "assert_conserved(Token.amount);");
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let result = compile(&source, diagnostics);
        // The raw helper sees the same u128 total. The declared u64 amount instead
        // changes from 2^64 to zero; the adjacent symbol supplies the upper limb.
        assert_eq!(execute(&result, 1, &[]), 0);
        assert_ne!(execute(&result, 0, &[]), 0);
        assert!(!aggregate(&result).on_chain_checked, "raw u128 evidence cannot prove this field: {:?}", aggregate(&result));
    }
}

#[test]
fn shorthand_and_explicit_sum_share_resolved_domain_and_checked_runtime() {
    let assertions =
        ["assert_conserved(Token.amount);", "assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount);"];
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        for field in ["amount", "quantity"] {
            let mut bundles = Vec::new();
            for assertion in assertions {
                let source = source("resource Token has store { amount: u128, }", assertion)
                    .replace(".amount", &format!(".{field}"))
                    .replace("amount: u128", &format!("{field}: u128"));
                let result = compile(&source, diagnostics);
                let plan = aggregate(&result);
                assert!(plan.on_chain_checked, "{plan:?}");
                assert_eq!(plan.evidence_tier, cellscript::EvidenceTier::CheckedRuntime);
                assert!(plan.input_output_relation_checks.iter().all(|check| check.contains("=checked-runtime:")), "{plan:?}");
                assert_eq!(execute(&result, 1, &[]), 0);
                assert_ne!(execute(&result, 0, &[]), 0);
                // The source field can have any name. The runtime API retains
                // its established spelling and checks the same byte domain.
                bundles.push(result.artifact_bytes);
            }
            assert_eq!(bundles[0], bundles[1], "equivalent aggregate spelling changed ELF");
        }
    }
}

#[test]
fn a_raw_helper_never_proves_a_shifted_padded_or_dynamic_field_domain() {
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        for schema in [
            "resource Token has store { prefix: u64, amount: u128, }",
            "resource Token has store { amount: u128, suffix: u64, }",
            "resource Token has store { prefix: Vec<u8>, amount: u128, }",
        ] {
            for assertion in [
                "assert_conserved(Token.amount);",
                "assert_sum(group_inputs<Token>.amount) == assert_sum(group_outputs<Token>.amount);",
            ] {
                let result = compile(&source(schema, assertion), diagnostics);
                assert!(!aggregate(&result).on_chain_checked, "{schema}: {:?}", aggregate(&result));
                assert_eq!(aggregate(&result).codegen_coverage_status, "gap:metadata-only");
            }
        }
    }
}

#[test]
fn conditional_helpers_and_early_returns_do_not_cover_all_accepting_paths() {
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        for action in [
            "action verify(witness check: bool) { if check { xudt::require_group_amount_conserved(); } }",
            "action verify(witness check: bool) { if !check { return; } xudt::require_group_amount_conserved(); }",
        ] {
            let result = compile(
                &source_with_actions("resource Token has store { amount: u128, }", "assert_conserved(Token.amount);", action),
                diagnostics,
            );
            assert!(!aggregate(&result).on_chain_checked, "{:?}", aggregate(&result));
            assert_eq!(aggregate(&result).codegen_coverage_status, "gap:runtime-helper-required");
            assert_eq!(execute(&result, 0, &[EntryWitnessArg::Bool(false)]), 0);
            assert_ne!(execute(&result, 0, &[EntryWitnessArg::Bool(true)]), 0);
            assert_eq!(execute(&result, 1, &[EntryWitnessArg::Bool(true)]), 0);
        }
    }
}

#[test]
fn a_conditional_direct_call_cannot_suppress_the_required_unconditional_prelude() {
    let source = source_with_actions(
        "resource Token has store, create, consume { amount: u128, }",
        "assert_conserved(Token.amount);",
        r#"
        action verify(input before: Token, witness check: bool, witness owner: Address, output after: Token) {
            if check { xudt::require_group_amount_conserved(); }
            consume before;
            create after = Token { amount: before.amount, } with_lock(owner)
        }
    "#,
    );
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let result = compile(&source, diagnostics);
        assert!(aggregate(&result).on_chain_checked, "{:?}", aggregate(&result));
        let owner = ckb_script_runner::deterministic_always_success_lock_hash();
        for check in [false, true] {
            for second_output in [5, 6] {
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 2, 2);
                fixture.current_type_script_input_indices = vec![0, 1];
                fixture.inputs[0].data = raw_amount(1, 0);
                fixture.inputs[1].data = raw_amount(5, 0);
                fixture.outputs[0].data = raw_amount(1, 0);
                fixture.outputs[1].data = raw_amount(second_output, 0);
                let payload = result.metadata.actions[0]
                    .entry_witness_args(&[EntryWitnessArg::Bool(check), EntryWitnessArg::Address(owner)])
                    .unwrap();
                fixture.witnesses = vec![
                    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes(),
                    Bytes::new(),
                ];
                let exit = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code;
                assert_eq!(exit == 0, second_output == 5, "check={check}, second_output={second_output}, exit={exit}");
            }
        }
    }
}

#[test]
fn checked_calls_and_two_covered_branches_prove_the_selected_entry() {
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let indirect = source_with_actions(
            "resource Token has store { amount: u128, }",
            "assert_conserved(Token.amount);",
            "fn check_group() { xudt::require_group_amount_conserved(); } action verify() { check_group(); }",
        );
        let result = compile(&indirect, diagnostics);
        assert!(aggregate(&result).on_chain_checked, "{:?}", aggregate(&result));
        assert_eq!(execute(&result, 1, &[]), 0);
        assert_ne!(execute(&result, 0, &[]), 0);

        let branches = source_with_actions("resource Token has store { amount: u128, }", "assert_conserved(Token.amount);",
            "action verify(witness choice: bool) { if choice { xudt::require_group_amount_conserved(); } else { xudt::require_group_amount_conserved(); } }");
        let result = compile(&branches, diagnostics);
        assert!(aggregate(&result).on_chain_checked, "{:?}", aggregate(&result));
        for choice in [false, true] {
            assert_eq!(execute(&result, 1, &[EntryWitnessArg::Bool(choice)]), 0);
            assert_ne!(execute(&result, 0, &[EntryWitnessArg::Bool(choice)]), 0);
        }
    }
}

#[test]
fn an_unused_or_unselected_helper_does_not_discharge_the_entry_invariant() {
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        for callables in [
            "action main() {} fn unused() { xudt::require_group_amount_conserved(); }",
            "action decoy() { xudt::require_group_amount_conserved(); } action main() {}",
        ] {
            let source =
                source_with_actions("resource Token has store { amount: u128, }", "assert_conserved(Token.amount);", callables);
            let result = compile(&source, diagnostics);
            assert!(!aggregate(&result).on_chain_checked, "{:?}", aggregate(&result));
            assert_eq!(execute(&result, 0, &[]), 0);
            let mut strict = options(diagnostics);
            strict.primitive_compat = Some("0.17".into());
            let error = cellscript::compile(&source, strict).unwrap_err();
            assert!(error.message.contains("PP0170"), "{error}");
        }
    }
}

#[test]
fn explicit_action_defined_selection_checks_the_selected_action() {
    use cellscript::artifact::{ArtifactContext, ArtifactDeclaration, ArtifactDispatch, EntryCellBindings, EntryCoverage};
    let source = source_with_actions(
        "resource Token has store { amount: u128, }",
        "assert_conserved(Token.amount);",
        "action checked() { xudt::require_group_amount_conserved(); } action unchecked() {}",
    );
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        for (entry, checked) in [("checked", true), ("unchecked", false)] {
            let declaration = ArtifactDeclaration {
                name: "selected".into(),
                context: ArtifactContext::TypeGroup {},
                bindings: EntryCellBindings::TransactionAbsolute,
                coverage: EntryCoverage::ActionDefined,
                layout: None,
                dispatch: ArtifactDispatch::SingleEntry,
                entry: Some(entry.into()),
                actions: Vec::new(),
                common_checks: Vec::new(),
            };
            let result = cellscript::artifact::compile_artifact(
                &source,
                options(diagnostics),
                declaration,
                ExecutableSurfacePolicy::AllowFailClosed,
            )
            .unwrap();
            result.validate().unwrap();
            assert_eq!(aggregate(&result).on_chain_checked, checked, "{entry}: {:?}", aggregate(&result));
            assert_eq!(execute(&result, 0, &[]) == 0, !checked);
            assert_eq!(execute(&result, 1, &[]), 0);
        }
    }
}

#[test]
fn tagged_dispatch_checks_every_variant_and_retains_common_helper_admission_boundary() {
    use cellscript::artifact::{
        ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch, EntryCellBindings, EntryCoverage,
    };
    let source = source_with_actions(
        "resource Token has store, create, consume { amount: u128, }",
        "assert_conserved(Token.amount);",
        r#"
        action common() { require true; }
        action checked(input before: Token, output after: Token) {
            xudt::require_group_amount_conserved();
            consume before;
            create after = Token { amount: before.amount, }
        }
        action unchecked(input before: Token, output after: Token) {
            consume before;
            create after = Token { amount: 0, }
        }
    "#,
    );
    for (common_checks, variants, checked) in [
        (Vec::new(), vec!["checked"], true),
        (Vec::new(), vec!["checked", "unchecked"], false),
        (vec!["common".into()], vec!["checked", "unchecked"], false),
    ] {
        let declaration = ArtifactDeclaration {
            name: "tagged".into(),
            context: ArtifactContext::TypeGroup {},
            bindings: EntryCellBindings::GroupRelative,
            coverage: EntryCoverage::FixedGroup,
            layout: Some("Token".into()),
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            entry: None,
            actions: variants
                .into_iter()
                .enumerate()
                .map(|(tag, action)| ArtifactAction { tag: tag as u32, action: action.into() })
                .collect(),
            common_checks,
        };
        let metadata =
            cellscript::artifact::compile_artifact_metadata(&source, options(CompilerDiagnostics::default()), declaration.clone())
                .unwrap();
        let plan = metadata.runtime.proof_plan.iter().find(|plan| plan.category == "aggregate-invariant").unwrap();
        assert_eq!(plan.on_chain_checked, checked, "{plan:?}");
        if !declaration.common_checks.is_empty() {
            // Raw group helpers in a common body are outside the independent
            // scalar/Unit callable contract. Do not widen that contract merely
            // to turn a native path analysis into accepted artifact evidence.
            let unsupported =
                source.replace("action common() { require true; }", "action common() { xudt::require_group_amount_conserved(); }");
            let error =
                cellscript::artifact::compile_artifact_metadata(&unsupported, options(CompilerDiagnostics::default()), declaration)
                    .unwrap_err();
            assert!(error.message.contains("policy common call requires a retained scalar/Unit callable body"), "{error}");
        }
    }
}

#[test]
fn explicit_fungible_profile_reports_owner_or_conservation_and_cannot_be_selected_by_a_source_name() {
    let source = source_with_actions(
        "resource Token has store { amount: u128, }",
        "assert_conserved(Token.amount);",
        "action placeholder() {}",
    );
    for assurance in ["0.16", "0.17"] {
        let mut profile_options = options(CompilerDiagnostics::default());
        profile_options.primitive_compat = Some(assurance.into());
        let result = cellscript::compile_fungible_type_group_entry(&source, profile_options).unwrap();
        result.validate().unwrap();
        for plan in result.metadata.runtime.proof_plan.iter().filter(|plan| plan.origin.starts_with("invariant:")) {
            assert!(plan.category.starts_with("selected-profile-"), "{plan:?}");
            assert!(plan.feature.contains("owner-or-conservation"), "{plan:?}");
            assert!(plan.input_output_relation_checks.iter().all(|check| check.starts_with("owner_authorized ||")), "{plan:?}");
        }
    }
    let impersonator = source.replace("placeholder", "__cellscript_fungible_type_group_v1");
    let error = cellscript::compile(&impersonator, options(CompilerDiagnostics::default())).unwrap_err();
    assert!(error.message.contains("reserved fungible type-group entry action without its structured runtime contract"), "{error}");
}

#[test]
fn fungible_profile_metadata_rejects_relabeling_its_condition_as_unconditional_equality() {
    let source = source_with_actions(
        "resource Token has store { amount: u128, }",
        "assert_conserved(Token.amount);",
        "action placeholder() {}",
    );
    let result = cellscript::compile_fungible_type_group_entry(&source, options(CompilerDiagnostics::default())).unwrap();
    for mutation in 0..3 {
        let mut forged = result.clone();
        let plan = forged.metadata.runtime.proof_plan.iter_mut().find(|plan| plan.category == "selected-profile-invariant").unwrap();
        match mutation {
            0 => {
                plan.input_output_relation_checks =
                    vec!["assert_conserved:Token.amount=checked-runtime:fungible::require_type_group_v1".into()]
            }
            1 => {
                plan.category = "declared-invariant".into();
                plan.feature = "supply".into();
            }
            2 => plan.coverage.clear(),
            _ => unreachable!(),
        }
        let error = forged.validate().unwrap_err();
        assert!(error.message.contains("owner-or-conservation profile coverage"), "{error}");
    }
}
