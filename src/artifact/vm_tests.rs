//! Runtime mechanics for explicit persistent Type policies. These fixtures do
//! not authenticate token issuance or ownership and are not a token standard.

use super::*;
use crate::policy_witness::{
    decode_policy_witness_bundle, encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord, POLICY_WITNESS_MAGIC,
};
use crate::{
    strip_vm_abi_trailer, CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg, ExecutableSurfacePolicy, NEXT_EDITION,
};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};
use std::sync::OnceLock;

const SOURCE: &str = r#"
module policy_vm_mechanics
resource Token has store, consume { amount: u64 }
action common() { require true }
action mint(witness amount: u64, witness recipient: Address) {
    require amount == 7
    create Token { amount: amount } with_lock(recipient)
}
action transfer(input before: Token, witness amount: u64, witness recipient: Address) {
    require before.amount == 7
    require amount == before.amount
    let preserved = before.amount
    consume before
    create Token { amount: preserved } with_lock(recipient)
}
action merge(input left: Token, input right: Token, witness recipient: Address) {
    require left.amount == 7
    require right.amount == 5
    let amount = left.amount + right.amount
    consume left
    consume right
    create Token { amount: amount } with_lock(recipient)
}
action burn(input before: Token) {
    require before.amount == 7
    consume before
}
"#;

const MINT: u32 = 0;
const TRANSFER: u32 = 17;
const MERGE: u32 = 255;
const BURN: u32 = u32::MAX;
const CAPACITY: u64 = 100_000_000_000;

fn declaration() -> ArtifactDeclaration {
    ArtifactDeclaration {
        name: "TokenPolicy".to_string(),
        context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![
            ArtifactAction { tag: MINT, action: "mint".to_string() },
            ArtifactAction { tag: TRANSFER, action: "transfer".to_string() },
            ArtifactAction { tag: MERGE, action: "merge".to_string() },
            ArtifactAction { tag: BURN, action: "burn".to_string() },
        ],
        common_checks: vec!["common".to_string()],
    }
}

fn options() -> CompileOptions {
    CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".to_string()), ..Default::default() }
}

fn policy(failing_common: bool) -> &'static CompileResult {
    static POLICY: OnceLock<CompileResult> = OnceLock::new();
    static FAILING: OnceLock<CompileResult> = OnceLock::new();
    let cache = if failing_common { &FAILING } else { &POLICY };
    cache.get_or_init(|| {
        let source = if failing_common { SOURCE.replace("require true", "require false") } else { SOURCE.to_string() };
        let compiled = compile_artifact(&source, options(), declaration(), ExecutableSurfacePolicy::DenyFailClosed)
            .expect("fixed policy fixture compiles without fail-closed placeholders");
        compiled.validate().expect("policy bundle independently validates");
        compiled
    })
}

fn foreign() -> &'static CompileResult {
    static FOREIGN: OnceLock<CompileResult> = OnceLock::new();
    FOREIGN.get_or_init(|| {
        crate::compile("module policy_vm_foreign\nlock allow() -> bool { true }", options()).expect("test-only always-success Lock")
    })
}

fn deploy(context: &mut Context, compiled: &CompileResult, args: &[u8]) -> packed::Script {
    let code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&compiled.artifact_bytes)));
    context.build_script(&code, Bytes::copy_from_slice(args)).unwrap()
}

fn cell(lock: &packed::Script, type_script: Option<&packed::Script>) -> packed::CellOutput {
    packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(CAPACITY.pack())
        .lock(lock.clone())
        .type_(type_script.cloned().pack())
        .build()
}

fn data(amount: u64) -> Bytes {
    Bytes::copy_from_slice(&amount.to_le_bytes())
}

fn args(compiled: &CompileResult, tag: u32, recipient: &packed::Script) -> Vec<u8> {
    let (name, values) = match tag {
        MINT => ("mint", vec![EntryWitnessArg::U64(7), EntryWitnessArg::Address(recipient.calc_script_hash().unpack())]),
        TRANSFER => ("transfer", vec![EntryWitnessArg::U64(7), EntryWitnessArg::Address(recipient.calc_script_hash().unpack())]),
        MERGE => ("merge", vec![EntryWitnessArg::Address(recipient.calc_script_hash().unpack())]),
        BURN => return Vec::new(),
        _ => return Vec::new(),
    };
    compiled.metadata.actions.iter().find(|action| action.name == name).unwrap().entry_witness_args(&values).unwrap()
}

fn record(script: &packed::Script, tag: u32, args: Vec<u8>) -> PolicyWitnessRecord {
    PolicyWitnessRecord { role: PolicyScriptRole::Type, script_hash: script.calc_script_hash().unpack(), tag, args }
}

fn witness(bundle: Vec<u8>) -> Bytes {
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(bundle)).pack()).build().as_bytes()
}

#[derive(Clone, Copy, Default)]
enum Invocation {
    #[default]
    Type,
    Lock,
    Ambiguous,
}

struct Case<'a> {
    tag: u32,
    inputs: &'a [u64],
    outputs: &'a [u64],
    prepend_input: bool,
    prepend_output: bool,
    invocation: Invocation,
    witness_at: Option<usize>,
}

impl<'a> Case<'a> {
    fn new(tag: u32, inputs: &'a [u64], outputs: &'a [u64]) -> Self {
        Self { tag, inputs, outputs, prepend_input: true, prepend_output: true, invocation: Invocation::Type, witness_at: None }
    }
}

fn execute(
    compiled: &CompileResult,
    case: Case<'_>,
    mutate: impl FnOnce(Vec<u8>, &packed::Script, &packed::Script) -> Vec<u8>,
) -> std::result::Result<u64, String> {
    let mut context = Context::new_with_deterministic_rng();
    let foreign = deploy(&mut context, foreign(), &[]);
    let script = deploy(&mut context, compiled, &[0x71]);
    let policy_cell = match case.invocation {
        Invocation::Type => cell(&foreign, Some(&script)),
        Invocation::Lock => cell(&script, None),
        Invocation::Ambiguous => cell(&script, Some(&script)),
    };
    let mut transaction = TransactionBuilder::default();
    if case.prepend_input {
        let out_point = context.create_cell(cell(&foreign, None), data(99));
        transaction = transaction.input(packed::CellInput::new_builder().previous_output(out_point).build());
    }
    for amount in case.inputs {
        let out_point = context.create_cell(policy_cell.clone(), data(*amount));
        transaction = transaction.input(packed::CellInput::new_builder().previous_output(out_point).build());
    }
    if case.prepend_output {
        transaction = transaction.output(cell(&foreign, None)).output_data(data(99).pack());
    }
    for amount in case.outputs {
        transaction = transaction.output(policy_cell.clone()).output_data(data(*amount).pack());
    }
    let bundle = encode_policy_witness_bundle(&[record(&script, case.tag, args(compiled, case.tag, &foreign))]).unwrap();
    let bundle = mutate(bundle, &script, &foreign);
    let index = case.witness_at.unwrap_or_else(|| {
        if case.inputs.is_empty() {
            usize::from(case.prepend_output)
        } else {
            usize::from(case.prepend_input)
        }
    });
    let mut witnesses = vec![Bytes::new(); index];
    witnesses.push(witness(bundle));
    let transaction = context.complete_tx(transaction.witnesses(witnesses.pack()).build());
    context.verify_tx(&transaction, 30_000_000).map_err(|error| format!("{error:?}"))
}

fn unchanged(bundle: Vec<u8>, _: &packed::Script, _: &packed::Script) -> Vec<u8> {
    bundle
}

fn assert_exit(error: String, code: u64) {
    assert!(error.contains(&format!("error code {code}")) || error.contains(&format!("error code: {code}")), "{error}");
}

#[test]
fn shared_policy_decoder_preserves_wide_and_outgoing_stack_arguments() {
    let parameters = (0..9).map(|index| format!("witness p{index}: u64")).collect::<Vec<_>>().join(", ");
    let checks = (0..9).map(|index| format!("require p{index} == {}\n", index + 1)).collect::<String>();
    let mut source = "module shared_stack_policy\nresource Token has store, consume { amount: u64 }\n".to_string();
    for name in ["first", "second"] {
        source.push_str(&format!(
            "action {name}(input before: Token, {parameters}, witness recipient: Address) {{\nverification\n\
             require before.amount == 7\n{checks}consume before\n\
             create Token {{ amount: 7 }} with_lock(recipient)\n}}\n"
        ));
    }
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for opt_level in [0, 3] {
            let declaration = ArtifactDeclaration {
                name: "SharedStackPolicy".into(),
                context: ArtifactContext::TypeGroup { resource: "Token".into() },
                dispatch: ArtifactDispatch::PolicyWitnessV1,
                actions: vec![
                    ArtifactAction { tag: 100, action: "first".into() },
                    ArtifactAction { tag: 101, action: "second".into() },
                ],
                common_checks: Vec::new(),
            };
            let compiled = compile_artifact(
                &source,
                CompileOptions { edition, opt_level, ..options() },
                declaration,
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            compiled.validate().unwrap();
            assert_eq!(
                compiled
                    .verified_lowering_record
                    .as_ref()
                    .unwrap()
                    .entries
                    .iter()
                    .filter(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_"))
                    .count(),
                1
            );
            for (tag, name) in [(100, "first"), (101, "second")] {
                for valid in [true, false] {
                    let result = execute(&compiled, Case::new(tag, &[7], &[7]), |_, script, recipient| {
                        let mut values = (1..=9).map(EntryWitnessArg::U64).collect::<Vec<_>>();
                        if !valid {
                            values[8] = EntryWitnessArg::U64(10);
                        }
                        values.push(EntryWitnessArg::Address(recipient.calc_script_hash().unpack()));
                        let args = compiled.metadata.actions.iter().find(|action| action.name == name).unwrap();
                        encode_policy_witness_bundle(&[record(script, tag, args.entry_witness_args(&values).unwrap())]).unwrap()
                    });
                    if valid {
                        assert!(result.expect("shared decoder must preserve every scalar and the recipient pointer") > 0);
                    } else {
                        assert_exit(result.unwrap_err(), 5);
                    }
                }
            }
        }
    }
}

#[test]
fn same_policy_bytes_select_all_four_group_cardinalities_at_nonzero_indices() {
    let compiled = policy(false);
    for case in
        [Case::new(MINT, &[], &[7]), Case::new(TRANSFER, &[7], &[7]), Case::new(MERGE, &[7, 5], &[12]), Case::new(BURN, &[7], &[])]
    {
        assert!(execute(compiled, case, unchanged).expect("valid explicitly selected transition") > 0);
    }
}

#[test]
fn optimized_policy_dispatch_keeps_variant_and_common_check_obligations() {
    for opt_level in 1..=3 {
        let compiled = compile_artifact(
            SOURCE,
            CompileOptions { opt_level, ..options() },
            declaration(),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .expect("optimized fixed policy");
        for case in
            [Case::new(MINT, &[], &[7]), Case::new(TRANSFER, &[7], &[7]), Case::new(MERGE, &[7, 5], &[12]), Case::new(BURN, &[7], &[])]
        {
            execute(&compiled, case, unchanged).expect("optimization preserves all selected variants");
        }
        assert!(execute(&compiled, Case::new(TRANSFER, &[7], &[8]), unchanged).is_err());
        let failing = compile_artifact(
            &SOURCE.replace("require true", "require false"),
            CompileOptions { opt_level, ..options() },
            declaration(),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .expect("optimized failing common action remains explicit");
        assert_exit(execute(&failing, Case::new(BURN, &[7], &[]), unchanged).expect_err("common failure cannot disappear"), 5);
    }
}

#[test]
fn policy_group_ordinals_do_not_follow_foreign_transaction_positions() {
    for (input, output) in [(false, false), (false, true), (true, false), (true, true)] {
        let case = Case { prepend_input: input, prepend_output: output, ..Case::new(TRANSFER, &[7], &[7]) };
        assert!(execute(policy(false), case, unchanged).expect("valid independent input/output positions") > 0);
        let invalid = Case { prepend_input: input, prepend_output: output, ..Case::new(TRANSFER, &[99], &[99]) };
        assert!(execute(policy(false), invalid, unchanged).is_err(), "foreign Cells must not supply selected roles");
    }
}

#[test]
fn selected_variant_checks_data_and_exact_missing_or_extra_roles() {
    for case in [
        Case::new(MINT, &[], &[8]),
        Case::new(MINT, &[7], &[7]),
        Case::new(MINT, &[], &[7, 7]),
        Case::new(TRANSFER, &[8], &[8]),
        Case::new(TRANSFER, &[7], &[8]),
        Case::new(TRANSFER, &[7, 7], &[7]),
        Case::new(TRANSFER, &[7], &[]),
        Case::new(TRANSFER, &[], &[7]),
        Case::new(MERGE, &[7, 6], &[13]),
        Case::new(MERGE, &[7], &[12]),
        Case::new(MERGE, &[7, 5], &[12, 12]),
        Case::new(BURN, &[8], &[]),
        Case::new(BURN, &[7], &[7]),
        Case::new(BURN, &[7, 7], &[]),
    ] {
        assert!(execute(policy(false), case, unchanged).is_err(), "selected fixed group obligations must reject");
    }
}

#[test]
fn common_unit_action_failure_prevents_every_selected_variant() {
    for case in
        [Case::new(MINT, &[], &[7]), Case::new(TRANSFER, &[7], &[7]), Case::new(MERGE, &[7, 5], &[12]), Case::new(BURN, &[7], &[])]
    {
        assert_exit(execute(policy(true), case, unchanged).expect_err("common requirement applies to all variants"), 5);
    }
}

#[test]
fn selector_is_bound_to_full_script_hash_role_and_declared_numeric_tag() {
    for mutation in 0..4 {
        assert_exit(
            execute(policy(false), Case::new(BURN, &[7], &[]), |bundle, _, _| {
                let mut records = decode_policy_witness_bundle(&bundle).unwrap();
                match mutation {
                    0 => records[0].role = PolicyScriptRole::Lock,
                    1 => records[0].script_hash[31] ^= 1,
                    2 => records[0].tag = 999,
                    _ => {
                        let mut wrong_version = bundle;
                        wrong_version[6] = b'2';
                        return wrong_version;
                    }
                }
                encode_policy_witness_bundle(&records).unwrap()
            })
            .expect_err("wrong selector key, tag or version"),
            25,
        );
    }
    for invocation in [Invocation::Lock, Invocation::Ambiguous] {
        let case = Case { invocation, ..Case::new(BURN, &[7], &[]) };
        assert!(execute(policy(false), case, unchanged).is_err(), "Type policy must not accept a Lock or ambiguous group");
    }
}

#[test]
fn payload_free_action_still_requires_selector_and_exact_empty_args() {
    execute(policy(false), Case::new(BURN, &[7], &[]), unchanged).expect("canonical empty burn args");
    for payload in [crate::ENTRY_WITNESS_ABI_MAGIC.to_vec(), Vec::new()] {
        assert_exit(
            execute(policy(false), Case::new(BURN, &[7], &[]), |bundle, _, _| {
                if payload.is_empty() {
                    return payload;
                }
                let mut records = decode_policy_witness_bundle(&bundle).unwrap();
                records[0].args = payload;
                encode_policy_witness_bundle(&records).unwrap()
            })
            .expect_err("a no-payload variant is not a no-selector entry"),
            25,
        );
    }
}

#[test]
fn selected_record_reuses_exact_positional_argument_widths() {
    for tag in [MINT, TRANSFER, MERGE] {
        for trailing in [false, true] {
            let case = match tag {
                MINT => Case::new(tag, &[], &[7]),
                TRANSFER => Case::new(tag, &[7], &[7]),
                _ => Case::new(tag, &[7, 5], &[12]),
            };
            assert_exit(
                execute(policy(false), case, |bundle, _, _| {
                    let mut records = decode_policy_witness_bundle(&bundle).unwrap();
                    if trailing {
                        records[0].args.push(0);
                    } else {
                        records[0].args.pop();
                    }
                    // Structural CSARG framing remains valid; only the selected
                    // variant decoder knows its exact parameter widths.
                    encode_policy_witness_bundle(&records).unwrap()
                })
                .expect_err("selected action rejects truncated and trailing typed args"),
                25,
            );
        }
    }
}

#[test]
fn malformed_canonical_layouts_and_oversized_whole_witness_fail_before_dispatch() {
    let corruptions: &[(usize, u8)] = &[
        (8, 0),   // DynVec total.
        (12, 9),  // Unaligned first offset.
        (12, 4),  // Empty DynVec with inconsistent bytes.
        (12, 40), // More than eight records.
        (16, 0),  // Record total.
        (20, 24), // Table offset0: extra field/incorrect header.
        (24, 22), // Role width.
        (28, 52), // Hash width.
        (32, 58), // Tag width.
        (36, 2),  // Unknown role.
        (73, 1),  // Bytes length exceeds record.
    ];
    for (offset, value) in corruptions {
        assert_exit(
            execute(policy(false), Case::new(BURN, &[7], &[]), |mut bundle, _, _| {
                bundle[*offset] = *value;
                bundle
            })
            .expect_err("noncanonical record must reject"),
            25,
        );
    }
    for size in [4096, 4097] {
        assert_exit(
            execute(policy(false), Case::new(BURN, &[7], &[]), |mut bundle, _, _| {
                bundle.resize(size, 0);
                bundle
            })
            .expect_err("whole WitnessArgs size includes the enclosing table"),
            25,
        );
    }
}

#[test]
fn all_records_are_canonical_but_only_the_current_key_interprets_its_tag() {
    execute(policy(false), Case::new(BURN, &[7], &[]), |bundle, script, _| {
        let mut records = decode_policy_witness_bundle(&bundle).unwrap();
        for index in 0..7 {
            let mut hash: [u8; 32] = script.calc_script_hash().unpack();
            hash[31] ^= index + 1;
            records.push(PolicyWitnessRecord { role: PolicyScriptRole::Type, script_hash: hash, tag: 999, args: Vec::new() });
        }
        encode_policy_witness_bundle(&records).unwrap()
    })
    .expect("eight sorted records; unknown foreign tags are opaque");
    assert_exit(
        execute(policy(false), Case::new(BURN, &[7], &[]), |bundle, _, _| {
            let mut records = decode_policy_witness_bundle(&bundle).unwrap();
            records.push(PolicyWitnessRecord {
                role: PolicyScriptRole::Lock,
                script_hash: [0; 32],
                tag: 999,
                args: crate::ENTRY_WITNESS_ABI_MAGIC.to_vec(),
            });
            let mut encoded = encode_policy_witness_bundle(&records).unwrap();
            // The Lock key sorts first. Its envelope is structurally exact,
            // but even a foreign record must keep the common CSARG framing.
            encoded[POLICY_WITNESS_MAGIC.len() + 12 + 61] ^= 1;
            encoded
        })
        .expect_err("foreign record args magic is part of canonical framing"),
        25,
    );
    for mutation in 0..3 {
        assert_exit(
            execute(policy(false), Case::new(BURN, &[7], &[]), |bundle, _, _| {
                let selected = decode_policy_witness_bundle(&bundle).unwrap().remove(0);
                let mut foreign = selected.clone();
                foreign.script_hash[31] ^= 1;
                let mut two = encode_policy_witness_bundle(&[selected, foreign]).unwrap();
                // Both records have exactly 61 bytes; DynVec header is 12.
                let first = POLICY_WITNESS_MAGIC.len() + 12;
                let second = first + 61;
                match mutation {
                    0 => {
                        let duplicate = two[first + 20..first + 53].to_vec();
                        two[second + 20..second + 53].copy_from_slice(&duplicate);
                    }
                    1 => {
                        for offset in 0..61 {
                            two.swap(first + offset, second + offset);
                        }
                    }
                    _ => two[second + 4] = 24,
                }
                two
            })
            .expect_err("duplicate/unsorted keys and malformed foreign records must reject"),
            25,
        );
    }
}

#[test]
fn one_shared_witness_slot_dispatches_two_script_args_groups_independently() {
    let compiled = policy(false);
    let mut context = Context::new_with_deterministic_rng();
    let foreign = deploy(&mut context, foreign(), &[]);
    let mint_script = deploy(&mut context, compiled, &[0xa1]);
    let burn_script = mint_script.clone().as_builder().args(Bytes::from_static(&[0xb2]).pack()).build();
    assert_eq!(mint_script.code_hash(), burn_script.code_hash(), "one persistent artifact");
    assert_ne!(mint_script.calc_script_hash(), burn_script.calc_script_hash(), "full Script identity includes args");
    let funding = context.create_cell(cell(&foreign, None), data(99));
    let old = context.create_cell(cell(&foreign, Some(&burn_script)), data(7));
    let records = vec![record(&mint_script, MINT, args(compiled, MINT, &foreign)), record(&burn_script, BURN, Vec::new())];
    let bundle = encode_policy_witness_bundle(&records).unwrap();
    let transaction = context.complete_tx(
        TransactionBuilder::default()
            .input(packed::CellInput::new_builder().previous_output(funding).build())
            .input(packed::CellInput::new_builder().previous_output(old).build())
            .output(cell(&foreign, None))
            .output_data(data(99).pack())
            .output(cell(&foreign, Some(&mint_script)))
            .output_data(data(7).pack())
            .witness(Bytes::new().pack())
            .witness(witness(bundle).pack())
            .build(),
    );
    context.verify_tx(&transaction, 30_000_000).expect("burn GroupInput[0] and mint GroupOutput[0] both map to witnesses[1]");
    let missing = encode_policy_witness_bundle(&records[..1]).unwrap();
    let missing_tx = transaction.as_advanced_builder().set_witnesses(vec![Bytes::new().pack(), witness(missing).pack()]).build();
    assert!(context.verify_tx(&missing_tx, 30_000_000).is_err(), "one policy record cannot authorize the other Script args group");
}

#[test]
fn missing_input_group_witness_cannot_fall_back_to_output_position() {
    let case = Case { prepend_input: true, prepend_output: false, witness_at: Some(0), ..Case::new(TRANSFER, &[7], &[7]) };
    assert_exit(execute(policy(false), case, unchanged).expect_err("existing GroupInput[0] at Input[1] has no witness"), 25);
}

fn callable_policy_source(common_body: &str, mint_check: Option<&str>, definitions: &str) -> String {
    let mut source = SOURCE.replace("action common() { require true }", &format!("action common() {{ {common_body} }}"));
    if let Some(mint_check) = mint_check {
        // Replace this fixture's mint predicate with the explicit tested call.
        // A swallowed failure would otherwise mint the requested bad amount.
        source = source.replace("    require amount == 7", mint_check);
    }
    source.push_str(definitions);
    // One explicit verification-section spelling is accepted by both editions.
    // This changes no expressions, control flow, or lifecycle mechanics.
    source
        .lines()
        .map(|line| if line.trim_start().starts_with("action ") { line.replacen('{', "{ verification ", 1) } else { line.to_string() })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compile_callable_policy(source: &str, edition: CellScriptEdition, opt_level: u8) -> CompileResult {
    let compiled = compile_artifact(
        source,
        CompileOptions { edition, opt_level, ..options() },
        declaration(),
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("callable policy: {edition:?}, opt={opt_level}: {error}\n{source}"));
    compiled.validate().expect("callable policy has consistent independent structural evidence");
    compiled
}

fn valid_cases() -> [Case<'static>; 4] {
    [Case::new(MINT, &[], &[7]), Case::new(TRANSFER, &[7], &[7]), Case::new(MERGE, &[7, 5], &[12]), Case::new(BURN, &[7], &[])]
}

fn discarded_calls(call: &str) -> [String; 4] {
    [call.to_string(), format!("let unused = {call}"), format!("let _ = {call}"), format!("let _ = ignore({call})")]
}

const DIVISION_HELPERS: &str = r#"
fn checked(value: u64) -> u64 { 100 / value }
fn outer(value: u64) -> u64 { checked(value) }
fn ignore(value: u64) -> u64 { 0 }
"#;

const CAST_HELPERS: &str = r#"
fn checked(value: u64) -> u8 { value as u8 }
fn outer(value: u64) -> u8 { checked(value) }
fn ignore(value: u8) -> u64 { 0 }
"#;

const UNIT_ACTION_HELPERS: &str = r#"
action checked(value: u64) { require value == 7 }
fn outer(value: u64) -> u64 { checked(value)
49 }
fn ignore(value: u64) -> u64 { 0 }
"#;

#[test]
fn policy_exported_callable_failures_remain_fatal_when_results_are_discarded() {
    for (name, definitions, bad_amount, expected) in
        [("division", DIVISION_HELPERS, 0, 20), ("cast", CAST_HELPERS, 256, 20), ("unit_action", UNIT_ACTION_HELPERS, 8, 5)]
    {
        for invocation in discarded_calls("outer(amount)") {
            let source = callable_policy_source("require true", Some(&invocation), definitions);
            for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
                for opt_level in 0..=3 {
                    let compiled = compile_callable_policy(&source, edition, opt_level);
                    for case in valid_cases() {
                        execute(&compiled, case, unchanged)
                            .unwrap_or_else(|error| panic!("valid {name}/{invocation}, {edition:?}, opt={opt_level}: {error}"));
                    }
                    let error = execute(&compiled, Case::new(MINT, &[], &[bad_amount]), |bundle, _, recipient| {
                        let mut records = decode_policy_witness_bundle(&bundle).unwrap();
                        records[0].args = compiled
                            .metadata
                            .actions
                            .iter()
                            .find(|action| action.name == "mint")
                            .unwrap()
                            .entry_witness_args(&[
                                EntryWitnessArg::U64(bad_amount),
                                EntryWitnessArg::Address(recipient.calc_script_hash().unpack()),
                            ])
                            .unwrap();
                        encode_policy_witness_bundle(&records).unwrap()
                    })
                    .err()
                    .unwrap_or_else(|| {
                        panic!("failed {name}/{invocation}, {edition:?}, opt={opt_level} accepted mint amount {bad_amount}")
                    });
                    assert_exit(format!("{name}/{invocation}, {edition:?}, opt={opt_level}: {error}"), expected);
                }
            }
        }
    }
}

#[test]
fn policy_common_callable_failure_rejects_every_tag_in_the_same_elf() {
    let cases = [
        ("division", DIVISION_HELPERS, "let _ = ignore(outer(7))", "let _ = ignore(outer(0))", 20),
        ("cast", CAST_HELPERS, "let unused = outer(7)", "let unused = outer(256)", 20),
        ("unit_direct", UNIT_ACTION_HELPERS, "checked(7)", "checked(8)", 5),
        ("unit_wrapped", UNIT_ACTION_HELPERS, "let _ = ignore(outer(7))", "let _ = ignore(outer(8))", 5),
    ];
    for (name, definitions, good, bad, expected) in cases {
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            for opt_level in 0..=3 {
                let successful = compile_callable_policy(&callable_policy_source(good, None, definitions), edition, opt_level);
                let failing = compile_callable_policy(&callable_policy_source(bad, None, definitions), edition, opt_level);
                for case in valid_cases() {
                    execute(&successful, case, unchanged)
                        .unwrap_or_else(|error| panic!("successful common {name}, {edition:?}, opt={opt_level}: {error}"));
                }
                for case in valid_cases() {
                    let tag = case.tag;
                    let error = execute(&failing, case, unchanged)
                        .err()
                        .unwrap_or_else(|| panic!("common {name}, tag={tag}, {edition:?}, opt={opt_level}: nested failure accepted"));
                    // All tags are evaluated against this one failing artifact.
                    // Returning normally from a nested failed check is unsound.
                    assert_exit(format!("common {name}, tag={tag}, {edition:?}, opt={opt_level}: {error}"), expected);
                }
            }
        }
    }
}

#[test]
fn policy_common_and_export_calls_keep_error_shaped_values_as_values() {
    let definitions = r#"
fn scalar(value: u64) -> u64 { value }
fn no() -> bool { false }
fn unit() -> () { () }
action status() -> u64 { return 49 }
"#;
    let common = "require scalar(5) == 5\nrequire scalar(20) == 20\nrequire status() == 49\nrequire !no()\nunit()";
    let mint = "require amount == 7\nrequire scalar(49) == 49\nrequire status() == 49\nunit()";
    let source = callable_policy_source(common, Some(mint), definitions);
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for opt_level in 0..=3 {
            let compiled = compile_callable_policy(&source, edition, opt_level);
            for case in valid_cases() {
                execute(&compiled, case, unchanged)
                    .unwrap_or_else(|error| panic!("normal callable values, {edition:?}, opt={opt_level}: {error}"));
            }
        }
    }
}
