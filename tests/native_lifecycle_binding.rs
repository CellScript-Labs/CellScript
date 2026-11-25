//! Native creation and retirement must bind their own Type Script group,
//! including the side on which that group must be empty.

#[path = "support/fixed_action.rs"]
mod fixed_action;
use fixed_action::compile_fixture as compile;

use cellscript::{strip_vm_abi_trailer, CompileOptions, CompileResult, EntryWitnessArg, CURRENT_EDITION};
use cellscript_artifact_checker::{InputDisposition, OutputOrigin};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

const FRESH: &str = r#"
module native_lifecycle::fresh;
resource Token has store, create { amount: u64, }
action mint(
        witness recipient: Address,
        output after: Token,
    ) {
        require true;
        fresh after {
            data { amount = 7, };
            identity = none;
            lock = exact(recipient);
        }
    }
"#;

const RETIRE: &str = r#"
module native_lifecycle::retire;
resource Token has store, consume, burn { amount: u64, }
action burn(input before: Token) {
        require before.amount == 7;
        retire before {
            absence = singleton_type;
        }
    }
"#;

fn compile_native(source: &str) -> CompileResult {
    let result =
        compile(source, CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..Default::default() })
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
        "module foreign_lock;\nlock allow() -> bool {  true }\n",
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
    let native = compile_native(
        &FRESH.replace("recipient: Address", "recipient: ScriptHash").replace("exact(recipient)", "exact_hash(recipient)"),
    );
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

#[test]
fn equivalent_lifecycle_forms_share_the_fixed_group_execution_plan() {
    use cellscript::artifact::{compile_artifact, ArtifactDeclaration};
    use cellscript::ExecutableSurfacePolicy;

    let create = FRESH.replace(
        "fresh after {\n            data { amount = 7, };\n            identity = none;\n            lock = exact(recipient);\n        }",
        "create after = Token { amount: 7, } with_lock(recipient);",
    );
    let destroy =
        RETIRE.replace("retire before {\n            absence = singleton_type;\n        }", "destroy_singleton_type(before);");
    assert!(!create.contains("fresh after"));
    assert!(!destroy.contains("retire before"));

    for (action, relation, primitive) in [("mint", FRESH, create.as_str()), ("burn", RETIRE, destroy.as_str())] {
        let compile = |source| {
            let result = compile_artifact(
                source,
                CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
                ArtifactDeclaration::single_type_group("fixture", action, "Token"),
                ExecutableSurfacePolicy::AllowFailClosed,
            )
            .unwrap();
            result.validate().unwrap();
            result
        };
        let relation = compile(relation);
        let primitive = compile(primitive);
        let left = &relation.metadata.typed_semantics;
        let right = &primitive.metadata.typed_semantics;
        assert_eq!(left.entries[0].cell_bindings, right.entries[0].cell_bindings);
        assert_eq!(left.foundation.entry_contract.cells, right.foundation.entry_contract.cells);
        let proofs = |result: &CompileResult| {
            result.metadata.actions[0]
                .proof_plan
                .iter()
                .map(|proof| (proof.feature.clone(), proof.evidence_tier, proof.on_chain_checked))
                .collect::<Vec<_>>()
        };
        assert_eq!(proofs(&relation), proofs(&primitive));
        assert_eq!(relation.artifact_bytes, primitive.artifact_bytes);
    }
}

#[test]
fn migrated_unique_creation_preserves_witness_and_vm_checks() {
    let before = r#"
module native_lifecycle::unique_mint;
resource Token has store, create identity(field(amount)) { amount: u64, }
action mint(recipient: Address) -> Token {
    create_unique::<Token>(identity = field(amount)) { amount: 7 } with_lock(recipient)
}
"#;
    let after = r#"
module native_lifecycle::unique_mint;
resource Token has store, create identity(field(amount)) { amount: u64, }
action mint(recipient: Address) -> minted: Token {
    fresh minted {
        data { amount = 7, };
        identity = field(amount);
        lock = exact(recipient);
    }
}
"#;
    let compile = |source| {
        let result = cellscript::compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
        result.validate().unwrap();
        result
    };
    let before = compile(before);
    let after = compile(after);
    let payload =
        |result: &CompileResult| result.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Address([5; 32])]).unwrap();
    assert_eq!(payload(&before), payload(&after));
    for result in [&before, &after] {
        assert!(execute_lifecycle(result, &[], &[7], true, false, true).expect("valid creation") > 0);
        assert_exit_code(&execute_lifecycle(result, &[], &[8], true, false, true).expect_err("wrong identity data"), 3);
        assert!(execute_lifecycle(result, &[], &[7], true, false, false).is_err(), "a missing Lock witness must reject");
    }
}

#[test]
fn fresh_uses_the_resolved_output_and_keeps_the_unclassified_input_disposition() {
    let source = r#"
module native_lifecycle::mixed;
resource Fee has store, consume { amount: u64, }
resource Token has store, create { amount: u64, }
action issue(input fee: Fee, witness recipient: Address, output issued: Token) {
    if fee.amount == 1 {
        consume fee;
    } else {
        consume fee;
    }
    fresh issued {
        data { amount = 7, };
        identity = none;
        lock = exact(recipient);
    }
}
"#;
    let result = cellscript::compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
    result.validate().unwrap();
    let typed = &result.metadata.typed_semantics;
    let output = typed.entries[0].cell_bindings.iter().find(|binding| binding.binding == "issued").unwrap();
    assert_eq!(output.selector(), "output[0]");
    let dispositions = &typed.foundation.dispositions;
    assert_eq!(dispositions.len(), 2);
    let fresh = dispositions.iter().find(|item| matches!(item.output, Some(OutputOrigin::Fresh { .. }))).unwrap();
    assert_eq!(fresh.envelope.correspondence, output.selector());
    let consumed = dispositions.iter().find_map(|item| item.input.as_ref()).unwrap();
    assert!(matches!(consumed, InputDisposition::Consumed { operation, boundary }
        if operation == "consume" && boundary == "business-disposition-not-established"));
}
