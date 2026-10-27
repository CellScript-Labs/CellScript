use super::*;

const PREFIX: &str = ".Lallocated_args_";

fn source(key: &str, value: &str, bytes: u32) -> String {
    format!(
        r#"module scalar_arguments
collections Work {{ limits {{ bytes: {bytes}, allocations: 8 }}
    values: Vec<{value}> {{ items: 4 }}
    totals: OrderedMap<{key}, {value}> {{ items: 4 }}
    seen: OrderedSet<{key}> {{ items: 4 }}
}}
fn read(view: &Work, key: {key}, fallback: {value}) -> {value} {{
    if !view.totals.contains_key(key) {{ return fallback }}
    return view.totals.get_or(key, fallback)
}}
action inspect(witness key: {key}, witness other: {key}, witness value: {value}, witness fallback: {value}, witness skip: bool) -> u64 {{
    let mut work = Work::new()
    if skip {{ return 0 }}
    work.values.push(value)
    work.values.insert(0, fallback)
    work.values.set(1, value)
    work.totals.insert(key, value)
    work.totals.insert(other, fallback)
    work.seen.insert(key)
    require work.totals.contains_key(key)
    require work.totals.get(key) == value
    require work.totals.get_or(other, fallback) == fallback
    require read(&work, key, fallback) == value
    work.totals.remove(other)
    require work.seen.contains(key)
    work.seen.remove(key)
    work.drop()
    return 0
}}"#
    )
}

pub(super) fn ranges(valid: &Fixture) -> Vec<(String, String, u64, u64)> {
    valid
        .record
        .blocks
        .iter()
        .flat_map(|block| {
            block.allocated_lifecycle_labels.iter().filter_map(|label| {
                if !label.starts_with(PREFIX) || !label.ends_with("_begin") {
                    return None;
                }
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                    .unwrap()
                    .range
                    .start;
                Some((label.clone(), block.owner_entry.clone(), block.range.start, end))
            })
        })
        .collect()
}

#[test]
fn scalar_collection_arguments_bind_sources_widths_and_consumers() {
    for (key, value) in [("u8", "bool"), ("u16", "u8"), ("u32", "u16"), ("u64", "u32"), ("u64", "i32"), ("u64", "u64")] {
        for bytes in [512, 65536] {
            for opt_level in [0, 3] {
                let valid = Fixture::from_result(
                    compile(
                        &source(key, value, bytes),
                        CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                    )
                    .unwrap(),
                );
                assert_argument_stages(&valid, bytes, opt_level);
            }
        }
    }
}

const WIDE_PROFILES: [(&str, &str, &str); 5] = [
    ("Hash", "u128", ""),
    ("[u8; 9]", "[u8; 32]", ""),
    ("u64", "Value", "struct Value { bytes: [u8; 1] }"),
    ("u64", "Value", "struct Value { bytes: [u8; 8] }"),
    ("u64", "Value", "struct Value { bytes: [u8; 32] }"),
];

fn wide_fixture(key: &str, value: &str, declaration: &str, bytes: u32, opt_level: u8) -> Fixture {
    let mut source = source(key, value, bytes).replace("module scalar_arguments", &format!("module wide_arguments\n{declaration}"));
    if value == "Value" {
        source = source
            .replace("work.totals.get(key) == value", "work.totals.get(key).bytes == value.bytes")
            .replace("work.totals.get_or(other, fallback) == fallback", "work.totals.get_or(other, fallback).bytes == fallback.bytes")
            .replace("read(&work, key, fallback) == value", "read(&work, key, fallback).bytes == value.bytes");
    }
    Fixture::from_result(
        compile(
            &source,
            CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap(),
    )
}

#[test]
fn wide_collection_arguments_bind_sources_widths_and_consumers() {
    for (key, value, declaration) in WIDE_PROFILES {
        for bytes in [512, 65536] {
            for opt_level in [0, 3] {
                let valid = wide_fixture(key, value, declaration, bytes, opt_level);
                assert_argument_stages(&valid, bytes, opt_level);
            }
        }
    }
}

#[test]
fn short_collection_arguments_bind_packed_values_and_pointer_producers() {
    for width in 1..=8 {
        for opt_level in [0, 3] {
            let ty = format!("[u8; {width}]");
            let mut text = source(&ty, &ty, if width % 2 == 0 { 65536 } else { 512 });
            // Ordinary arguments and helper ABI use packed words. Reassign the
            // arguments from field and collection producers to exercise the
            // pointed copy path without changing the consuming operations.
            text = text.replace("module scalar_arguments", &format!("module short_arguments\nstruct Value {{ bytes: {ty} }}"));
            text = text.replace(
                "    work.totals.insert(key, value)",
                "    let record = Value { bytes: value }\n    work.totals.insert(key, record.bytes)",
            );
            text = text.replace("    work.totals.insert(other, fallback)", "    work.totals.insert(other, work.values.get(0))");
            let valid = Fixture::from_result(
                compile(
                    &text,
                    CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            assert_argument_stages(&valid, if width % 2 == 0 { 65536 } else { 512 }, opt_level);
        }
    }
}

#[test]
fn short_collection_literal_arguments_bind_each_byte() {
    for width in [1, 8] {
        let ty = format!("[u8; {width}]");
        let literal = (0..width).map(|index| (255 - index).to_string()).collect::<Vec<_>>().join(", ");
        let text = source(&ty, &ty, 512).replace("work.values.push(value)", &format!("work.values.push([{literal}])"));
        for opt_level in [0, 3] {
            let valid = Fixture::from_result(
                compile(
                    &text,
                    CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            assert_argument_stages(&valid, 512, opt_level);
        }
    }
}

#[test]
fn short_collection_indexed_arguments_bind_typed_element_pointer() {
    for width in [1, 8] {
        for dynamic in [false, true] {
            let ty = format!("[u8; {width}]");
            let index = if dynamic { "index" } else { "1" };
            let text = source(&ty, &ty, 512)
                .replace("witness skip: bool", &format!("witness pairs: [{ty}; 2], witness index: u64, witness skip: bool"))
                .replace("work.values.push(value)", &format!("work.values.push(pairs[{index}])"));
            for opt_level in [0, 3] {
                let valid = Fixture::from_result(
                    compile(
                        &text,
                        CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                    )
                    .unwrap(),
                );
                assert_argument_stages(&valid, 512, opt_level);
            }
        }
    }
}

#[test]
fn wide_collection_argument_copy_preserves_its_final_byte() {
    for (key, value, declaration) in WIDE_PROFILES {
        for bytes in [512, 65536] {
            for opt_level in [0, 3] {
                let valid = wide_fixture(key, value, declaration, bytes, opt_level);
                let (_, _, start, end) = ranges(&valid)
                    .into_iter()
                    .find(|(label, owner, _, _)| {
                        label.strip_prefix(PREFIX).unwrap().split('_').nth(1) == Some("0")
                            && valid.record.typed_semantics.entries.iter().any(|entry| entry.id == *owner && entry.kind == "action")
                    })
                    .unwrap();
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                for opcode in [0x4003, 0x0023] {
                    let last = elf
                        .instructions
                        .iter()
                        .rev()
                        .find(|instruction| (start..end).contains(&instruction.address) && instruction.word & 0x707f == opcode)
                        .expect("complete fixed-value byte copy");
                    let mut changed = valid.clone();
                    changed.replace_machine_word(last.address, if opcode == 0x4003 { last.word ^ (1 << 20) } else { 0x13 });
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
        }
    }
}

fn assert_argument_stages(valid: &Fixture, bytes: u32, opt_level: u8) {
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let ranges = ranges(valid);
    assert_eq!(ranges.len(), 14, "complete vector, map, set and readonly-helper argument inventory");
    let mut classes = std::collections::BTreeSet::new();
    for (label, owner, start, end) in &ranges {
        let member = label.strip_prefix(PREFIX).unwrap().split('_').nth(1).unwrap();
        let helper = valid.record.typed_semantics.entries.iter().any(|entry| entry.id == *owner && entry.kind == "helper");
        for instruction in elf.instructions.iter().filter(|instruction| (*start..*end).contains(&instruction.address)) {
            let word = instruction.word;
            let opcode = word & 0x7f;
            if !classes.insert((helper, member, opcode, word >> 12 & 7)) {
                continue;
            }
            let replacement = match opcode {
                0x03 => word ^ (8 << 20),
                0x23 => word ^ (8 << 7),
                0x37 => word ^ (1 << 12),
                _ => word ^ (1 << 20),
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, replacement);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
    assert!(classes.len() >= 8, "exercise scalar loads, byte stores and shifts for distinct consumers");
    let mut missing = valid.clone();
    for block in &mut missing.record.blocks {
        block.allocated_lifecycle_labels.retain(|label| !label.starts_with(PREFIX));
    }
    missing.rebind_sidecars();
    assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    if bytes == 512 && opt_level == 0 {
        let (_, owner, start, end) = ranges
            .iter()
            .find(|(_, owner, _, _)| {
                valid.record.typed_semantics.entries.iter().any(|entry| entry.id == *owner && entry.kind == "action")
            })
            .unwrap();
        let branch = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.word & 0x7f == 0x63
                    && instruction.address < *start
                    && *end - instruction.address < 4096
                    && valid.record.blocks.iter().any(|block| block.owner_entry == *owner && block.range.contains(instruction.address))
            })
            .expect("source branch before argument preparation");
        let mut bypass = valid.clone();
        bypass.redirect_conditional_branch(branch.address, *end);
        assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }

    for coordinate in [0, 1] {
        let mut changed = valid.clone();
        let label = &ranges[0].0;
        let stem = label.strip_suffix("_begin").unwrap();
        for block in &mut changed.record.blocks {
            for marker in &mut block.allocated_lifecycle_labels {
                if marker == label || marker == &format!("{stem}_end") {
                    let mut parts = marker.strip_prefix(PREFIX).unwrap().split('_').map(str::to_string).collect::<Vec<_>>();
                    parts[coordinate] = (parts[coordinate].parse::<u64>().unwrap() + 1).to_string();
                    *marker = format!("{PREFIX}{}", parts.join("_"));
                }
            }
        }
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn scalar_collection_key_cannot_substitute_another_typed_local() {
    substituted_key_changes_vm_outcome("u64");
}

#[test]
fn wide_collection_key_cannot_substitute_another_typed_local() {
    substituted_key_changes_vm_outcome("Hash");
}

#[test]
fn short_collection_key_cannot_substitute_another_typed_local() {
    for width in [1, 8] {
        substituted_key_changes_vm_outcome(&format!("[u8; {width}]"));
    }
}

fn substituted_key_changes_vm_outcome(key_type: &str) {
    let wide = key_type == "Hash";
    for opt_level in [0, 3] {
        let compiled = compile(
            &source(key_type, if wide { "u128" } else { "u64" }, 512),
            CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        use cellscript::EntryWitnessArg;
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let key_arg = |byte| match key_type {
            "Hash" => EntryWitnessArg::Hash([byte; 32]),
            "u64" => EntryWitnessArg::U64(u64::from(byte)),
            short => {
                EntryWitnessArg::Bytes(vec![byte; short.strip_prefix("[u8; ").unwrap().strip_suffix(']').unwrap().parse().unwrap()])
            }
        };
        let payload = compiled.metadata.actions[0]
            .entry_witness_args(&[
                key_arg(1),
                key_arg(2),
                if wide { EntryWitnessArg::U128(17) } else { EntryWitnessArg::U64(17) },
                if wide { EntryWitnessArg::U128(29) } else { EntryWitnessArg::U64(29) },
                EntryWitnessArg::Bool(false),
            ])
            .unwrap();
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.current_type_script_input_indices = vec![0];
        transaction.witnesses =
            vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        let valid = Fixture::from_result(compiled);
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.kind == "action").unwrap();
        let local_slot = |name| {
            let param = entry.params.iter().find(|param| param.name == name).unwrap();
            entry.locals.iter().find(|local| local.id == param.binding_id).unwrap().source_id as i32 * 8
        };
        let key = local_slot("key");
        let other = local_slot("other");
        let stage = ranges(&valid)
            .into_iter()
            .find(|(label, owner, _, _)| owner == &entry.id && label.strip_prefix(PREFIX).unwrap().split('_').nth(1) == Some("1"))
            .unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let load = elf
            .instructions
            .iter()
            .find(|instruction| {
                let word = instruction.word;
                (stage.2..stage.3).contains(&instruction.address)
                    && word & 0x707f == 0x3003
                    && word >> 15 & 31 == 2
                    && (word as i32) >> 20 == key
            })
            .expect("map insertion loads the typed key");
        let mut changed = valid.clone();
        changed.replace_machine_word(load.address, replace_i_immediate(load.word, other));
        let execute =
            |artifact: &[u8]| ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(artifact), &transaction);
        assert_eq!(execute(&valid.artifact).exit_code, 0);
        assert_eq!(execute(&changed.artifact).exit_code, 5, "the substituted key changes the complete program's outcome");
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}
