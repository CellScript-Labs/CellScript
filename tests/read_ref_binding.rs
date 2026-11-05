use cellscript::{compile, CellScriptEdition, CompileOptions, CURRENT_EDITION};
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
module read_ref_fixture_lock
action main() -> u64 { verification return 0 }
"#;

const READ_PARAMETERS: &str = r#"
module read_parameter_bindings
shared Config { value: u64 }
action inspect(read first: Config, read second: &Config) -> u64 {
    verification
        require first.value == 11
        require second.value == 22
        return 0
}
"#;

const READ_EXPRESSIONS: &str = r#"
module read_expression_bindings
shared Config { value: u64 }
action inspect() -> u64 {
    verification
        let first = read_ref<Config>()
        let second = read_ref<Config>()
        require first.value == 11
        require second.value == 22
        return 0
}
"#;

const MIXED_READS: &str = r#"
module mixed_read_bindings
shared Config { value: u64 }
action inspect(read first: Config, read second: &Config) -> u64 {
    verification
        let third = read_ref<Config>()
        let fourth = read_ref<Config>()
        require first.value == 11
        require second.value == 22
        require third.value == 33
        require fourth.value == 44
        return 0
}
"#;

const MIXED_READ_EQUALITY: &str = r#"
module mixed_read_equality
shared Config { value: u64 }
action inspect(read first: Config, read second: &Config) -> u64 {
    verification
        let third = read_ref<Config>()
        let fourth = read_ref<Config>()
        require first.value == 11
        require second.value == 22
        require third.value == first.value
        require fourth.value == second.value
        return 0
}
"#;

const COLLIDING_READ_NAMES: &str = r#"
module colliding_read_names
shared Config { value: u64 }
action inspect(read read_ref_Config: Config) -> u64 {
    verification
        let config = read_ref<Config>()
        require read_ref_Config.value == 11
        require config.value == 22
        return 0
}
"#;

fn execute_reads(source: &str, edition: CellScriptEdition, values: &[u64]) -> Result<(), String> {
    let options = CompileOptions { edition, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() };
    let mut context = Context::new_with_deterministic_rng();
    let lock = compile(ALWAYS_SUCCESS, options.clone()).expect("fixture lock compiles");
    let lock_code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&lock.artifact_bytes)));
    let lock_script = context.build_script(&lock_code, Bytes::new()).unwrap();
    let result = compile(source, options).expect("read binding contract compiles");
    let code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&result.artifact_bytes)));
    let script = context.build_script(&code, Bytes::new()).unwrap();
    let cell = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(100_000_000_000u64.pack())
        .lock(lock_script.clone())
        .type_(Some(script).pack())
        .build();
    let input = context.create_cell(cell.clone(), Bytes::new());
    let mut tx = TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input).build())
        .output(cell)
        .output_data(Bytes::new().pack());
    // Explicit data dependencies precede the executable dependencies added by
    // complete_tx. Create distinct Cells even when their data bytes are equal.
    for value in values {
        let dep = context.create_cell(
            packed::CellOutput::new_builder().capacity::<packed::Uint64>(100_000_000_000u64.pack()).lock(lock_script.clone()).build(),
            Bytes::copy_from_slice(&value.to_le_bytes()),
        );
        tx = tx.cell_dep(packed::CellDep::new_builder().out_point(dep).dep_type(DepType::Code).build());
    }
    let tx = context.complete_tx(tx.build());
    context.verify_tx(&tx, 10_000_000).map(|_| ()).map_err(|error| format!("{error:?}"))
}

#[test]
fn homogeneous_read_forms_bind_distinct_dependencies_in_edition_2027() {
    {
        let edition = CURRENT_EDITION;
        for source in [READ_PARAMETERS, READ_EXPRESSIONS] {
            execute_reads(source, edition, &[11, 22]).expect("distinct dependencies must satisfy the read constraints");
            assert!(execute_reads(source, edition, &[22, 11]).is_err(), "{edition:?}: swapped data dependencies must reject");
        }
    }
}

#[test]
fn mixed_read_forms_bind_four_distinct_dependencies_in_edition_2027() {
    let mut failures = Vec::new();
    {
        let edition = CURRENT_EDITION;
        if let Err(error) = execute_reads(MIXED_READS, edition, &[11, 22, 33, 44]) {
            failures.push(format!("{edition:?}: two read parameters and two expressions must bind CellDep[0..4]: {error}"));
        }
        for index in 0..4 {
            let mut values = [11, 22, 33, 44];
            values[index] += 1;
            assert!(execute_reads(MIXED_READS, edition, &values).is_err(), "{edition:?}: incorrect CellDep[{index}] must reject");
        }
        assert!(
            execute_reads(MIXED_READS, edition, &[11, 22, 33]).is_err(),
            "{edition:?}: missing fourth data dependency must reject"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn mixed_read_expressions_cannot_alias_parameter_dependencies() {
    let mut failures = Vec::new();
    {
        let edition = CURRENT_EDITION;
        execute_reads(MIXED_READ_EQUALITY, edition, &[11, 22, 11, 22]).expect("four separately bound matching values must pass");
        if execute_reads(MIXED_READ_EQUALITY, edition, &[11, 22, 33, 44]).is_ok() {
            failures
                .push(format!("{edition:?}: third and fourth dependency values must be checked, not aliases of the read parameters"));
        }
        if execute_reads(MIXED_READ_EQUALITY, edition, &[11, 22]).is_ok() {
            failures.push(format!("{edition:?}: missing expression dependencies must not be satisfied by parameter aliases"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn read_binding_identity_is_not_its_generated_name() {
    {
        let edition = CURRENT_EDITION;
        execute_reads(COLLIDING_READ_NAMES, edition, &[11, 22]).expect("a parameter may share the generated read_ref name");
        assert!(
            execute_reads(COLLIDING_READ_NAMES, edition, &[11, 11]).is_err(),
            "{edition:?}: expression identity must remain distinct from a same-named parameter"
        );
    }
}
