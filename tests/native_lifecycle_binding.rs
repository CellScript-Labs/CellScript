//! Native creation and retirement must bind their own Type Script group,
//! including the side on which that group must be empty.

#[path = "support/fixed_action.rs"]
mod fixed_action;
use fixed_action::compile_fixture as compile;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use cellscript::{strip_vm_abi_trailer, CompileOptions, CompileResult, EntryWitnessArg, CURRENT_EDITION};
use cellscript_artifact_checker::{InputDisposition, OutputOrigin};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

const FRESH: &str = r#"// test-artifact: {"entry":"mint","layout":"Token"}

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

const RETIRE: &str = r#"// test-artifact: {"entry":"burn","layout":"Token"}

module native_lifecycle::retire;
resource Token has store, consume, burn { amount: u64, }
action burn(input before: Token) {
        require before.amount == 7;
        retire before {
            absence = singleton_type;
        }
    }
"#;

#[test]
fn absolute_fresh_and_pool_report_no_type_group_authority_from_a_nominal_schema() {
    use ckb_script_runner::{
        build_simple_fixture, deterministic_always_success_lock_hash, deterministic_always_success_script, execute_cellscript_script,
    };
    let mut policies = Vec::new();
    for (name, source) in [
        ("fresh", include_str!("syntax_combo/authoring/absolute-fresh.cell")),
        ("pool", include_str!("syntax_combo/authoring/absolute-pool.cell")),
    ] {
        let result = cellscript::compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
        result.validate().unwrap();
        let mut fixture = build_simple_fixture(Bytes::new(), 1, 2);
        fixture.current_type_script_input_indices = vec![0];
        fixture.inputs[0].data = Bytes::copy_from_slice(&7u64.to_le_bytes());
        fixture.outputs[0].type_script = Some(deterministic_always_success_script(Bytes::new()));
        fixture.outputs[0].data = Bytes::copy_from_slice(&7u64.to_le_bytes());
        fixture.outputs[1].data = Bytes::copy_from_slice(&9u64.to_le_bytes());
        let payload = result.metadata.actions[0]
            .entry_witness_args(&[EntryWitnessArg::Address(deterministic_always_success_lock_hash())])
            .unwrap();
        fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        let valid = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
        assert_eq!(valid.exit_code, 0, "{name}: {valid:?}");
        fixture.outputs[0].data = Bytes::copy_from_slice(&8u64.to_le_bytes());
        let invalid = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
        assert_ne!(invalid.exit_code, 0, "the selected Script must actually check the absolute role: {name}: {invalid:?}");
        eprintln!("policy-coordinate baseline {name}: {}", cellscript_artifact_checker::hex_encode(&result.artifact_hash));
        for disposition in &result.metadata.typed_semantics.foundation.dispositions {
            if disposition.lifecycle_execution.is_some() {
                policies.push((name, disposition.envelope.type_script.clone()));
            }
        }
    }
    assert!(policies.iter().all(|(_, policy)| policy == "unconstrained-by-this-relation"), "{policies:?}");
}

fn compile_native(source: &str) -> CompileResult {
    let result =
        compile(source, CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..Default::default() })
            .expect("native lifecycle fixture compiles");
    result.validate().expect("native lifecycle bundle independently validates");
    let entry = &result.metadata.typed_semantics.entries[0];
    for access in &result.metadata.actions[0].ckb_runtime_accesses {
        if access.syscall != "LOAD_CELL" {
            continue;
        }
        let bindings = entry.cell_bindings.iter().filter(|binding| binding.binding == access.binding).collect::<Vec<_>>();
        assert!(
            bindings.iter().any(|binding| {
                let source = match binding.source {
                    cellscript_artifact_checker::CellBindingSource::Input => "Input",
                    cellscript_artifact_checker::CellBindingSource::Output => "Output",
                    cellscript_artifact_checker::CellBindingSource::GroupInput => "GroupInput",
                    cellscript_artifact_checker::CellBindingSource::GroupOutput => "GroupOutput",
                    cellscript_artifact_checker::CellBindingSource::CellDep => "CellDep",
                };
                access.source == source
                    && access.index == binding.ordinal as usize
                    && access.provenance.source.resolved_source == source
            }),
            "runtime access must use a real physical role, including alternative branches: {access:?}"
        );
    }
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
    execute_lifecycle_branch(native, native_inputs, native_outputs, foreign_input, foreign_output, has_witness, None)
}

fn execute_lifecycle_branch(
    native: &CompileResult,
    native_inputs: &[u64],
    native_outputs: &[u64],
    foreign_input: bool,
    foreign_output: bool,
    has_witness: bool,
    choose: Option<bool>,
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
        let arguments = if choose.is_none() {
            vec![if native.metadata.actions[0].params[0].ty == "ScriptHash" {
                EntryWitnessArg::Bytes(hash.to_vec())
            } else {
                EntryWitnessArg::Address(hash)
            }]
        } else {
            native.metadata.actions[0]
                .params
                .iter()
                .filter(|param| param.witness_data_source || param.source == "witness")
                .map(|param| match param.ty.as_str() {
                    "bool" => EntryWitnessArg::Bool(choose.expect("branch witness")),
                    "ScriptHash" => EntryWitnessArg::Bytes(hash.to_vec()),
                    "Address" | "address" => EntryWitnessArg::Address(hash),
                    other => panic!("unsupported lifecycle fixture witness: {other}"),
                })
                .collect::<Vec<_>>()
        };
        let payload = native.metadata.actions[0].entry_witness_args(&arguments).unwrap();
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
fn branch_local_lifecycle_executes_each_selected_policy_in_ckb_vm() {
    let fresh = compile_native(include_str!("syntax_combo/authoring/branch-fresh.cell"));
    let pool = compile_native(include_str!("syntax_combo/authoring/branch-pool.cell"));
    let many = compile_native(include_str!("syntax_combo/authoring/branch-pool-many.cell"));
    let retire = compile_native(include_str!("syntax_combo/authoring/branch-retire.cell"));
    let mixed = compile_native(include_str!("syntax_combo/authoring/branch-continue-retire.cell"));
    for choose in [true, false] {
        let amount = if choose { 1 } else { 2 };
        assert!(execute_lifecycle_branch(&fresh, &[], &[amount], true, true, true, Some(choose)).unwrap() > 0);
        assert_exit_code(&execute_lifecycle_branch(&fresh, &[], &[3 - amount], true, true, true, Some(choose)).unwrap_err(), 3);
        assert!(execute_lifecycle_branch(&pool, &[7], &[7], true, true, true, Some(choose)).unwrap() > 0);
        assert_exit_code(&execute_lifecycle_branch(&pool, &[7], &[8], true, true, true, Some(choose)).unwrap_err(), 5);
        assert!(execute_lifecycle_branch(&many, &[3, 4], &[2, 5], true, true, true, Some(choose)).unwrap() > 0);
        assert_exit_code(&execute_lifecycle_branch(&many, &[3, 4], &[2, 6], true, true, true, Some(choose)).unwrap_err(), 5);
        assert!(execute_lifecycle_branch(&many, &[u64::MAX, 1], &[u64::MAX, 1], true, true, true, Some(choose)).unwrap() > 0);
        assert_exit_code(&execute_lifecycle_branch(&many, &[u64::MAX, 1], &[0, 0], true, true, true, Some(choose)).unwrap_err(), 5);
        assert!(execute_lifecycle_branch(&retire, &[1, 2], &[], true, true, true, Some(choose)).unwrap() > 0);
        assert!(execute_lifecycle_branch(&mixed, &[1, 2], &[amount], true, true, true, Some(choose)).unwrap() > 0);
        assert!(execute_lifecycle_branch(&mixed, &[1, 2], &[3 - amount], true, true, true, Some(choose)).is_err());
        assert!(execute_lifecycle_branch(&mixed, &[1, 1], &[1], true, true, true, Some(choose)).is_err());
    }
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
action mint(witness recipient: Address) -> Token {
    create_unique::<Token>(identity = field(amount)) { amount: 7 } with_lock(recipient)
}
"#;
    let after = r#"// test-artifact: {"entry":"mint","layout":"Token"}

module native_lifecycle::unique_mint;
resource Token has store, create identity(field(amount)) { amount: u64, }
action mint(witness recipient: Address,  output minted: Token) {
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
fn omitted_identity_preserves_native_bytes_witnesses_and_selected_vm_checks() {
    for policy in ["script_args", "singleton_type", "ckb_type_id"] {
        let type_id = if policy == "ckb_type_id" { "#[type_id(\"identity_defaults::Token\")]" } else { "" };
        let schema = format!("{type_id} resource Token has store,create,consume,replace identity({policy}) {{amount:u64,}}");
        for (params, result, body) in [
            (
                "witness recipient:Address,output next:Token",
                "",
                format!("fresh next {{data {{amount=7,}};identity={policy};lock=exact(recipient);}}"),
            ),
            (
                "witness recipient:Address",
                "->Token",
                format!("create_unique::<Token>(identity={policy}) {{amount:7,}} with_lock(recipient)"),
            ),
            ("input before:Token", "->Token", format!("replace_unique::<Token>(identity={policy}) before {{amount:before.amount,}}")),
        ] {
            let source = format!("module identity_defaults; {schema} action mint({params}){result} {{{body}}}");
            let omitted = source.replace(&format!("identity={policy};"), "").replace(&format!("identity={policy}"), "");
            let explicit = compile_native(&source);
            let derived = compile_native(&omitted);
            assert_eq!(explicit.artifact_bytes, derived.artifact_bytes, "{policy}, {body}");
            assert_eq!(explicit.metadata.typed_semantics, derived.metadata.typed_semantics, "{policy}, {body}");
        }
    }
    let field = FRESH
        .replace("has store, create {", "has store, create identity(field(amount)) {")
        .replace("identity = none;", "identity = field(amount);");
    let anonymous = "module identity_defaults; resource Token has store,create identity(field(amount)) {amount:u64,} action mint(witness recipient:Address)->Token {create_unique::<Token>(identity = field(amount)) {amount:7,} with_lock(recipient)}";
    for source in [FRESH, field.as_str(), anonymous] {
        let omitted =
            source.replace("identity = none;", "").replace("identity = field(amount);", "").replace("identity = field(amount)", "");
        assert_ne!(source, omitted);
        let explicit = compile_native(source);
        let derived = compile_native(&omitted);
        assert_eq!(explicit.artifact_bytes, derived.artifact_bytes);
        assert_eq!(explicit.metadata.typed_semantics, derived.metadata.typed_semantics);
        let payload =
            |result: &CompileResult| result.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Address([5; 32])]).unwrap();
        assert_eq!(payload(&explicit), payload(&derived));
        for result in [&explicit, &derived] {
            assert!(execute_lifecycle(result, &[], &[7], false, false, true).unwrap() > 0);
            assert!(execute_lifecycle(result, &[], &[8], false, false, true).is_err());
            assert!(execute_lifecycle(result, &[], &[7], false, false, false).is_err());
        }
    }
    let explicit_source = include_str!("syntax_combo/authoring/branch-unique-retire.cell");
    let derived_source = explicit_source.replace("identity = field(amount)", "");
    let explicit = compile_native(explicit_source);
    let derived = compile_native(&derived_source);
    assert_eq!(explicit.artifact_bytes, derived.artifact_bytes);
    assert_eq!(explicit.metadata.typed_semantics, derived.metadata.typed_semantics);
    for result in [&explicit, &derived] {
        assert!(execute_lifecycle_branch(result, &[7], &[7], false, false, true, Some(true)).unwrap() > 0);
        assert!(execute_lifecycle_branch(result, &[7], &[8], false, false, true, Some(true)).is_err());
        assert!(execute_lifecycle_branch(result, &[7], &[], false, false, true, Some(false)).unwrap() > 0);
    }
}

#[test]
fn fresh_uses_the_resolved_output_and_keeps_the_unclassified_input_disposition() {
    let source = r#"// test-artifact: {"entry":"issue","layout":"Token"}

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
    assert_eq!(dispositions.len(), 3);
    let kernel_sites = dispositions.iter().filter_map(|item| item.kernel_execution.as_ref()).collect::<Vec<_>>();
    assert_eq!(kernel_sites.len(), 2);
    assert_ne!(kernel_sites[0].block, kernel_sites[1].block);
    let fresh = dispositions.iter().find(|item| matches!(item.output, Some(OutputOrigin::Fresh { .. }))).unwrap();
    assert_eq!(fresh.envelope.correspondence, output.selector());
    let consumed = dispositions.iter().find_map(|item| item.input.as_ref()).unwrap();
    assert!(matches!(consumed, InputDisposition::Consumed { operation, boundary }
        if operation == "consume" && boundary == "business-disposition-not-established"));
}

#[test]
fn mixed_structured_and_kernel_branches_keep_their_own_execution_sites() {
    let retire = compile_native(include_str!("syntax_combo/authoring/branch-retire-consume.cell"));
    let fresh = compile_native(include_str!("syntax_combo/authoring/branch-fresh-kernel.cell"));
    for result in [&retire, &fresh] {
        let dispositions = &result.metadata.typed_semantics.foundation.dispositions;
        assert_eq!(dispositions.len(), 2);
        assert_eq!(dispositions.iter().filter(|record| record.kernel_execution.is_some()).count(), 1);
        assert_eq!(dispositions.iter().filter(|record| record.lifecycle_execution.is_some()).count(), 1);
        assert!(cellscript::semantic_expansion::render(&result.metadata).unwrap().contains("kernel-execution disposition="));
    }
    for choose in [true, false] {
        assert!(execute_lifecycle_branch(&retire, &[7], &[], true, false, true, Some(choose)).unwrap() > 0);
        let amount = if choose { 1 } else { 2 };
        assert!(execute_lifecycle_branch(&fresh, &[], &[amount], true, false, true, Some(choose)).unwrap() > 0);
        assert_exit_code(&execute_lifecycle_branch(&fresh, &[], &[3 - amount], true, false, true, Some(choose)).unwrap_err(), 3);
    }
}

#[test]
fn a_raw_conditional_output_keeps_its_partial_validation_contract() {
    let source = r#"
module lifecycle::conditional_kernel;
resource Token has store, create { amount: u64, }
action mint(witness recipient: Address, witness selected: bool, output token: Token) {
    if selected {
        create token = Token { amount: 1, } with_lock(recipient);
    } else {
        require token.amount == 2;
    }
}
"#;
    let result = cellscript::compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
    result.validate().unwrap();
    let dispositions = &result.metadata.typed_semantics.foundation.dispositions;
    assert_eq!(dispositions.len(), 1);
    assert!(dispositions[0].kernel_execution.is_some());
    assert!(execute_lifecycle_branch(&result, &[], &[1], true, false, true, Some(true)).unwrap() > 0);
    assert_exit_code(&execute_lifecycle_branch(&result, &[], &[2], true, false, true, Some(true)).unwrap_err(), 3);
    assert!(execute_lifecycle_branch(&result, &[], &[2], true, false, true, Some(false)).unwrap() > 0);
    assert!(execute_lifecycle_branch(&result, &[], &[3], true, false, true, Some(false)).is_err());
}

#[test]
fn quantity_burn_on_another_branch_does_not_relabel_the_actual_retirement() {
    let source = r#"// test-artifact: {"entry":"close","layout":"Token"}
module lifecycle::retire_or_quantity;
resource Token has store, consume, burn { amount: u64, }
action close(input token: Token, witness retire_object: bool) {
    if retire_object {
        destroy_singleton_type(token);
    } else {
        burn_amount(token, field = amount);
    }
}
"#;
    let result = compile_native(source);
    let records = &result.metadata.typed_semantics.foundation.dispositions;
    assert_eq!(records.len(), 2);
    assert_eq!(records.iter().filter(|record| matches!(record.input, Some(InputDisposition::Retired { .. }))).count(), 1);
    assert_eq!(records.iter().filter(|record| record.enforcement == "runtime-helper-required").count(), 1);
    assert!(execute_lifecycle_branch(&result, &[7], &[], true, false, true, Some(true)).unwrap() > 0);
    assert_exit_code(&execute_lifecycle_branch(&result, &[7], &[], true, false, true, Some(false)).unwrap_err(), 89);
}

#[test]
fn advanced_kernel_operations_keep_identity_receipt_and_output_contracts() {
    let unique = compile_native(include_str!("syntax_combo/authoring/branch-unique-retire.cell"));
    let claim = compile_native(include_str!("syntax_combo/authoring/receipt-claim-kernel.cell"));
    let settle = compile_native(include_str!("syntax_combo/authoring/settle-kernel.cell"));
    let mixed = compile_native(include_str!("syntax_combo/authoring/branch-claim-fresh.cell"));
    let missing_fields = include_str!("syntax_combo/authoring/branch-claim-fresh.cell").replace("{ amount, }", "{ }");
    let error =
        cellscript::compile(&missing_fields, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap_err();
    assert!(error.message.contains("amount"), "{error}");
    for result in [&unique, &claim, &settle] {
        assert!(result
            .metadata
            .typed_semantics
            .foundation
            .dispositions
            .iter()
            .any(|record| record.kernel_execution.as_ref().is_some_and(|site| site.result_local.is_some())));
    }
    let record =
        unique.metadata.typed_semantics.foundation.dispositions.iter().find(|record| record.kernel_execution.is_some()).unwrap();
    assert_eq!(record.envelope.logical_identity, "preserve-declared:field:amount");
    assert_eq!(record.envelope.capacity, "unconstrained-by-this-relation");
    assert!(execute_lifecycle_branch(&unique, &[7], &[7], false, false, true, Some(true)).unwrap() > 0);
    assert!(execute_lifecycle_branch(&unique, &[7], &[8], false, false, true, Some(true)).is_err());
    assert!(execute_lifecycle_branch(&unique, &[7], &[], false, false, true, Some(false)).unwrap() > 0);
    let receipt = claim.metadata.typed_semantics.types.iter().find(|ty| ty.name == "Voucher").unwrap();
    assert_eq!(receipt.claim_output.as_deref(), Some("Token"));
    for result in [&claim, &settle] {
        assert!(execute_lifecycle(result, &[7], &[7], false, false, false).unwrap() > 0);
        assert!(execute_lifecycle(result, &[7], &[8], false, false, false).is_err());
    }
    for choose in [true, false] {
        let amount = if choose { 7 } else { 9 };
        assert!(execute_lifecycle_branch(&mixed, &[7], &[amount], false, false, true, Some(choose)).unwrap() > 0);
        assert!(execute_lifecycle_branch(&mixed, &[7], &[16 - amount], false, false, true, Some(choose)).is_err());
    }
}

#[test]
fn a_scalar_receipt_result_keeps_its_runtime_required_boundary() {
    let source = "module lifecycle::scalar_claim; receipt Voucher has store,consume {amount:u64,} action redeem(input voucher:Voucher)->u64 {claim voucher}";
    let result = compile_native(source);
    let record = &result.metadata.typed_semantics.foundation.dispositions[0];
    assert_eq!(record.enforcement, "runtime-helper-required");
    assert_eq!(record.envelope.cardinality, "one-input-to-value-result");
    assert!(record.output_role.is_none());
    assert!(record.kernel_execution.as_ref().unwrap().result_local.is_some());
    assert_exit_code(&execute_lifecycle(&result, &[7], &[], false, false, false).unwrap_err(), 23);
}

#[test]
fn quantity_burn_keeps_its_runtime_gap_and_never_substitutes_object_absence() {
    let source = "module quantity_boundary; resource Token has store,consume,burn { amount:u64, } action burn(input token:Token) { burn_amount(token,field=amount); }";
    let metadata = cellscript::compile_metadata(source, CURRENT_EDITION, None).unwrap();
    let disposition = &metadata.typed_semantics.foundation.dispositions[0];
    assert!(matches!(&disposition.input,Some(InputDisposition::Consumed {operation,boundary})
        if operation=="burn-amount" && boundary.contains("object-absence-not-established")));
    assert_eq!(disposition.enforcement, "runtime-helper-required");
    assert_eq!(disposition.envelope.logical_identity, "application-identity-not-established");
    let options = CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() };
    let error = cellscript::compile_with_executable_surface_policy(
        source,
        options.clone(),
        cellscript::ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_err();
    assert!(error.message.contains("destroy-expression"), "{error}");
    let native = cellscript::compile(source, options).unwrap();
    native.validate().unwrap();
    let error = execute_lifecycle(&native, &[7], &[], false, true, false).unwrap_err();
    assert!(error.contains("error code 89"), "{error}");
    let retirement = source.replace("burn_amount(token,field=amount)", "destroy_singleton_type(token)");
    assert_ne!(source, retirement);
    let metadata = cellscript::compile_metadata(&retirement, CURRENT_EDITION, None).unwrap();
    assert!(matches!(metadata.typed_semantics.foundation.dispositions[0].input, Some(InputDisposition::Retired { .. })));
}
