//! Experimental bounded witness views and the legacy opaque Vec boundary.
//! VM behavior does not replace stable source/codec/machine proof admission.
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[path = "support/cost_measurement.rs"]
#[allow(dead_code)]
mod cost_measurement;

use ckb_script_runner::{build_simple_fixture, execute_cellscript_script};

#[test]
fn legacy_nested_vec_length_cannot_treat_encoded_byte_size_as_cardinality() {
    // Canonical one-element DynVec<Bytes>: outer size 12, first offset 8,
    // empty Bytes count 0. Its cardinality is one, never twelve.
    let exit = source_exit("Vec<Vec<u8>>", "items.len() == 12", "0c0000000800000000000000");
    assert_eq!(exit, 24, "unadmitted nested collection operation must fail closed");
}

#[test]
fn legacy_nested_empty_and_dynamic_record_lengths_also_fail_closed() {
    assert_eq!(source_exit("Vec<Vec<u8>>", "!items.is_empty()", "04000000"), 24);
    // One record with id=0x55 and an empty payload. Total 25 bytes,
    // record table 17 bytes, one element.
    assert_eq!(source_exit("Vec<Record>", "items.len() == 25", "1900000008000000110000000c0000000d0000005500000000"), 24);
}

#[test]
fn fixed_element_witness_vectors_keep_cardinality_and_empty_semantics() {
    assert_eq!(source_exit("Vec<u8>", "items.len() == 2", "02000000aabb"), 0);
    assert_eq!(source_exit("Vec<u8>", "items.is_empty()", "00000000"), 0);
    assert_eq!(source_exit("Vec<u64>", "items.len() == 1", "010000000100000000000000"), 0);
    assert_eq!(source_exit("Vec<u64>", "items.len() == 1", "020000000100000000000000"), 2);
}

fn source_exit(ty: &str, predicate: &str, wire: &str) -> i64 {
    let source = format!(
        r#"
module nested_length

struct Record {{
    id: u8,
    payload: Vec<u8>,
}}

action inspect(witness items: {ty}) -> u64 {{
    verification
        require {predicate}
        return 0
}}
"#
    );
    let encoded = hex::decode(wire).unwrap();
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
    payload.extend_from_slice(&encoded);
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
    let mut transaction = build_simple_fixture(Bytes::default(), 1, 1);
    transaction.witnesses = vec![witness.as_bytes()];
    let mut observed_exit = None;
    for opt_level in 0..=3 {
        let compiled = cellscript::compile(
            &source,
            cellscript::CompileOptions { opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let action = compiled.metadata.actions.iter().find(|action| action.name == "inspect").unwrap();
        assert_eq!(
            action.fail_closed_runtime_features.iter().any(|feature| feature == "dynamic-length"),
            matches!(ty, "Vec<Vec<u8>>" | "Vec<Record>"),
            "metadata must agree with the admitted codec at O{opt_level}"
        );
        let elf = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
        let result = execute_cellscript_script(elf, &transaction);
        if let Some(expected) = observed_exit {
            assert_eq!(result.exit_code, expected, "vector cardinality behavior changed at O{opt_level}");
        }
        observed_exit = Some(result.exit_code);
    }
    observed_exit.unwrap()
}

fn nested_fixture_source(element: &str, body: &str) -> String {
    format!(
        r#"
module explicit_nested
struct Record {{
    id: u8,
    nonce: u16,
    payload: Vec<u8>,
}}
action inspect(witness values: NestedValues<{element}, 3, 8, 128>) -> u64 {{
    verification
        {body}
        return 0
}}
"#
    )
}

fn execute_nested(source: &str, encoded: &[u8], opt_level: u8) -> (i64, u64) {
    let mut arguments = (encoded.len() as u32).to_le_bytes().to_vec();
    arguments.extend_from_slice(encoded);
    execute_nested_arguments(source, &arguments, opt_level)
}

fn execute_nested_arguments(source: &str, arguments: &[u8], opt_level: u8) -> (i64, u64) {
    let compiled = cellscript::compile(
        source,
        cellscript::CompileOptions { target: Some("riscv64-elf".into()), opt_level, ..Default::default() },
    )
    .unwrap();
    execute_compiled_nested(&compiled, arguments)
}

fn execute_compiled_nested(compiled: &cellscript::CompileResult, arguments: &[u8]) -> (i64, u64) {
    assert!(
        compiled.metadata.actions[0].fail_closed_runtime_features.iter().any(|feature| feature == "nested-values-artifact-proof"),
        "machine proof admission must remain explicit while the codec slice is integrated"
    );
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend_from_slice(arguments);
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
    let mut transaction = build_simple_fixture(Bytes::default(), 1, 1);
    transaction.witnesses = vec![witness.as_bytes()];
    let (result, group) = ckb_script_runner::execute_cellscript_script_with_observer(
        cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
        &transaction,
        |tx, _| tx,
        |context, tx, script| {
            cost_measurement::measure_group(context, tx, script, ckb_testtool::ckb_script::ScriptGroupType::Type, 10_000_000)
        },
    );
    let cost_measurement::CycleObservation::Measured { exit_code, .. } = &group.observation else {
        panic!("selected group measurement unavailable: {group:?}");
    };
    assert_eq!(i64::from(*exit_code), result.exit_code, "group replay disagrees with transaction verification");
    (result.exit_code, group.required_cycles())
}

#[test]
fn named_data_shapes_preserve_witness_bytes_bounds_and_vm_results() {
    let named = include_str!("syntax_combo/seeds/named-data-shapes.cell");
    let concrete = named
        .replace("data_shape Messages {\n    element: Vec<u8>,\n    limits { items: 3, payload_bytes: 8, encoded_bytes: 128 }\n}", "")
        .replace("data_shape Entries {\n    element: Entry,\n    limits { items: 3, payload_bytes: 8, encoded_bytes: 128 }\n}", "")
        .replace("Messages", "NestedValues<Vec<u8>, 3, 8, 128>")
        .replace("Entries", "NestedValues<Entry, 3, 8, 128>");
    let messages = molecule_fields(&[vec![2, 0, 0, 0, 1, 2], vec![0; 4]]);
    let entries = molecule_fields(&[molecule_fields(&[vec![9], vec![3, 0, 0, 0, 4, 5, 6]])]);
    let arguments = |messages: &[u8], entries: &[u8], expected: u64| {
        let mut args = (messages.len() as u32).to_le_bytes().to_vec();
        args.extend_from_slice(messages);
        args.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        args.extend_from_slice(entries);
        args.extend_from_slice(&expected.to_le_bytes());
        args
    };
    let mut malformed = messages.clone();
    malformed[4] += 1;
    let too_many = molecule_fields(&vec![vec![0; 4]; 4]);
    let long_payload = molecule_fields(&[vec![9, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]]);
    let cases = [
        (arguments(&messages, &entries, 3), 0),
        (arguments(&4u32.to_le_bytes(), &4u32.to_le_bytes(), 0), 0),
        (arguments(&messages, &entries, 4), cellscript::runtime_errors::CellScriptRuntimeError::AssertionFailed as i64),
        (arguments(&malformed, &entries, 3), 16),
        (arguments(&too_many, &entries, 0), 16),
        (arguments(&long_payload, &entries, 45), 16),
    ];
    for opt_level in 0..=3 {
        let compile = |source| {
            cellscript::compile(
                source,
                cellscript::CompileOptions {
                    edition: cellscript::NEXT_EDITION,
                    target: Some("riscv64-elf".into()),
                    opt_level,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let named = compile(named);
        let concrete = compile(&concrete);
        assert_eq!(
            cellscript::strip_vm_abi_trailer(&named.artifact_bytes),
            cellscript::strip_vm_abi_trailer(&concrete.artifact_bytes),
            "transparent shape declarations must not change executable ELF at O{opt_level}"
        );
        for (args, exit) in &cases {
            let actual = execute_compiled_nested(&named, args);
            assert_eq!(actual.0, *exit, "named shape O{opt_level}");
            assert_eq!(actual, execute_compiled_nested(&concrete, args), "wire, rejection and cycle parity O{opt_level}");
        }
    }
}

#[test]
fn nested_value_helpers_read_named_shapes_across_register_and_stack_calls() {
    for source in
        [include_str!("syntax_combo/seeds/named-nested-helpers.cell"), include_str!("syntax_combo/seeds/named-derived-helpers.cell")]
    {
        check_nested_helpers(source);
    }
}

fn check_nested_helpers(source: &str) {
    let messages = molecule_fields(&[vec![2, 0, 0, 0, 1, 2], vec![0; 4]]);
    let entries = molecule_fields(&[molecule_fields(&[vec![9], vec![3, 0, 0, 0, 4, 5, 6]])]);
    let mut malformed = entries.clone();
    malformed[4] += 1;
    let arguments = |messages: &[u8], entries: &[u8], expected: u64| {
        let mut args = (messages.len() as u32).to_le_bytes().to_vec();
        args.extend_from_slice(messages);
        args.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        args.extend_from_slice(entries);
        args.extend_from_slice(&expected.to_le_bytes());
        args
    };
    for prefix in [0, 6, 7, 8, 9] {
        let params = (0..prefix).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let args = "0, ".repeat(prefix);
        let source = source
            .replace("fn total(messages:", &format!("fn total({params}messages:"))
            .replace("total(messages, entries)", &format!("total({args}messages, entries)"))
            .replace("fn payload_total(payload:", &format!("fn payload_total({params}payload:"))
            .replace("payload_total(alias)", &format!("payload_total({args}alias)"))
            .replace("payload_total(entry.payload)", &format!("payload_total({args}entry.payload)"))
            .replace("fn record_total(entry:", &format!("fn record_total({params}entry:"))
            .replace("record_total(entry)", &format!("record_total({args}entry)"));
        for opt_level in 0..=3 {
            let compiled = cellscript::compile(
                &source,
                cellscript::CompileOptions {
                    edition: cellscript::NEXT_EDITION,
                    opt_level,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap();
            for (args, exit) in [
                (arguments(&messages, &entries, 27), 0),
                (arguments(&4u32.to_le_bytes(), &4u32.to_le_bytes(), 0), 0),
                (arguments(&messages, &entries, 28), cellscript::runtime_errors::CellScriptRuntimeError::AssertionFailed as i64),
                (arguments(&messages, &malformed, 27), 16),
            ] {
                assert_eq!(execute_compiled_nested(&compiled, &args).0, exit, "prefix={prefix}, O{opt_level}");
            }
        }
    }
}

// Fixture encoding is deliberately independent of both codec validators.
#[test]
fn nested_views_survive_register_and_stack_fixed_result_calls() {
    let encoded = molecule_fields(&[vec![3, 0, 0, 0, 0xaa, 0xbb, 0xcc]]);
    for count in [1, 7, 8, 9] {
        let params = (0..count).map(|i| format!("p{i}: u64")).collect::<Vec<_>>().join(", ");
        let args = vec!["at"; count].join(", ");
        let last = count - 1;
        let source = format!("module nested_result_vm\nstruct Pair {{ left: u64, right: u64 }}\nfn make({params}) -> Pair {{ Pair {{ left: p{last}, right: p{last} + 1 }} }}\nfn wrap(at: u64) -> Pair {{ make({args}) }}\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) {{ verification\nlet bytes = values[0]\nlet pair = wrap(bytes.len())\nrequire pair.left == 3\nrequire pair.right == 4\nrequire values.len() == 1\nrequire bytes.len() == 3\nrequire bytes[2] == 204 }}");
        let direct = source.replace("wrap(bytes.len())", &format!("make({})", vec!["bytes.len()"; count].join(", ")));
        for source in [source, direct] {
            for opt_level in 0..=3 {
                assert_eq!(execute_nested(&source, &encoded, opt_level).0, 0, "{count} args, O{opt_level}");
                let rejecting = source.replace("pair.right == 4", "pair.right == 5");
                assert_ne!(execute_nested(&rejecting, &encoded, opt_level).0, 0, "result must affect acceptance");
            }
        }
    }
}

fn molecule_fields(fields: &[Vec<u8>]) -> Vec<u8> {
    let header = 4 + fields.len() * 4;
    let total = header + fields.iter().map(Vec::len).sum::<usize>();
    let mut encoded = (total as u32).to_le_bytes().to_vec();
    let mut offset = header;
    for field in fields {
        encoded.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += field.len();
    }
    for field in fields {
        encoded.extend_from_slice(field);
    }
    encoded
}

#[test]
fn nested_witness_transport_rejects_wrong_field_source_and_framing() {
    let source = nested_fixture_source("Vec<u8>", "require values.len() == 0");
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend_from_slice(&4u32.to_le_bytes());
    payload.extend_from_slice(&4u32.to_le_bytes());
    let wrap = |bytes: Vec<u8>| packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(bytes)).pack()).build().as_bytes();
    let canonical = wrap(payload.clone());
    let mut bad_length = payload.clone();
    bad_length[8] = 5;
    let mut trailing = payload.clone();
    trailing.push(0);
    let mut wrong_magic = payload.clone();
    wrong_magic[0] ^= 1;
    let mut wrong_table = canonical.to_vec();
    wrong_table[4] = 20;
    let wrong_lock = packed::WitnessArgs::new_builder().lock(Some(Bytes::from(payload.clone())).pack()).build().as_bytes();
    let wrong_output = packed::WitnessArgs::new_builder().output_type(Some(Bytes::from(payload.clone())).pack()).build().as_bytes();
    for opt_level in 0..=3 {
        let compiled = cellscript::compile(
            &source,
            cellscript::CompileOptions { target: Some("riscv64-elf".into()), opt_level, ..Default::default() },
        )
        .unwrap();
        let execute = |witnesses: Vec<Bytes>| {
            let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
            fixture.witnesses = witnesses;
            execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture).exit_code
        };
        assert_eq!(execute(vec![canonical.clone()]), 0);
        for (name, witness) in [
            ("raw payload", Bytes::from(payload.clone())),
            ("lock field", wrong_lock.clone()),
            ("output_type field", wrong_output.clone()),
            ("table header", Bytes::from(wrong_table.clone())),
            ("segment length", wrap(bad_length.clone())),
            ("trailing byte", wrap(trailing.clone())),
            ("magic", wrap(wrong_magic.clone())),
        ] {
            assert_eq!(execute(vec![witness]), 25, "{name} O{opt_level}");
        }
        assert_eq!(execute(vec![Bytes::default(), canonical.clone()]), 25, "wrong witness index O{opt_level}");
    }
}

#[test]
fn nested_mixed_script_args_transport_preserves_both_cursors_and_stack_arguments() {
    let source = r#"
module mixed_nested
lock inspect(lock_args domain: u64, witness a: u64, witness b: u64,
    witness c: u64, witness d: u64, witness e: u64, witness f: u64,
    witness values: NestedValues<Vec<u8>, 2, 3, 32>, lock_args owner: Hash,
    lock_args nonce: u16, lock_args delta: i32, witness expected_delta: i32,
    witness nonnegative: i32) -> bool {
    verification
        require domain == 42
        require a == 1 && b == 2 && c == 3 && d == 4 && e == 5 && f == 6
        require values.len() == 1 && values[0][0] == 171
        require owner == Hash::zero()
        require nonce == 9 && delta < nonnegative && delta == expected_delta
        true
}
"#;
    let nested = molecule_fields(&[vec![1, 0, 0, 0, 171]]);
    let mut payload = b"CSARGv1\0".to_vec();
    for value in 1..=6u64 {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    payload.extend_from_slice(&(nested.len() as u32).to_le_bytes());
    payload.extend_from_slice(&nested);
    payload.extend_from_slice(&(-7i32).to_le_bytes());
    payload.extend_from_slice(&0i32.to_le_bytes());
    let mut script_args = 42u64.to_le_bytes().to_vec();
    script_args.extend_from_slice(&[0; 32]);
    script_args.extend_from_slice(&9u16.to_le_bytes());
    script_args.extend_from_slice(&(-7i32).to_le_bytes());
    for opt_level in 0..=3 {
        let compiled = cellscript::compile(
            source,
            cellscript::CompileOptions { target: Some("riscv64-elf".into()), opt_level, ..Default::default() },
        )
        .unwrap();
        let execute = |args: Vec<u8>, payload: Vec<u8>| {
            let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
            let code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes)));
            let script = context.build_script(&code, Bytes::from(args)).unwrap();
            let cell =
                packed::CellOutput::new_builder().capacity::<packed::Uint64>(1_000_000_000_000u64.pack()).lock(script.clone()).build();
            let input = context.create_cell(cell.clone(), Bytes::default());
            let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
            let tx = context.complete_tx(
                ckb_testtool::ckb_types::core::TransactionBuilder::default()
                    .input(packed::CellInput::new_builder().previous_output(input).build())
                    .output(cell)
                    .output_data(Bytes::default().pack())
                    .witness(witness.as_bytes().pack())
                    .build(),
            );
            let oracle = context.verify_tx(&tx, 10_000_000);
            let group =
                cost_measurement::measure_group(&context, &tx, &script, ckb_testtool::ckb_script::ScriptGroupType::Lock, 10_000_000);
            let cost_measurement::CycleObservation::Measured { exit_code, .. } = group.observation else {
                panic!("mixed Lock group did not execute: {group:?}");
            };
            assert_eq!(oracle.is_ok(), exit_code == 0, "Lock group replay disagrees with transaction: {oracle:?}");
            exit_code
        };
        assert_eq!(execute(script_args.clone(), payload.clone()), 0, "mixed positive O{opt_level}");
        for index in [0, 8, 39, 40, 42, 45] {
            let mut changed = script_args.clone();
            changed[index] ^= 1;
            assert_ne!(execute(changed, payload.clone()), 0, "Script.args byte {index} O{opt_level}");
        }
        let mut trailing = script_args.clone();
        trailing.push(0);
        assert_eq!(execute(trailing, payload.clone()), 25, "trailing Script.args O{opt_level}");
        assert_eq!(execute(script_args[..45].to_vec(), payload.clone()), 25, "short Script.args O{opt_level}");
        assert_eq!(execute(vec![0; 1024], payload.clone()), 25, "oversized Script O{opt_level}");
        let mut misplaced = b"CSARGv1\0".to_vec();
        misplaced.extend_from_slice(&script_args);
        misplaced.extend_from_slice(&payload[8..]);
        assert_eq!(execute(Vec::new(), misplaced), 25, "Script.args cannot move to witness O{opt_level}");
    }
}

#[test]
fn nested_record_wide_fields_preserve_bytes_and_high_integer_bits() {
    let source = r#"
module wide_nested
struct Record { tag: u8, amount: u128, digest: Hash, key: [u8; 17], payload: Vec<u8> }
action inspect(witness values: NestedValues<Record, 2, 8, 256>, witness expected: Hash) -> u64 {
    verification
        let record = values[0]
        require record.tag == 7
        require record.amount == 18446744073709551617
        require record.digest == expected
        let key = record.key
        require key[0] == 3
        require key[16] == 19
        require record.payload[0] == 171
        return 0
}
"#;
    let digest = (32..64).collect::<Vec<u8>>();
    let record = molecule_fields(&[
        vec![7],
        ((1u128 << 64) + 1).to_le_bytes().to_vec(),
        digest.clone(),
        (3..20).collect(),
        vec![1, 0, 0, 0, 171],
    ]);
    let encoded = molecule_fields(&[record]);
    let mut arguments = (encoded.len() as u32).to_le_bytes().to_vec();
    arguments.extend_from_slice(&encoded);
    arguments.extend_from_slice(&digest);
    for opt in 0..=3 {
        assert_eq!(execute_nested_arguments(source, &arguments, opt).0, 0, "O{opt}");
        let mut wrong = arguments.clone();
        *wrong.last_mut().unwrap() ^= 1;
        assert_ne!(execute_nested_arguments(source, &wrong, opt).0, 0, "wide comparison O{opt}");
    }
}

#[test]
fn nested_iteration_and_multiple_parameters_keep_independent_lengths() {
    let source = r#"
module nested_loops
action inspect(witness left: NestedValues<Vec<u8>, 3, 8, 128>, witness right: NestedValues<Vec<u8>, 3, 8, 128>, witness a: u64, witness b: u64, witness c: u64, witness d: u64, witness e: u64, witness f: u64) -> u64 {
    verification
        let mut total: u64 = 0
        for bytes in left {
            for byte in bytes { total = total + (byte as u64) }
        }
        require total == 174
        require right.len() == 1
        require right[0].len() == 3
        require right[0][2] == 99
        require a == 11 && b == 12 && c == 13 && d == 14 && e == 15 && f == 16
        return 0
}

"#;
    let left = hex::decode("1f0000001000000014000000190000000000000001000000ab020000000102").unwrap();
    let right = molecule_fields(&[vec![3, 0, 0, 0, 97, 98, 99]]);
    let mut arguments = (left.len() as u32).to_le_bytes().to_vec();
    arguments.extend_from_slice(&left);
    arguments.extend_from_slice(&(right.len() as u32).to_le_bytes());
    arguments.extend_from_slice(&right);
    // Two pointer/length pairs plus six scalars cross the eight-register ABI.
    // Nested entry validation must not clobber unspilled trailing arguments.
    for value in 11u64..=16 {
        arguments.extend_from_slice(&value.to_le_bytes());
    }
    for opt in 0..=3 {
        assert_eq!(execute_nested_arguments(source, &arguments, opt).0, 0, "O{opt}");
    }
}

#[test]
fn nested_witness_joint_maximum_respects_the_complete_entry_envelope() {
    let source =
        nested_fixture_source("Vec<u8>", "require values.len() == 32\nrequire values[14].len() == 220\nrequire values[31].is_empty()")
            .replace("3, 8, 128", "32, 256, 4096");
    // 132-byte outer header + 32 four-byte Bytes counts + 3804 payload
    // bytes = 4064. The CSARGv1 argument and WitnessArgs add 32 bytes.
    let mut fields = (0..32)
        .map(|index| {
            let count: u32 = if index < 14 {
                256
            } else if index == 14 {
                220
            } else {
                0
            };
            let mut bytes = count.to_le_bytes().to_vec();
            bytes.resize(4 + count as usize, 0xab);
            bytes
        })
        .collect::<Vec<_>>();
    let encoded = molecule_fields(&fields);
    assert_eq!(encoded.len(), 4064);
    let mut late_invalid = encoded.clone();
    *late_invalid.last_mut().unwrap() = 1; // Nonzero count for the last empty Bytes.
    fields[14][..4].copy_from_slice(&221u32.to_le_bytes());
    fields[14].push(0xab);
    let oversized_witness = molecule_fields(&fields);
    assert_eq!(oversized_witness.len(), 4065);
    for opt in 0..=3 {
        let valid = execute_nested(&source, &encoded, opt);
        let invalid = execute_nested(&source, &late_invalid, opt);
        assert_eq!(valid.0, 0, "joint maximum O{opt}");
        assert_eq!(invalid.0, 16, "last payload count O{opt}");
        assert_eq!(execute_nested(&source, &oversized_witness, opt).0, 25, "complete witness budget O{opt}");
        eprintln!("nested witness O{opt}: 4096 witness bytes, valid {} cycles, late-invalid {} cycles", valid.1, invalid.1);
    }
}

#[test]
fn nested_witness_enforces_count_payload_total_and_zero_budgets() {
    let source = nested_fixture_source("Vec<u8>", "");
    let empty_payload = vec![0, 0, 0, 0];
    let too_many = molecule_fields(&vec![empty_payload.clone(); 4]);
    let mut long_payload = 9u32.to_le_bytes().to_vec();
    long_payload.resize(13, 42);
    let payload_exceeded = molecule_fields(&[long_payload]);
    let total_source = source.replace("3, 8, 128", "3, 8, 11");
    let one_empty = molecule_fields(&[empty_payload]);
    let zero_source = source.replace("3, 8, 128", "0, 0, 4");
    for opt in 0..=3 {
        assert_eq!(execute_nested(&source, &too_many, opt).0, 16);
        assert_eq!(execute_nested(&source, &payload_exceeded, opt).0, 16);
        assert_eq!(execute_nested(&total_source, &one_empty, opt).0, 16);
        assert_eq!(execute_nested(&zero_source, &[4, 0, 0, 0], opt).0, 0);
        assert_eq!(execute_nested(&zero_source, &one_empty, opt).0, 16);
    }
}

#[test]
fn declared_nested_bytes_witness_exposes_read_only_counts_and_indexed_bytes() {
    let source = nested_fixture_source(
        "Vec<u8>",
        r#"
        require values.len() == 3
        require values[0].is_empty()
        require values[1][0] == 171
        let last = values[2]
        let alias = last
        require alias.len() == 2
        require alias[1] == 2
    "#,
    );
    let encoded = hex::decode("1f0000001000000014000000190000000000000001000000ab020000000102").unwrap();
    for opt in 0..=3 {
        assert_eq!(execute_nested(&source, &encoded, opt).0, 0, "O{opt}");
    }
}

#[test]
fn declared_nested_record_witness_exposes_identity_fields_and_read_only_payload() {
    let source = nested_fixture_source(
        "Record",
        r#"
        require values.len() == 1
        let record = values[0]
        require record.id == 127
        require record.nonce == 4660
        let payload = record.payload
        require payload.len() == 2
        require payload[0] == 170
        require payload[1] == 187
    "#,
    );
    let encoded = hex::decode("2100000008000000190000001000000011000000130000007f341202000000aabb").unwrap();
    for opt in 0..=3 {
        assert_eq!(execute_nested(&source, &encoded, opt).0, 0, "O{opt}");
    }
}

#[test]
fn nested_witness_is_validated_even_when_unused_and_indexes_are_checked() {
    let unused = nested_fixture_source("Vec<u8>", "");
    for encoded in ["", "00000000", "0400000000", "0c0000000800000001000000", "0c00000008000000ffffffff"] {
        assert_eq!(execute_nested(&unused, &hex::decode(encoded).unwrap(), 0).0, 16, "{encoded}");
    }
    assert_eq!(execute_nested(&unused, &[4, 0, 0, 0], 0).0, 0);
    let bad_index = nested_fixture_source("Vec<u8>", "let value = values[0]");
    assert_eq!(execute_nested(&bad_index, &[4, 0, 0, 0], 0).0, 21);
    let bad_byte = nested_fixture_source("Vec<u8>", "let value = values[0][0]");
    assert_eq!(execute_nested(&bad_byte, &hex::decode("0c0000000800000000000000").unwrap(), 0).0, 21);
    let huge_index = nested_fixture_source("Vec<u8>", "let value = values[18446744073709551615]");
    assert_eq!(execute_nested(&huge_index, &hex::decode("0c0000000800000000000000").unwrap(), 0).0, 21);
}

#[test]
fn nested_source_rejects_mutable_aliases_pointer_identity_and_view_escape() {
    for body in [
        "let mut alias = values",
        "let mut alias = values[0]",
        "let alias = values[0]\nalias.push(1)",
        "let alias = values[0]\nalias.type_hash()",
        "require values == values",
        "let raw = values as u64",
        "let wide: u128 = 0\nlet alias = values[wide]",
    ] {
        let source = nested_fixture_source("Vec<u8>", body);
        assert!(cellscript::compile(&source, Default::default()).is_err(), "unsupported nested operation accepted: {body}");
    }
    for suffix in [
        "fn escape(witness value: NestedValues<Vec<u8>, 3, 8, 128>) -> u64 { return 0 }",
        "fn escape() -> NestedBytesView<8> { return 0 }",
        "struct Escape { value: NestedBytesView<8> }",
        "action forged(witness value: NestedBytesView<8>) -> u64 { verification return 0 }",
    ] {
        let source = format!("{}\n{suffix}", nested_fixture_source("Vec<u8>", ""));
        assert!(cellscript::compile(&source, Default::default()).is_err(), "nested view escaped: {suffix}");
    }
}

#[test]
fn nested_source_rejects_unbounded_recursive_and_non_bitpattern_record_shapes() {
    let base = nested_fixture_source("Vec<u8>", "");
    for spelling in [
        "NestedValues<Vec<u8>, 33, 8, 128>",
        "NestedValues<Vec<u8>, 3, 257, 128>",
        "NestedValues<Vec<u8>, 3, 8, 4097>",
        "NestedValues<Vec<u8>, 3, 8, 3>",
        "NestedValues<Vec<Vec<u8>>, 3, 8, 128>",
        "NestedValues<Vec<u64>, 3, 8, 128>",
        "NestedValues<Record, 3>",
        "NestedValues<Vec<u8>, 3, 8, 128, 0>",
    ] {
        let source = base.replace("NestedValues<Vec<u8>, 3, 8, 128>", spelling);
        assert!(cellscript::compile(&source, Default::default()).is_err(), "invalid bound/shape accepted: {spelling}");
    }
    for record in [
        "struct Record { payload: Vec<u8>, id: u64 }",
        "struct Record { id: bool, payload: Vec<u8> }",
        "struct Record { id: [u8; 257], payload: Vec<u8> }",
        "struct Record { id: u8, child: Record, payload: Vec<u8> }",
        "resource Record has store, consume { id: u8, payload: Vec<u8> }",
    ] {
        let source = format!("module invalid_nested\n{record}\naction inspect(witness values: NestedValues<Record, 3, 8, 128>) -> u64 {{ verification return 0 }}");
        assert!(cellscript::compile(&source, Default::default()).is_err(), "invalid record admitted: {record}");
    }
}

#[test]
fn nested_types_survive_formatting_and_reject_invalid_editor_input() {
    let source = nested_fixture_source("Record", "require values[0].payload.len() == 2");
    cellscript::fmt::verify_idempotent(&source, Default::default()).unwrap();
    let tokens = cellscript::lexer::lex(&source).unwrap();
    let module = cellscript::parser::parse(&tokens).unwrap();
    let formatted = cellscript::fmt::format(&module, Default::default()).unwrap();
    let encoded = hex::decode("2100000008000000190000001000000011000000130000007f341202000000aabb").unwrap();
    assert_eq!(execute_nested(&formatted, &encoded, 2).0, 0);
    for seed in [
        include_str!("syntax_combo/seeds/nested-view-mutation-reject.cell"),
        include_str!("syntax_combo/seeds/nested-bound-reject.cell"),
    ] {
        let tokens = cellscript::lexer::lex(seed).unwrap();
        let module = cellscript::parser::parse(&tokens).unwrap();
        let error = cellscript::types::check(&module).unwrap_err();
        let expected = seed.lines().find_map(|line| line.strip_prefix("// audit: contains=")).unwrap();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

const NESTED_CELL_SOURCE: &str = r#"
module nested_cell
resource Document has store, consume, create {
    id: u64,
    entries: NestedValues<Vec<u8>, 32, 256, 4096>,
}
action roll(input before: Document, witness recipient: Address) -> after: Document {
    verification
        require ckb::cell_capacity(source::output(0)) >= ckb::cell_occupied_capacity(source::output(0))
        let original = before.entries
        require original.len() == 1
        require original[0][0] == 171
        let identity = before.id
        consume before
        create after = Document { id: identity, entries: original } with_lock(recipient)
}
"#;

fn execute_nested_cell(source: &str, before: &[u8], after: &[u8], opt_level: u8) -> i64 {
    execute_nested_cell_with_witness(source, before, after, &[], 100_000_000_000, opt_level)
}

fn execute_nested_cell_with_witness(source: &str, before: &[u8], after: &[u8], extra: &[u8], capacity: u64, opt_level: u8) -> i64 {
    let compiled = cellscript::compile(
        source,
        cellscript::CompileOptions { opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
    )
    .unwrap();
    execute_compiled_nested_cell(&compiled, before, after, extra, capacity)
}

fn execute_compiled_nested_cell(
    compiled: &cellscript::CompileResult,
    before: &[u8],
    after: &[u8],
    extra: &[u8],
    capacity: u64,
) -> i64 {
    assert!(compiled.metadata.actions[0].fail_closed_runtime_features.iter().any(|feature| feature == "nested-values-artifact-proof"));
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend_from_slice(&ckb_script_runner::deterministic_always_success_lock_hash());
    payload.extend_from_slice(extra);
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
    let mut transaction = build_simple_fixture(Bytes::default(), 1, 1);
    transaction.inputs[0].data = Bytes::copy_from_slice(before);
    transaction.inputs[0].capacity = capacity;
    transaction.outputs[0].data = Bytes::copy_from_slice(after);
    transaction.outputs[0].capacity = capacity;
    transaction.witnesses = vec![witness.as_bytes()];
    execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &transaction).exit_code
}

#[test]
fn named_data_shapes_preserve_actual_cell_successors_and_capacity_checks() {
    let concrete = NESTED_CELL_SOURCE.replace("verification", "");
    let named = concrete.replace("NestedValues<Vec<u8>, 32, 256, 4096>", "Messages").replace(
        "resource Document",
        "data_shape Messages { element: Vec<u8>, limits { items: 32, payload_bytes: 256, encoded_bytes: 4096 } }\nresource Document",
    );
    let values = molecule_fields(&[vec![1, 0, 0, 0, 171]]);
    let cell = molecule_fields(&[42u64.to_le_bytes().to_vec(), values.clone()]);
    let wrong_id = molecule_fields(&[43u64.to_le_bytes().to_vec(), values]);
    let wrong_payload = molecule_fields(&[42u64.to_le_bytes().to_vec(), molecule_fields(&[vec![1, 0, 0, 0, 172]])]);
    let mut malformed = cell.clone();
    malformed[4] += 1;
    for opt_level in 0..=3 {
        let compile = |source| {
            cellscript::compile(
                source,
                cellscript::CompileOptions {
                    edition: cellscript::NEXT_EDITION,
                    target: Some("riscv64-elf".into()),
                    opt_level,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let named = compile(&named);
        let concrete = compile(&concrete);
        assert_eq!(
            cellscript::strip_vm_abi_trailer(&named.artifact_bytes),
            cellscript::strip_vm_abi_trailer(&concrete.artifact_bytes)
        );
        for (before, after, capacity, accepted) in [
            (&cell, &cell, 100_000_000_000, true),
            (&cell, &wrong_id, 100_000_000_000, false),
            (&cell, &wrong_payload, 100_000_000_000, false),
            (&cell, &malformed, 100_000_000_000, false),
            (&malformed, &cell, 100_000_000_000, false),
            (&cell, &cell, 1, false),
        ] {
            let result = execute_compiled_nested_cell(&named, before, after, &[], capacity);
            assert_eq!(result == 0, accepted, "named successor O{opt_level}");
            assert_eq!(result, execute_compiled_nested_cell(&concrete, before, after, &[], capacity));
        }
    }
}

#[test]
fn nested_cell_input_and_successor_bind_canonical_data_and_exact_value() {
    let metadata = cellscript::compile_metadata(NESTED_CELL_SOURCE, cellscript::CURRENT_EDITION, None).unwrap();
    let field = &metadata.types.iter().find(|ty| ty.name == "Document").unwrap().fields[1];
    let values = cellscript::nested_builder::encode_nested_values(
        &metadata,
        &field.ty,
        &[cellscript::nested_builder::NestedElementInput { fixed_fields: &[], payload: &[171] }],
    )
    .unwrap();
    assert_eq!(values, molecule_fields(&[vec![1, 0, 0, 0, 171]]));
    let cell = cellscript::nested_builder::encode_nested_cell_data(
        &metadata,
        "Document",
        &std::collections::BTreeMap::from([
            (
                "entries".into(),
                cellscript::nested_builder::NestedCellFieldInput::Nested(&[cellscript::nested_builder::NestedElementInput {
                    fixed_fields: &[],
                    payload: &[171],
                }]),
            ),
            ("id".into(), cellscript::nested_builder::NestedCellFieldInput::FixedBytes(&42u64.to_le_bytes())),
        ]),
    )
    .unwrap();
    assert_eq!(cell, molecule_fields(&[42u64.to_le_bytes().to_vec(), values.clone()]));
    for opt in 0..=3 {
        assert_eq!(execute_nested_cell(NESTED_CELL_SOURCE, &cell, &cell, opt), 0, "O{opt}");
        let wrong_id = molecule_fields(&[43u64.to_le_bytes().to_vec(), values.clone()]);
        assert_ne!(execute_nested_cell(NESTED_CELL_SOURCE, &cell, &wrong_id, opt), 0, "identity O{opt}");
        let wrong_values = molecule_fields(&[42u64.to_le_bytes().to_vec(), molecule_fields(&[vec![1, 0, 0, 0, 172]])]);
        assert_ne!(execute_nested_cell(NESTED_CELL_SOURCE, &cell, &wrong_values, opt), 0, "payload O{opt}");
    }
}

#[test]
fn nested_cell_fixed_siblings_preserve_scalar_and_pointer_representations() {
    let types = [
        ("u8", 1),
        ("u16", 2),
        ("u32", 4),
        ("u64", 8),
        ("u128", 16),
        ("Hash", 32),
        ("Address", 32),
        ("[u8; 1]", 1),
        ("[u8; 8]", 8),
        ("[u8; 9]", 9),
        ("[u8; 256]", 256),
    ];
    let declarations = types.iter().enumerate().map(|(index, (ty, _))| format!("f{index}: {ty}, ")).collect::<String>();
    let values = molecule_fields(&[vec![1, 0, 0, 0, 171]]);
    let fields = types
        .iter()
        .enumerate()
        .map(|(index, (_, width))| (0..*width).map(|byte| (0x80 + index + byte) as u8).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let cell = |fields: &[Vec<u8>]| {
        molecule_fields(
            &std::iter::once(42u64.to_le_bytes().to_vec())
                .chain(fields.iter().cloned())
                .chain(std::iter::once(values.clone()))
                .collect::<Vec<_>>(),
        )
    };
    let before = cell(&fields);
    for witness in [false, true] {
        let extra_params = if witness {
            types.iter().enumerate().map(|(index, (ty, _))| format!(", witness e{index}: {ty}")).collect::<String>()
        } else {
            String::new()
        };
        let assignments = types
            .iter()
            .enumerate()
            .map(|(index, _)| if witness { format!("f{index}: e{index}, ") } else { format!("f{index}: before.f{index}, ") })
            .collect::<String>();
        let source = NESTED_CELL_SOURCE
            .replace("id: u64,", &format!("id: u64, {declarations}"))
            .replace("witness recipient: Address", &format!("witness recipient: Address{extra_params}"))
            .replace("id: identity, entries: original", &format!("id: identity, {assignments}entries: original"));
        let expected = if witness {
            fields.iter().map(|field| field.iter().map(|byte| byte ^ 0x51).collect::<Vec<_>>()).collect::<Vec<_>>()
        } else {
            fields.clone()
        };
        let extra = if witness { expected.concat() } else { Vec::new() };
        let after = cell(&expected);
        for opt in 0..=3 {
            assert_eq!(
                execute_nested_cell_with_witness(&source, &before, &after, &extra, 1_000_000_000_000, opt),
                0,
                "witness={witness}, O{opt}"
            );
            for (index, _) in types.iter().enumerate() {
                let mut altered = expected.clone();
                *altered[index].last_mut().unwrap() ^= 0x80;
                assert_eq!(
                    execute_nested_cell_with_witness(&source, &before, &cell(&altered), &extra, 1_000_000_000_000, opt),
                    if index < 4 { 3 } else { 32 },
                    "field {index}, witness={witness}, O{opt}"
                );
            }
        }
    }
}

#[test]
fn nested_cell_fixed_literal_successors_compare_complete_values() {
    let source = NESTED_CELL_SOURCE.replace("id: u64,", "id: u64, big: u128, hash: Hash, owner: Address, pad: [u8; 9],").replace(
        "id: identity, entries: original",
        "id: 18446744073709551615, big: 0 as u128, hash: Hash::zero(), owner: Address::zero(), pad: b\"abcdefghi\", entries: original",
    );
    let values = molecule_fields(&[vec![1, 0, 0, 0, 171]]);
    let fields = vec![u64::MAX.to_le_bytes().to_vec(), vec![0; 16], vec![0; 32], vec![0; 32], b"abcdefghi".to_vec(), values];
    let after = molecule_fields(&fields);
    let mut initial = fields.clone();
    for field in &mut initial[..5] {
        for byte in field {
            *byte ^= 0x80;
        }
    }
    let before = molecule_fields(&initial);
    for opt in 0..=3 {
        assert_eq!(execute_nested_cell_with_witness(&source, &before, &after, &[], 1_000_000_000_000, opt), 0);
        for index in 0..5 {
            let mut altered = fields.clone();
            *altered[index].last_mut().unwrap() ^= 0x80;
            assert_eq!(
                execute_nested_cell_with_witness(&source, &before, &molecule_fields(&altered), &[], 1_000_000_000_000, opt),
                if index == 0 { 3 } else { 32 },
                "constant field {index}, O{opt}"
            );
        }
    }
}

#[test]
fn nested_cell_fixed_successors_accept_computed_scalars_and_wide_carry() {
    let source = format!(
        "{}\nfn increment(value: u64) -> u64 {{ return value + 1 }}",
        NESTED_CELL_SOURCE
            .replace("id: u64,", "id: u64, big: u128,")
            .replace("let identity = before.id", "let identity = increment(before.id)\nlet next = before.big + 1")
            .replace("id: identity, entries: original", "id: identity, big: next, entries: original")
    );
    let values = molecule_fields(&[vec![1, 0, 0, 0, 171]]);
    let previous = u128::from(u64::MAX);
    let before = molecule_fields(&[42u64.to_le_bytes().to_vec(), previous.to_le_bytes().to_vec(), values.clone()]);
    let fields = vec![43u64.to_le_bytes().to_vec(), (previous + 1).to_le_bytes().to_vec(), values];
    for opt in 0..=3 {
        assert_eq!(execute_nested_cell_with_witness(&source, &before, &molecule_fields(&fields), &[], 1_000_000_000_000, opt), 0);
        for index in 0..2 {
            let mut altered = fields.clone();
            altered[index][if index == 0 { 0 } else { 8 }] ^= 1;
            assert_eq!(
                execute_nested_cell_with_witness(&source, &before, &molecule_fields(&altered), &[], 1_000_000_000_000, opt),
                if index == 0 { 3 } else { 32 }
            );
        }
    }
}

#[test]
fn nested_cell_replacement_validates_unused_input_and_actual_capacity() {
    let source = NESTED_CELL_SOURCE
        .replace("witness recipient: Address", "witness recipient: Address, witness replacement: NestedValues<Vec<u8>, 32, 256, 4096>")
        .replace("let original = before.entries", "let original = replacement");
    let initial = molecule_fields(&[42u64.to_le_bytes().to_vec(), molecule_fields(&[vec![0, 0, 0, 0]])]);
    let values = molecule_fields(&[vec![2, 0, 0, 0, 171, 172]]);
    let replacement = molecule_fields(&[42u64.to_le_bytes().to_vec(), values.clone()]);
    let mut extra = (values.len() as u32).to_le_bytes().to_vec();
    extra.extend_from_slice(&values);
    let mut malformed = initial.clone();
    *malformed.last_mut().unwrap() = 1; // Invalid count in an unused input field.
    let noncanonical = molecule_fields(&[42u64.to_le_bytes().to_vec(), values.clone(), vec![]]);
    let mixed = source.replace(
        "let original = replacement",
        "let prior = before.entries\nrequire prior.len() == replacement.len()\nlet original = replacement",
    );
    for source in [source, mixed] {
        for opt in 0..=3 {
            assert_eq!(execute_nested_cell_with_witness(&source, &initial, &replacement, &extra, 100_000_000_000, opt), 0);
            assert_eq!(execute_nested_cell_with_witness(&source, &malformed, &replacement, &extra, 100_000_000_000, opt), 16);
            assert_ne!(execute_nested_cell_with_witness(&source, &initial, &initial, &extra, 100_000_000_000, opt), 0);
            assert_ne!(execute_nested_cell_with_witness(&source, &initial, &noncanonical, &extra, 100_000_000_000, opt), 0);
            assert_eq!(execute_nested_cell_with_witness(&source, &initial, &replacement, &extra, 1_000_000_000, opt), 5);
        }
    }
}

#[test]
fn nested_cell_supports_the_full_codec_limit_above_legacy_buffers() {
    let source = NESTED_CELL_SOURCE.replace("original.len() == 1", "original.len() == 32");
    let fields = (0..32)
        .map(|index| {
            let count: u32 = if index < 14 {
                256
            } else if index == 14 {
                252
            } else {
                0
            };
            let mut bytes = count.to_le_bytes().to_vec();
            bytes.resize(4 + count as usize, 171);
            bytes
        })
        .collect::<Vec<_>>();
    let values = molecule_fields(&fields);
    assert_eq!(values.len(), 4096);
    let cell = molecule_fields(&[42u64.to_le_bytes().to_vec(), values.clone()]);
    assert_eq!(cell.len(), 4116);
    let mut malformed = cell.clone();
    *malformed.last_mut().unwrap() = 1;
    let mut bad_fixed = 42u64.to_le_bytes().to_vec();
    bad_fixed.push(0);
    let wrong_width = molecule_fields(&[bad_fixed, values]);
    for opt in 0..=3 {
        assert_eq!(execute_nested_cell_with_witness(&source, &cell, &cell, &[], 1_000_000_000_000, opt), 0, "large Cell O{opt}");
        assert_eq!(execute_nested_cell_with_witness(&source, &malformed, &cell, &[], 1_000_000_000_000, opt), 16);
        assert_eq!(execute_nested_cell_with_witness(&source, &cell, &malformed, &[], 1_000_000_000_000, opt), 16);
        assert_ne!(execute_nested_cell_with_witness(&source, &wrong_width, &cell, &[], 1_000_000_000_000, opt), 0);
    }
}

#[test]
fn nested_record_cells_and_standalone_creates_use_the_same_checked_codec() {
    let source = NESTED_CELL_SOURCE
        .replace("resource Document", "struct Record { id: u8, payload: Vec<u8> }\nresource Document")
        .replace("NestedValues<Vec<u8>,", "NestedValues<Record,")
        .replace("require original[0][0] == 171", "require original[0].id == 7\nrequire original[0].payload[0] == 171");
    let record = molecule_fields(&[vec![7], vec![1, 0, 0, 0, 171]]);
    let metadata = cellscript::compile_metadata(&source, cellscript::CURRENT_EDITION, None).unwrap();
    let cell = cellscript::nested_builder::encode_nested_cell_data(
        &metadata,
        "Document",
        &std::collections::BTreeMap::from([
            ("id".into(), cellscript::nested_builder::NestedCellFieldInput::FixedBytes(&42u64.to_le_bytes())),
            (
                "entries".into(),
                cellscript::nested_builder::NestedCellFieldInput::Nested(&[cellscript::nested_builder::NestedElementInput {
                    fixed_fields: &[&[7]],
                    payload: &[171],
                }]),
            ),
        ]),
    )
    .unwrap();
    assert_eq!(cell, molecule_fields(&[42u64.to_le_bytes().to_vec(), molecule_fields(&[record])]));
    let standalone = source
        .replace("-> after: Document", "-> u64")
        .replace("create after = Document", "create Document")
        .replace("with_lock(recipient)", "with_lock(recipient)\nreturn 0");
    for opt in 0..=3 {
        assert_eq!(execute_nested_cell(&source, &cell, &cell, opt), 0);
        assert_eq!(execute_nested_cell(&standalone, &cell, &cell, opt), 0);
        let mut malformed = cell.clone();
        let payload_count = malformed.len() - 5;
        malformed[payload_count] = 0;
        assert_eq!(execute_nested_cell(&standalone, &cell, &malformed, opt), 16);
    }
}

#[test]
fn nested_cell_multiple_fields_keep_distinct_buffers_and_views() {
    let source = NESTED_CELL_SOURCE
        .replace(
            "entries: NestedValues<Vec<u8>, 32, 256, 4096>,",
            "entries: NestedValues<Vec<u8>, 32, 256, 4096>, spare: NestedValues<Vec<u8>, 32, 256, 4096>,",
        )
        .replace("let identity = before.id", "let spare = before.spare\nrequire spare[0][0] == 187\nlet identity = before.id")
        .replace("entries: original }", "entries: original, spare: spare }");
    let first = molecule_fields(&[vec![1, 0, 0, 0, 171]]);
    let second = molecule_fields(&[vec![2, 0, 0, 0, 187, 188]]);
    let cell = molecule_fields(&[42u64.to_le_bytes().to_vec(), first.clone(), second.clone()]);
    let swapped = molecule_fields(&[42u64.to_le_bytes().to_vec(), second, first]);
    let different_bounds = source.replace("spare: NestedValues<Vec<u8>, 32, 256, 4096>", "spare: NestedValues<Vec<u8>, 1, 5, 32>");
    for source in [source, different_bounds] {
        for opt in 0..=3 {
            assert_eq!(execute_nested_cell(&source, &cell, &cell, opt), 0);
            assert_ne!(execute_nested_cell(&source, &cell, &swapped, opt), 0);
        }
    }
}

#[test]
fn nested_cell_and_witness_profiles_keep_stable_admission_blocked() {
    for source in [NESTED_CELL_SOURCE.to_string(), nested_fixture_source("Vec<u8>", "")] {
        let error = cellscript::compile_with_executable_surface_policy(
            &source,
            cellscript::CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_err();
        assert!(error.to_string().contains("nested-values-artifact-proof"), "{error}");
    }
}

#[test]
fn nested_cell_layout_diagnostics_reject_unbounded_siblings_before_codegen() {
    for sibling in ["Vec<u8>", "bool", "[u8; 257]"] {
        let source = NESTED_CELL_SOURCE.replace("id: u64,", &format!("id: {sibling},"));
        let tokens = cellscript::lexer::lex(&source).unwrap();
        let module = cellscript::parser::parse(&tokens).unwrap();
        let error = cellscript::types::check(&module).unwrap_err();
        assert!(error.to_string().contains("a Cell containing NestedValues requires"), "{sibling}: {error}");
    }
}

#[test]
fn borrowed_nested_cell_helpers_retain_the_artifact_proof_blocker() {
    let cell = molecule_fields(&[42u64.to_le_bytes().to_vec(), molecule_fields(&[vec![1, 0, 0, 0, 171]])]);
    for count in [0, 1, 6, 7, 8, 9] {
        let params = (0..count).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let arguments = "0, ".repeat(count);
        let forwarded = (0..count).map(|i| format!("p{i}, ")).collect::<String>();
        for caller in ["count", "wrap"] {
            let source = format!("{}\n#[effect(ReadOnly)]\nfn count({params}document: &Document) -> u64 {{ return document.entries.len() }}\n#[effect(ReadOnly)]\nfn wrap({params}document: &Document) -> u64 {{ return count({forwarded}document) }}\n",
                NESTED_CELL_SOURCE.replace("require original.len() == 1", &format!("require {caller}({arguments}&before) == 1")));
            let compiled =
                cellscript::compile(&source, cellscript::CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() })
                    .unwrap();
            let helper = compiled.metadata.functions.iter().find(|function| function.name == "count").unwrap();
            assert!(helper.fail_closed_runtime_features.iter().any(|feature| feature == "nested-values-artifact-proof"));
            for opt in 0..=3 {
                assert_eq!(execute_nested_cell(&source, &cell, &cell, opt), 0);
            }
        }
    }
    let mixed = format!("{}\n#[effect(ReadOnly)]\nfn count(values: NestedValues<Vec<u8>, 32, 256, 4096>, document: &Document) -> u64 {{ return values.len() + document.entries.len() }}\n",
        NESTED_CELL_SOURCE.replace("require original.len() == 1", "require count(original, &before) == 2"));
    for opt in 0..=3 {
        assert_eq!(execute_nested_cell(&mixed, &cell, &cell, opt), 0);
        assert_ne!(execute_nested_cell(&mixed.replace("== 2", "== 3"), &cell, &cell, opt), 0);
    }
}

#[test]
fn nested_policy_dispatch_executes_only_the_selected_bounded_arguments() {
    use cellscript::artifact::{compile_artifact, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    use cellscript::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
    let source = r#"
module nested_policy_vm
resource Token has store, consume { amount: u64, expected: Hash }
action first(input token: Token, witness a: u64, witness b: u64, witness c: u64, witness d: u64, witness e: u64, witness values: NestedValues<Vec<u8>, 1, 3, 32>, witness tail: Hash) {
    verification
    require a == 1
    require b == 2
    require c == 3
    require d == 4
    require e == 5
    require values.len() == 1
    let value = values[0]
    require value.len() == 1
    require value[0] == 171
    require tail == token.expected
    consume token
}
action second(input token: Token, witness a: u64, witness b: u64, witness c: u64, witness d: u64, witness e: u64, witness values: NestedValues<Vec<u8>, 2, 5, 64>, witness tail: Hash) {
    verification
    require values.len() == 2
    let value = values[1]
    require value.len() == 1
    require value[0] == 205
    require tail == token.expected
    consume token
}
"#;
    let value = |byte: u8| vec![1, 0, 0, 0, byte];
    let first = molecule_fields(&[value(171)]);
    let second = molecule_fields(&[value(171), value(205)]);
    let args = |nested: &[u8]| {
        let mut data = b"CSARGv1\0".to_vec();
        for scalar in 1u64..=5 {
            data.extend_from_slice(&scalar.to_le_bytes());
        }
        data.extend_from_slice(&(nested.len() as u32).to_le_bytes());
        data.extend_from_slice(nested);
        data.extend_from_slice(&[0; 32]);
        data
    };
    for opt_level in 0..=3 {
        let compiled = compile_artifact(
            source,
            cellscript::CompileOptions {
                opt_level,
                target: Some("riscv64-elf".into()),
                target_profile: Some("ckb".into()),
                ..Default::default()
            },
            ArtifactDeclaration {
                name: "nested-policy".into(),
                context: ArtifactContext::TypeGroup { resource: "Token".into() },
                dispatch: ArtifactDispatch::PolicyWitnessV1,
                actions: vec![ArtifactAction { tag: 40, action: "first".into() }, ArtifactAction { tag: 10, action: "second".into() }],
                common_checks: vec![],
            },
            cellscript::ExecutableSurfacePolicy::AllowFailClosed,
        )
        .unwrap();
        let run = |tag: u32, payload: Vec<u8>, wrong_identity: bool, wrong_field: bool| {
            let mut fixture = build_simple_fixture(Bytes::default(), 2, 0);
            fixture.current_type_script_input_indices = vec![1];
            fixture.inputs[1].data = Bytes::from(vec![0; 40]);
            let (result, group) = ckb_script_runner::execute_cellscript_script_with_observer(
                cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
                &fixture,
                |tx, script| {
                    let mut hash: [u8; 32] = script.calc_script_hash().as_slice().try_into().unwrap();
                    if wrong_identity {
                        hash[31] ^= 1;
                    }
                    let policy = encode_policy_witness_bundle(&[PolicyWitnessRecord {
                        role: PolicyScriptRole::Type,
                        script_hash: hash,
                        tag,
                        args: payload,
                    }])
                    .unwrap();
                    let witness = if wrong_field {
                        packed::WitnessArgs::new_builder().output_type(Some(Bytes::from(policy)).pack()).build()
                    } else {
                        packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(policy)).pack()).build()
                    };
                    tx.as_advanced_builder().set_witnesses(vec![Bytes::from(vec![0xff]).pack(), witness.as_bytes().pack()]).build()
                },
                |context, tx, script| {
                    cost_measurement::measure_group(context, tx, script, ckb_testtool::ckb_script::ScriptGroupType::Type, 10_000_000)
                },
            );
            let cost_measurement::CycleObservation::Measured { exit_code, .. } = group.observation else {
                panic!("missing selected policy observation");
            };
            assert_eq!(i64::from(exit_code), result.exit_code);
            result.exit_code
        };
        assert_eq!(run(40, args(&first), false, false), 0, "first O{opt_level}");
        assert_eq!(run(10, args(&second), false, false), 0, "second O{opt_level}");
        assert_eq!(run(40, args(&second), false, false), 16, "selected nested bound");
        assert_ne!(run(10, args(&first), false, false), 0, "selected action predicate");
        for offset in [8, 16, 24, 32, 40, args(&first).len() - 1] {
            let mut bad = args(&first);
            bad[offset] ^= 1;
            assert_ne!(run(40, bad, false, false), 0, "argument offset {offset}");
        }
        let mut trailing = args(&first);
        trailing.push(0);
        let mut malformed = args(&first);
        malformed[48] += 1;
        for bad in [trailing, malformed, args(&first)[..args(&first).len() - 1].to_vec()] {
            assert_eq!(run(40, bad, false, false), 25, "positional framing");
        }
        assert_eq!(run(99, args(&first), false, false), 25, "unknown tag");
        assert_eq!(run(40, args(&first), true, false), 25, "wrong selected Script");
        assert_eq!(run(40, args(&first), false, true), 25, "wrong witness field");
    }
}
