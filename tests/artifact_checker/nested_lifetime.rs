#[test]
fn nested_lifetime_rejects_ordinary_full_partial_and_aliased_stack_clobbers() {
    let source = "module nested_lifetime;\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness at: u64) -> u64 { \nlet bytes = values[at];\nlet marker = at + 7;\nrequire marker > at;\nreturn bytes.len() + values.len(); }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
        let frame = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap().frame_size_bytes as i32;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let sites = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| {
                block.nested_read_labels.iter().filter(|label| !label.ends_with("_done")).map(|label| {
                    let done = format!("{label}_done");
                    let end = valid.record.blocks.iter().find(|block| block.nested_read_labels.contains(&done)).unwrap().range.start;
                    (block.range.start, end)
                })
            })
            .collect::<Vec<_>>();
        let first_end = sites.iter().min_by_key(|site| site.0).unwrap().1;
        let store = elf
            .instructions
            .iter()
            .find(|instruction| {
                instruction.address >= first_end
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                    && !sites.iter().any(|(start, end)| *start <= instruction.address && instruction.address < *end)
                    && instruction.word & 0x707f == 0x3023
                    && instruction.word >> 15 & 31 == 2
            })
            .unwrap();
        let mut slots = typed
            .locals
            .iter()
            .filter(|local| local.ty.starts_with("Nested"))
            .map(|local| local.source_id as i32 * 8)
            .collect::<std::collections::BTreeSet<_>>();
        slots.extend(
            elf.instructions
                .iter()
                .filter(|instruction| {
                    sites.iter().any(|(start, end)| *start <= instruction.address && instruction.address < *end)
                        && instruction.word & 0x707f == 0x3003
                        && instruction.word >> 15 & 31 == 2
                        && instruction.word >> 7 & 31 == 11
                })
                .map(|instruction| (instruction.word as i32) >> 20),
        );
        assert!(slots.len() >= 4, "root/derived pointers and independent sizes");
        for slot in slots {
            for (base, offset, kind) in [(2, slot, 3), (2, slot + 7, 0), (8, slot - frame, 3)] {
                assert!((-2048..2048).contains(&offset));
                let word = (store.word & !0xfe0fff80)
                    | ((offset as u32 & 0xfe0) << 20)
                    | ((offset as u32 & 31) << 7)
                    | (base << 15)
                    | (kind << 12);
                let mut changed = valid.clone();
                changed.replace_machine_word(store.address, word);
                let error = check_bundle_values(
                    &changed.artifact,
                    &changed.metadata,
                    &changed.record,
                    &changed.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.to_string().contains("unverified write overlaps"), "{error}");
            }
        }
    }
}

#[test]
fn nested_lifetime_rejects_a_rebound_branch_that_skips_the_view_definition() {
    let source = "module nested_dominance;\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness at: u64) -> u64 { \nrequire at < 2;\nlet bytes = values[at];\nrequire bytes.len() > 0;\nreturn bytes.len(); }";
    let cell_source =
        NESTED_CELL_CODEC_SOURCE.replace("let values = before.entries;", "require before.id == 42;\nlet values = before.entries;");
    for (source, producer_prefix) in [(source, ".Lnested_use_index_"), (cell_source.as_str(), ".Lnested_use_cellfield_")] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            let producer = valid
                .record
                .blocks
                .iter()
                .find(|block| {
                    block.nested_read_labels.iter().any(|label| label.starts_with(producer_prefix) && !label.ends_with("_done"))
                })
                .unwrap();
            let consumer = valid
                .record
                .blocks
                .iter()
                .find(|block| {
                    block.nested_read_labels.iter().any(|label| label.starts_with(".Lnested_use_length_") && !label.ends_with("_done"))
                })
                .unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let flow = elf
                .control_flow
                .iter()
                .find(|flow| {
                    flow.address < producer.range.start
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == producer.owner_entry && block.range.contains(flow.address))
                        && valid.record.verifier_failure_exits.iter().any(|exit| exit.code == 5 && exit.address == flow.target)
                })
                .unwrap();
            let block = valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap();
            let word = elf.instructions.iter().find(|instruction| instruction.address == flow.address).unwrap().word;
            assert_eq!(word & 0x7f, 0x63);
            let offset = consumer.range.start as i64 - flow.address as i64;
            assert!((-4096..4096).contains(&offset));
            let immediate = offset as u32;
            let branch = (word & 0x01fff07f)
                | (((immediate >> 12) & 1) << 31)
                | (((immediate >> 5) & 0x3f) << 25)
                | (((immediate >> 1) & 15) << 8)
                | (((immediate >> 11) & 1) << 7);
            let mut changed = valid.clone();
            let edge =
                changed.record.edges.iter_mut().find(|edge| edge.from == block.id && edge.kind == EdgeKind::ConditionalTaken).unwrap();
            edge.to = consumer.id.clone();
            // Rebind graph reachability too: the test must reach the independent
            // definition proof rather than fail on stale producer bookkeeping.
            let mut reachable = std::collections::BTreeSet::new();
            let mut pending = changed.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
            while let Some(id) = pending.pop() {
                if reachable.insert(id.clone()) {
                    pending.extend(changed.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
                }
            }
            for block in &mut changed.record.blocks {
                block.reachable = reachable.contains(&block.id);
            }
            changed.record.canonicalize();
            changed.replace_machine_word(flow.address, branch);
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
            assert!(error.to_string().contains("before its source definition"), "{error}");
        }
    }
}

#[test]
fn nested_lifetime_accepts_caller_owned_fixed_results() {
    for count in [0, 1, 7, 8, 9] {
        let params = (0..count).map(|i| format!("p{i}: u64")).collect::<Vec<_>>().join(", ");
        let args = vec!["at"; count].join(", ");
        let value = if count == 0 { "0".to_string() } else { format!("p{}", count - 1) };
        let source = format!("module nested_result;\nstruct Pair {{ left: u64, right: u64 }}\nfn make({params}) -> Pair {{ new Pair {{ left: {value}, right: {value} + 1 }} }}\nfn wrap(at: u64) -> Pair {{ return make({args}); }}\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 {{\nlet bytes = values[0];\nlet pair = wrap(bytes.len());\nrequire pair.right > pair.left;\nreturn bytes.len(); }}");
        let direct = source.replace("wrap(bytes.len())", &format!("make({})", vec!["bytes.len()"; count].join(", ")));
        for source in [source, direct] {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                        .unwrap_or_else(|error| panic!("{count} args, {diagnostics}: {error}")),
                );
                if count == 9 && !diagnostics.is_standard() {
                    let mut unsorted = valid.clone();
                    let block = unsorted.record.blocks.iter_mut().find(|block| block.nested_read_labels.len() > 1).unwrap();
                    block.nested_read_labels.swap(0, 1);
                    unsorted.rebind_sidecars();
                    assert_code(&unsorted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
        }
    }
}

const NESTED_CELL_CODEC_SOURCE: &str = r#"
module nested_cell_codec;
resource Document has store, consume, create {
    id: u64,
    entries: NestedValues<Vec<u8>, 2, 3, 32>,
}
action roll(input before: Document, witness recipient: Address) -> after: Document {

    let values = before.entries;
    require values.len() == 1;
    let id = before.id;
    consume before;
    create after = Document { id: id, entries: values, } with_lock(recipient)
}
"#;

fn nested_cell_two_fields_source() -> String {
    NESTED_CELL_CODEC_SOURCE
        .replace(
            "entries: NestedValues<Vec<u8>, 2, 3, 32>,",
            "entries: NestedValues<Vec<u8>, 2, 3, 32>, spare: NestedValues<Vec<u8>, 2, 3, 32>,",
        )
        .replace("let id = before.id;", "let spare = before.spare;\nrequire spare.len() == 1;\nlet id = before.id;")
        .replace("entries: values, }", "entries: values, spare: spare, }")
}

#[test]
fn nested_output_comparisons_bind_spans_expected_values_and_byte_loops() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &nested_cell_two_fields_source(),
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        let mut sites = 0;
        for block in &valid.record.blocks {
            for label in block.nested_output_labels.iter().filter(|label| !label.ends_with("_done")) {
                sites += 1;
                let end_name = format!("{label}_done");
                let end = valid.record.blocks.iter().find(|block| block.nested_output_labels.contains(&end_name)).unwrap().range.start;
                for instruction in elf.instructions.iter().filter(|i| (block.range.start..end).contains(&i.address)) {
                    let word = instruction.word;
                    let changed_word = match word & 0x7f {
                        0x63 => word ^ (1 << 12),
                        0x03 => replace_i_immediate(word, ((word as i32) >> 20) + 1),
                        0x13 | 0x33 => word ^ (1 << 15),
                        _ => continue,
                    };
                    let mut changed = valid.clone();
                    changed.replace_machine_word(instruction.address, changed_word);
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    mutations += 1;
                }
            }
        }
        assert_eq!(sites, 3);
        assert!(mutations > 70);
        let mut missing = valid.clone();
        for block in &mut missing.record.blocks {
            block.nested_output_labels.clear();
        }
        missing.rebind_sidecars();
        assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn nested_output_fixed_fields_bind_all_widths_and_literal_bytes() {
    let types = ["u8", "u16", "u32", "u64", "u128", "Hash", "Address", "[u8; 1]", "[u8; 8]", "[u8; 9]", "[u8; 256]"];
    let declarations = types.iter().enumerate().map(|(index, ty)| format!("f{index}: {ty}, ")).collect::<String>();
    let assignments = types.iter().enumerate().map(|(index, _)| format!("f{index}: before.f{index}, ")).collect::<String>();
    let copied = NESTED_CELL_CODEC_SOURCE
        .replace("id: u64,", &format!("id: u64, {declarations}"))
        .replace("id: id, entries: values", &format!("id: id, {assignments}entries: values"));
    let literals = NESTED_CELL_CODEC_SOURCE
        .replace("id: u64,", "id: u64, big: u128, hash: Hash, owner: Address, pad: [u8; 9],")
        .replace(
        "id: id, entries: values",
        "id: 18446744073709551615, big: 0 as u128, hash: Hash::zero(), owner: Address::zero(), pad: b\"abcdefghi\", entries: values",
    );
    for source in [copied, literals] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let mut mutations = 0;
            for block in &valid.record.blocks {
                for label in block.nested_output_labels.iter().filter(|label| !label.ends_with("_done")) {
                    let end_name = format!("{label}_done");
                    let end =
                        valid.record.blocks.iter().find(|block| block.nested_output_labels.contains(&end_name)).unwrap().range.start;
                    let body = elf.instructions.iter().filter(|i| (block.range.start..end).contains(&i.address)).collect::<Vec<_>>();
                    // Both ends of each field's reads, expected-slot load and
                    // width constant; include high-byte and full-u64 paths.
                    let mut selected = std::collections::BTreeMap::new();
                    for instruction in body
                        .iter()
                        .filter(|i| matches!(i.word & 0x7f, 0x03 | 0x13 | 0x33 | 0x63))
                        .take(4)
                        .chain(body.iter().rev().filter(|i| matches!(i.word & 0x7f, 0x03 | 0x13 | 0x33 | 0x63)).take(8))
                        .chain(
                            body.iter().filter(|i| matches!(i.word & 0x7f, 0x03 | 0x13 | 0x33) && matches!(i.word >> 7 & 31, 11 | 13)),
                        )
                    {
                        selected.insert(instruction.address, instruction.word);
                    }
                    for (address, word) in selected {
                        let replacement = match word & 0x7f {
                            0x63 => word ^ (1 << 12),
                            0x03 => replace_i_immediate(word, ((word as i32) >> 20) + 1),
                            _ => word ^ (1 << 15),
                        };
                        let mut changed = valid.clone();
                        changed.replace_machine_word(address, replacement);
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        mutations += 1;
                    }
                }
            }
            assert!(mutations > 50);
        }
    }
}

#[test]
fn nested_output_small_byte_arguments_keep_inline_abi_through_aliases() {
    for width in [1, 8] {
        for alias in [false, true] {
            let source = NESTED_CELL_CODEC_SOURCE
                .replace("id: u64,", &format!("id: u64, tiny: [u8; {width}],"))
                .replace("witness recipient: Address", &format!("witness recipient: Address, witness bytes: [u8; {width}]"))
                .replace("consume before;", if alias { "let copy = bytes;\nconsume before;" } else { "consume before;" })
                .replace(
                    "id: id, entries: values",
                    if alias { "id: id, tiny: copy, entries: values" } else { "id: id, tiny: bytes, entries: values" },
                );
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                        .unwrap(),
                );
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let start = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| {
                        block.nested_output_labels.iter().any(|label| {
                            !label.ends_with("_done")
                                && label.strip_prefix(".Lnested_output_check_").unwrap().split('_').nth(1) == Some("1")
                        })
                    })
                    .unwrap();
                let label = start.nested_output_labels.iter().find(|label| !label.ends_with("_done")).unwrap();
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.nested_output_labels.contains(&format!("{label}_done")))
                    .unwrap()
                    .range
                    .start;
                let pointer = elf
                    .instructions
                    .iter()
                    .find(|i| {
                        (start.range.start..end).contains(&i.address)
                            && i.word & 0x707f == 0x13
                            && i.word >> 7 & 31 == 11
                            && i.word >> 15 & 31 == 2
                    })
                    .unwrap();
                let mut changed = valid.clone();
                changed.replace_machine_word(pointer.address, (pointer.word & !0x707f) | 0x3003);
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn nested_output_comparisons_cannot_be_bypassed_on_a_rebound_success_path() {
    let standalone = NESTED_CELL_CODEC_SOURCE
        .replace("-> after: Document", "-> u64")
        .replace("create after = Document", "create Document")
        .replace("with_lock(recipient)", "with_lock(recipient);\nreturn 0;");
    for (source, skip_comparison, message) in [
        (NESTED_CELL_CODEC_SOURCE, true, "bypasses a declared nested successor comparison"),
        (standalone.as_str(), false, "precedes its loaded Cell or expected value definition"),
    ] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            let end = valid
                .record
                .blocks
                .iter()
                .find(|block| block.nested_output_labels.iter().any(|label| label.ends_with("_done") == skip_comparison))
                .unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let flow = elf
                .control_flow
                .iter()
                .find(|flow| {
                    flow.address < end.range.start
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == end.owner_entry && block.range.contains(flow.address))
                        && valid.record.verifier_failure_exits.iter().any(|exit| exit.code == 5 && exit.address == flow.target)
                })
                .unwrap();
            let block = valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap();
            let word = elf.instructions.iter().find(|i| i.address == flow.address).unwrap().word;
            assert_eq!(word & 0x7f, 0x63);
            let offset = end.range.start as i64 - flow.address as i64;
            assert!((-4096..4096).contains(&offset));
            let immediate = offset as u32;
            let branch = (word & 0x01fff07f)
                | (((immediate >> 12) & 1) << 31)
                | (((immediate >> 5) & 0x3f) << 25)
                | (((immediate >> 1) & 15) << 8)
                | (((immediate >> 11) & 1) << 7);
            let mut changed = valid.clone();
            changed
                .record
                .edges
                .iter_mut()
                .find(|edge| edge.from == block.id && edge.kind == EdgeKind::ConditionalTaken)
                .unwrap()
                .to = end.id.clone();
            let mut reachable = std::collections::BTreeSet::new();
            let mut pending = changed.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
            while let Some(id) = pending.pop() {
                if reachable.insert(id.clone()) {
                    pending.extend(changed.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
                }
            }
            for block in &mut changed.record.blocks {
                block.reachable = reachable.contains(&block.id);
            }
            changed.record.canonicalize();
            changed.replace_machine_word(flow.address, branch);
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
            assert!(error.to_string().contains(message), "{error}");
        }
    }
}
