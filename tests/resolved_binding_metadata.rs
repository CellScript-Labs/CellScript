//! Physical Cell locations are compiler output, not guesses from parameter order.
//!
//! These tests keep the public typed plan, role records and value provenance in
//! agreement without claiming that positional CellDep reads authenticate data.

#[path = "support/fixed_action.rs"]
mod fixed_action;
use fixed_action::compile_fixture as compile;

use cellscript::{CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg};
use cellscript_artifact_checker::{
    CellBindingMembership, CellBindingRole, CellBindingSource, TypedSemanticCellBinding, TypedSemanticEntry, ValueProvenance,
};

#[test]
fn omitted_operation_identity_uses_the_resolved_cell_declaration() {
    for (declaration, policy) in [
        ("", "none"),
        ("identity(field(id))", "field(id)"),
        ("identity(script_args)", "script_args"),
        ("identity(ckb_type_id)", "ckb_type_id"),
        ("identity(singleton_type)", "singleton_type"),
    ] {
        let type_id = if policy == "ckb_type_id" { "#[type_id(\"identity_defaults::Token\")]" } else { "" };
        let schema = format!("{type_id} resource Token has store,create,consume,replace {declaration} {{id:u64,}}");
        let explicit = format!("module identity_defaults; {schema} action mint(witness owner:Address,output next:Token) {{fresh next {{data {{id=7,}};identity={policy};lock=exact(owner);}}}}");
        let omitted = explicit.replace(&format!("identity={policy};"), "");
        let expected = cellscript::compile_metadata(&explicit, CellScriptEdition::Edition2027, None).unwrap();
        let actual = cellscript::compile_metadata(&omitted, CellScriptEdition::Edition2027, None)
            .unwrap_or_else(|error| panic!("fresh {policy}: {error}"));
        assert_eq!(expected.typed_semantics, actual.typed_semantics, "fresh {policy}");
        if policy == "none" {
            continue;
        }
        for body in [
            format!("create_unique::<Token>(identity={policy}) {{id:7,}}"),
            format!("replace_unique::<Token>(identity={policy}) before {{id:before.id,}}"),
        ] {
            let params = if body.starts_with("replace") { "input before:Token" } else { "" };
            let explicit = format!("module identity_defaults; {schema} action issue({params})->Token {{{body}}}");
            let omitted = explicit.replace(&format!("identity={policy}"), "");
            let expected = cellscript::compile_metadata(&explicit, CellScriptEdition::Edition2027, None).unwrap();
            let actual = cellscript::compile_metadata(&omitted, CellScriptEdition::Edition2027, None)
                .unwrap_or_else(|error| panic!("{body}: {error}"));
            assert_eq!(expected.typed_semantics, actual.typed_semantics, "{body}");
        }
    }
}

#[test]
fn omitted_operation_identity_resolves_imports_aliases_and_qualified_schemas() {
    for policy in ["field(id)", "script_args", "ckb_type_id", "singleton_type"] {
        let type_id = if policy == "ckb_type_id" { "#[type_id(\"identity_defaults::Token\")]" } else { "" };
        let schema =
            format!("module schema; {type_id} public resource Token has store,create,consume,replace identity({policy}) {{id:u64,}}");
        for (imports, name, shadow) in [
            ("use schema::Token;", "Token", ""),
            ("use schema::Token as Asset;", "Asset", "resource Token has store identity(none) {id:u64,}"),
            ("", "schema::Token", "resource Token has store identity(none) {id:u64,}"),
        ] {
            for (params, result, body) in [
                (
                    "witness owner:Address".to_string() + ",output next:" + name,
                    String::new(),
                    format!("fresh next {{data {{id=7,}};identity={policy};lock=exact(owner);}}"),
                ),
                (String::new(), format!("->{name}"), format!("create_unique::<{name}>(identity={policy}) {{id:7,}}")),
                (
                    format!("input before:{name}"),
                    format!("->{name}"),
                    format!("replace_unique::<{name}>(identity={policy}) before {{id:before.id,}}"),
                ),
            ] {
                let explicit = format!("module caller; {imports} {shadow} action issue({params}){result} {{{body}}}");
                let omitted = explicit.replace(&format!("identity={policy};"), "").replace(&format!("identity={policy}"), "");
                let compile = |source: String| {
                    let sources = [
                        cellscript::InMemorySource { path: "main.cell".into(), source, role: None },
                        cellscript::InMemorySource { path: "schema.cell".into(), source: schema.clone(), role: None },
                    ];
                    let report = cellscript::compile_sources_metadata_with_diagnostics(
                        &sources,
                        "main.cell",
                        CellScriptEdition::Edition2027,
                        None,
                    );
                    report.metadata.unwrap_or_else(|| panic!("{policy}, {name}, {body}: {:?}", report.diagnostics))
                };
                assert_eq!(compile(explicit).typed_semantics, compile(omitted).typed_semantics, "{policy}, {name}, {body}");
            }
        }
    }
}

#[test]
fn identity_omission_never_weakens_explicit_conflicts_or_unique_admission() {
    for (declaration, operation) in [
        ("identity(field(id))", "fresh next {data {id=7,};identity=none;lock=exact(owner);}"),
        ("identity(field(id))", "fresh next {data {id=7,};identity=script_args;lock=exact(owner);}"),
        ("", "create_unique::<Token>() {id:7,};"),
        ("", "replace_unique::<Token>() before {id:before.id,};"),
    ] {
        let ports = if operation.starts_with("fresh") {
            "witness owner:Address,output next:Token"
        } else if operation.starts_with("replace") {
            "input before:Token"
        } else {
            ""
        };
        let source = format!("module invalid_identity; resource Token has store,create,consume,replace {declaration} {{id:u64,}} action issue({ports}) {{{operation}}}");
        let error = cellscript::compile_metadata(&source, CellScriptEdition::Edition2027, None).unwrap_err();
        assert!(error.message.contains("identity"), "{source}: {error}");
    }
}

#[test]
fn type_id_aliases_require_one_unambiguous_defining_declaration() {
    let sources = [
        cellscript::InMemorySource { path:"main.cell".into(),role:None, source:"module caller; use schema::Token as Asset; use schema::Other; action mint(witness owner:Address,output next:Asset) {fresh next {data {id=7,};lock=exact(owner);}}".into() },
        cellscript::InMemorySource { path:"schema.cell".into(),role:None, source:"module schema; #[type_id(\"identity_defaults::Token\")] public resource Token has store,create identity(ckb_type_id) {id:u64,} #[type_id(\"identity_defaults::Other\")] public resource Other has store {id:u64,}".into() },
    ];
    let report = cellscript::compile_sources_metadata_with_diagnostics(
        &sources,
        "main.cell",
        CellScriptEdition::Edition2027,
        Some("riscv64-elf".into()),
    );
    let metadata = report.metadata.unwrap_or_else(|| panic!("{:?}", report.diagnostics));
    cellscript::validate_compile_metadata(&metadata, cellscript::ArtifactFormat::RiscvElf).unwrap();
    for mutation in 0..4 {
        let mut forged = metadata.clone();
        match mutation {
            0 => forged.typed_semantics.nominal_declarations = None,
            1 => {
                let catalog = forged.typed_semantics.nominal_declarations.as_mut().unwrap();
                let binding = catalog.layout_bindings.iter_mut().find(|binding| binding.lowered_name == "Asset").unwrap();
                binding.source_name = "Other".into();
            }
            2 => {
                let catalog = forged.typed_semantics.nominal_declarations.as_mut().unwrap();
                let binding = catalog.layout_bindings.iter().find(|binding| binding.lowered_name == "Asset").unwrap().clone();
                catalog.layout_bindings.push(binding);
            }
            _ => {
                let original = forged.types.iter().find(|ty| ty.name == "Asset").unwrap().clone();
                for ty in &mut forged.types {
                    if ty.name == "Other" || ty.name == "schema::Other" {
                        ty.type_id = original.type_id.clone();
                        ty.type_id_hash = original.type_id_hash.clone();
                    }
                }
            }
        }
        forged.typed_semantics_hash =
            cellscript_artifact_checker::canonical_hash(cellscript_artifact_checker::TYPED_SEMANTICS_SCHEMA, &forged.typed_semantics)
                .unwrap();
        let error = cellscript::validate_compile_metadata(&forged, cellscript::ArtifactFormat::RiscvElf).unwrap_err();
        assert!(error.message.contains("declared by both"), "mutation {mutation}: {error}");
    }
}

#[test]
fn receipt_policy_metadata_resolves_import_aliases_and_qualified_results() {
    let schema = "module schema; resource Token has store {amount:u64,} receipt Voucher -> Token has store,consume {amount:u64,}";
    for (imports, receipt, result) in [
        ("use schema::Voucher; use schema::Token;", "Voucher", "Token"),
        ("use schema::Voucher as Ticket; use schema::Token as Asset;", "Ticket", "Asset"),
        ("use schema::Voucher as Ticket;", "Ticket", "schema::Token"),
    ] {
        let source = format!("module caller; {imports} action redeem(input voucher:{receipt})->{result} {{claim voucher}}");
        let sources = [
            cellscript::InMemorySource { path: "main.cell".into(), source, role: None },
            cellscript::InMemorySource { path: "schema.cell".into(), source: schema.into(), role: None },
        ];
        let report =
            cellscript::compile_sources_metadata_with_diagnostics(&sources, "main.cell", CellScriptEdition::Edition2027, None);
        let metadata = report.metadata.unwrap_or_else(|| panic!("{imports}: {:?}", report.diagnostics));
        let declared = metadata.typed_semantics.types.iter().find(|ty| ty.name == receipt).unwrap();
        assert_eq!(declared.claim_output.as_deref(), Some(result));
        let entry = metadata.typed_semantics.entries.iter().find(|entry| entry.name == "redeem").unwrap();
        assert!(entry.cell_bindings.iter().any(|binding| binding.role == CellBindingRole::Output && binding.ty == result));
        assert!(metadata
            .typed_semantics
            .foundation
            .dispositions
            .iter()
            .any(|record| record.kernel_execution.as_ref().is_some_and(|site| site.opcode == "claim" && site.result_local.is_some())));
    }
}

#[test]
fn named_output_order_is_independent_of_creation_and_other_parameter_positions() {
    let source = r#"
module resolved_bindings::declared_outputs;
resource Token has store, create, consume { amount: u64, }
action split(witness recipient: Address, input seed: Token, witness amount: u64, output first: Token, output second: Token) {
    create second = Token { amount, } with_lock(recipient);
    create first = Token { amount: seed.amount, } with_lock(recipient);
    consume seed;
}
"#;
    let interleaved = source.replace(
        "witness recipient: Address, input seed: Token, witness amount: u64, output first: Token, output second: Token",
        "output first: Token, witness recipient: Address, input seed: Token, output second: Token, witness amount: u64",
    );
    assert_ne!(source, interleaved, "the parameter mutation must execute");
    let before = compile_source(source, CellScriptEdition::Edition2027);
    let after = compile_source(&interleaved, CellScriptEdition::Edition2027);
    assert_eq!(before.artifact_bytes, after.artifact_bytes);
    assert_eq!(before.metadata.typed_semantics, after.metadata.typed_semantics);
    for result in [&before, &after] {
        let entry = entry(result, "action:split");
        for (binding, ordinal) in [("first", 0), ("second", 1)] {
            assert_cell_binding(
                result,
                "action:split",
                binding,
                CellBindingRole::Output,
                CellBindingSource::Output,
                ordinal,
                CellBindingMembership::Unproven,
            );
        }
        assert_eq!(entry.cell_bindings.iter().filter(|binding| binding.role == CellBindingRole::Output).count(), 2);
    }
    let values = [EntryWitnessArg::Address([5; 32]), EntryWitnessArg::U64(9)];
    assert_eq!(
        before.metadata.actions[0].entry_witness_args(&values).unwrap(),
        after.metadata.actions[0].entry_witness_args(&values).unwrap()
    );
}

#[test]
fn anonymous_outputs_have_distinct_physical_roles_and_dispositions() {
    let source = r#"
module resolved_bindings::anonymous_outputs;
resource Token has store, consume { amount: u64, }
action split(input seed: Token, witness recipient: Address,  output named: Token) {

        create named = Token { amount: seed.amount, } with_lock(recipient);
        create Token { amount: 1, } with_lock(recipient);
        create Token { amount: 2, } with_lock(recipient);
        consume seed;
}
"#;
    {
        let edition = CellScriptEdition::Edition2027;
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
module resolved_bindings::mixed_inputs;
resource Token has store, consume { amount: u64, }
shared Config { value: u64, }

action inspect(
    witness tag: u64,
    input first: Token,
    read first_config: Config,
    witness ceiling: u64,
    input second: Token,
    read second_config: &Config,
) -> u64 {

        require ((tag == 1));
        require ((first.amount <= ceiling));
        require ((second.amount <= ceiling));
        require ((first_config.value == 11));
        require ((second_config.value == 22));
        consume first;
        consume second;
        return 0;
}
"#;

const MIXED_READS: &str = r#"
module resolved_bindings::mixed_reads;
shared Config { value: u64, }
action inspect(witness tag: u64, read first: Config, read second: &Config) -> u64 {

        let third = read_ref::<Config>();
        let fourth = read_ref::<Config>();
        require (tag == 1);
        require (first.value == 11);
        require (second.value == 22);
        require (third.value == 33);
        require (fourth.value == 44);
        return 0;
}
"#;

const MIXED_LOCK: &str = r#"
module resolved_bindings::mixed_lock;
resource Token { amount: u64, }
shared Config { value: u64, }

lock permit(
    lock_args minimum: u64,
    protected token: Token,
    witness claimed: u64,
    read config: Config,
    lock_args maximum: u64,
) -> bool {

        require token.amount >= minimum;
        require token.amount <= maximum;
        require claimed == config.value;
        true
}
"#;

const ORDINARY_SUCCESSOR: &str = r#"
module resolved_bindings::ordinary_successor;
resource Token has store, replace, relock { amount: u64, }
shared Config { value: u64, }

action transfer(read config: Config, input token: Token, witness recipient: Address,  output next: Token) {

        require token.amount == config.value;
        std::lifecycle::transfer(token, next, recipient) { amount, };
        std::cell::preserve_capacity(next, token)
}
"#;

const NATIVE_SUCCESSOR: &str = r#"// test-artifact: {"entry":"retain_value","layout":"Token"}

module resolved_bindings::native_successor;
resource Token has store, replace, relock { amount: u64, }
action retain_value(
        witness recipient: Address,
        input before: Token,
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
    {
        let edition = CellScriptEdition::Edition2027;
        let result = compile_source(MIXED_INPUTS, edition);
        let entry_id = "action:inspect";
        assert_eq!(entry(&result, entry_id).cell_bindings.len(), 4);
        let source_params = &entry(&result, entry_id).params;
        assert_eq!(source_params.iter().find(|param| param.name == "first").unwrap().source, "input");
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
    {
        let edition = CellScriptEdition::Edition2027;
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
    {
        let edition = CellScriptEdition::Edition2027;
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
fn ordinary_successors_remain_transaction_absolute_while_native_roles_are_group_relative() {
    {
        let edition = CellScriptEdition::Edition2027;
        let result = compile_source(ORDINARY_SUCCESSOR, edition);
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
