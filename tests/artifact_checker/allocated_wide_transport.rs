//! Wide collection copies must survive rebinding the complete artifact bundle.
use super::*;

#[test]
fn allocated_transport_binds_map_key_and_value_result_slices() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                include_str!("../../examples/language/batches/multi_asset_transfer.cell"),
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let mut widths = std::collections::BTreeSet::new();
        for block in &valid.record.blocks {
            for label in &block.allocated_lifecycle_labels {
                let Some(parts) = label.strip_prefix(".Lallocated_result_").map(|rest| rest.split('_').collect::<Vec<_>>()) else {
                    continue;
                };
                if parts.len() != 6 || parts[5] != "begin" {
                    continue;
                }
                let width = parts[2].parse::<u32>().unwrap();
                if !widths.insert(width) {
                    continue;
                }
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let mut shifted = valid.clone();
                for block in &mut shifted.record.blocks {
                    for candidate in &mut block.allocated_lifecycle_labels {
                        if candidate == label || candidate == &end_label {
                            let mut fields = candidate
                                .strip_prefix(".Lallocated_result_")
                                .unwrap()
                                .split('_')
                                .map(str::to_string)
                                .collect::<Vec<_>>();
                            fields[1] = (fields[1].parse::<u32>().unwrap() + 1).to_string();
                            *candidate = format!(".Lallocated_result_{}", fields.join("_"));
                        }
                    }
                }
                shifted.rebind_sidecars();
                assert_code(&shifted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
        assert_eq!(widths, std::collections::BTreeSet::from([16, 32]));
    }
}

#[test]
fn allocated_transport_binds_wide_copies_and_private_result_buffers() {
    let profiles = [
        ("[u8; 1]", "b\"x\"".to_string(), "", 1),
        ("[u8; 8]", "b\"abcdefgh\"".to_string(), "", 8),
        ("u128", "18446744073709551617".to_string(), "", 16),
        ("[u8; 17]", "b\"abcdefghijklmnopq\"".to_string(), "", 17),
        (
            "Pair",
            "Pair { left: 18446744073709551617, right: 36893488147419103233 }".to_string(),
            "struct Pair { left: u128, right: u128 }",
            32,
        ),
        ("[u8; 64]", format!("b\"{}\"", "x".repeat(64)), "", 64),
    ];
    for (ty, literal, declaration, width) in profiles {
        for bytes in [512, 65536] {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let source = format!(
                    r#"module wide_collection_transport
{declaration}
collections Work {{ limits {{ bytes: {bytes}, allocations: 4 }}
    values: Vec<{ty}> {{ items: 4 }}
}}
fn read(view: &Work) -> {ty} {{ return view.values.get(0) }}
action inspect(witness flag: bool) -> u64 {{
    let value: {ty} = {literal}
    let mut work = Work::new()
    work.values.reserve(4)
    work.values.push(value)
    let copied = work.values.get(0)
    work.values.push(copied)
    let removed = work.values.remove(0)
    work.values.push(removed)
    let popped = work.values.pop()
    work.values.push(popped)
    let borrowed = read(&work)
    work.values.push(borrowed)
    if flag {{ return 0 }}
    let second = work.values.get(0)
    work.values.set(0, second)
    work.drop()
    return 0
}}"#
                );
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
                    .unwrap_or_else(|error| panic!("{ty} bytes={bytes} {diagnostics}: {error}")),
                );
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let mut count = 0;
                let mut bounded_destination = false;
                for block in &valid.record.blocks {
                    for label in &block.allocated_lifecycle_labels {
                        let Some(parts) = label.strip_prefix(".Lallocated_result_").map(|rest| rest.split('_').collect::<Vec<_>>())
                        else {
                            continue;
                        };
                        if parts.len() != 6 || parts[5] != "begin" {
                            continue;
                        }
                        count += 1;
                        assert_eq!(parts[2].parse::<u32>().unwrap(), width);
                        let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                            .unwrap()
                            .range
                            .start;
                        let body = elf
                            .instructions
                            .iter()
                            .filter(|instruction| (block.range.start..end).contains(&instruction.address))
                            .collect::<Vec<_>>();
                        let loads = body.iter().filter(|instruction| instruction.word & 0x707f == 0x4003).collect::<Vec<_>>();
                        let stores = body.iter().filter(|instruction| instruction.word & 0x707f == 0x23).collect::<Vec<_>>();
                        assert_eq!(loads.len(), width as usize);
                        assert_eq!(stores.len(), width as usize);
                        // First and last bytes matter, including high words and
                        // non-word-aligned tails. The scalar publication is a pointer.
                        for instruction in [
                            *loads[0],
                            **loads.last().unwrap(),
                            *stores[0],
                            **stores.last().unwrap(),
                            *body.iter().find(|instruction| instruction.word & 0x707f == 0x3023).unwrap(),
                        ] {
                            let mut changed = valid.clone();
                            let shift = if instruction.word & 0x7f == 0x23 { 7 } else { 20 };
                            changed.replace_machine_word(instruction.address, instruction.word ^ (1 << shift));
                            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        }
                        if count == 1 {
                            let mut missing = valid.clone();
                            for block in &mut missing.record.blocks {
                                block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                            }
                            missing.rebind_sidecars();
                            assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                            let frame =
                                valid.record.entries.iter().find(|entry| entry.id == block.owner_entry).unwrap().frame_size_bytes;
                            for (field, value) in [(2, width + 1), (3, frame), (3, u32::MAX)] {
                                let mut forged = valid.clone();
                                for block in &mut forged.record.blocks {
                                    for candidate in &mut block.allocated_lifecycle_labels {
                                        if candidate == label || candidate == &end_label {
                                            let mut fields = candidate
                                                .strip_prefix(".Lallocated_result_")
                                                .unwrap()
                                                .split('_')
                                                .map(str::to_string)
                                                .collect::<Vec<_>>();
                                            fields[field] = value.to_string();
                                            *candidate = format!(".Lallocated_result_{}", fields.join("_"));
                                        }
                                    }
                                }
                                forged.rebind_sidecars();
                                assert_code(&forged, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                            }
                        }
                        // Rebind both machine copies and their label destination
                        // into scalar, scratch and another result's storage:
                        // exact-byte recognition alone is not enough.
                        if !bounded_destination && stores.iter().all(|instruction| instruction.word >> 15 & 31 == 2) {
                            let output = parts[3].parse::<u32>().unwrap();
                            let other = valid
                                .record
                                .blocks
                                .iter()
                                .filter(|candidate| candidate.owner_entry == block.owner_entry)
                                .flat_map(|candidate| &candidate.allocated_lifecycle_labels)
                                .filter_map(|candidate| candidate.strip_prefix(".Lallocated_result_"))
                                .map(|suffix| suffix.split('_').collect::<Vec<_>>())
                                .filter(|fields| fields.len() == 6 && fields[5] == "begin" && fields[0] != parts[0])
                                .filter_map(|fields| fields[3].parse::<u32>().ok())
                                .find(|destination| *destination != output && *destination + width <= 2047);
                            let pointer = body.iter().find(|instruction| {
                                instruction.word & 0xfffff == ((2 << 15) | (5 << 7) | 0x13) && instruction.word >> 20 == output
                            });
                            if let (Some(pointer), Some(other)) = (pointer, other) {
                                let mut targets =
                                    vec![(0, "result buffer overlaps scalar slots"), (other, "distinct copied results overlap")];
                                let scratch = parts[1].parse::<u32>().unwrap() - 64;
                                if scratch + width <= 2047 {
                                    targets.push((scratch, "overlaps arena/scratch"));
                                }
                                for (destination, reason) in targets {
                                    let mut replacements = stores
                                        .iter()
                                        .enumerate()
                                        .map(|(byte, instruction)| {
                                            let offset = destination + byte as u32;
                                            (
                                                instruction.address,
                                                (instruction.word & !0xfe000f80) | ((offset >> 5) << 25) | ((offset & 31) << 7),
                                            )
                                        })
                                        .collect::<Vec<_>>();
                                    replacements.push((pointer.address, (pointer.word & 0xfffff) | (destination << 20)));
                                    let mut changed = valid.clone();
                                    for block in &mut changed.record.blocks {
                                        for candidate in &mut block.allocated_lifecycle_labels {
                                            if candidate == label || candidate == &end_label {
                                                let mut fields = candidate
                                                    .strip_prefix(".Lallocated_result_")
                                                    .unwrap()
                                                    .split('_')
                                                    .map(str::to_string)
                                                    .collect::<Vec<_>>();
                                                fields[3] = destination.to_string();
                                                *candidate = format!(".Lallocated_result_{}", fields.join("_"));
                                            }
                                        }
                                    }
                                    changed.replace_machine_words(&replacements);
                                    let error = check_bundle_values(
                                        &changed.artifact,
                                        &changed.metadata,
                                        &changed.record,
                                        &changed.source_map,
                                        &CheckerBudgets::default(),
                                    )
                                    .unwrap_err();
                                    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                                    assert!(error.message.contains(reason), "{error}");
                                }
                                bounded_destination = true;
                            }
                        }
                    }
                }
                assert!(count >= 5, "{ty} bytes={bytes} {diagnostics}: {count}");
                assert!(bounded_destination, "missing rebound destination mutation: {ty} bytes={bytes} {diagnostics}");
                let mut erased = valid.clone();
                for block in &mut erased.record.blocks {
                    block.allocated_lifecycle_labels.retain(|label| !label.starts_with(".Lallocated_result_"));
                }
                erased.rebind_sidecars();
                assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}
