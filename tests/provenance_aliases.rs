//! Source aliases must not create or overwrite physical Cell bindings.

use std::collections::BTreeSet;

use cellscript::{compile, CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg};
use cellscript_artifact_checker::{CellBindingSource, TypedSemanticEntry, TypedSemanticOperationDetail, ValueProvenance};
use ckb_testtool::{
    ckb_types::{
        bytes::Bytes,
        core::{DepType, TransactionBuilder},
        packed,
        prelude::*,
    },
    context::Context,
};

const ALWAYS_SUCCESS: &str = r#"
module provenance_aliases::fixture_lock
action main() -> u64 { verification return 0 }
"#;

const COLLIDING_READ_NAMES: &str = r#"
module provenance_aliases::colliding_names
shared Config { value: u64 }
action inspect(read read_ref_Config: Config) -> u64 {
    verification
        let config = read_ref<Config>()
        require read_ref_Config.value == 11
        require config.value == 22
        return 0
}
"#;

const READ_ALIASES: &str = r#"
module provenance_aliases::read_aliases
shared Config { value: u64 }
action inspect(read first: Config) -> u64 {
    verification
        let first_alias = first
        let second = read_ref<Config>()
        let second_alias = second
        require first_alias.value == 11
        require second_alias.value == 22
        return 0
}
"#;

const HELPER_READS: &str = r#"
module provenance_aliases::helper_reads
shared Config { value: u64 }
fn inspect_helper(read forwarded: Config) -> u64 {
    let alias = forwarded
    let independent = read_ref<Config>()
    return alias.value + independent.value
}
action inspect(read config: Config) -> u64 {
    verification
        return inspect_helper(config)
}
"#;

const UNIT_BEFORE_WITNESS: &str = r#"
module provenance_aliases::unit_before_witness
action inspect(witness empty: (), witness amount: u64) -> u64 {
    verification
        require amount == 7
        return 0
}
"#;

const WITNESS_READ_NAME_COLLISION: &str = r#"
module provenance_aliases::witness_read_name_collision
shared Config { value: u64 }
action inspect(witness read_ref_Config: u64) -> u64 {
    verification
        let config = read_ref<Config>()
        require read_ref_Config == 7
        require config.value == 11
        return 0
}
"#;

const SCRIPT_ARGS_AGGREGATES: &str = r#"
module provenance_aliases::script_args_aggregates
lock permit(
    lock_args empty: (),
    lock_args addresses: [Address; 2],
    witness amount: u64,
    lock_args pair: (Address, u16),
    lock_args owner: Address,
    lock_args small: (u8, u16),
    lock_args bytes: [u8; 3],
) -> bool {
    verification
        require amount == 7
}
"#;

const UNSUPPORTED_SCRIPT_ARGS: &str = r#"
module provenance_aliases::unsupported_script_args
struct Snapshot { value: u64 }
lock permit(lock_args config: (Address, Snapshot)) -> bool {
    verification
        require true
}
"#;

fn compile_source(source: &str, edition: CellScriptEdition) -> CompileResult {
    let result = compile(source, CompileOptions { edition, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() })
        .unwrap_or_else(|error| panic!("{edition:?}: provenance fixture must compile: {error}"));
    result.validate().expect("alias provenance must pass the independent structural checker");
    result
}

fn entry<'a>(result: &'a CompileResult, id: &str) -> &'a TypedSemanticEntry {
    result.metadata.typed_semantics.entries.iter().find(|entry| entry.id == id).expect("expected typed entry")
}

fn execute_entry(result: &CompileResult, witness_payload: &[u8], script_args: &[u8]) -> Result<(), String> {
    execute_entry_with_deps(result, witness_payload, script_args, &[])
}

fn execute_entry_with_deps(
    result: &CompileResult,
    witness_payload: &[u8],
    script_args: &[u8],
    dependency_values: &[u64],
) -> Result<(), String> {
    let mut context = Context::new_with_deterministic_rng();
    let fixture_lock = compile(ALWAYS_SUCCESS, CompileOptions { target: Some("riscv64-elf".to_string()), ..Default::default() })
        .expect("fixture lock compiles");
    let lock_code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&fixture_lock.artifact_bytes)));
    let lock_script = context.build_script(&lock_code, Bytes::new()).unwrap();
    let code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&result.artifact_bytes)));
    let tested_script = context.build_script(&code, Bytes::copy_from_slice(script_args)).unwrap();
    let (lock, type_script) =
        if result.metadata.locks.is_empty() { (lock_script, Some(tested_script)) } else { (tested_script, None) };
    let cell = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(100_000_000_000u64.pack())
        .lock(lock)
        .type_(type_script.pack())
        .build();
    let input = context.create_cell(cell.clone(), Bytes::new());
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::copy_from_slice(witness_payload)).pack()).build();
    let mut tx = TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input).build())
        .output(cell)
        .output_data(Bytes::new().pack())
        .witness(witness.as_bytes().pack());
    for value in dependency_values {
        let dep = context.deploy_cell(Bytes::copy_from_slice(&value.to_le_bytes()));
        tx = tx.cell_dep(packed::CellDep::new_builder().out_point(dep).dep_type(DepType::Code).build());
    }
    let tx = context.complete_tx(tx.build());
    context.verify_tx(&tx, 10_000_000).map(|_| ()).map_err(|error| format!("{error:?}"))
}

fn param_provenance<'a>(result: &'a CompileResult, entry_id: &str, name: &str) -> &'a ValueProvenance {
    let param = entry(result, entry_id).params.iter().find(|param| param.name == name).expect("expected source parameter");
    let graph = &result.metadata.typed_semantics.foundation.provenance;
    let bindings = graph
        .bindings
        .iter()
        .filter(|binding| binding.entry_id == entry_id && binding.local_id == param.binding_id)
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 1, "source parameter has one provenance root");
    &graph.nodes.iter().find(|node| node.id == bindings[0].node_id).expect("parameter provenance node exists").provenance
}

fn provenance_leaves(result: &CompileResult, entry_id: &str, local_id: u32) -> BTreeSet<String> {
    let graph = &result.metadata.typed_semantics.foundation.provenance;
    let mut pending = graph
        .bindings
        .iter()
        .filter(|binding| binding.entry_id == entry_id && binding.local_id == local_id)
        .map(|binding| binding.node_id.as_str())
        .collect::<Vec<_>>();
    assert!(!pending.is_empty(), "local {entry_id}:{local_id} must have provenance");
    let mut visited = BTreeSet::new();
    let mut leaves = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let node = graph.nodes.iter().find(|node| node.id == id).expect("provenance node exists");
        match &node.provenance {
            ValueProvenance::Derived { operation, inputs } if inputs.is_empty() => {
                leaves.insert(format!("derived:{operation}"));
            }
            ValueProvenance::Derived { inputs, .. } => pending.extend(inputs.iter().map(String::as_str)),
            ValueProvenance::CellDep { identity_policy, selector, .. } => {
                leaves.insert(format!("{selector}:{identity_policy}"));
            }
            provenance => {
                leaves.insert(format!("{provenance:?}"));
            }
        }
    }
    leaves
}

fn assert_field_roots(result: &CompileResult, entry_id: &str, expected: &[&str]) {
    let entry = entry(result, entry_id);
    // Immutable source aliases may already have been eliminated by lowering.
    // Check the actual field receiver and derived result, not a temporary name.
    let fields = entry
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .filter(|operation| {
            operation.opcode == "field-access"
                && matches!(&operation.detail, TypedSemanticOperationDetail::Field { name } if name == "value")
        })
        .collect::<Vec<_>>();
    assert_eq!(fields.len(), expected.len(), "fixture must exercise every expected field access");
    for (index, (field, expected)) in fields.into_iter().zip(expected).enumerate() {
        let receiver = field.operands[0].local.expect("field receiver is a typed local");
        assert_eq!(
            provenance_leaves(result, entry_id, receiver),
            BTreeSet::from([expected.to_string()]),
            "field receiver {entry_id}:{index} must retain its own ancestry"
        );
        assert!(!field.destinations.is_empty(), "field access must produce a value");
        for destination in &field.destinations {
            assert_eq!(
                provenance_leaves(result, entry_id, *destination),
                BTreeSet::from([expected.to_string()]),
                "field result {entry_id}:{index} must retain its receiver ancestry"
            );
        }
    }
}

#[test]
fn generated_read_ref_names_do_not_overwrite_parameter_provenance() {
    {
        let edition = CellScriptEdition::Edition2027;
        let result = compile_source(COLLIDING_READ_NAMES, edition);
        assert_field_roots(&result, "action:inspect", &["cell-dep[0]:unproven", "cell-dep[1]:unproven"]);
        let roots = &entry(&result, "action:inspect").cell_bindings;
        assert_eq!(roots.len(), 2);
        assert_ne!(roots[0].local_id, roots[1].local_id, "identical generated names do not merge physical identities");
    }
}

#[test]
fn source_aliases_preserve_dependency_ancestry_without_new_physical_roles() {
    {
        let edition = CellScriptEdition::Edition2027;
        let result = compile_source(READ_ALIASES, edition);
        assert_field_roots(&result, "action:inspect", &["cell-dep[0]:unproven", "cell-dep[1]:unproven"]);
        assert_eq!(entry(&result, "action:inspect").cell_bindings.len(), 2, "source aliases are not extra Cell reads");
    }
}

#[test]
fn helper_parameters_are_caller_values_not_new_cell_dependencies() {
    {
        let edition = CellScriptEdition::Edition2027;
        let result = compile_source(HELPER_READS, edition);
        let helper = entry(&result, "helper:inspect_helper");
        let param = &helper.params[0];
        assert_eq!(
            provenance_leaves(&result, &helper.id, param.binding_id),
            BTreeSet::from(["derived:call-parameter:forwarded".to_string()])
        );
        assert_field_roots(&result, &helper.id, &["derived:call-parameter:forwarded", "cell-dep[0]:unproven"]);
        assert_eq!(helper.cell_bindings.len(), 1, "only the helper's explicit read_ref creates a physical binding");
        assert_eq!((helper.cell_bindings[0].source, helper.cell_bindings[0].ordinal), (CellBindingSource::CellDep, 0));
        assert_ne!(helper.cell_bindings[0].local_id, Some(param.binding_id));
    }
}

#[test]
fn zero_width_unit_is_constant_and_does_not_shift_encoded_witness_values() {
    {
        let edition = CellScriptEdition::Edition2027;
        let result = compile_source(UNIT_BEFORE_WITNESS, edition);
        assert_eq!(
            param_provenance(&result, "action:inspect", "empty"),
            &ValueProvenance::Constant { declaration: "Unit".to_string() }
        );
        assert!(matches!(
            param_provenance(&result, "action:inspect", "amount"),
            ValueProvenance::EntryWitness { field_path, .. } if field_path == "args[0].amount"
        ));
        let action = &result.metadata.actions[0];
        let abi = result.metadata.constraints.entry_abi.iter().find(|entry| entry.entry_name == "inspect").unwrap();
        assert_eq!((abi.abi_slots_used, abi.witness_payload_bytes), (2, 8), "Unit retains one value slot but no witness bytes");
        assert_eq!((abi.params[0].abi_slots, abi.params[0].witness_bytes), (1, 0));
        let without_placeholder =
            action.entry_witness_args(&[EntryWitnessArg::U64(7)]).expect("Unit need not be supplied to the host encoder");
        let with_placeholder = action
            .entry_witness_args(&[EntryWitnessArg::Unit, EntryWitnessArg::U64(7)])
            .expect("explicit host Unit placeholder remains compatible");
        assert_eq!(without_placeholder, with_placeholder, "optional host Unit values consume no wire bytes");
        execute_entry(&result, &without_placeholder, &[]).expect("VM must accept witness bytes with no host Unit placeholder");
        execute_entry(&result, &with_placeholder, &[]).expect("explicit host Unit encoding must remain executable");
        let bad_value = action.entry_witness_args(&[EntryWitnessArg::U64(8)]).unwrap();
        assert!(execute_entry(&result, &bad_value, &[]).is_err(), "VM must read and reject the incorrect real value after Unit");
    }
}

#[test]
fn script_args_ranges_follow_original_ir_array_and_tuple_widths() {
    {
        let edition = CellScriptEdition::Edition2027;
        let result = compile_source(SCRIPT_ARGS_AGGREGATES, edition);
        assert_eq!(param_provenance(&result, "lock:permit", "empty"), &ValueProvenance::Constant { declaration: "Unit".to_string() });
        for (name, expected_range) in
            [("addresses", "0..64"), ("pair", "64..98"), ("owner", "98..130"), ("small", "130..133"), ("bytes", "133..136")]
        {
            assert_eq!(
                param_provenance(&result, "lock:permit", name),
                &ValueProvenance::ScriptArgs {
                    script_role: "lock".to_string(),
                    byte_range: expected_range.to_string(),
                    codec: "typed-fixed-bytes".to_string(),
                }
            );
        }
        assert!(matches!(
            param_provenance(&result, "lock:permit", "amount"),
            ValueProvenance::EntryWitness { field_path, .. } if field_path == "args[0].amount"
        ));
        let witness = result.metadata.locks[0].entry_witness_args(&[EntryWitnessArg::U64(7)]).unwrap();
        execute_entry(&result, &witness, &[0; 136]).expect("VM must accept exactly the projected Script.args bytes");
        assert!(execute_entry(&result, &witness, &[0; 135]).is_err(), "truncated Script.args must reject");
        assert!(execute_entry(&result, &witness, &[0; 137]).is_err(), "trailing Script.args must reject");
    }
}

#[test]
fn unsupported_script_args_shapes_stay_rejected() {
    {
        let edition = CellScriptEdition::Edition2027;
        let error = compile(UNSUPPORTED_SCRIPT_ARGS, CompileOptions { edition, ..CompileOptions::default() })
            .expect_err("named struct inside Script.args tuple is not an executable fixed-width entry type");
        assert!(error.message.contains("fixed-width script-args type"), "unexpected boundary error: {error}");
    }
}

#[test]
fn scalar_witness_named_like_read_ref_is_not_rebound_to_a_cell() {
    {
        let edition = CellScriptEdition::Edition2027;
        let result = compile_source(WITNESS_READ_NAME_COLLISION, edition);
        assert!(matches!(
            param_provenance(&result, "action:inspect", "read_ref_Config"),
            ValueProvenance::EntryWitness { field_path, .. } if field_path == "args[0].read_ref_Config"
        ));
        assert_eq!(entry(&result, "action:inspect").cell_bindings.len(), 1, "only the explicit read has a Cell binding");
        let action = &result.metadata.actions[0];
        let witness = action.entry_witness_args(&[EntryWitnessArg::U64(7)]).expect("scalar parameter remains a witness value");
        let legacy_name_override = cellscript::encode_entry_witness_args_for_params_with_runtime_bound(
            &action.params,
            &[EntryWitnessArg::U64(7)],
            &BTreeSet::from(["read_ref_Config".to_string()]),
        )
        .expect("legacy name hints cannot override an ordinary scalar witness parameter");
        assert_eq!(witness, legacy_name_override);
        execute_entry_with_deps(&result, &witness, &[], &[11]).expect("VM must read the witness and independent CellDep");
        let bad_witness = action.entry_witness_args(&[EntryWitnessArg::U64(8)]).unwrap();
        assert!(execute_entry_with_deps(&result, &bad_witness, &[], &[11]).is_err(), "incorrect witness must reject");
        assert!(
            execute_entry_with_deps(&result, &[], &[], &[11]).is_err(),
            "missing witness must not be satisfied by a same-named CellDep"
        );
    }
}
