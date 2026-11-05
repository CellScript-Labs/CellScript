//! Structural vector elements must survive checking, lowering, typed records
//! and actual execution; stack storage remains distinct from arena handles.
use cellscript::{CellScriptEdition, CompileOptions, CompilerDiagnostics, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn execute(source: &str, arguments: &[EntryWitnessArg]) {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let result = cellscript::compile_with_executable_surface_policy(
                source,
                CompileOptions {
                    edition,
                    diagnostics,
                    source_contracts: true,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("{edition:?}/{diagnostics:?}: {error}\n{source}"));
            result.validate().unwrap();
            for param in
                result.metadata.functions.iter().flat_map(|function| &function.params).filter(|param| param.ty.starts_with("Vec<"))
            {
                assert!(param.schema_pointer_abi && param.schema_length_abi, "{param:?}");
                assert!(!param.cell_bound_abi, "{param:?}");
            }
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            if !arguments.is_empty() {
                let params = &result.metadata.actions.iter().find(|action| action.name == "verify").unwrap().params;
                let payload = cellscript::encode_entry_witness_args_for_params(params, arguments).unwrap();
                fixture.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            }
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, 0, "{edition:?}/{diagnostics:?}: {execution:?}\n{source}");
        }
    }
}

#[test]
fn schema_vector_fields_preserve_length_and_elements_across_receipt_replacement() {
    let source = r#"
module vector_receipts
receipt Approval has create, consume, replace {
    approvers: Vec<Address>,
    state: u8,
}
action verify(before: Approval, witness added: Address) -> after: Approval {
    verification
    for existing in &before.approvers {
        require *existing != added
    }
    preserve after from before { state }
    require after.approvers.len() == before.approvers.len() + 1
    for i in 0..before.approvers.len() {
        require after.approvers[i] == before.approvers[i]
    }
    require after.approvers[before.approvers.len()] == added
}
"#;
    fn cell(owners: &[[u8; 32]]) -> Vec<u8> {
        let mut vector = (owners.len() as u32).to_le_bytes().to_vec();
        for owner in owners {
            vector.extend(owner);
        }
        let size = 12 + vector.len() + 1;
        let mut table = (size as u32).to_le_bytes().to_vec();
        table.extend(12u32.to_le_bytes());
        table.extend(((12 + vector.len()) as u32).to_le_bytes());
        table.extend(vector);
        table.push(0);
        assert_eq!(table.len(), size);
        table
    }
    let first = [7; 32];
    let added = [9; 32];
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let result = cellscript::compile(
                source,
                CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap();
            let params = &result.metadata.actions.iter().find(|action| action.name == "verify").unwrap().params;
            let payload = cellscript::encode_entry_witness_args_for_params(params, &[EntryWitnessArg::Address(added)]).unwrap();
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = cell(&[first]).into();
            fixture.outputs[0].data = cell(&[first, added]).into();
            fixture.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, 0, "{edition:?}/{diagnostics:?}: {execution:?}");
            result.validate().unwrap();

            for owners in [vec![], vec![first, [8; 32]]] {
                let mut valid = fixture.clone();
                valid.inputs[0].data = cell(&owners).into();
                let mut next = owners;
                next.push(added);
                valid.outputs[0].data = cell(&next).into();
                let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &valid);
                assert_eq!(execution.exit_code, 0, "{edition:?}/{diagnostics:?}: {execution:?}");
            }

            let mut changed = fixture.clone();
            changed.outputs[0].data = cell(&[[8; 32], added]).into();
            assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &changed).exit_code, 5);
            changed.outputs[0].data = cell(&[first, [8; 32]]).into();
            assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &changed).exit_code, 5);
            let mut invalid_count = cell(&[first]);
            invalid_count[12..16].copy_from_slice(&2u32.to_le_bytes());
            changed.inputs[0].data = invalid_count.into();
            changed.outputs[0].data = fixture.outputs[0].data.clone();
            assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &changed).exit_code, 2);
            let duplicate = cellscript::encode_entry_witness_args_for_params(params, &[EntryWitnessArg::Address(first)]).unwrap();
            changed = fixture.clone();
            changed.outputs[0].data = cell(&[first, first]).into();
            changed.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(duplicate)).pack()).build().as_bytes()];
            assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &changed).exit_code, 5);
        }
    }
}

#[test]
fn schema_vector_fixed_record_and_scalar_elements_keep_their_wire_widths() {
    let source = r#"
module vector_field_widths
receipt Votes has consume, burn {
    approvals: Vec<Approval>,
    flags: Vec<u16>,
}
struct Approval { approver: Address, nonce: u16 }
action verify(votes: Votes, witness expected: Address) -> u64 {
    verification
    require votes.approvals.len() == 2
    let approval = votes.approvals[1]
    require approval.approver == expected
    require approval.nonce == 513
    require votes.flags.len() == 2
    require votes.flags[0] == 7
    require votes.flags[1] == 65535
    destroy votes
    return 0
}
"#;
    let expected = [9; 32];
    let mut records = 2u32.to_le_bytes().to_vec();
    for (owner, nonce) in [([7; 32], 1u16), (expected, 513u16)] {
        records.extend(owner);
        records.extend(nonce.to_le_bytes());
    }
    let mut flags = 2u32.to_le_bytes().to_vec();
    flags.extend(7u16.to_le_bytes());
    flags.extend(u16::MAX.to_le_bytes());
    let total = 12 + records.len() + flags.len();
    let mut table = (total as u32).to_le_bytes().to_vec();
    table.extend(12u32.to_le_bytes());
    table.extend(((12 + records.len()) as u32).to_le_bytes());
    table.extend(records);
    table.extend(flags);
    assert_eq!(table.len(), total);
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let result = cellscript::compile(
                source,
                CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap();
            let params = &result.metadata.actions[0].params;
            let payload = cellscript::encode_entry_witness_args_for_params(params, &[EntryWitnessArg::Address(expected)]).unwrap();
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 0);
            fixture.inputs[0].data = table.clone().into();
            fixture.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            result.validate().unwrap();
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, 0, "{edition:?}/{diagnostics:?}: {execution:?}");
        }
    }
}

#[test]
fn schema_vector_timelock_approval_executes_the_recorded_cell_bytes() {
    let root = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
    let recipe: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("crates/cellscript-tools/fixtures/ckb_acceptance/transactions-v0.23.json")).unwrap(),
    )
    .unwrap();
    let case = recipe["action_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "timelock.cell:approve_emergency_release")
        .unwrap();
    let tx = &recipe["transactions"][case["valid_tx"].as_str().unwrap()];
    let previous = &tx["inputs"][0]["previous_output"];
    let index = usize::from_str_radix(previous["index"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
    let input = &recipe["transactions"][previous["tx_hash"].as_str().unwrap()]["outputs_data"][index];
    let decode = |value: &serde_json::Value| hex::decode(value.as_str().unwrap().trim_start_matches("0x")).unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.inputs[0].data = decode(input).into();
    fixture.outputs[0].data = decode(&tx["outputs_data"][0]).into();
    fixture.witnesses = tx["witnesses"].as_array().unwrap().iter().map(|value| Bytes::from(decode(value))).collect();
    assert_eq!(fixture.inputs[0].data.len(), 158);
    assert_eq!(fixture.outputs[0].data.len(), 190);
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let result = cellscript::compile_file_with_entry_action(
                root.join("examples/timelock.cell"),
                CompileOptions {
                    edition,
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    primitive_compat: Some("0.16".into()),
                    ..Default::default()
                },
                "approve_emergency_release",
            )
            .unwrap();
            assert_eq!(result.metadata.actions.len(), 1);
            result.validate().unwrap();
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, 0, "{edition:?}/{diagnostics:?}: {execution:?}");
        }
    }
}

#[test]
fn structured_vec_scalar_inference_and_annotations_execute_all_stack_operations() {
    execute(
        r#"
module vector_scalars
action verify() -> u64 {
    verification
    let mut values = Vec::with_capacity(8)
    require values.is_empty()
    values.push(7)
    values.push(9)
    values.insert(1, 8)
    values.set(0, 6)
    require values.first() == 6
    require values.last() == 9
    require values.contains(8)
    values.swap(0, 2)
    require values[0] == 9
    values.reverse()
    require values[0] == 6
    require values.remove(1) == 8
    require values.pop() == 9
    values.extend_from_slice([10, 11])
    require values.len() == 3
    values.truncate(2)
    require values[1] == 10
    values.clear()
    require values.is_empty()
    let declared: Vec<u64> = [7, 9]
    require declared[0] + declared[1] == 16
    return 0
}
"#,
        &[],
    );
}

#[test]
fn structured_vec_narrow_literals_reach_metadata_checker_and_vm() {
    for (ty, maximum) in [("u8", 255_u64), ("u16", 65_535), ("u32", 4_294_967_295)] {
        execute(
            &format!(
                "module narrow_push\naction verify() -> u64 {{ verification\n\
                 let mut values: Vec<{ty}> = []\nvalues.push({maximum})\n\
                 require values.first() == {maximum}\nreturn 0\n}}"
            ),
            &[],
        );
        let source = format!(
            r#"
module narrow_vector
action verify() -> u64 {{
    verification
    let mut values: Vec<{ty}> = []
    values.push(5)
    values.insert(0, {maximum})
    values.set(1, {maximum})
    require values.contains({maximum})
    require values[0] == {maximum}
    require values[1] == {maximum}
    require values.pop() == {maximum}
    require values.remove(0) == {maximum}
    require values.is_empty()
    return 0
}}
"#
        );
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            cellscript::compile_with_executable_surface_policy(
                &source,
                CompileOptions { edition, source_contracts: true, ..Default::default() },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("{edition:?}: {error}\n{source}"))
            .validate()
            .unwrap();
        }
        execute(&source, &[]);
    }
}

#[test]
fn structured_vec_mutations_preserve_typed_aggregate_elements() {
    execute(
        r#"
module narrow_vector_aggregates
action verify() -> u64 {
    verification
    let mut pairs: Vec<(u8, u16)> = []
    pairs.push((255 as u8, 65535 as u16))
    pairs.insert(0, (1 as u8, 2 as u16))
    pairs.set(0, (254 as u8, 65534 as u16))
    let changed = pairs[0]
    require changed.0 == 254
    require changed.1 == 65534
    let pair = pairs.pop()
    require pair.0 == 255
    require pair.1 == 65535
    let mut arrays: Vec<[u8; 2]> = []
    arrays.push([255 as u8, 1 as u8])
    arrays.insert(0, [2 as u8, 254 as u8])
    arrays.set(0, [253 as u8, 3 as u8])
    let changed_array = arrays[0]
    require changed_array[0] == 253
    require changed_array[1] == 3
    let array = arrays.pop()
    require array[0] == 255
    require array[1] == 1
    return 0
}
"#,
        &[],
    );
}

#[test]
fn structured_vec_insert_shifts_packed_fixed_bytes_without_native_store_widths() {
    for width in [3, 5, 6, 7, 9] {
        let original = "a".repeat(width);
        let inserted = "b".repeat(width);
        execute(
            &format!(
                r#"
module packed_vector_shift
action verify() -> u64 {{
    verification
    let mut values: Vec<[u8; {width}]> = [b"{original}"]
    values.insert(0, b"{inserted}")
    let first = values[0]
    let shifted = values[1]
    require first[0] == 98
    require first[{last}] == 98
    require shifted[0] == 97
    require shifted[{last}] == 97
    values.insert(0, values[1])
    let copied = values[0]
    require copied[0] == 97
    require copied[{last}] == 97
    let preserved = values[2]
    require preserved[{last}] == 97
    return 0
}}
"#,
                last = width - 1
            ),
            &[],
        );
    }
}

#[test]
fn structured_vec_narrow_literals_reject_overflow_before_artifact_generation() {
    for (ty, overflow) in [("u8", 256_u64), ("u16", 65_536), ("u32", 4_294_967_296)] {
        for expression in [
            format!("values.push({overflow})"),
            format!("values.insert(0, {overflow})"),
            format!("values.set(0, {overflow})"),
            format!("values.contains({overflow})"),
        ] {
            let source = format!(
                "module invalid_narrow_vector\naction verify() -> u64 {{ verification\n\
                 let mut values: Vec<{ty}> = [0]\n{expression}\nreturn 0\n}}"
            );
            for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
                for target in [None, Some("riscv64-elf".to_owned())] {
                    let error = cellscript::compile_with_executable_surface_policy(
                        &source,
                        CompileOptions { edition, target, ..Default::default() },
                        ExecutableSurfacePolicy::DenyFailClosed,
                    )
                    .expect_err(&source);
                    assert_eq!(error.message, format!("integer literal {overflow} does not fit expected type {ty}"));
                }
            }
        }
    }
}

#[test]
fn structured_vec_fixed_bytes_and_records_preserve_element_layouts() {
    execute(
        r#"
module vector_records
struct Snapshot { owner: Address, amount: u64 }
action verify(witness owner: Address) -> u64 {
    verification
    let hash = ckb::current_script_hash()
    let mut hashes: Vec<Hash> = []
    hashes.push(hash)
    require hashes[0] == hash
    let mut owners: Vec<Address> = [owner]
    require owners.pop() == owner
    let mut snapshots: Vec<Snapshot> = []
    snapshots.push(Snapshot { owner: owner, amount: 42 })
    let snapshot = snapshots.pop()
    require snapshot.owner == owner
    require snapshot.amount == 42
    let mut arrays: Vec<[u8; 2]> = [[1, 2], [3, 4]]
    let pair = arrays[1]
    require pair[0] == 3
    require pair[1] == 4
    let removed = arrays.remove(0)
    require removed[0] == 1
    let popped = arrays.pop()
    require popped[1] == 4
    let mut pairs: Vec<(u64, u64)> = [(7, 9), (11, 13)]
    let last = pairs.pop()
    require last.0 == 11
    require last.1 == 13
    return 0
}
"#,
        &[EntryWitnessArg::Address([7; 32])],
    );
}

#[test]
fn declared_record_array_extensions_copy_retained_items_at_packed_and_maximum_widths() {
    for width in [1, 2, 3, 7, 8, 9, 12, 64] {
        let first = "\\x07".repeat(width);
        let second = "\\x09".repeat(width);
        execute(
            &format!(
                r#"
module record_array_extension
struct Record {{ bytes: [u8; {width}] }}
action verify() -> u64 {{ verification
    let first = Record {{ bytes: b"{first}" }}
    let second = Record {{ bytes: b"{second}" }}
    let extra: [Record; 2] = [first, second]
    let alias = extra
    let empty: [Record; 0] = []
    let mut values: Vec<Record> = []
    values.extend_from_slice(empty)
    values.extend_from_slice(extra)
    values.extend_from_slice(alias)
    require values.len() == 4
    require values[0].bytes[{last}] == 7
    require values[1].bytes[0] == 9
    require values[2].bytes[0] == 7
    require values[3].bytes[{last}] == 9
    return 0
}}
"#,
                last = width - 1
            ),
            &[],
        );
    }
}

#[test]
fn structured_vec_function_parameters_keep_span_transport() {
    execute(
        r#"
module vector_parameters
fn sum(values: Vec<u64>) -> u64 { return values[0] + values[1] }
fn forwarded(values: Vec<u64>) -> u64 { return sum(values) }
fn duplicated(left: Vec<u64>, right: Vec<u64>) -> u64 { return left[0] + right[1] }
fn spilled(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64,
           h: u64, values: Vec<u64>, tail: u64) -> u64 {
    return a + b + c + d + e + f + g + h + sum(values) + tail
}
fn empty(values: Vec<u64>) -> bool { return values.is_empty() }
fn five(a: Vec<u64>, b: Vec<u64>, c: Vec<u64>, d: Vec<u64>, e: Vec<u64>) -> u64 {
    return a[0] + b[0] + c[0] + d[0] + e[0]
}
action verify() -> u64 {
    verification
    let mut values: Vec<u64> = [7, 9]
    require sum(values) == 16
    require forwarded(values) == 16
    require duplicated(values, values) == 16
    require spilled(1, 2, 3, 4, 5, 6, 7, 8, values, 10) == 62
    require values.len() == 2
    values.push(11)
    require values[2] == 11
    let no_values: Vec<u64> = []
    require empty(no_values)
    let one: Vec<u64> = [1]
    let two: Vec<u64> = [2]
    let three: Vec<u64> = [3]
    let four: Vec<u64> = [4]
    let five_values: Vec<u64> = [5]
    require five(one, two, three, four, five_values) == 15
    return 0
}
"#,
        &[],
    );
}

#[test]
fn structured_vec_gather_helpers_use_structural_byte_and_offset_elements() {
    execute(
        r#"
module vector_gather
action verify(witness expected: Hash) -> u64 {
    verification
    let offsets: Vec<u64> = []
    let lengths: Vec<u64> = []
    let prefix: Vec<u8> = [97, 98]
    let suffix: Vec<u8> = [99, 100]
    require ckb::transaction_blake2b_gather(offsets, lengths, prefix, suffix) == expected
    let selection: Vec<u8> = []
    let view = witness::args(0)
    require witness::blake2b_select_chunks(view, 0, 1, selection, prefix, suffix) == expected
    let mut inferred = Vec::new()
    inferred.extend_from_slice([7, 9])
    require inferred.len() == 2
    require inferred[1] == 9
    return 0
}
"#,
        &[EntryWitnessArg::Hash(ckb_testtool::ckb_hash::blake2b_256(b"abcd"))],
    );
}

#[test]
fn structured_vec_rejects_wrong_argument_roles_and_unowned_elements() {
    for (declaration, ty) in [
        ("", "Vec<16>"),
        ("", "Vec<u64, 16>"),
        ("", "Vec<&u64>"),
        ("resource Token has store { amount: u64 }", "Vec<Token>"),
        ("", "Vec<Unknown>"),
        ("struct Vec { amount: u64 }", "Vec<u64>"),
    ] {
        let source = format!("module invalid_vector\n{declaration}\naction verify(value: {ty}) -> u64 {{ verification return 0 }}");
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            for target in [None, Some("riscv64-elf".to_owned())] {
                let result = cellscript::compile_with_executable_surface_policy(
                    &source,
                    CompileOptions { edition, target, ..Default::default() },
                    ExecutableSurfacePolicy::DenyFailClosed,
                );
                assert!(result.is_err(), "accepted {source}");
            }
        }
    }
}

#[test]
fn structured_vec_ckb_domains_keep_their_intrinsic_widths() {
    execute(
        r#"
module vector_domains
fn first_block(values: Vec<BlockNumber>) -> BlockNumber { return values[0] }
action verify(witness hash: ScriptHash, witness block: BlockNumber) -> u64 {
    verification
    let mut hashes: Vec<ScriptHash> = []
    hashes.push(hash)
    require hashes[0] == hash
    let blocks: Vec<BlockNumber> = [block]
    require first_block(blocks) == block
    return 0
}
"#,
        &[EntryWitnessArg::Bytes(vec![7; 32]), EntryWitnessArg::U64(42)],
    );
}
