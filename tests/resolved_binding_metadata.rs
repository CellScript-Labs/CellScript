//! Physical Cell locations are compiler output, not guesses from parameter order.
//!
//! These tests keep the public typed plan, role records and value provenance in
//! agreement without claiming that positional CellDep reads authenticate data.

use cellscript::{compile, CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg};
use cellscript_artifact_checker::{
    CellBindingMembership, CellBindingRole, CellBindingSource, TypedSemanticCellBinding, TypedSemanticEntry, ValueProvenance,
};

const EDITIONS: [CellScriptEdition; 2] = [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027];

#[test]
fn anonymous_outputs_have_distinct_physical_roles_and_dispositions() {
    let source = r#"
module resolved_bindings::anonymous_outputs
resource Token has store, consume { amount: u64 }
action split(input seed: Token, witness recipient: Address) -> named: Token {
    verification
        create named = Token { amount: seed.amount } with_lock(recipient)
        create Token { amount: 1 } with_lock(recipient)
        create Token { amount: 2 } with_lock(recipient)
        consume seed
}
"#;
    for edition in EDITIONS {
        let result = compile_source(source, edition);
        let outputs = entry(&result, "action:split")
            .cell_bindings
            .iter()
            .filter(|binding| binding.role == CellBindingRole::Output)
            .collect::<Vec<_>>();
        let mut ordinals = outputs.iter().map(|binding| binding.ordinal).collect::<Vec<_>>();
        ordinals.sort();
        assert_eq!(ordinals, [0, 1, 2]);
        let foundation = &result.metadata.typed_semantics.foundation;
        let roles = foundation.roles.iter().filter(|role| role.direction == "output").collect::<Vec<_>>();
        assert_eq!(roles.len(), 3);
        for role in roles {
            assert_eq!(
                foundation.dispositions.iter().filter(|disposition| disposition.output_role.as_ref() == Some(&role.role_id)).count(),
                1
            );
        }
    }
}

const MIXED_INPUTS: &str = r#"
module resolved_bindings::mixed_inputs
resource Token has store, consume { amount: u64 }
shared Config { value: u64 }

action inspect(
    witness tag: u64,
    first: Token,
    read first_config: Config,
    witness ceiling: u64,
    input second: Token,
    read second_config: &Config,
) -> u64 {
    verification
        require tag == 1
        require first.amount <= ceiling
        require second.amount <= ceiling
        require first_config.value == 11
        require second_config.value == 22
        consume first
        consume second
        return 0
}
"#;

const MIXED_READS: &str = r#"
module resolved_bindings::mixed_reads
shared Config { value: u64 }
action inspect(witness tag: u64, read first: Config, read second: &Config) -> u64 {
    verification
        let third = read_ref<Config>()
        let fourth = read_ref<Config>()
        require tag == 1
        require first.value == 11
        require second.value == 22
        require third.value == 33
        require fourth.value == 44
        return 0
}
"#;

const MIXED_LOCK: &str = r#"
module resolved_bindings::mixed_lock
resource Token { amount: u64 }
shared Config { value: u64 }

lock permit(
    lock_args minimum: u64,
    protected token: Token,
    witness claimed: u64,
    read config: Config,
    lock_args maximum: u64,
) -> bool {
    verification
        require token.amount >= minimum
        require token.amount <= maximum
        require claimed == config.value
}
"#;

const LEGACY_SUCCESSOR: &str = r#"
module resolved_bindings::legacy_successor
resource Token has store, replace, relock { amount: u64 }
shared Config { value: u64 }

action transfer(read config: Config, input token: Token, witness recipient: Address) -> next: Token {
    verification
        require token.amount == config.value
        std::lifecycle::transfer(token, next, recipient) { amount }
        std::cell::preserve_capacity(next, token)
}
"#;

const NATIVE_SUCCESSOR: &str = r#"
module resolved_bindings::native_successor
resource Token has store, replace, relock { amount: u64 }
type_script TokenPolicy on type_group<Token> {
    entry retain_value(
        witness recipient: Address from group_witness.input_type,
        input before: Token from group_input[0],
        output after: Token from group_output[0],
    ) {
        verify { enforce before.amount == 7 }
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
"#;

fn compile_source(source: &str, edition: CellScriptEdition) -> CompileResult {
    let result = compile(
        source,
        CompileOptions {
            edition,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
    )
    .unwrap_or_else(|error| panic!("{edition:?}: binding fixture must compile: {error}"));
    result.validate().expect("typed binding bundle must pass independent validation");
    result
}

fn entry<'a>(result: &'a CompileResult, entry_id: &str) -> &'a TypedSemanticEntry {
    result.metadata.typed_semantics.entries.iter().find(|entry| entry.id == entry_id).expect("expected checked entry")
}

fn provenance_for_local(result: &CompileResult, entry_id: &str, local_id: u32) -> ValueProvenance {
    let graph = &result.metadata.typed_semantics.foundation.provenance;
    let binding = graph
        .bindings
        .iter()
        .find(|binding| binding.entry_id == entry_id && binding.local_id == local_id)
        .expect("physical local must have provenance");
    graph.nodes.iter().find(|node| node.id == binding.node_id).expect("provenance node must exist").provenance.clone()
}

fn param_provenance(result: &CompileResult, entry_id: &str, name: &str) -> ValueProvenance {
    let param = entry(result, entry_id).params.iter().find(|param| param.name == name).expect("expected source parameter");
    provenance_for_local(result, entry_id, param.binding_id)
}

fn assert_cell_binding(
    result: &CompileResult,
    entry_id: &str,
    name: &str,
    expected_role: CellBindingRole,
    expected_source: CellBindingSource,
    expected_ordinal: u32,
    expected_membership: CellBindingMembership,
) {
    let bindings = &entry(result, entry_id).cell_bindings;
    let matches = bindings.iter().filter(|binding| binding.binding == name && binding.role == expected_role).collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "exactly one resolved {expected_role:?} binding for {entry_id}:{name}: {bindings:?}");
    let binding = matches[0];
    assert_resolved_cell(result, entry_id, binding, expected_source, expected_ordinal, expected_membership);
}

fn assert_resolved_cell(
    result: &CompileResult,
    entry_id: &str,
    binding: &TypedSemanticCellBinding,
    expected_source: CellBindingSource,
    expected_ordinal: u32,
    expected_membership: CellBindingMembership,
) {
    let name = binding.binding.as_str();
    assert_eq!((binding.source, binding.ordinal, binding.membership), (expected_source, expected_ordinal, expected_membership));

    // Literal expectations below deliberately do not use the production
    // projection helpers: one bad projection must not validate itself.
    let (scope, prefix, direction) = match expected_source {
        CellBindingSource::Input => ("transaction-absolute", "input", "input"),
        CellBindingSource::Output => ("transaction-absolute", "output", "output"),
        CellBindingSource::GroupInput => ("group-relative", "group-input", "input"),
        CellBindingSource::GroupOutput => ("group-relative", "group-output", "output"),
        CellBindingSource::CellDep => ("cell-dep", "cell-dep", "read-only-dependency"),
    };
    let selector = format!("{prefix}[{expected_ordinal}]");
    let membership = match expected_membership {
        CellBindingMembership::Unproven => "unproven",
        CellBindingMembership::CurrentTypeGroup => "current-type-group",
        CellBindingMembership::CurrentLockGroup => "current-lock-group",
    };
    let role = result
        .metadata
        .typed_semantics
        .foundation
        .roles
        .iter()
        .find(|role| role.entry_id == entry_id && role.binding == name && role.direction == direction && role.selector == selector)
        .expect("resolved Cell must have a role record");
    assert_eq!(role.source, scope);
    assert_eq!(role.selector, selector);
    assert_eq!(role.script_identity_policy, membership);
    assert_eq!(role.lock_or_type_role, if entry_id.starts_with("lock:") { "lock" } else { "type" });

    if let Some(local_id) = binding.local_id {
        let field_path = "data".to_string();
        let expected = match expected_source {
            CellBindingSource::Input => ValueProvenance::TransactionInput { selector, field_path },
            CellBindingSource::Output => ValueProvenance::TransactionOutput { selector, field_path },
            CellBindingSource::GroupInput => {
                ValueProvenance::GroupInput { role: format!("{entry_id}:{name}"), ordinal: selector, field_path }
            }
            CellBindingSource::GroupOutput => {
                ValueProvenance::GroupOutput { role: format!("{entry_id}:{name}"), ordinal: selector, field_path }
            }
            CellBindingSource::CellDep => ValueProvenance::CellDep { identity_policy: membership.to_string(), selector, field_path },
        };
        assert_eq!(provenance_for_local(result, entry_id, local_id), expected);
    }

    let json = serde_json::to_value(binding).expect("public physical binding serializes");
    let roundtrip: TypedSemanticCellBinding = serde_json::from_value(json).expect("public physical binding deserializes");
    assert_eq!(*binding, roundtrip);
}

#[test]
fn mixed_parameters_use_independent_transaction_input_and_dependency_ordinals() {
    for edition in EDITIONS {
        let result = compile_source(MIXED_INPUTS, edition);
        let entry_id = "action:inspect";
        assert_eq!(entry(&result, entry_id).cell_bindings.len(), 4);
        let source_params = &entry(&result, entry_id).params;
        assert_eq!(source_params.iter().find(|param| param.name == "first").unwrap().source, "default");
        assert_eq!(source_params.iter().find(|param| param.name == "second").unwrap().source, "input");
        for (name, role, source, ordinal) in [
            ("first", CellBindingRole::Input, CellBindingSource::Input, 0),
            ("second", CellBindingRole::Input, CellBindingSource::Input, 1),
            ("first_config", CellBindingRole::ReadOnly, CellBindingSource::CellDep, 0),
            ("second_config", CellBindingRole::ReadOnly, CellBindingSource::CellDep, 1),
        ] {
            assert_cell_binding(&result, entry_id, name, role, source, ordinal, CellBindingMembership::Unproven);
        }
        for (ordinal, name) in ["tag", "ceiling"].into_iter().enumerate() {
            assert!(matches!(
                param_provenance(&result, entry_id, name),
                ValueProvenance::EntryWitness { field_path, .. } if field_path == format!("args[{ordinal}].{name}")
            ));
        }
        let payload = result.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::U64(1), EntryWitnessArg::U64(99)]);
        assert!(payload.is_ok(), "only two witness values belong in the payload: {payload:?}");
        assert!(result.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::U64(1)]).is_err());
    }
}

#[test]
fn expression_dependencies_continue_after_read_parameters_in_public_plan() {
    for edition in EDITIONS {
        let result = compile_source(MIXED_READS, edition);
        let entry_id = "action:inspect";
        let typed_entry = entry(&result, entry_id);
        assert_eq!(typed_entry.cell_bindings.len(), 4);
        let mut local_ids = std::collections::BTreeSet::new();
        for ordinal in 0..4 {
            let binding = typed_entry
                .cell_bindings
                .iter()
                .find(|binding| binding.source == CellBindingSource::CellDep && binding.ordinal == ordinal)
                .expect("each dependency ordinal must have a resolved root");
            assert_eq!(binding.role, CellBindingRole::ReadOnly);
            assert!(local_ids.insert(binding.local_id.expect("read dependency has a local")), "read roots must be distinct");
            assert_resolved_cell(&result, entry_id, binding, CellBindingSource::CellDep, ordinal, CellBindingMembership::Unproven);
        }
        // Source aliases may be optimized away, and both calls may have the
        // same generated temporary name. The four distinct physical local
        // roots and their provenances above must survive either representation.
        assert_eq!(typed_entry.cell_bindings.iter().find(|binding| binding.binding == "first").unwrap().ordinal, 0);
        assert_eq!(typed_entry.cell_bindings.iter().find(|binding| binding.binding == "second").unwrap().ordinal, 1);
    }
}

#[test]
fn protected_lock_input_is_not_a_type_group_or_an_authenticated_witness() {
    for edition in EDITIONS {
        let result = compile_source(MIXED_LOCK, edition);
        let entry_id = "lock:permit";
        assert_eq!(entry(&result, entry_id).cell_bindings.len(), 2);
        assert_cell_binding(
            &result,
            entry_id,
            "token",
            CellBindingRole::ReadOnly,
            CellBindingSource::GroupInput,
            0,
            CellBindingMembership::CurrentLockGroup,
        );
        assert_cell_binding(
            &result,
            entry_id,
            "config",
            CellBindingRole::ReadOnly,
            CellBindingSource::CellDep,
            0,
            CellBindingMembership::Unproven,
        );
        for (name, byte_range) in [("minimum", "0..8"), ("maximum", "8..16")] {
            assert_eq!(
                param_provenance(&result, entry_id, name),
                ValueProvenance::ScriptArgs {
                    script_role: "lock".to_string(),
                    byte_range: byte_range.to_string(),
                    codec: "typed-fixed-bytes".to_string(),
                }
            );
        }
        assert!(matches!(
            param_provenance(&result, entry_id, "claimed"),
            ValueProvenance::EntryWitness { field_path, .. } if field_path == "args[0].claimed"
        ));
        assert!(result.metadata.locks[0].entry_witness_args(&[EntryWitnessArg::U64(7)]).is_ok());
        assert!(result.metadata.locks[0].entry_witness_args(&[EntryWitnessArg::U64(7), EntryWitnessArg::U64(8)]).is_err());
    }
}

#[test]
fn legacy_successors_remain_transaction_absolute_while_native_roles_are_group_relative() {
    for edition in EDITIONS {
        let result = compile_source(LEGACY_SUCCESSOR, edition);
        assert_cell_binding(
            &result,
            "action:transfer",
            "token",
            CellBindingRole::Input,
            CellBindingSource::Input,
            0,
            CellBindingMembership::Unproven,
        );
        assert_cell_binding(
            &result,
            "action:transfer",
            "next",
            CellBindingRole::Output,
            CellBindingSource::Output,
            0,
            CellBindingMembership::Unproven,
        );
        assert_cell_binding(
            &result,
            "action:transfer",
            "config",
            CellBindingRole::ReadOnly,
            CellBindingSource::CellDep,
            0,
            CellBindingMembership::Unproven,
        );
    }
    let native = compile_source(NATIVE_SUCCESSOR, CellScriptEdition::Edition2027);
    for (name, role, source) in [
        ("before", CellBindingRole::Input, CellBindingSource::GroupInput),
        ("after", CellBindingRole::Output, CellBindingSource::GroupOutput),
    ] {
        assert_cell_binding(&native, "action:retain_value", name, role, source, 0, CellBindingMembership::CurrentTypeGroup);
    }
}
