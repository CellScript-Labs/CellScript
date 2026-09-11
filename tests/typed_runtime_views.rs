//! Executable evidence for the bounded typed CKB transaction-view surface.

use cellscript::{
    artifact::{
        compile_artifact, encode_policy_action_record, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch,
    },
    compile_with_executable_surface_policy, strip_vm_abi_trailer, CellScriptEdition, CompileOptions, EntryWitnessArg,
    ExecutableSurfacePolicy,
};
use cellscript_ckb_adapter::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
use ckb_testtool::{
    ckb_hash::blake2b_256,
    ckb_types::{bytes::Bytes, packed, prelude::*},
};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{
    build_simple_fixture, deterministic_always_success_lock_hash, deterministic_always_success_script, execute_cellscript_script,
    execute_cellscript_script_with_transaction_transform, FixtureCell, FixtureHeaderContext,
};

const SOURCE: &str = r#"
module runtime_views::header

resource Token has store { amount: u64 }

fn preserve_epoch_since(value: AbsoluteEpochSince) -> AbsoluteEpochSince {
    return value
}

action inspect(witness expected_data_hash: Hash) -> u64 {
    let input = ckb::input<Token>(0)
    let dep = ckb::cell_dep(0)
    let header = ckb::header_dep(0)
    let transaction_hash = ckb::transaction_hash()
    let earlier = preserve_epoch_since(ckb::since_absolute_epoch(42, 3, 10))
    let later = ckb::since_absolute_epoch(43, 0, 10)
    let half = ckb::since_absolute_epoch(42, 1, 2)
    let two_fifths = ckb::since_absolute_epoch(42, 2, 5)
    let equivalent_half = ckb::since_absolute_epoch(42, 2, 4)
    let relative = ckb::since_relative_epoch(2, 1, 4)
    let absolute_block = ckb::since_absolute_block(123)
    let later_absolute_block = ckb::since_absolute_block(124)
    let relative_block = ckb::since_relative_block(7)
    let absolute_timestamp = ckb::since_absolute_timestamp(1700000000)
    let later_absolute_timestamp = ckb::since_absolute_timestamp(1700000001)
    let relative_timestamp = ckb::since_relative_timestamp(3600)
    let disabled = ckb::since_decode(input.since)
    let five_epochs = ckb::epoch_duration(5)
    let seven_epochs = ckb::epoch_duration(7)
    let epoch_after = ckb::epoch_add(header.epoch_number, five_epochs)
    let epoch_before = ckb::epoch_sub(header.epoch_number, five_epochs)
    let decoded_epoch = ckb::since_from_raw_checked(2305854004380303402)
    let decoded_zero_fraction = ckb::since_from_raw_checked(2305843009213693994)
    let decoded_relative_timestamp = ckb::since_from_raw_checked(13835058055282167312)
    require ckb::since_to_raw(earlier) == 2305854004380303402
    require earlier < later
    require earlier <= later
    require later > earlier
    require later >= earlier
    require half > two_fifths
    require half == equivalent_half
    require half != two_fifths
    require ckb::since_to_raw(relative) == 11529219444131758082
    require ckb::since_to_raw(absolute_block) == 123
    require ckb::since_to_raw(relative_block) == 9223372036854775815
    require absolute_block < later_absolute_block
    require ckb::since_to_raw(absolute_timestamp) == 4611686020127387904
    require ckb::since_to_raw(relative_timestamp) == 13835058055282167312
    require absolute_timestamp < later_absolute_timestamp
    require ckb::since_is_disabled(disabled)
    require !ckb::since_is_relative(disabled)
    require ckb::since_metric(disabled) == 0
    require ckb::since_value(disabled) == 0
    require ckb::since_metric(decoded_epoch) == 1
    require ckb::since_value(decoded_epoch) == 10995166609450
    require ckb::since_as_absolute_epoch(decoded_epoch) == earlier
    require ckb::since_as_absolute_epoch(decoded_zero_fraction) == ckb::since_absolute_epoch(42, 0, 1)
    require ckb::since_is_relative(decoded_relative_timestamp)
    require ckb::since_metric(decoded_relative_timestamp) == 2
    require ckb::since_value(decoded_relative_timestamp) == 3600
    require ckb::since_as_relative_timestamp(decoded_relative_timestamp) == relative_timestamp
    require five_epochs < seven_epochs
    require ckb::epoch_duration_to_u64(five_epochs) == 5
    require ckb::epoch_number_to_u64(epoch_after) == 47
    require ckb::epoch_number_to_u64(epoch_before) == 37
    require ckb::since_to_raw(input.since) == 0
    require input.occupied_capacity <= input.capacity
    require input.unoccupied_capacity + input.occupied_capacity == input.capacity
    require dep.data_hash == expected_data_hash
    require transaction_hash != Hash::zero()
    require ckb::epoch_number_to_u64(header.epoch_number) == 42
    require ckb::block_number_to_u64(header.epoch_start_block_number) == 97
    require ckb::epoch_length_to_u64(header.epoch_length) == 10
    require ckb::block_number_to_u64(header.block_number) == 100
    require ckb::timestamp_millis_to_u64(header.timestamp) == 1700000000123
    return 0
}
"#;

const DYNAMIC_INDEX_SOURCE: &str = r#"
module runtime_views::dynamic_index

resource Token has store { amount: u64 }

action inspect(witness source_index: u64, witness expected_data_hash: Hash) -> u64 {
    let input = ckb::input<Token>(source_index)
    let dep = ckb::cell_dep(source_index)
    let witness_args = witness::args(source_index)
    require input.capacity > 0
    require dep.data_hash == expected_data_hash
    require witness_args.size > 0
    return 0
}
"#;

const OUTPUT_SCRIPT_SOURCE: &str = r#"
module runtime_views::output_script

resource Token has store { amount: u64 }

action inspect() -> u64 {
    let input = ckb::input<Token>(0)
    let output = ckb::output<Token>(0)
    let group_output = ckb::group_output<Token>(0)
    let output_lock = output.lock
    let group_lock = group_output.lock
    let output_type = output.type_script
    let group_type = group_output.type_script
    if ckb::cell_has_type(input) || !ckb::cell_has_type(output) || !ckb::cell_has_type(group_output) {
        return 80
    }
    if output.output_index != 0 || group_output.output_index != 0 {
        return 81
    }
    if output.capacity != 200000000000 || group_output.capacity != 200000000000 {
        return 82
    }
    if output.occupied_capacity > output.capacity || group_output.occupied_capacity > group_output.capacity {
        return 83
    }
    if output.unoccupied_capacity + output.occupied_capacity != output.capacity || group_output.unoccupied_capacity + group_output.occupied_capacity != group_output.capacity {
        return 84
    }
    if output.data_size != 257 || group_output.data_size != 513 {
        return 85
    }
    if output.data_hash == group_output.data_hash || output.lock_hash != group_output.lock_hash {
        return 86
    }
    if output_lock.hash != group_lock.hash || output_lock.code_hash != group_lock.code_hash || output_lock.hash_type != group_lock.hash_type || !output_lock.args_empty || !group_lock.args_empty {
        return 87
    }
    if output.type_hash != output_type.hash || group_output.type_hash != group_type.hash || output_type.hash == group_type.hash {
        return 88
    }
    if output_type.code_hash == group_type.code_hash || output_type.hash_type != group_type.hash_type || output_type.args_empty || group_type.args_empty || output_type.args_hash == Hash::zero() {
        return 89
    }
    return 0
}
"#;

const INPUT_GROUP_DEP_WITNESS_SOURCE: &str = r#"
module runtime_views::input_group_dep_witness

resource Token has store { amount: u64 }

action inspect() -> u64 {
    let input = ckb::input<Token>(0)
    let group_input = ckb::group_input<Token>(0)
    let dep = ckb::cell_dep(0)
    let witness_args = witness::args(0)
    let input_lock = input.lock
    let group_lock = group_input.lock
    let input_type = input.type_script
    let group_type = group_input.type_script
    let dep_lock = dep.lock
    let dep_type = dep.type_script
    let input_out_point = input.out_point
    let group_out_point = group_input.out_point
    if input.capacity != group_input.capacity || input.data_size != group_input.data_size {
        return 80
    }
    if input.occupied_capacity != group_input.occupied_capacity || input.unoccupied_capacity != group_input.unoccupied_capacity {
        return 81
    }
    if input.unoccupied_capacity + input.occupied_capacity != input.capacity {
        return 82
    }
    if input.data_hash != group_input.data_hash || input.lock_hash != group_input.lock_hash || input.type_hash != group_input.type_hash {
        return 83
    }
    if input_lock.hash != group_lock.hash || input_lock.code_hash != group_lock.code_hash || input_lock.hash_type != group_lock.hash_type || !input_lock.args_empty || !group_lock.args_empty {
        return 84
    }
    if input_type.hash != group_type.hash || input_type.code_hash != group_type.code_hash || input_type.hash_type != group_type.hash_type || input_type.args_empty || group_type.args_empty || input_type.args_hash != group_type.args_hash {
        return 85
    }
    if input_out_point.tx_hash != group_out_point.tx_hash || input_out_point.index != group_out_point.index || ckb::since_to_raw(input.since) != ckb::since_to_raw(group_input.since) {
        return 86
    }
    if dep.capacity == 0 || dep.data_size != 73 || dep.occupied_capacity > dep.capacity || dep.unoccupied_capacity + dep.occupied_capacity != dep.capacity {
        return 87
    }
    if dep.data_hash == input.data_hash || dep.lock_hash != dep_lock.hash || dep.type_hash != dep_type.hash {
        return 88
    }
    if dep_lock.code_hash != input_lock.code_hash || dep_lock.hash_type != input_lock.hash_type || !dep_lock.args_empty {
        return 89
    }
    if dep_type.code_hash == input_type.code_hash || dep_type.hash_type != input_type.hash_type || dep_type.args_empty || dep_type.args_hash == input_type.args_hash {
        return 90
    }
    if witness_args.size == 0 || witness_args.lock == Hash::zero() || witness_args.input_type == Hash::zero() || witness_args.output_type == Hash::zero() {
        return 91
    }
    if witness_args.lock == witness_args.input_type || witness_args.input_type == witness_args.output_type {
        return 92
    }
    return 0
}
"#;

const PERSISTENT_RUNTIME_VIEW_SOURCE: &str = r#"
module runtime_views::persistent_complete

resource Token has store { amount: u64 }

action inspect(
    input before: Token,
    witness expected_dep_size: u64,
    witness owner: Address,
) -> after: Token {
    let input = ckb::group_input<Token>(0)
    let output = ckb::group_output<Token>(0)
    let dep = ckb::cell_dep(0)
    let header = ckb::header_dep(0)
    let witness_args = witness::args(0)
    let entry = witness::bounded_entry(witness_args, 256)
    let input_lock = input.lock
    let output_lock = output.lock
    let input_type = input.type_script
    let output_type = output.type_script
    let dep_lock = dep.lock
    let dep_type = dep.type_script
    let out_point = input.out_point
    let transaction_hash = ckb::transaction_hash()
    require input.capacity == output.capacity
    require input.data_size == output.data_size
    require input.data_hash == output.data_hash
    require input.occupied_capacity <= input.capacity
    require output.occupied_capacity <= output.capacity
    require input.unoccupied_capacity + input.occupied_capacity == input.capacity
    require output.unoccupied_capacity + output.occupied_capacity == output.capacity
    require input.lock_hash == output.lock_hash
    require input.type_hash == output.type_hash
    require output.output_index == 0
    require input_lock.hash == output_lock.hash
    require input_lock.code_hash == output_lock.code_hash
    require input_lock.hash_type == output_lock.hash_type
    require input_lock.args_empty
    require output_lock.args_empty
    require input_type.hash == output_type.hash
    require input_type.code_hash == output_type.code_hash
    require input_type.hash_type == output_type.hash_type
    require !input_type.args_empty
    require !output_type.args_empty
    require input_type.args_hash == output_type.args_hash
    require out_point.tx_hash != Hash::zero()
    require out_point.index == 0
    require ckb::since_to_raw(input.since) == 0
    require dep.capacity > 0
    require dep.data_size == expected_dep_size
    require dep.occupied_capacity <= dep.capacity
    require dep.unoccupied_capacity + dep.occupied_capacity == dep.capacity
    require dep.data_hash != input.data_hash
    require dep.lock_hash == dep_lock.hash
    require dep.type_hash == dep_type.hash
    require dep_lock.code_hash == input_lock.code_hash
    require dep_lock.hash_type == input_lock.hash_type
    require dep_lock.args_empty
    require dep_type.code_hash != input_type.code_hash
    require dep_type.hash_type == input_type.hash_type
    require !dep_type.args_empty
    require dep_type.args_hash != input_type.args_hash
    require ckb::epoch_number_to_u64(header.epoch_number) == 42
    require ckb::block_number_to_u64(header.epoch_start_block_number) == 97
    require ckb::epoch_length_to_u64(header.epoch_length) == 10
    require ckb::block_number_to_u64(header.block_number) == 100
    require ckb::timestamp_millis_to_u64(header.timestamp) == 1700000000123
    require witness_args.size > 0
    require entry.size > 0
    require witness::byte(entry, 0) > 0
    require transaction_hash != Hash::zero()
    consume before
    create after = Token { amount: before.amount } with_lock(owner)
}
"#;

const OUT_POINT_INDEX_SOURCE: &str = r#"
module runtime_views::out_point_index

struct OutPoint {
    tx_hash: Hash,
    index: u32
}

action inspect(witness expected: OutPoint) -> u64 {
    let actual_index = ckb::input_out_point_index(source::group_input(0))
    require expected.index == actual_index
    return 0
}
"#;

fn compile(source: &str) -> cellscript::CompileResult {
    compile_with_executable_surface_policy(
        source,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("typed runtime-view source must compile: {error}\n{source}"))
}

fn compile_persistent_runtime_view_policy() -> cellscript::CompileResult {
    compile_artifact(
        PERSISTENT_RUNTIME_VIEW_SOURCE,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
        ArtifactDeclaration {
            name: "PersistentRuntimeView".to_string(),
            context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            actions: vec![ArtifactAction { tag: 10, action: "inspect".to_string() }],
            common_checks: Vec::new(),
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("persistent runtime-view policy must compile: {error}"))
}

#[test]
fn input_out_point_index_retains_its_u32_type_through_verified_lowering() {
    let result = compile(OUT_POINT_INDEX_SOURCE);
    let call = result
        .verified_lowering_record
        .as_ref()
        .expect("verified lowering")
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .find_map(|operation| operation.call.as_ref().filter(|call| call.target == "__ckb_input_out_point_index"));
    assert_eq!(call.expect("input OutPoint index runtime call").return_type, "u32");
}

fn witness(result: &cellscript::CompileResult, expected_data_hash: [u8; 32]) -> Bytes {
    let payload = result.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::Hash(expected_data_hash)])
        .expect("encode expected CellDep data hash");
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

fn dynamic_index_witness(result: &cellscript::CompileResult, source_index: u64, expected_data_hash: [u8; 32]) -> Bytes {
    let payload = result.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::U64(source_index), EntryWitnessArg::Hash(expected_data_hash)])
        .expect("encode dynamic source index and expected CellDep data hash");
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

fn fixture(dep_data: Bytes, witness: Bytes) -> ckb_script_runner::CkbVmFixture {
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.cell_deps.push(FixtureCell { capacity: 100_000_000_000, type_script: None, data: dep_data });
    fixture.witnesses = vec![witness];
    fixture.header_dao_fields = vec![[0; 32]];
    fixture.header_contexts =
        vec![FixtureHeaderContext { number: 100, timestamp: 1_700_000_000_123, epoch_number: 42, epoch_index: 3, epoch_length: 10 }];
    fixture
}

fn output_script_fixture() -> ckb_script_runner::CkbVmFixture {
    let mut fixture = build_simple_fixture(Bytes::from(vec![0x22; cellscript::CKB_SCRIPT_HASH_MAX_ARGS_BYTES]), 1, 2);
    fixture.inputs[0].capacity = 500_000_000_000;
    fixture.inputs[0].data = Bytes::from(vec![0x44; 17]);
    fixture.outputs[0] = FixtureCell {
        capacity: 200_000_000_000,
        type_script: Some(deterministic_always_success_script(Bytes::from(vec![0x33; 32]))),
        data: Bytes::from(vec![0x55; 257]),
    };
    fixture.outputs[1].capacity = 200_000_000_000;
    fixture.outputs[1].data = Bytes::from(vec![0x66; 513]);
    fixture
}

fn input_group_dep_witness_fixture() -> ckb_script_runner::CkbVmFixture {
    let mut fixture = build_simple_fixture(Bytes::from(vec![0x21; 32]), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.inputs[0].capacity = 300_000_000_000;
    fixture.inputs[0].data = Bytes::from(vec![0x31; 64]);
    fixture.outputs[0].capacity = 300_000_000_000;
    fixture.outputs[0].data = Bytes::from(vec![0x41; 64]);
    fixture.cell_deps.push(FixtureCell {
        capacity: 200_000_000_000,
        type_script: Some(deterministic_always_success_script(Bytes::from(vec![0x51; 32]))),
        data: Bytes::from(vec![0x61; 73]),
    });
    fixture.witnesses = vec![packed::WitnessArgs::new_builder()
        .lock(Some(Bytes::from(vec![0xa1; 32])).pack())
        .input_type(Some(Bytes::from(vec![0xb2; 32])).pack())
        .output_type(Some(Bytes::from(vec![0xc3; 32])).pack())
        .build()
        .as_bytes()];
    fixture
}

fn persistent_runtime_view_fixture() -> ckb_script_runner::CkbVmFixture {
    let mut fixture = input_group_dep_witness_fixture();
    fixture.inputs[0].data = Bytes::copy_from_slice(&7u64.to_le_bytes());
    fixture.outputs[0].data = Bytes::copy_from_slice(&7u64.to_le_bytes());
    fixture.header_dao_fields = vec![[0; 32]];
    fixture.header_contexts =
        vec![FixtureHeaderContext { number: 100, timestamp: 1_700_000_000_123, epoch_number: 42, epoch_index: 3, epoch_length: 10 }];
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().build().as_bytes()];
    fixture
}

#[test]
fn typed_cell_input_and_header_views_execute_and_fail_closed() {
    let result = compile(SOURCE);
    let dep_data = Bytes::from_static(b"cellscript-0.30-runtime-view");
    let expected_hash = blake2b_256(&dep_data);

    let valid = fixture(dep_data.clone(), witness(&result, expected_hash));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &valid);
    assert_eq!(execution.exit_code, 0, "all typed runtime-view fields must match: {:?}", execution.captured_debug);
    assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-epoch-checked-arithmetic".to_string()));
    assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-header-full-decode".to_string()));
    assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-header-block-number".to_string()));
    assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-header-timestamp-millis".to_string()));
    assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-transaction-hash".to_string()));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.syscall == "LOAD_HEADER" && access.source == "HeaderDep" && access.operation == "header-dep-timestamp-millis"
    }));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.syscall == "LOAD_TX_HASH"
            && access.source == "Transaction"
            && access.operation == "transaction-hash"
            && access.provenance.range.kind == "fixed-width"
            && access.provenance.range.length.value == Some(32)
    }));

    let mut wrong_hash = expected_hash;
    wrong_hash[0] ^= 0xff;
    let invalid = fixture(dep_data.clone(), witness(&result, wrong_hash));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &invalid);
    assert_ne!(execution.exit_code, 0, "a substituted CellDep data hash must reject");

    let missing_header_result = compile(&SOURCE.replace("ckb::header_dep(0)", "ckb::header_dep(1)"));
    let missing_header = fixture(dep_data, witness(&missing_header_result, expected_hash));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&missing_header_result.artifact_bytes), &missing_header);
    assert_eq!(execution.exit_code, 45, "a one-past-last HeaderDep must use the stable header-dep-missing error");

    let malformed_since_result =
        compile(&SOURCE.replace("ckb::since_absolute_epoch(42, 3, 10)", "ckb::since_absolute_epoch(42, 0, 0)"));
    let malformed_since =
        fixture(Bytes::from_static(b"cellscript-0.30-runtime-view"), witness(&malformed_since_result, expected_hash));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&malformed_since_result.artifact_bytes), &malformed_since);
    assert_eq!(execution.exit_code, 37, "a zero-length epoch fraction must use ckb-since-malformed");

    for source in [
        SOURCE.replace("ckb::since_absolute_block(123)", "ckb::since_absolute_block(72057594037927936)"),
        SOURCE.replace("ckb::since_absolute_timestamp(1700000000)", "ckb::since_absolute_timestamp(18446744073709552)"),
        SOURCE.replace("ckb::since_from_raw_checked(2305854004380303402)", "ckb::since_from_raw_checked(72057594037927936)"),
        SOURCE.replace("ckb::since_from_raw_checked(2305854004380303402)", "ckb::since_from_raw_checked(6917529027641081856)"),
        SOURCE.replace("ckb::since_from_raw_checked(2305854004380303402)", "ckb::since_from_raw_checked(2305844108742098986)"),
        SOURCE.replace("ckb::since_from_raw_checked(2305854004380303402)", "ckb::since_from_raw_checked(4630132762501097456)"),
        SOURCE.replace(
            "require ckb::since_as_absolute_epoch(decoded_epoch) == earlier",
            "require ckb::since_to_raw(ckb::since_as_relative_epoch(decoded_epoch)) >= 0",
        ),
    ] {
        let result = compile(&source);
        let invalid = fixture(Bytes::from_static(b"cellscript-0.30-runtime-view"), witness(&result, expected_hash));
        let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &invalid);
        assert_eq!(execution.exit_code, 37, "malformed or mismatched typed Since values must fail closed");
    }

    for source in [
        SOURCE.replace("ckb::epoch_duration(5)", "ckb::epoch_duration(16777216)"),
        SOURCE.replace(
            "let epoch_after = ckb::epoch_add(header.epoch_number, five_epochs)",
            "let epoch_after = ckb::epoch_add(header.epoch_number, ckb::epoch_duration(16777215))",
        ),
        SOURCE.replace(
            "let epoch_before = ckb::epoch_sub(header.epoch_number, five_epochs)",
            "let epoch_before = ckb::epoch_sub(header.epoch_number, ckb::epoch_duration(43))",
        ),
    ] {
        let result = compile(&source);
        let invalid = fixture(Bytes::from_static(b"cellscript-0.30-runtime-view"), witness(&result, expected_hash));
        let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &invalid);
        assert_eq!(execution.exit_code, 20, "invalid EpochDuration arithmetic must use numeric-or-discriminant-invalid");
    }
}

#[test]
fn fixed_transaction_header_temporal_view_resource_profile_is_exact_and_bounded() {
    let result = compile(SOURCE);
    let dep_data = Bytes::from_static(b"cellscript-0.30-runtime-view");
    let expected_hash = blake2b_256(&dep_data);
    let valid = fixture(dep_data, witness(&result, expected_hash));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &valid);
    assert_eq!(execution.exit_code, 0, "resource profile failed: {:?}", execution.captured_debug);
    let max_stack_frame_bytes =
        result.verified_lowering_record.as_ref().unwrap().entries.iter().map(|entry| entry.frame_size_bytes).max().unwrap();
    let actual = serde_json::json!({
        "cycles": execution.cycles,
        "elf_bytes": strip_vm_abi_trailer(&result.artifact_bytes).len(),
        "max_stack_frame_bytes": max_stack_frame_bytes,
        "witness_bytes": execution.witness_bytes,
        "transaction_bytes": execution.transaction_bytes,
        "dependency_bytes": execution.dependency_bytes,
    });
    let manifest: serde_json::Value = serde_json::from_str(include_str!("fixtures/runtime_view_resource_budgets.json")).unwrap();
    let profile = &manifest["profiles"][0];
    assert_eq!(actual, profile["measured"], "recorded runtime-view resource measurement is stale: {actual}");
    for field in ["cycles", "elf_bytes", "max_stack_frame_bytes", "witness_bytes", "transaction_bytes", "dependency_bytes"] {
        assert!(actual[field].as_u64().unwrap() <= profile["budgets"][field].as_u64().unwrap(), "{field} exceeded budget");
    }
}

#[test]
fn dynamic_source_indexes_execute_and_emit_checked_provenance() {
    let result = compile(DYNAMIC_INDEX_SOURCE);
    let dep_data = Bytes::from_static(b"cellscript-0.30-dynamic-index");
    let expected_hash = blake2b_256(&dep_data);

    let valid = fixture(dep_data.clone(), dynamic_index_witness(&result, 0, expected_hash));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &valid);
    assert_eq!(execution.exit_code, 0, "dynamic index zero must select the first Input, CellDep, and Witness");

    let dynamic_accesses = result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .filter(|access| access.provenance.index.kind == "dynamic")
        .collect::<Vec<_>>();
    assert!(!dynamic_accesses.is_empty(), "runtime metadata must preserve dynamic source-index provenance");
    assert!(dynamic_accesses.iter().all(|access| {
        access.provenance.contract == cellscript::CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT
            && access.provenance.index.binding.as_deref() == Some("source_index")
            && access.provenance.index.max_inclusive == Some(u64::from(u32::MAX))
            && access.index == 0
    }));
    assert!(dynamic_accesses.iter().any(|access| {
        access.operation == "cell-data-hash-field"
            && access.provenance.source.resolved_source == "CellDep"
            && access.provenance.source.origin == "inherited-source-view"
            && access.provenance.range.kind == "fixed-width"
            && access.provenance.range.length.value == Some(32)
    }));
    assert!(result.metadata.runtime.transaction_view_handles.iter().any(|handle| {
        handle.handle_type == "InputView<Token>"
            && handle.provenance.index.kind == "dynamic"
            && handle.provenance.index.binding.as_deref() == Some("source_index")
    }));

    let invalid = fixture(dep_data, dynamic_index_witness(&result, u64::from(u32::MAX) + 1, expected_hash));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &invalid);
    assert_eq!(execution.exit_code, 44, "a dynamic source index outside the packed 32-bit view domain must fail closed");

    let mut tampered = result.metadata.clone();
    let access = tampered
        .runtime
        .ckb_runtime_accesses
        .iter_mut()
        .find(|access| access.provenance.index.kind == "dynamic")
        .expect("dynamic runtime access");
    access.provenance.index.max_inclusive = Some(u64::from(u32::MAX) - 1);
    let error = cellscript::validate_compile_metadata(&tampered, result.artifact_format)
        .expect_err("a narrowed source-view index contract must not validate");
    assert!(error.message.contains("32-bit source-view index"), "unexpected validation error: {error}");
}

#[test]
fn output_group_output_and_maximum_script_views_execute_and_fail_closed() {
    let result = compile(OUTPUT_SCRIPT_SOURCE);
    let valid = output_script_fixture();
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &valid);
    assert_eq!(execution.exit_code, 0, "Output/GroupOutput and maximum Script fields must execute: {:?}", execution.captured_debug);
    for (handle_type, source) in [("OutputView<Token>", "Output"), ("OutputView<Token>", "GroupOutput")] {
        assert!(result.metadata.runtime.transaction_view_handles.iter().any(|handle| {
            handle.handle_type == handle_type
                && handle.source == source
                && handle.provenance.source.resolved_source == source
                && handle.provenance.index.kind == "static"
        }));
    }
    assert!(result.metadata.runtime.transaction_view_handles.iter().filter(|handle| handle.handle_type == "ScriptView").count() >= 4);

    for source in [
        OUTPUT_SCRIPT_SOURCE.replace("ckb::output<Token>(0)", "ckb::output<Token>(2)"),
        OUTPUT_SCRIPT_SOURCE.replace("ckb::group_output<Token>(0)", "ckb::group_output<Token>(1)"),
    ] {
        let missing = compile(&source);
        let execution = execute_cellscript_script(strip_vm_abi_trailer(&missing.artifact_bytes), &valid);
        assert_eq!(execution.exit_code, 44, "a one-past-last Output view must fail with ckb-source-view-invalid");
    }

    let invalid = compile_failure(&OUTPUT_SCRIPT_SOURCE.replace("ckb::output<Token>(0)", "ckb::input<Token>(0)"));
    assert!(invalid.message.contains("output_index"), "unexpected invalid-view diagnostic: {invalid}");
}

#[test]
fn output_group_output_and_maximum_script_view_resource_profile_is_exact_and_bounded() {
    let result = compile(OUTPUT_SCRIPT_SOURCE);
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &output_script_fixture());
    assert_eq!(execution.exit_code, 0, "resource profile failed: {:?}", execution.captured_debug);
    let max_stack_frame_bytes =
        result.verified_lowering_record.as_ref().unwrap().entries.iter().map(|entry| entry.frame_size_bytes).max().unwrap();
    let actual = serde_json::json!({
        "cycles": execution.cycles,
        "elf_bytes": strip_vm_abi_trailer(&result.artifact_bytes).len(),
        "max_stack_frame_bytes": max_stack_frame_bytes,
        "witness_bytes": execution.witness_bytes,
        "transaction_bytes": execution.transaction_bytes,
        "dependency_bytes": execution.dependency_bytes,
    });
    let identities = serde_json::json!({
        "artifact_hash": format!("0x{}", result.metadata.artifact_hash.as_deref().unwrap()),
        "lowering_record_hash": format!("0x{}", result.metadata.verified_artifact.lowering_record_hash.as_deref().unwrap()),
        "source_map_hash": format!("0x{}", result.metadata.verified_artifact.source_map_hash.as_deref().unwrap()),
        "verified_bundle_id": format!("0x{}", result.metadata.verified_artifact.verified_bundle_id.as_deref().unwrap()),
        "raw_transaction_hash": execution.raw_transaction_hash,
        "serialized_transaction_hash": execution.serialized_transaction_hash,
    });
    let manifest: serde_json::Value = serde_json::from_str(include_str!("fixtures/runtime_view_resource_budgets.json")).unwrap();
    let profile = manifest["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["id"] == "output-group-output-maximum-script-view")
        .expect("output/Script resource profile");
    assert_eq!(actual, profile["measured"], "recorded runtime-view resource measurement is stale: {actual}");
    assert_eq!(identities, profile["identities"], "recorded runtime-view identities are stale: {identities}");
    for field in ["cycles", "elf_bytes", "max_stack_frame_bytes", "witness_bytes", "transaction_bytes", "dependency_bytes"] {
        assert!(actual[field].as_u64().unwrap() <= profile["budgets"][field].as_u64().unwrap(), "{field} exceeded budget");
    }
}

#[test]
fn input_group_input_cell_dep_and_witness_view_resource_profile_is_exact_and_bounded() {
    let result = compile(INPUT_GROUP_DEP_WITNESS_SOURCE);
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &input_group_dep_witness_fixture());
    assert_eq!(execution.exit_code, 0, "resource profile failed: {:?}", execution.captured_debug);
    let max_stack_frame_bytes =
        result.verified_lowering_record.as_ref().unwrap().entries.iter().map(|entry| entry.frame_size_bytes).max().unwrap();
    let actual = serde_json::json!({
        "cycles": execution.cycles,
        "elf_bytes": strip_vm_abi_trailer(&result.artifact_bytes).len(),
        "max_stack_frame_bytes": max_stack_frame_bytes,
        "witness_bytes": execution.witness_bytes,
        "transaction_bytes": execution.transaction_bytes,
        "dependency_bytes": execution.dependency_bytes,
    });
    let identities = serde_json::json!({
        "artifact_hash": format!("0x{}", result.metadata.artifact_hash.as_deref().unwrap()),
        "lowering_record_hash": format!("0x{}", result.metadata.verified_artifact.lowering_record_hash.as_deref().unwrap()),
        "source_map_hash": format!("0x{}", result.metadata.verified_artifact.source_map_hash.as_deref().unwrap()),
        "verified_bundle_id": format!("0x{}", result.metadata.verified_artifact.verified_bundle_id.as_deref().unwrap()),
        "raw_transaction_hash": execution.raw_transaction_hash,
        "serialized_transaction_hash": execution.serialized_transaction_hash,
    });
    let manifest: serde_json::Value = serde_json::from_str(include_str!("fixtures/runtime_view_resource_budgets.json")).unwrap();
    let profile = manifest["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["id"] == "input-group-input-cell-dep-witness-view")
        .expect("Input/GroupInput/CellDep/WitnessArgs resource profile");
    assert_eq!(actual, profile["measured"], "recorded runtime-view resource measurement is stale: {actual}");
    assert_eq!(identities, profile["identities"], "recorded runtime-view identities are stale: {identities}");
    for field in ["cycles", "elf_bytes", "max_stack_frame_bytes", "witness_bytes", "transaction_bytes", "dependency_bytes"] {
        assert!(actual[field].as_u64().unwrap() <= profile["budgets"][field].as_u64().unwrap(), "{field} exceeded budget");
    }
}

#[test]
fn persistent_policy_and_generated_builder_cover_complete_runtime_view_profile() {
    let result = compile_persistent_runtime_view_policy();
    let metadata = result.metadata.clone();
    let execution = execute_cellscript_script_with_transaction_transform(
        strip_vm_abi_trailer(&result.artifact_bytes),
        &persistent_runtime_view_fixture(),
        move |transaction, type_script| {
            let selected = encode_policy_action_record(
                &metadata,
                &type_script.calc_script_hash().unpack(),
                "inspect",
                &[EntryWitnessArg::U64(73), EntryWitnessArg::Address(deterministic_always_success_lock_hash())],
            )
            .expect("generated persistent-policy action record");
            let bundle = encode_policy_witness_bundle(&[PolicyWitnessRecord {
                role: PolicyScriptRole::Type,
                script_hash: selected.script_hash,
                tag: selected.tag,
                args: selected.args,
            }])
            .expect("persistent runtime-view policy witness bundle");
            let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(bundle)).pack()).build();
            let mut witnesses = transaction.witnesses().into_iter().collect::<Vec<_>>();
            witnesses[0] = witness.as_bytes().pack();
            transaction.as_advanced_builder().set_witnesses(witnesses).build()
        },
    );
    assert_eq!(execution.exit_code, 0, "persistent runtime-view profile failed: {:?}", execution.captured_debug);
    let max_stack_frame_bytes =
        result.verified_lowering_record.as_ref().unwrap().entries.iter().map(|entry| entry.frame_size_bytes).max().unwrap();
    let actual = serde_json::json!({
        "cycles": execution.cycles,
        "elf_bytes": strip_vm_abi_trailer(&result.artifact_bytes).len(),
        "max_stack_frame_bytes": max_stack_frame_bytes,
        "witness_bytes": execution.witness_bytes,
        "transaction_bytes": execution.transaction_bytes,
        "dependency_bytes": execution.dependency_bytes,
    });
    let identities = serde_json::json!({
        "artifact_hash": format!("0x{}", result.metadata.artifact_hash.as_deref().unwrap()),
        "lowering_record_hash": format!("0x{}", result.metadata.verified_artifact.lowering_record_hash.as_deref().unwrap()),
        "source_map_hash": format!("0x{}", result.metadata.verified_artifact.source_map_hash.as_deref().unwrap()),
        "verified_bundle_id": format!("0x{}", result.metadata.verified_artifact.verified_bundle_id.as_deref().unwrap()),
        "raw_transaction_hash": execution.raw_transaction_hash,
        "serialized_transaction_hash": execution.serialized_transaction_hash,
    });
    let manifest: serde_json::Value = serde_json::from_str(include_str!("fixtures/runtime_view_resource_budgets.json")).unwrap();
    let profile = manifest["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["id"] == "persistent-policy-complete-runtime-view")
        .expect("persistent-policy complete runtime-view resource profile");
    assert_eq!(actual, profile["measured"], "recorded persistent runtime-view measurement is stale: {actual}");
    assert_eq!(identities, profile["identities"], "recorded persistent runtime-view identities are stale: {identities}");
    for field in ["cycles", "elf_bytes", "max_stack_frame_bytes", "witness_bytes", "transaction_bytes", "dependency_bytes"] {
        assert!(actual[field].as_u64().unwrap() <= profile["budgets"][field].as_u64().unwrap(), "{field} exceeded budget");
    }
}

fn byte_string_literal(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("\\x{byte:02x}")).collect()
}

fn bounded_witness_fixture(witness: Bytes) -> ckb_script_runner::CkbVmFixture {
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![witness];
    fixture
}

fn bounded_read_source(raw: &[u8], lock: &[u8], entry: &[u8], output_type: &[u8]) -> String {
    let entry_u32 = u32::from_le_bytes(entry[501..505].try_into().expect("entry u32 bytes"));
    let output_u64 = u64::from_le_bytes(output_type[777..785].try_into().expect("output u64 bytes"));
    format!(
        r#"module runtime_views::bounded_witness

action inspect() -> u64 {{
    verification
        let witness_args = witness::args(0)
        let raw = witness::bounded_raw(witness_args, 4096)
        let lock = witness::bounded_lock(witness_args, 700)
        let entry = witness::bounded_entry(witness_args, 900)
        let output_type = witness::bounded_output_type(witness_args, 1024)
        require raw.size == {raw_size}
        require lock.size == 700
        require entry.size == 900
        require output_type.size == 1024
        require witness::byte(lock, 0) == {lock_first}
        require witness::byte(lock, 699) == {lock_last}
        require witness::u32_le(entry, 501) == {entry_u32}
        require witness::u64_le(output_type, 777) == {output_u64}
        require witness::blake2b(raw) == Hash::from_bytes(b"{raw_hash}")
        require witness::blake2b(lock) == Hash::from_bytes(b"{lock_hash}")
        require witness::blake2b(entry) == Hash::from_bytes(b"{entry_hash}")
        require witness::blake2b(output_type) == Hash::from_bytes(b"{output_hash}")
        return 0
}}
"#,
        raw_size = raw.len(),
        lock_first = lock[0],
        lock_last = lock[699],
        raw_hash = byte_string_literal(&blake2b_256(raw)),
        lock_hash = byte_string_literal(&blake2b_256(lock)),
        entry_hash = byte_string_literal(&blake2b_256(entry)),
        output_hash = byte_string_literal(&blake2b_256(output_type)),
    )
}

fn bounded_probe_source(constructor: &str, maximum: u64, expression: &str) -> String {
    format!(
        r#"module runtime_views::bounded_witness_probe

action inspect() -> u64 {{
    verification
        let witness_args = witness::args(0)
        let bytes = witness::{constructor}(witness_args, {maximum})
        let observed = {expression}
        return 0
}}
"#
    )
}

fn compile_failure(source: &str) -> cellscript::error::CompileError {
    match compile_with_executable_surface_policy(
        source,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    ) {
        Ok(_) => panic!("source unexpectedly compiled:\n{source}"),
        Err(error) => error,
    }
}

#[test]
fn bounded_witness_owners_stream_large_fields_and_preserve_provenance() {
    let lock = (0..700).map(|index| ((index * 3 + 1) & 0xff) as u8).collect::<Vec<_>>();
    let entry = (0..900).map(|index| ((index * 5 + 2) & 0xff) as u8).collect::<Vec<_>>();
    let output_type = (0..1024).map(|index| ((index * 7 + 3) & 0xff) as u8).collect::<Vec<_>>();
    let witness = packed::WitnessArgs::new_builder()
        .lock(Some(Bytes::copy_from_slice(&lock)).pack())
        .input_type(Some(Bytes::copy_from_slice(&entry)).pack())
        .output_type(Some(Bytes::copy_from_slice(&output_type)).pack())
        .build()
        .as_bytes();
    assert!(witness.len() > 512, "fixture must exercise the streaming path beyond the legacy fixed buffer");

    let result = compile(&bounded_read_source(&witness, &lock, &entry, &output_type));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &bounded_witness_fixture(witness.clone()));
    assert_eq!(execution.exit_code, 0, "all bounded witness owners and hashes must execute: {:?}", execution.captured_debug);
    assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-bounded-witness-view".to_string()));
    assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-blake2b".to_string()));

    for (owner, maximum) in [("raw", 4096), ("lock", 700), ("entry", 900), ("output_type", 1024)] {
        assert!(result.metadata.runtime.transaction_view_handles.iter().any(|handle| {
            handle.handle_type == format!("WitnessBytesView<{owner},{maximum}>")
                && handle.witness_owner.as_deref() == Some(owner)
                && handle.max_bytes == Some(maximum)
                && handle.provenance.source.resolved_source == "Input"
                && handle.provenance.range.kind == "bounded-range"
                && handle.provenance.range.length.max_inclusive == Some(maximum)
        }));
        assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
            access.operation == format!("witness-bounded-{owner}-blake2b")
                && access.provenance.source.resolved_source == "Input"
                && access.provenance.range.kind == "bounded-range"
                && access.provenance.range.length.max_inclusive == Some(maximum)
        }));
    }

    let mut tampered = result.metadata.clone();
    for access in tampered
        .runtime
        .ckb_runtime_accesses
        .iter_mut()
        .chain(tampered.actions.iter_mut().flat_map(|action| action.ckb_runtime_accesses.iter_mut()))
        .filter(|access| access.operation == "witness-bounded-lock-blake2b")
    {
        access.operation = "witness-bounded-lock-unknown".to_string();
    }
    let error = cellscript::validate_compile_metadata(&tampered, result.artifact_format)
        .expect_err("a non-canonical bounded witness runtime operation must not validate");
    assert!(error.message.contains("not canonical"), "unexpected bounded witness metadata error: {error}");

    let mut tampered = result.metadata.clone();
    let handle = tampered
        .runtime
        .transaction_view_handles
        .iter_mut()
        .find(|handle| handle.handle_type == "WitnessBytesView<lock,700>")
        .expect("bounded lock handle");
    handle.provenance.range.length.max_inclusive = Some(699);
    let error = cellscript::validate_compile_metadata(&tampered, result.artifact_format)
        .expect_err("a bounded witness handle range must remain tied to its declared maximum");
    assert!(error.message.contains("bounded witness range"), "unexpected bounded witness handle error: {error}");

    let group_output_source = format!(
        r#"module runtime_views::bounded_group_output

action inspect() -> u64 {{
    verification
        let output_type = witness::bounded_output_type(source::group_output(0), 1024)
        require output_type.size == 1024
        require witness::byte(output_type, 1023) == {last_byte}
        return 0
}}
"#,
        last_byte = output_type[1023],
    );
    let group_output = compile(&group_output_source);
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&group_output.artifact_bytes), &bounded_witness_fixture(witness));
    assert_eq!(execution.exit_code, 0, "GroupOutput witness provenance must remain executable");
    assert!(group_output.metadata.runtime.transaction_view_handles.iter().any(|handle| {
        handle.handle_type == "WitnessBytesView<output_type,1024>"
            && handle.source == "GroupOutput"
            && handle.provenance.source.resolved_source == "GroupOutput"
    }));
}

#[test]
fn bounded_witness_empty_absent_bound_and_range_semantics_fail_closed() {
    let empty = packed::WitnessArgs::new_builder()
        .lock(Some(Bytes::default()).pack())
        .input_type(Some(Bytes::default()).pack())
        .output_type(Some(Bytes::default()).pack())
        .build()
        .as_bytes();
    let empty_hash = byte_string_literal(&blake2b_256([]));
    let empty_source = format!(
        r#"module runtime_views::bounded_witness_empty

action inspect() -> u64 {{
    verification
        let witness_args = witness::args(0)
        let lock = witness::bounded_lock(witness_args, 0)
        let entry = witness::bounded_entry(witness_args, 0)
        let output_type = witness::bounded_output_type(witness_args, 0)
        require lock.size == 0
        require entry.size == 0
        require output_type.size == 0
        require witness::blake2b(lock) == Hash::from_bytes(b"{empty_hash}")
        require witness::blake2b(entry) == Hash::from_bytes(b"{empty_hash}")
        require witness::blake2b(output_type) == Hash::from_bytes(b"{empty_hash}")
        return 0
}}
"#
    );
    let result = compile(&empty_source);
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &bounded_witness_fixture(empty));
    assert_eq!(execution.exit_code, 0, "Some(empty) must remain distinct from an absent WitnessArgs field");

    let absent = packed::WitnessArgs::new_builder().build().as_bytes();
    for constructor in ["bounded_lock", "bounded_entry", "bounded_output_type"] {
        let result = compile(&bounded_probe_source(constructor, 16, "bytes.size"));
        let execution =
            execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &bounded_witness_fixture(absent.clone()));
        assert_eq!(
            execution.exit_code,
            cellscript::runtime_errors::CellScriptRuntimeError::WitnessFieldAbsent.code() as i64,
            "{constructor} must reject an absent field"
        );
    }

    let long_lock = packed::WitnessArgs::new_builder().lock(Some(Bytes::from(vec![7u8; 65])).pack()).build().as_bytes();
    let result = compile(&bounded_probe_source("bounded_lock", 64, "bytes.size"));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &bounded_witness_fixture(long_lock));
    assert_eq!(
        execution.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::WitnessBoundExceeded.code() as i64,
        "a field one byte above its declared bound must reject"
    );

    let short_lock = packed::WitnessArgs::new_builder().lock(Some(Bytes::from(vec![1u8; 7])).pack()).build().as_bytes();
    let result = compile(&bounded_probe_source("bounded_lock", 7, "witness::u64_le(bytes, 0)"));
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &bounded_witness_fixture(short_lock));
    assert_eq!(
        execution.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::BoundsCheckFailed.code() as i64,
        "an exact read beyond the logical field view must reject"
    );
}

#[test]
fn bounded_witness_rejects_malformed_tables_and_invalid_static_bounds() {
    let result = compile(&bounded_probe_source("bounded_lock", 32, "bytes.size"));
    let malformed_total = Bytes::from(vec![17, 0, 0, 0, 16, 0, 0, 0, 16, 0, 0, 0, 16, 0, 0, 0]);
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &bounded_witness_fixture(malformed_total));
    assert_eq!(
        execution.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::WitnessMalformed.code() as i64,
        "a mismatched WitnessArgs total_size must reject"
    );

    let truncated_offset = Bytes::from(vec![16, 0, 0, 0, 16, 0, 0, 0, 16, 0, 0, 0, 17, 0, 0, 0]);
    let execution =
        execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &bounded_witness_fixture(truncated_offset));
    assert_eq!(
        execution.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::WitnessFieldTruncated.code() as i64,
        "a WitnessArgs field offset beyond total_size must reject"
    );

    for source in [
        bounded_probe_source("bounded_raw", 65537, "bytes.size"),
        r#"module runtime_views::bounded_dynamic_limit

action inspect(witness maximum: u64) -> u64 {
    verification
        let witness_args = witness::args(0)
        let bytes = witness::bounded_raw(witness_args, maximum)
        return bytes.size
}
"#
        .to_string(),
    ] {
        let error = compile_failure(&source);
        assert!(error.message.contains("maximum_bytes"), "unexpected bounded-witness diagnostic: {error}");
    }

    let error = compile_failure(
        r#"module runtime_views::unbounded_witness_hash

action inspect() -> u64 {
    verification
        let witness_args = witness::args(0)
        let digest = witness::blake2b(witness_args)
        return 0
}
"#,
    );
    assert!(error.message.contains("bounded witness byte view"), "unexpected unbounded hash diagnostic: {error}");
}
