//! Native creation and retirement must bind their own Type Script group,
//! including the side on which that group must be empty.

use cellscript::{compile, strip_vm_abi_trailer, CompileOptions, CompileResult, EntryWitnessArg, NEXT_EDITION};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

const FRESH: &str = r#"
module native_lifecycle::fresh
resource Token has store, create { amount: u64 }
type_script TokenPolicy on type_group<Token> {
    entry mint(
        witness recipient: Address from group_witness.input_type,
        output after: Token from group_output[0],
    ) {
        verify { enforce true }
        effects {
            fresh after {
                data { amount = 7 }
                identity = none
                type_script = declared
                lock_script = exact_hash(recipient)
                capacity = builder_computed
                cardinality = one
            }
        }
    }
}
"#;

const RETIRE: &str = r#"
module native_lifecycle::retire
resource Token has store, consume, burn { amount: u64 }
type_script TokenPolicy on type_group<Token> {
    entry burn(input before: Token from group_input[0]) {
        verify { enforce before.amount == 7 }
        effects {
            retire before {
                absence = singleton_type
                data = discarded
                lock_script = none
                type_script = absent
                capacity = released
                cardinality = one
            }
        }
    }
}
"#;

fn compile_native(source: &str) -> CompileResult {
    let result =
        compile(source, CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".to_string()), ..Default::default() })
            .expect("native lifecycle fixture compiles");
    result.validate().expect("native lifecycle bundle independently validates");
    result
}

fn execute_lifecycle(
    native: &CompileResult,
    native_inputs: &[u64],
    native_outputs: &[u64],
    foreign_input: bool,
    foreign_output: bool,
    has_witness: bool,
) -> Result<u64, String> {
    let mut context = Context::new_with_deterministic_rng();
    let foreign = compile(
        "module foreign_lock\nlock allow() -> bool { verification true }\n",
        CompileOptions { target: Some("riscv64-elf".to_string()), ..Default::default() },
    )
    .unwrap();
    let foreign_code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&foreign.artifact_bytes)));
    let foreign_script = context.build_script(&foreign_code, Bytes::new()).unwrap();
    let native_code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&native.artifact_bytes)));
    let native_script = context.build_script(&native_code, Bytes::new()).unwrap();
    let make_cell = |in_native_group: bool| {
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(100_000_000_000u64.pack())
            .lock(foreign_script.clone())
            .type_(in_native_group.then(|| native_script.clone()).pack())
            .build()
    };
    let mut transaction = TransactionBuilder::default();
    if foreign_input {
        let out_point = context.create_cell(make_cell(false), Bytes::copy_from_slice(&7u64.to_le_bytes()));
        transaction = transaction.input(packed::CellInput::new_builder().previous_output(out_point).build());
    }
    for amount in native_inputs {
        let out_point = context.create_cell(make_cell(true), Bytes::copy_from_slice(&amount.to_le_bytes()));
        transaction = transaction.input(packed::CellInput::new_builder().previous_output(out_point).build());
    }
    if foreign_output {
        transaction = transaction.output(make_cell(false)).output_data(Bytes::copy_from_slice(&7u64.to_le_bytes()).pack());
    }
    for amount in native_outputs {
        transaction = transaction.output(make_cell(true)).output_data(Bytes::copy_from_slice(&amount.to_le_bytes()).pack());
    }
    if has_witness {
        let hash: [u8; 32] = foreign_script.calc_script_hash().unpack();
        let argument = if native.metadata.actions[0].params[0].ty == "ScriptHash" {
            EntryWitnessArg::Bytes(hash.to_vec())
        } else {
            EntryWitnessArg::Address(hash)
        };
        let payload = native.metadata.actions[0].entry_witness_args(&[argument]).unwrap();
        let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();
        let index = if native_inputs.is_empty() { usize::from(foreign_output) } else { usize::from(foreign_input) };
        let mut witnesses = vec![Bytes::new(); index];
        witnesses.push(witness);
        transaction = transaction.witnesses(witnesses.pack());
    }
    let transaction = context.complete_tx(transaction.build());
    context.verify_tx(&transaction, 10_000_000).map_err(|error| format!("{error:?}"))
}

fn assert_exit_code(error: &str, code: i8) {
    assert!(
        error.contains(&format!("error code {code}")) || error.contains(&format!("error code: {code}")),
        "wrong rejection: {error}"
    );
}

#[test]
fn fresh_output_binds_nonzero_output_group_and_rejects_nonempty_input_side() {
    let native = compile_native(FRESH);
    assert!(execute_lifecycle(&native, &[], &[7], true, false, true).expect("output-only group at Output[0]") > 0);
    assert!(execute_lifecycle(&native, &[], &[7], true, true, true).expect("output-only group at Output[1]") > 0);
    assert_exit_code(
        &execute_lifecycle(&native, &[], &[8], true, true, true).expect_err("foreign correct data cannot mask bad creation"),
        3,
    );
    assert_exit_code(
        &execute_lifecycle(&native, &[7], &[7], true, true, true).expect_err("fresh role requires an empty input group"),
        21,
    );
    assert_exit_code(
        &execute_lifecycle(&native, &[], &[7, 7], true, true, true).expect_err("fresh role covers exactly one output"),
        21,
    );
}

#[test]
fn nominal_script_hash_fresh_output_executes_with_the_existing_witness_abi() {
    let native = compile_native(&FRESH.replace("recipient: Address", "recipient: ScriptHash"));
    assert!(execute_lifecycle(&native, &[], &[7], true, true, true).unwrap() > 0);
    assert!(execute_lifecycle(&native, &[], &[8], true, true, true).is_err());
}

#[test]
fn retirement_binds_nonzero_input_group_and_rejects_nonempty_output_side() {
    let native = compile_native(RETIRE);
    assert!(execute_lifecycle(&native, &[7], &[], false, true, false).expect("retirement at Input[0]") > 0);
    assert!(execute_lifecycle(&native, &[7], &[], true, true, false).expect("retirement at Input[1]") > 0);
    assert_exit_code(
        &execute_lifecycle(&native, &[8], &[], true, true, false).expect_err("foreign correct data cannot mask bad retirement"),
        5,
    );
    assert_exit_code(
        &execute_lifecycle(&native, &[7], &[7], true, true, false).expect_err("retired Type group must have no outputs"),
        21,
    );
    assert_exit_code(
        &execute_lifecycle(&native, &[7, 7], &[], true, true, false).expect_err("retire role covers exactly one input"),
        21,
    );
}
