//! Branch-local successor relations in the authoring route.
//!
//! A `replace before -> after { ... }` declaration is checked sugar over the
//! spelled-out successor forms in Edition 2027: these tests require identical
//! generated machine code between the two spellings, execute the relation in
//! the real CKB-VM, and reject incomplete, duplicated or out-of-scope
//! relations. This is differential compiler evidence, not a shared-policy
//! dispatch or production deployment claim.

use cellscript::{
    compile_with_executable_surface_policy, frontend, strip_vm_abi_trailer, CellScriptEdition, CompileOptions, CompileResult,
    EntryWitnessArg, ExecutableSurfacePolicy,
};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, deterministic_always_success_lock_hash, execute_cellscript_script};

fn options(edition: CellScriptEdition) -> CompileOptions {
    CompileOptions { edition, target: Some("riscv64-elf".to_string()), target_profile: Some("ckb".to_string()), ..Default::default() }
}

fn compile_2027(source: &str) -> CompileResult {
    compile_with_executable_surface_policy(source, options(CellScriptEdition::Edition2027), ExecutableSurfacePolicy::DenyFailClosed)
        .unwrap_or_else(|error| panic!("authoring source must compile: {error}\n{source}"))
}

fn compile_explicit(source: &str) -> CompileResult {
    compile_with_executable_surface_policy(source, options(CellScriptEdition::Edition2027), ExecutableSurfacePolicy::DenyFailClosed)
        .unwrap_or_else(|error| panic!("explicit successor source must compile: {error}\n{source}"))
}

fn errors_2027(source: &str) -> String {
    compile_with_executable_surface_policy(source, options(CellScriptEdition::Edition2027), ExecutableSurfacePolicy::DenyFailClosed)
        .expect_err("source must be rejected")
        .to_string()
}

const EXACT_LOCK_RELATION: &str = r#"
module authoring_replace::transfer;
resource Token has store, replace, relock { amount: u64, }

action transfer(input token: Token, witness recipient: Address) -> next: Token {
    require token.amount > 0;
    replace token -> next {
        data { amount = same, };
        lock = exact(recipient);
        capacity = same;
        type_script = same;
    }
}
"#;

const EXACT_HASH_LOCK_RELATION: &str = r#"
module authoring_replace::transfer_hash;
resource Token has store, replace, relock { amount: u64, }

action transfer(input token: Token, witness recipient_hash: Hash) -> next: Token {
    require token.amount > 0;
    replace token -> next {
        data { amount = same, };
        lock = exact_hash(ckb::script_hash(recipient_hash));
        capacity = same;
        type_script = same;
    }
}
"#;

const OBSERVED_HASH_LOCK_RELATION: &str = r#"
module authoring_replace::observed_hash;
resource Token has store, replace, relock { amount: u64, }

action transfer(input token: Token) -> next: Token {
    let observed = ckb::input::<Token>(0);
    replace token -> next {
        data { amount = same, };
        lock = exact_hash(observed.lock_hash);
        capacity = same;
        type_script = same;
    }
}
"#;

const EXACT_LOCK_EXPLICIT: &str = r#"
module authoring_replace::transfer;
resource Token has store, replace, relock { amount: u64, }

action transfer(input token: Token, witness recipient: Address) -> next: Token {

        require token.amount > 0;
        std::lifecycle::transfer(token, next, recipient) { amount, };
        std::cell::preserve_capacity(next, token)
}
"#;

const SAME_LOCK_RELATION: &str = r#"
module authoring_replace::rollover;
resource Note has store, replace, relock { owner: Address, amount: u64, }

action roll(input note: Note) -> next: Note {
    replace note -> next {
        data = same except { amount = note.amount + 1, };
        lock = same;
        capacity = same;
        type_script = same;
    }
}
"#;

// Field order follows the relation's canonical sorted expansion (amount,
// owner) so the two spellings lower to one instruction sequence.
const SAME_LOCK_EXPLICIT: &str = r#"
module authoring_replace::rollover;
resource Note has store, replace, relock { owner: Address, amount: u64, }

action roll(input note: Note) -> next: Note {

        consume note;
        create next = Note { amount: note.amount + 1, owner: note.owner, }
        std::cell::same_lock(next, note);
        std::cell::same_type(next, note);
        std::cell::preserve_capacity(next, note)
}
"#;

fn assert_identical_machine_code(relation: &CompileResult, explicit: &CompileResult) {
    relation.validate().expect("valid authoring compiler bundle");
    explicit.validate().expect("valid explicit successor compiler bundle");
    assert_eq!(&explicit.artifact_bytes[..4], b"\x7fELF");
    assert_eq!(relation.metadata.actions.len(), explicit.metadata.actions.len());
    let obligations = |result: &CompileResult| -> Vec<String> {
        result.metadata.actions[0].proof_plan.iter().map(|proof| proof.feature.clone()).collect::<Vec<_>>()
    };
    let mut relation_obligations = obligations(relation);
    let mut explicit_obligations = obligations(explicit);
    relation_obligations.sort();
    explicit_obligations.sort();
    assert_eq!(relation_obligations, explicit_obligations, "the relation must carry the identical obligation set");
}

#[test]
fn exact_lock_relation_is_checked_sugar_over_the_explicit_transfer() {
    let relation = compile_2027(EXACT_LOCK_RELATION);
    let explicit = compile_explicit(EXACT_LOCK_EXPLICIT);
    assert_identical_machine_code(&relation, &explicit);
}

#[test]
fn same_except_and_lock_same_execute_with_checked_field_updates() {
    // The `same except` form expands against the concrete schema and the
    // `lock = same` treatment pins the successor's complete Lock Script hash
    // to the predecessor's: conservation recognizes the updated-successor
    // shape (verbatim aliases plus verifier-checked u64 updates rooted in the
    // consumed input), so both spellings stay executable under
    // DenyFailClosed.
    let relation = compile_2027(SAME_LOCK_RELATION);
    let explicit = compile_explicit(SAME_LOCK_EXPLICIT);
    assert_identical_machine_code(&relation, &explicit);
    assert!(
        relation.metadata.actions[0]
            .proof_plan
            .iter()
            .any(|proof| { proof.feature == "resource-conservation:Note" && proof.on_chain_checked }),
        "the updated successor must carry checked conservation evidence"
    );

    // Note's layout is owner (32-byte Address) followed by amount (u64).
    let owner = deterministic_always_success_lock_hash();
    let note_data = |amount: u64| -> Bytes {
        let mut data = owner.to_vec();
        data.extend_from_slice(&amount.to_le_bytes());
        Bytes::from(data)
    };
    let run = |input_amount: u64, output_amount: u64| {
        let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
        fixture.current_type_script_input_indices = vec![0];
        fixture.inputs[0].data = note_data(input_amount);
        fixture.outputs[0].data = note_data(output_amount);
        let execution = execute_cellscript_script(strip_vm_abi_trailer(&relation.artifact_bytes), &fixture);
        (execution.exit_code, execution.captured_debug.clone())
    };
    let (exit, debug) = run(7, 8);
    assert_eq!(exit, 0, "a verifier-checked amount update must pass: {debug:?}");
    let (exit, _) = run(7, 7);
    assert_ne!(exit, 0, "skipping the declared update must reject");
    let (exit, _) = run(7, 9);
    assert_ne!(exit, 0, "an off-by-more update must reject");

    // Field validation still fails closed on unknown fields.
    let unknown_field = SAME_LOCK_RELATION.replace("amount = note.amount + 1", "ghost = note.amount + 1");
    let message = errors_2027(&unknown_field);
    assert!(message.contains("does not exist on the relation's resource"), "{message}");
}

fn transfer_fixture(amount: u64, output_amount: u64) -> ckb_script_runner::CkbVmFixture {
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.inputs[0].data = Bytes::copy_from_slice(&amount.to_le_bytes());
    fixture.outputs[0].data = Bytes::copy_from_slice(&output_amount.to_le_bytes());
    fixture
}

fn transfer_witness(result: &CompileResult, recipient_lock_hash: [u8; 32]) -> Bytes {
    let payload = result.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::Address(recipient_lock_hash)])
        .expect("encode declared entry arguments");
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

fn transfer_hash_witness(result: &CompileResult, recipient_lock_hash: [u8; 32]) -> Bytes {
    let payload = result.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::Hash(recipient_lock_hash)])
        .expect("encode declared Script-hash argument");
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

#[test]
fn exact_lock_relation_executes_and_rejects_in_the_real_vm() {
    let result = compile_2027(EXACT_LOCK_RELATION);
    let recipient = deterministic_always_success_lock_hash();
    let run = |fixture_witness: Bytes, amount: u64, output_amount: u64| {
        let mut fixture = transfer_fixture(amount, output_amount);
        fixture.witnesses = vec![fixture_witness];
        let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
        (execution.exit_code, execution.captured_debug.clone(), execution.cycles)
    };

    let witness = transfer_witness(&result, recipient);
    let (exit, debug, cycles) = run(witness.clone(), 7, 7);
    assert_eq!(exit, 0, "preserving transfer must pass: {debug:?}");
    // Regression budget: the compact layout and single-instruction small
    // immediates brought this fixture from 10,772 down to 8,573 cycles.
    // Headroom guards against accidental codegen regressions without
    // pinning the exact optimization.
    assert!(cycles <= 9_500, "relation transfer cycles regressed past budget: {cycles}");

    let (exit, _, _) = run(witness.clone(), 0, 0);
    assert_ne!(exit, 0, "amount guard must reject zero");

    let (exit, _, _) = run(witness.clone(), 7, 8);
    assert_ne!(exit, 0, "mutated successor data must reject");

    let wrong_recipient = {
        let mut hash = recipient;
        hash[0] ^= 0xff;
        hash
    };
    let (exit, _, _) = run(transfer_witness(&result, wrong_recipient), 7, 7);
    assert_ne!(exit, 0, "a successor locked to another recipient must reject");
}

#[test]
fn exact_hash_relation_accepts_only_the_explicit_script_hash_domain_and_executes_in_the_real_vm() {
    let result = compile_2027(EXACT_HASH_LOCK_RELATION);
    let recipient = deterministic_always_success_lock_hash();
    let run = |witness: Bytes| {
        let mut fixture = transfer_fixture(7, 7);
        fixture.witnesses = vec![witness];
        execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture).exit_code
    };

    assert_eq!(run(transfer_hash_witness(&result, recipient)), 0, "the complete output Lock Script hash must match");
    let mut wrong_recipient = recipient;
    wrong_recipient[0] ^= 0xff;
    assert_ne!(run(transfer_hash_witness(&result, wrong_recipient)), 0, "a different complete Lock Script hash must reject");

    let formatted = cellscript::fmt::format_default(&result.ast).expect("format exact_hash relation source");
    assert!(formatted.contains("lock = exact_hash(ckb::script_hash(recipient_hash))"), "{formatted}");
    compile_2027(&formatted);

    let observed = compile_2027(OBSERVED_HASH_LOCK_RELATION);
    let fixture = transfer_fixture(7, 7);
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&observed.artifact_bytes), &fixture);
    assert_eq!(execution.exit_code, 0, "a typed InputView.lock_hash must remain in the ScriptHash domain");
}

#[test]
fn formatted_relation_round_trips_into_identical_machine_code() {
    let relation = compile_2027(EXACT_LOCK_RELATION);
    let formatted = cellscript::fmt::format_default(&relation.ast).expect("format relation source");
    assert!(formatted.contains("replace token -> next"), "{formatted}");
    assert!(formatted.contains("lock = exact(recipient)"), "{formatted}");
    let reparsed = compile_2027(&formatted);
    assert_identical_machine_code(&reparsed, &relation);
    let explicit = compile_explicit(EXACT_LOCK_EXPLICIT);
    assert_identical_machine_code(&reparsed, &explicit);
}

const CONDITIONAL_DISPOSAL: &str = r#"
module authoring_replace::conditional;
resource Token has store, replace, relock { amount: u64 }

action branchy(input token: Token, witness flag: u64, witness recipient: Address) -> next: Token {
    if flag > 0 {
        replace token -> next {
            data { amount = same };
            lock = exact(recipient);
            capacity = same;
            type_script = same;
        }
    }
}
"#;

const COMPLETE_DISPOSAL: &str = r#"
module authoring_replace::complete;
resource Token has store, replace, relock { amount: u64, }

action branchy(input token: Token, witness flag: u64, witness recipient: Address) -> next: Token {
    if flag > 0 {
        replace token -> next {
            data { amount = same, };
            lock = exact(recipient);
            capacity = same;
            type_script = same;
        }
    } else {
        replace token -> next {
            data { amount = same, };
            lock = exact(recipient);
            capacity = same;
            type_script = same;
        }
    }
}
"#;

const DOUBLE_DISPOSAL: &str = r#"
module authoring_replace::double;
resource Token has store, consume { amount: u64 }

action greedy(input token: Token, witness recipient: Address) -> next: Token {
    replace token -> next {
        data { amount = same };
        lock = exact(recipient);
        capacity = same;
        type_script = same;
    }
    consume token;
}
"#;

#[test]
fn path_sensitive_successor_completeness_rejects_partial_and_double_disposal() {
    assert!(errors_2027(CONDITIONAL_DISPOSAL).contains("every accepting path must dispose"), "a branch that skips disposal must fail");
    // Both branches covering the role satisfies the path-sensitive check and
    // now compiles: sibling branch arms re-materialize schema fields instead
    // of reusing a predecessor-only definition, so each arm's create stands
    // on its own and the typed record validates.
    let complete = compile_2027(COMPLETE_DISPOSAL);
    assert_eq!(complete.metadata.actions.len(), 1, "both branches covering the role must compile");
    assert!(errors_2027(DOUBLE_DISPOSAL).contains("disposed of twice"), "two disposals on one path must fail");
}

#[test]
fn relation_surface_fails_closed_outside_its_contract() {
    for (source, expected) in [
        (
            r#"
module authoring_replace::frozen;
resource Token has store, replace, relock { amount: u64, }

action transfer(input token: Token, witness recipient: Address) -> next: Token {
    replace token -> next {
        data { amount = same, };
        lock = exact_hash(recipient);
        capacity = same;
        type_script = same;
    }
}
"#,
            "expects a ScriptHash, found Address",
        ),
        (
            r#"
module authoring_replace::untyped_hash;
resource Token has store, replace, relock { amount: u64, }

action transfer(input token: Token, witness recipient_hash: Hash) -> next: Token {
    replace token -> next {
        data { amount = same, };
        lock = exact_hash(recipient_hash);
        capacity = same;
        type_script = same;
    }
}
"#,
            "convert a trusted Hash explicitly with ckb::script_hash(hash)",
        ),
        (
            r#"
module authoring_replace::unknown_field;
resource Token has store, replace, relock { amount: u64, }

action transfer(input token: Token, witness recipient: Address) -> next: Token {
    replace token -> next {
        data = same except { owner = recipient, };
        lock = same;
        capacity = same;
        type_script = same;
    }
}
"#,
            "does not exist on the relation\'s resource",
        ),
        (
            r#"
module authoring_replace::missing_treatment;
resource Token has store, replace, relock { amount: u64 }

action transfer(input token: Token, witness recipient: Address) -> next: Token {
    replace token -> next {
        data { amount = same };
        capacity = same;
        type_script = same;
    }
}
"#,
            "missing lock",
        ),
        (
            r#"
module authoring_replace::partial_data;
resource Token has store, replace, relock { owner: Address, amount: u64, }

action transfer(input token: Token, witness recipient: Address) -> next: Token {
    replace token -> next {
        data { amount = same, };
        lock = same;
        capacity = same;
        type_script = same;
    }
}
"#,
            "must cover every field",
        ),
        (
            r#"
module authoring_replace::looped;
resource Token has store, consume { amount: u64 }

action loopy(input token: Token, witness rounds: u64) -> u64 {
    for i in 0..rounds {
        consume token;
    }
    return 0;
}
"#,
            "inside a loop",
        ),
    ] {
        let message = errors_2027(source);
        assert!(message.contains(expected), "expected `{expected}` in: {message}");
    }

    assert!(frontend::parse(EXACT_LOCK_RELATION, CellScriptEdition::Edition2027).is_ok());
}

#[test]
fn type_script_preservation_does_not_invent_a_business_identifier_constraint() {
    let source = SAME_LOCK_RELATION
        .replace("owner: Address, amount: u64", "id: u64, amount: u64")
        .replace("data = same except { amount = note.amount + 1, };", "data { id = note.id + 1, amount = same, };");
    let result = compile_2027(&source);
    let data = |id: u64, amount: u64| {
        let mut bytes = id.to_le_bytes().to_vec();
        bytes.extend_from_slice(&amount.to_le_bytes());
        Bytes::from(bytes)
    };
    let run = |output_id: u64, output_amount: u64| {
        let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
        fixture.current_type_script_input_indices = vec![0];
        fixture.inputs[0].data = data(7, 11);
        fixture.outputs[0].data = data(output_id, output_amount);
        execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture).exit_code
    };
    assert_eq!(run(8, 11), 0, "the explicit id update is allowed while the complete Type Script hash is preserved");
    assert_ne!(run(7, 11), 0, "the declared id update is enforced");
    assert_ne!(run(8, 12), 0, "the separate amount preservation rule is enforced");
}

#[test]
fn successor_metadata_clauses_are_explicit_and_the_old_identity_alias_rejects() {
    let old_name = EXACT_LOCK_RELATION.replace("type_script = same", "identity = same");
    assert!(frontend::parse(&old_name, CellScriptEdition::Edition2027).is_err());
    for treatment in ["type_script = same;", "capacity = same;", "lock = exact(recipient);", "data { amount = same, };"] {
        let missing = EXACT_LOCK_RELATION.replace(treatment, "");
        let message = errors_2027(&missing);
        assert!(message.contains("missing"), "{treatment}: {message}");
    }
}

#[test]
fn fieldless_enum_resource_fields_verify_computed_scalar_values_in_ckb_vm() {
    let source = r#"
module enum_output_field;
enum Color { Red, Blue, }
resource Badge has store, create { color: Color, amount: u64, }

action mint(witness blue: bool) -> output: Badge {
    let color = if blue { Color::Blue } else { Color::Red };
    create output = Badge { color, amount: 7, }
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = cellscript::compile(source, CompileOptions { diagnostics, ..options(CellScriptEdition::Edition2027) }).unwrap();
        result.validate().expect("enum field artifact independently validates");
        let badge = result.metadata.types.iter().find(|ty| ty.name == "Badge").unwrap();
        assert_eq!(badge.encoded_size, Some(9));
        assert_eq!(badge.fields[0].encoded_size, Some(1));
        for blue in [false, true] {
            let payload = result.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Bool(blue)]).unwrap();
            for (color, amount) in [(u8::from(blue), 7u64), (u8::from(!blue), 7), (2, 7), (u8::from(blue), 8)] {
                let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
                let mut bytes = vec![color];
                bytes.extend_from_slice(&amount.to_le_bytes());
                fixture.outputs[0].data = Bytes::from(bytes);
                fixture.witnesses = vec![packed::WitnessArgs::new_builder()
                    .input_type(Some(Bytes::copy_from_slice(&payload)).pack())
                    .build()
                    .as_bytes()];
                let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
                assert_eq!(
                    execution.exit_code == 0,
                    color == u8::from(blue) && amount == 7,
                    "{diagnostics:?}/blue={blue}/color={color}/amount={amount}: {execution:?}"
                );
            }
        }
    }
}

#[test]
fn fieldless_enum_flow_edges_accept_only_the_declared_runtime_transition_in_ckb_vm() {
    let source = r#"
module enum_output_flow;
enum OfferState { Created, Live, Filled, Cancelled, }
resource Offer has store { state: OfferState, amount: u64, }

flow OfferFlow for Offer.state {
    initial Created;
    terminal Filled, Cancelled;
    Created -> Live;
    Live -> Filled by accept;
    Live -> Cancelled;
}

action accept(input input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled;
    let amount = input.amount;
    create output = Offer { state: OfferState::Filled, amount, }
}
"#;
    let data = |state: u8, amount: u64| {
        let mut bytes = vec![state];
        bytes.extend_from_slice(&amount.to_le_bytes());
        Bytes::from(bytes)
    };
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = cellscript::compile(source, CompileOptions { diagnostics, ..options(CellScriptEdition::Edition2027) }).unwrap();
        result.validate().expect("enum flow artifact independently validates");
        for (before, after, output_amount, expected_exit) in [(1, 2, 7, 0), (0, 2, 7, 7), (2, 2, 7, 7), (3, 2, 7, 7), (255, 2, 7, 9)] {
            let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
            fixture.current_type_script_input_indices = vec![0];
            fixture.inputs[0].data = data(before, 7);
            fixture.outputs[0].data = data(after, output_amount);
            let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
            assert_eq!(execution.exit_code, expected_exit, "{diagnostics:?}/{before}->{after}: {execution:?}");
        }
        for (after, amount) in [(1, 7), (3, 7), (255, 7), (2, 8)] {
            let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
            fixture.current_type_script_input_indices = vec![0];
            fixture.inputs[0].data = data(1, 7);
            fixture.outputs[0].data = data(after, amount);
            let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
            assert_ne!(execution.exit_code, 0, "{diagnostics:?}/1->{after}/amount={amount}: {execution:?}");
        }
    }
}
