#[path = "support/fixed_action.rs"]
mod fixed_action;
use fixed_action::compile_fixture as compile;

use cellscript::{CompileOptions, CURRENT_EDITION};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

const NATIVE_REPLACEMENT: &str = r#"
module native_group_binding;
resource Token has store, replace, relock { amount: u64, }
action retain_value(
        input before: Token,
        witness recipient: Address,
        output after: Token,
    ) {
        require before.amount == 7;
        replace before -> after {
            data { amount = same, };
            type_script = same;
            lock = exact(recipient);
            capacity = same;
        }
    }
"#;

const ALWAYS_SUCCESS: &str = r#"
module unrelated_group;
action main() -> u64 {  return 0; }
"#;

#[test]
fn selected_coverage_controls_bindings_and_rejects_a_layout_mismatch() {
    let options = CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() };
    let absolute = cellscript::compile(NATIVE_REPLACEMENT, options.clone()).unwrap();
    assert!(absolute.metadata.typed_semantics.foundation.entry_contract.cells.is_none());
    assert!(absolute.metadata.typed_semantics.entries[0].cell_bindings.iter().all(|binding| {
        matches!(
            binding.source,
            cellscript_artifact_checker::CellBindingSource::Input | cellscript_artifact_checker::CellBindingSource::Output
        )
    }));
    let fixed = compile(NATIVE_REPLACEMENT, options.clone()).unwrap();
    let fixed_contract = fixed.metadata.typed_semantics.foundation.entry_contract.cells.as_ref().unwrap();
    assert_eq!(fixed_contract.coverage, cellscript_artifact_checker::EntryCoverage::FixedGroup);
    assert_ne!(
        absolute.metadata.typed_semantics.foundation.identities.entry_contract_id,
        fixed.metadata.typed_semantics.foundation.identities.entry_contract_id
    );

    let source = format!("{NATIVE_REPLACEMENT}\nresource Other has store {{ amount: u64, }}\naction verify() -> u64 {{ return 0; }}");
    let declaration = cellscript::artifact::ArtifactDeclaration::single_type_group("token", "retain_value", "Other");
    let error = cellscript::artifact::compile_artifact(
        &source,
        options.clone(),
        declaration,
        cellscript::ExecutableSurfacePolicy::AllowFailClosed,
    )
    .unwrap_err();
    assert!(error.message.contains("fixed-group layout") || error.message.contains("cannot rebind"), "{error}");

    let temp = tempfile::tempdir().unwrap();
    let path = camino::Utf8PathBuf::from_path_buf(temp.path().join("main.cell")).unwrap();
    std::fs::write(&path, source).unwrap();
    let manual = cellscript::compile_file_with_entry_action(&path, options, "verify").unwrap();
    assert_eq!(manual.metadata.typed_semantics.foundation.entry_contract.exact_entry, "action:verify");
    assert!(manual.metadata.actions[0].params.is_empty());
}

#[test]
fn fixed_group_coverage_can_check_typed_transaction_funding_and_settlement() {
    let source = NATIVE_REPLACEMENT.replace("resource Token", "resource Funding has store { units: u64, }\nresource Token").replace(
        "require before.amount == 7;",
        r#"
        let funding = ckb::input::<Funding>(0);
        let settlement = ckb::output::<Funding>(0);
        require funding.type_hash == recipient;
        require settlement.type_hash == recipient;
        require funding.data.units == before.amount;
        require settlement.data.units == funding.data.units;
        "#,
    );
    assert!(execute_source_replacement_case(&source, (true, true), 7, 7, (&[], &[]), 0).is_ok());
    assert!(execute_source_replacement_case(&source, (false, true), 7, 7, (&[], &[]), 0).is_err());
    assert!(execute_source_replacement_case(&source, (true, false), 7, 7, (&[], &[]), 0).is_err());
    assert!(execute_source_replacement_case(&source, (true, true), 7, 7, (&[], &[7]), 0).is_err());
}

#[test]
fn action_defined_coverage_preserves_absolute_bindings_with_multiple_layouts() {
    use cellscript::artifact::{
        compile_artifact, ArtifactContext, ArtifactDeclaration, ArtifactDispatch, EntryCellBindings, EntryCoverage,
    };
    use cellscript::ExecutableSurfacePolicy;
    use cellscript_artifact_checker::CellBindingSource;

    let source = r#"
module absolute_payment;
resource State has store, replace { amount: u64, }
resource Fee has store, consume { amount: u64, }
action settle(input state: State, input fee: Fee, output next: State) {
    require fee.amount > 0;
    consume fee;
    replace state -> next {
        data = same;
        lock = same;
        type_script = same;
        capacity = same;
    }
}
"#;
    let options = CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() };
    let implicit = cellscript::compile(source, options.clone()).unwrap();
    let declaration = ArtifactDeclaration {
        name: "payment".into(),
        context: ArtifactContext::TypeGroup {},
        bindings: EntryCellBindings::TransactionAbsolute,
        coverage: EntryCoverage::ActionDefined,
        layout: None,
        dispatch: ArtifactDispatch::SingleEntry,
        action: Some("settle".into()),
        actions: Vec::new(),
        common_checks: Vec::new(),
    };
    let result = compile_artifact(source, options, declaration, ExecutableSurfacePolicy::AllowFailClosed).unwrap();
    result.validate().unwrap();
    assert_eq!(result.artifact_bytes, implicit.artifact_bytes);
    let typed = &result.metadata.typed_semantics;
    let cells = typed.foundation.entry_contract.cells.as_ref().unwrap();
    assert_eq!(cells.coverage, EntryCoverage::ActionDefined);
    assert!(cells.layout.is_none());
    let bindings = &typed.entries[0].cell_bindings;
    for (name, schema, source, ordinal) in [
        ("state", "State", CellBindingSource::Input, 0),
        ("fee", "Fee", CellBindingSource::Input, 1),
        ("next", "State", CellBindingSource::Output, 0),
    ] {
        let binding = bindings.iter().find(|binding| binding.binding == name).unwrap();
        assert_eq!((binding.ty.as_str(), binding.source, binding.ordinal), (schema, source, ordinal));
    }
}

#[test]
fn membership_helper_has_explicit_frame_evidence_and_is_demand_driven() {
    let options = CompileOptions { target: Some("riscv64-elf".to_string()), ..Default::default() };
    let native = compile(NATIVE_REPLACEMENT, CompileOptions { edition: CURRENT_EDITION, ..options.clone() }).unwrap();
    native.validate().expect("membership helper's frame and machine ownership validate");
    let entries = &native.verified_lowering_record.as_ref().unwrap().entries;
    let helper = entries.iter().find(|entry| entry.name == "__cellscript_require_cell_membership").expect("shared membership helper");
    assert_eq!(helper.frame_size_bytes, 96);
    let ordinary = compile(ALWAYS_SUCCESS, options).unwrap();
    assert!(!ordinary.verified_lowering_record.as_ref().unwrap().entries.iter().any(|entry| entry.name == helper.name));
}

fn execute_replacement(prepend_foreign_group: bool, input_amount: u64, output_amount: u64) -> Result<(), String> {
    execute_replacement_case(prepend_foreign_group, prepend_foreign_group, input_amount, output_amount, 0, 0, 0)
}

fn execute_replacement_case(
    prepend_input: bool,
    prepend_output: bool,
    input_amount: u64,
    output_amount: u64,
    extra_inputs: usize,
    extra_outputs: usize,
    capacity_delta: u64,
) -> Result<(), String> {
    let extra_input_amounts = vec![7; extra_inputs];
    let extra_output_amounts = vec![7; extra_outputs];
    execute_source_replacement_case(
        NATIVE_REPLACEMENT,
        (prepend_input, prepend_output),
        input_amount,
        output_amount,
        (&extra_input_amounts, &extra_output_amounts),
        capacity_delta,
    )
}

fn execute_source_replacement_case(
    source: &str,
    positions: (bool, bool),
    input_amount: u64,
    output_amount: u64,
    extras: (&[u64], &[u64]),
    capacity_delta: u64,
) -> Result<(), String> {
    execute_source_replacement_with_flags(source, positions, input_amount, output_amount, extras, capacity_delta, None)
}

fn execute_source_replacement_with_flags(
    source: &str,
    positions: (bool, bool),
    input_amount: u64,
    output_amount: u64,
    extras: (&[u64], &[u64]),
    capacity_delta: u64,
    flags: Option<(bool, bool)>,
) -> Result<(), String> {
    let (prepend_input, prepend_output) = positions;
    let (extra_inputs, extra_outputs) = extras;
    let mut context = Context::new_with_deterministic_rng();
    let options = CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() };
    let foreign = compile(ALWAYS_SUCCESS, options.clone()).unwrap();
    let foreign_code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&foreign.artifact_bytes)));
    let foreign_script = context.build_script(&foreign_code, Bytes::new()).unwrap();
    let source = source.replace("recipient: Address", "recipient: ScriptHash").replace("exact(recipient)", "exact_hash(recipient)");
    let native =
        compile(&source, CompileOptions { edition: CURRENT_EDITION, ..options }).unwrap_or_else(|error| panic!("{error}\n{source}"));
    native.validate().expect("native artifact bundle must validate independently");
    let native_code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&native.artifact_bytes)));
    let native_script = context.build_script(&native_code, Bytes::new()).unwrap();
    let mut arguments = vec![cellscript::EntryWitnessArg::Bytes(foreign_script.calc_script_hash().as_slice().to_vec())];
    if let Some((outer, inner)) = flags {
        arguments.extend([cellscript::EntryWitnessArg::Bool(outer), cellscript::EntryWitnessArg::Bool(inner)]);
    }
    let payload = native.metadata.actions[0].entry_witness_args(&arguments).unwrap();
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();

    let make_cell = |policy: &packed::Script| {
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(100_000_000_000u64.pack())
            .lock(foreign_script.clone())
            .type_(Some(policy.clone()).pack())
            .build()
    };
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut output_data = Vec::new();
    if prepend_input {
        let cell = make_cell(&foreign_script);
        let out_point = context.create_cell(cell.clone(), Bytes::copy_from_slice(&7u64.to_le_bytes()));
        inputs.push(packed::CellInput::new_builder().previous_output(out_point).build());
    }
    if prepend_output {
        let cell = make_cell(&foreign_script);
        outputs.push(cell);
        output_data.push(Bytes::copy_from_slice(&7u64.to_le_bytes()));
    }
    let cell = make_cell(&native_script);
    let out_point = context.create_cell(cell.clone(), Bytes::copy_from_slice(&input_amount.to_le_bytes()));
    inputs.push(packed::CellInput::new_builder().previous_output(out_point).build());
    outputs.push(cell.as_builder().capacity::<packed::Uint64>((100_000_000_000u64 + capacity_delta).pack()).build());
    output_data.push(Bytes::copy_from_slice(&output_amount.to_le_bytes()));
    for amount in extra_inputs {
        let out_point = context.create_cell(make_cell(&native_script), Bytes::copy_from_slice(&amount.to_le_bytes()));
        inputs.push(packed::CellInput::new_builder().previous_output(out_point).build());
    }
    for amount in extra_outputs {
        outputs.push(make_cell(&native_script));
        output_data.push(Bytes::copy_from_slice(&amount.to_le_bytes()));
    }
    let mut witnesses = if prepend_input { vec![Bytes::new()] } else { Vec::new() };
    witnesses.push(witness);
    let tx = context.complete_tx(
        TransactionBuilder::default()
            .inputs(inputs)
            .outputs(outputs)
            .outputs_data(output_data.pack())
            .witnesses(witnesses.pack())
            .build(),
    );
    context.verify_tx(&tx, 10_000_000).map(|_| ()).map_err(|error| format!("{error:?}"))
}

#[test]
fn native_replacement_checks_its_zero_index_group() {
    execute_replacement(false, 7, 7).expect("valid native replacement");
    assert!(execute_replacement(false, 8, 9).is_err(), "native verifier must reject a bad input and successor");
}

#[test]
fn native_replacement_checks_nonzero_group_indices_instead_of_foreign_cells() {
    execute_replacement(true, 7, 7).expect("unrelated leading Cells must not change a valid native replacement");
    assert!(
        execute_replacement(true, 8, 9).is_err(),
        "foreign Cells matching the policy must not disguise a bad input and successor in the active Type Script group"
    );
}

#[test]
fn native_input_and_successor_ordinals_are_independently_group_relative() {
    for (input, output) in [(false, true), (true, false), (true, true)] {
        execute_replacement_case(input, output, 7, 7, 0, 0, 0)
            .expect("independent absolute positions preserve valid group replacement");
        assert!(execute_replacement_case(input, output, 7, 9, 0, 0, 0).is_err(), "group successor data must be checked");
        assert!(execute_replacement_case(input, output, 8, 8, 0, 0, 0).is_err(), "group input business predicate must be checked");
        assert!(execute_replacement_case(input, output, 7, 7, 0, 0, 1).is_err(), "group successor capacity must be checked");
    }
}

#[test]
fn native_fixed_relations_reject_uncovered_group_members() {
    assert!(execute_replacement_case(true, true, 7, 7, 1, 0, 0).is_err(), "extra current-group input is not disposed");
    assert!(execute_replacement_case(true, true, 7, 7, 0, 1, 0).is_err(), "extra current-group output is not constrained");
}

fn branch_successor_source(use_match: bool) -> String {
    let start = NATIVE_REPLACEMENT.find("        require before.amount == 7;").unwrap();
    let relation_start = NATIVE_REPLACEMENT.find("        replace before -> after").unwrap();
    let end = NATIVE_REPLACEMENT.rfind("\n    }").unwrap();
    let relation = &NATIVE_REPLACEMENT[relation_start..end];
    let body = if use_match {
        format!("        let amount = before.amount;\n        let choice = if amount == 7 {{ Branch::Seven }} else {{ Branch::Eight }};\n        match choice {{\n            Branch::Seven => {{\n                require amount == 7;\n{relation}\n            }}\n            Branch::Eight => {{\n                require amount == 8;\n{relation}\n            }}\n        }}")
    } else {
        format!("        let amount = before.amount;\n        if amount > 7 {{\n            require amount == 8;\n{relation}\n        }} else {{\n            require amount == 7;\n{relation}\n        }}")
    };
    let source = format!("{}{body}{}", &NATIVE_REPLACEMENT[..start], &NATIVE_REPLACEMENT[end..]);
    if use_match {
        source.replacen("resource Token", "enum Branch { Seven, Eight }\nresource Token", 1)
    } else {
        source
    }
}

#[test]
fn ordinary_branch_bodies_preserve_group_coverage_and_checked_successors() {
    for use_match in [false, true] {
        let source = branch_successor_source(use_match);
        let formatted = cellscript::fmt::format_source(&source, CURRENT_EDITION, Default::default()).unwrap();
        assert!(!formatted.contains("verification"));
        cellscript::fmt::verify_idempotent_for_edition(&formatted, CURRENT_EDITION, Default::default()).unwrap();
        for amount in [7, 8] {
            execute_source_replacement_case(&formatted, (true, true), amount, amount, (&[], &[]), 0)
                .unwrap_or_else(|error| panic!("accepting branch use_match={use_match}, amount={amount}: {error}"));
        }
        assert!(execute_source_replacement_case(&formatted, (true, true), 9, 9, (&[], &[]), 0).is_err());
        assert!(execute_source_replacement_case(&formatted, (true, true), 8, 7, (&[], &[]), 0).is_err());
        assert!(execute_source_replacement_case(&formatted, (true, true), 7, 7, (&[], &[7]), 0).is_err());
    }
}

#[test]
fn branch_local_successor_mapping_uses_declared_output_roles_in_the_real_vm() {
    let source = include_str!("syntax_combo/seeds/group-branch-successor.cell");
    execute_source_replacement_case(source, (true, true), 8, 8, (&[7], &[7]), 0)
        .expect("first branch preserves declared left-to-first and right-to-second mapping");
    execute_source_replacement_case(source, (true, true), 9, 7, (&[7], &[9]), 0)
        .expect("second branch checks swapped successors despite reverse textual output order");
    assert!(execute_source_replacement_case(source, (true, true), 9, 9, (&[7], &[7]), 0).is_err());
    assert!(execute_source_replacement_case(source, (true, true), 8, 7, (&[7], &[8]), 0).is_err());
    assert!(execute_source_replacement_case(source, (true, true), 9, 7, (&[7, 3], &[9]), 0).is_err());
    assert!(execute_source_replacement_case(source, (true, true), 9, 7, (&[7], &[9, 3]), 0).is_err());
}

#[test]
fn nested_sibling_and_join_field_reads_execute_in_their_own_cfg_paths() {
    let start = NATIVE_REPLACEMENT.find("        require before.amount == 7;").unwrap();
    let relation_start = NATIVE_REPLACEMENT.find("        replace before -> after").unwrap();
    let end = NATIVE_REPLACEMENT.rfind("\n    }").unwrap();
    let relation = &NATIVE_REPLACEMENT[relation_start..end];
    // The first field read must stay inside each branch. An unconditional read
    // before the outer if would dominate its siblings and hide the cache bug.
    let body = format!("        if flag {{\n            require before.amount > 0;\n{relation}\n        }} else {{\n            if flag2 {{ require before.amount > 0; }} else {{ require before.amount > 0; }}\n{relation}\n        }}\n        require after.amount == 7;");
    let source = format!("{}{body}{}", &NATIVE_REPLACEMENT[..start], &NATIVE_REPLACEMENT[end..]).replace(
        "witness recipient: Address,",
        "witness recipient: Address,\n        witness flag: bool,\n        witness flag2: bool,",
    );
    for outer in [false, true] {
        for inner in [false, true] {
            let flags = Some((outer, inner));
            execute_source_replacement_with_flags(&source, (true, true), 7, 7, (&[], &[]), 0, flags)
                .unwrap_or_else(|error| panic!("accepting nested branch outer={outer}, inner={inner}: {error}"));
            assert!(execute_source_replacement_with_flags(&source, (true, true), 7, 8, (&[], &[]), 0, flags).is_err());
            assert!(execute_source_replacement_with_flags(&source, (true, true), 0, 0, (&[], &[]), 0, flags).is_err());
            assert!(
                execute_source_replacement_with_flags(&source, (true, true), 8, 8, (&[], &[]), 0, flags).is_err(),
                "the output field condition after the join remains executable"
            );
        }
    }
}
