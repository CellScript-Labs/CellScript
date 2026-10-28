use super::*;

#[test]
fn collection_queries_bind_owned_and_borrowed_descriptors_and_scalar_results() {
    let checks = r#"
    require work.values.len() == 1
    require work.values.capacity() == 1
    require !work.values.is_empty()
    require work.totals.len() == 1
    require work.totals.capacity() == 1
    require !work.totals.is_empty()
    require work.totals.contains_key(7)
    require work.keys.len() == 1
    require work.keys.capacity() == 1
    require !work.keys.is_empty()
    require work.keys.contains(7)
"#;
    let helper_checks = checks
        .lines()
        .filter_map(|line| line.trim().strip_prefix("require "))
        .map(|condition| format!("if !({condition}) {{ return false }}\n"))
        .collect::<String>();
    for bytes in [512, 65536] {
        let source = format!(
            r#"module checked_collection_queries
collections Work {{ limits {{ bytes: {bytes}, allocations: 8 }}
    values: Vec<u64> {{ items: 4 }}
    totals: OrderedMap<u64, u64> {{ items: 4 }}
    keys: OrderedSet<u64> {{ items: 4 }}
}}
fn inspect(work: &Work) -> bool {{ {helper_checks} return true }}
action inspect_queries() -> u64 {{
    let mut work = Work::new()
    work.values.push(7)
    work.totals.insert(7, 9)
    work.keys.insert(7)
    {checks}
    require inspect(&work)
    return 0
}}"#
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap_or_else(|error| panic!("{bytes}/{diagnostics}: {error}")),
            );
            use ckb_testtool::ckb_types::bytes::Bytes;
            let transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let words = elf
                .instructions
                .iter()
                .map(|instruction| (instruction.address, instruction.word))
                .collect::<std::collections::BTreeMap<_, _>>();
            let mut results = 0;
            let mut descriptors = 0;
            for block in &valid.record.blocks {
                for label in &block.allocated_lifecycle_labels {
                    let query = ["len", "capacity", "empty", "find"].iter().any(|kind| {
                        label.starts_with(&format!(".Lallocated_{kind}_"))
                            || label.starts_with(&format!(".Lallocated_borrowed_{kind}_"))
                    });
                    if !query || !label.ends_with("_begin") {
                        continue;
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
                    let word = words[&end];
                    // Searches nested in insertion do not directly publish a
                    // source contains result; those have a checked parent.
                    if word & 0x01fff07f != (10 << 20) | (2 << 15) | (3 << 12) | 0x23 {
                        continue;
                    }
                    results += 1;
                    let mut zeroed = valid.clone();
                    zeroed.replace_machine_word(end, word & !(31 << 20));
                    assert_code(&zeroed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if !label.contains("_empty_") {
                        assert_ne!(
                            ckb_script_runner::execute_cellscript_script(&zeroed.artifact, &transaction).exit_code,
                            0,
                            "{label}"
                        );
                    }
                    let mut redirected = valid.clone();
                    redirected.replace_machine_word(end, word ^ (8 << 7));
                    assert_code(&redirected, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if label.starts_with(".Lallocated_len_")
                        || label.starts_with(".Lallocated_capacity_")
                        || label.starts_with(".Lallocated_empty_")
                    {
                        descriptors += 1;
                        let mut producer = valid.clone();
                        producer.replace_machine_word(end - 4, 0x13);
                        assert_code(&producer, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        let mut missing = valid.clone();
                        for block in &mut missing.record.blocks {
                            block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                        }
                        missing.rebind_sidecars();
                        assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
            }
            assert_eq!(results, 22, "owned and borrowed Vec/Map/Set queries {bytes}/{diagnostics}");
            assert_eq!(descriptors, 9);
        }
    }
}
