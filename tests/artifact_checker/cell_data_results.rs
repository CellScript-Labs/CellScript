use super::*;

#[test]
fn copied_cell_data_results_reject_intervening_body_writes() {
    for nested in [false, true] {
        let tail = if nested { ", messages: NestedValues<Vec<u8>, 2, 8, 32>" } else { "" };
        let source = format!(
            "module copied_data_writes\nresource Token has store {{ value: Hash{tail} }}\n\
             fn same(before: InputView<Token>, after: OutputView<Token>, delta: u64) -> bool {{\n\
             let mut current = before.data.value\nlet original = current\ncurrent = after.data.value\n\
             let adjusted = delta + 1\nif adjusted <= delta {{ return false }}\nreturn original == current\n}}\n\
             action inspect() -> u64 {{ verification\n\
             require same(ckb::input<Token>(0), ckb::output<Token>(0), 4)\nreturn 0\n}}"
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile(
                &source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap();
            // Reassigning current must preserve the earlier value in original.
            // Exercise real Cell data for both layouts before mutating code.
            use cellscript::nested_builder::{encode_nested_cell_data, NestedCellFieldInput as Field};
            use ckb_testtool::ckb_types::bytes::Bytes;
            let encode = |value: &[u8]| {
                if nested {
                    encode_nested_cell_data(
                        &compiled.metadata,
                        "Token",
                        &std::collections::BTreeMap::from([
                            ("value".into(), Field::FixedBytes(value)),
                            ("messages".into(), Field::Nested(&[])),
                        ]),
                    )
                    .unwrap()
                } else {
                    value.to_vec()
                }
            };
            for equal in [false, true] {
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                fixture.current_type_script_input_indices = vec![0];
                fixture.inputs[0].data = encode(&[0xab; 32]).into();
                fixture.outputs[0].data = encode(&[if equal { 0xab } else { 0 }; 32]).into();
                let execution =
                    ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
                assert_eq!(execution.exit_code, if equal { 0 } else { 5 }, "{diagnostics} nested={nested} equal={equal}");
            }
            let valid = Fixture::from_result(compiled);
            let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("same")).unwrap();
            let addition = entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add"))
                .unwrap();
            let slot = entry.locals.iter().find(|local| local.id == addition.destinations[0]).unwrap().source_id as u32 * 8;
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let store = elf
                .instructions
                .iter()
                .find(|instruction| {
                    let word = instruction.word;
                    word & 0x707f == 0x3023
                        && word >> 15 & 31 == 2
                        && ((word >> 7 & 31) | (word >> 25 << 5)) == slot
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
                })
                .expect("arithmetic result has a store in the helper");
            let last_copy_end = valid
                .record
                .blocks
                .iter()
                .filter(|block| {
                    block.owner_entry == entry.id
                        && block
                            .exact_role_labels
                            .iter()
                            .any(|label| label.starts_with(".Lexact_role_data_") && label.ends_with("_done"))
                })
                .map(|block| block.range.start)
                .max()
                .unwrap();
            assert!(store.address >= last_copy_end, "mutate an ordinary store after both checked producers");
            let buffers = valid
                .record
                .blocks
                .iter()
                .filter(|block| block.owner_entry == entry.id)
                .flat_map(|block| &block.exact_role_labels)
                .filter_map(|label| {
                    let fields = label.strip_prefix(".Lexact_role_data_")?.split('_').collect::<Vec<_>>();
                    (matches!(fields.len(), 4 | 6) && fields[2] != "0").then(|| fields[2].parse::<u32>().unwrap())
                })
                .collect::<Vec<_>>();
            assert_eq!(buffers.len(), 2);
            for buffer in buffers {
                assert!(buffer + 40 < 2048);
                for width in 0..=3 {
                    let mut offsets = vec![buffer, buffer + 31];
                    if width != 0 {
                        offsets.push(buffer - 1);
                    }
                    for offset in offsets {
                        let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                            | ((offset & 31) << 7)
                            | (width << 12)
                            | ((offset >> 5) << 25);
                        let mut changed = valid.clone();
                        changed.replace_machine_word(store.address, word);
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
            }
        }
    }
}

#[test]
fn copied_cell_data_results_cannot_alias_after_coherent_rebinding() {
    for (ty, width) in [("Hash", 32), ("u128", 16), ("[u8; 9]", 9), ("[u8; 256]", 256)] {
        for nested in [false, true] {
            let tail = if nested { ", messages: NestedValues<Vec<u8>, 2, 8, 32>" } else { "" };
            let source = format!(
                "module copied_data_results\nresource Token has store {{ id: u64, value: {ty}{tail} }}\n\
                 action inspect() -> u64 {{ verification\n\
                 let before = ckb::input<Token>(0).data.value\n\
                 let after = ckb::output<Token>(0).data.value\n\
                 require before == after\nreturn 0\n}}"
            );
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(
                        &source,
                        CompileOptions {
                            edition: NEXT_EDITION,
                            diagnostics,
                            target: Some("riscv64-elf".into()),
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                );
                let mut results = Vec::new();
                for block in &valid.record.blocks {
                    for label in &block.exact_role_labels {
                        let Some(suffix) = label.strip_prefix(".Lexact_role_data_") else { continue };
                        let parts = suffix.split('_').collect::<Vec<_>>();
                        if !matches!(parts.len(), 4 | 6) || parts[2] == "0" {
                            continue;
                        }
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|candidate| candidate.exact_role_labels.contains(&format!("{label}_done")))
                            .unwrap()
                            .range
                            .start;
                        results.push((
                            label.clone(),
                            block.range.start..end,
                            parts[2].parse::<i32>().unwrap(),
                            (parts.len() == 6).then(|| parts[3].parse::<i32>().unwrap()),
                        ));
                    }
                }
                assert_eq!(results.len(), 2);
                let (label, range, original, _) = &results[1];
                assert!(*original + width < 2048, "fixture uses directly encoded stack offsets");
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                for replacement in [results[0].2, results[0].2 + width - 1].into_iter().chain(results[0].3) {
                    let rewrites = elf
                        .instructions
                        .iter()
                        .filter(|instruction| range.contains(&instruction.address) && instruction.word >> 15 & 31 == 2)
                        .filter_map(|instruction| {
                            let word = instruction.word;
                            let opcode = word & 0x7f;
                            let offset = match opcode {
                                0x23 => (((word >> 25) << 5) | ((word >> 7) & 31)) as i32,
                                0x13 => (word as i32) >> 20,
                                _ => return None,
                            };
                            (*original..*original + width).contains(&offset).then(|| {
                                let offset = offset - original + replacement;
                                (
                                    instruction.address,
                                    if opcode == 0x23 { replace_s_immediate(word, offset) } else { replace_i_immediate(word, offset) },
                                )
                            })
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(rewrites.len(), width as usize + 1, "rewrite every byte store and the result pointer");
                    let mut changed = valid.clone();
                    let mut parts =
                        label.strip_prefix(".Lexact_role_data_").unwrap().split('_').map(str::to_string).collect::<Vec<_>>();
                    parts[2] = replacement.to_string();
                    let renamed = format!(".Lexact_role_data_{}", parts.join("_"));
                    for block in &mut changed.record.blocks {
                        for marker in &mut block.exact_role_labels {
                            if marker == label || marker == &format!("{label}_done") {
                                *marker = marker.replace(label, &renamed);
                            }
                        }
                        if let Some(marker) = &mut block.machine_label
                            && (marker == label || marker == &format!("{label}_done"))
                        {
                            *marker = marker.replace(label, &renamed);
                        }
                    }
                    changed.replace_machine_words(&rewrites);
                    if ty == "Hash" && !nested && replacement == results[0].2 {
                        use ckb_testtool::ckb_types::bytes::Bytes;
                        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                        fixture.current_type_script_input_indices = vec![0];
                        let mut input = 42u64.to_le_bytes().to_vec();
                        input.extend_from_slice(&[0xab; 32]);
                        let mut output = 42u64.to_le_bytes().to_vec();
                        output.extend_from_slice(&[0; 32]);
                        fixture.inputs[0].data = input.into();
                        fixture.outputs[0].data = output.into();
                        let run = |artifact: &[u8]| {
                            ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(artifact), &fixture)
                        };
                        assert_eq!(run(&valid.artifact).exit_code, 5, "different hashes must reject");
                        assert_eq!(run(&changed.artifact).exit_code, 0, "the coherent alias changes the program's comparison");
                    }
                    let error = check_bundle_values(
                        &changed.artifact,
                        &changed.metadata,
                        &changed.record,
                        &changed.source_map,
                        &CheckerBudgets::default(),
                    )
                    .expect_err("coherently rebound copied Cell data results must not alias");
                    assert_eq!(
                        error.code,
                        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                        "{diagnostics} {ty} nested={nested}"
                    );
                    assert!(error.message.contains("copied Cell data results overlap"), "{error}");
                }
            }
        }
    }
}
