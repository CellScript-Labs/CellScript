use cellscript::{compile, CompileOptions, NEXT_EDITION};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

const NATIVE_REPLACEMENT: &str = r#"
module native_group_binding
resource Token has store, replace, relock { amount: u64 }
type_script TokenPolicy on type_group<Token> {
    entry retain_value(
        input before: Token from group_input[0],
        witness recipient: Address from group_witness.input_type,
        output after: Token from group_output[0],
    ) {
        verify { enforce before.amount == 7 }
        effects {
            replace before -> after {
                data { amount = same }
                identity = same
                type_script = same
                lock_script = exact_hash(recipient)
                capacity = same
                cardinality = one_to_one
            }
        }
    }
}
"#;

const ALWAYS_SUCCESS: &str = r#"
module unrelated_group
action main() -> u64 { verification return 0 }
"#;

#[test]
fn membership_helper_has_explicit_frame_evidence_and_is_demand_driven() {
    let options = CompileOptions { target: Some("riscv64-elf".to_string()), ..Default::default() };
    let native = compile(NATIVE_REPLACEMENT, CompileOptions { edition: NEXT_EDITION, ..options.clone() }).unwrap();
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
    let mut context = Context::new_with_deterministic_rng();
    let options = CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() };
    let foreign = compile(ALWAYS_SUCCESS, options.clone()).unwrap();
    let foreign_code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&foreign.artifact_bytes)));
    let foreign_script = context.build_script(&foreign_code, Bytes::new()).unwrap();
    let source = NATIVE_REPLACEMENT.replace("recipient: Address", "recipient: ScriptHash");
    let native = compile(&source, CompileOptions { edition: NEXT_EDITION, ..options }).unwrap();
    native.validate().expect("native artifact bundle must validate independently");
    let native_code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&native.artifact_bytes)));
    let native_script = context.build_script(&native_code, Bytes::new()).unwrap();
    let payload = native.metadata.actions[0]
        .entry_witness_args(&[cellscript::EntryWitnessArg::Bytes(foreign_script.calc_script_hash().as_slice().to_vec())])
        .unwrap();
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
    for _ in 0..extra_inputs {
        let out_point = context.create_cell(make_cell(&native_script), Bytes::copy_from_slice(&7u64.to_le_bytes()));
        inputs.push(packed::CellInput::new_builder().previous_output(out_point).build());
    }
    for _ in 0..extra_outputs {
        outputs.push(make_cell(&native_script));
        output_data.push(Bytes::copy_from_slice(&7u64.to_le_bytes()));
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
