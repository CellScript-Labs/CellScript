use super::*;

#[test]
fn bounded_cell_read_helpers_reject_rebound_capacity_length_and_pointer_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (call, name, capacity_word, guard_kind, guard_registers, output_word) in [
            ("ckb::cell_data_hash_field(source::input(0))", "__ckb_cell_data_hash_field", 0x02000293, 0x1063, (7, 0), 0x02813f83),
            ("ckb::cell_data_bytes32(source::input(0), 7)", "__ckb_cell_data_bytes32", 0x02000293, 0x6063, (7, 6), 0x00813503),
        ] {
            let source =
                format!("module read_bounds\naction inspect() -> u64 {{ let value = {call} require value == value return 0 }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let owner = format!("runtime:{name}");
            let blocks = valid.record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
            let words = elf
                .instructions
                .iter()
                .filter(|word| blocks.iter().any(|block| block.range.contains(word.address)))
                .collect::<Vec<_>>();
            let capacity = words.iter().find(|word| word.word == capacity_word).unwrap();
            let guard = words
                .iter()
                .find(|word| word.word & 0x707f == guard_kind && (word.word >> 15 & 31, word.word >> 20 & 31) == guard_registers)
                .unwrap();
            let output = words.iter().find(|word| word.word == output_word).unwrap();
            let returned_size = words.iter().rev().find(|word| word.word & 0x707f == 0x3023 && word.word >> 15 & 31 != 2).unwrap();
            valid.check().unwrap();
            let restore = words.iter().rev().find(|word| word.word & 0x000f_ffff == 0x00013083).unwrap();
            let syscall = words.iter().find(|word| word.word == 0x73).unwrap();
            let mut mutations = vec![
                (capacity.address, capacity.word.wrapping_add(1 << 20)),
                (guard.address, guard.word ^ (1 << 12)), // signed/opposite relation cannot authorize the read
                (guard.address, 0x00000013),             // omitted bound after outer hashes are rebound
                (output.address, output.word.wrapping_add(8 << 20)),
                (returned_size.address, returned_size.word ^ (1 << 20)),
                (restore.address, restore.word.wrapping_sub(8 << 20)),
                (syscall.address, 0x00000013),
            ];
            if name == "__ckb_cell_data_hash_field" {
                for (before, after) in [(0x00100793, 0x00300793), (0x02000393, 0x02100393), (0x00400f13, 0x00500f13)] {
                    let instruction = words.iter().find(|word| word.word == before).unwrap();
                    mutations.push((instruction.address, after));
                }
            }
            for (address, word) in mutations {
                let mut rebound = valid.clone();
                rebound.replace_machine_words(&[(address, word)]);
                assert!(rebound.check().is_err(), "{name} mutation at {address:#x}, {diagnostics}");
            }
        }
    }
}

#[test]
fn bounded_large_incoming_arguments_reject_rebound_load_offsets() {
    let parameters = (0..270).map(|index| format!("p{index}: u64")).collect::<Vec<_>>().join(", ");
    let arguments = (0..270).map(|index| (index + 100).to_string()).collect::<Vec<_>>().join(", ");
    let source = format!(
        "module large_incoming_args\ncollections Work {{ limits {{ bytes: 128, allocations: 1 }}\nvalues: Vec<u64> {{ items: 4 }} }}\nfn last({parameters}, work: &Work) -> u64 {{ return p0 + p263 + p264 + p269 + work.values.len() }}\naction inspect() -> u64 {{ let work = Work::new() require last({arguments}, &work) == 1196 return 0 }}"
    );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        valid.check().unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let entry = valid.record.entries.iter().find(|entry| entry.name == "last").unwrap();
        let blocks = valid.record.blocks.iter().filter(|block| block.owner_entry == entry.id).collect::<Vec<_>>();
        // Large incoming loads materialize FP + offset into t6, then load t0.
        // Change only that load to the adjacent argument, rebinding all hashes.
        let load = elf
            .instructions
            .windows(2)
            .find(|pair| {
                blocks.iter().any(|block| block.range.contains(pair[0].address))
                && pair[0].word == 0x01f40fb3 // add t6, fp, t6
                && pair[1].word == 0x000fb283 // ld t0, 0(t6)
            })
            .expect("fixture must retain a materialized large incoming argument");
        let mut rebound = valid.clone();
        rebound.replace_machine_words(&[(load[1].address, load[1].word | (8 << 20))]);
        assert_code(&rebound, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn aligned_stack_discipline_rejects_rebound_balanced_but_misaligned_frames() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                "module stack_alignment\nfn step(value: u64) -> u64 { return value + 1 }\naction inspect(witness value: u64) -> u64 { return step(value) }",
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        valid.check().unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let entry = valid.record.entries.iter().find(|entry| entry.name == "step").unwrap();
        let owned = valid.record.blocks.iter().filter(|block| block.owner_entry == entry.id).collect::<Vec<_>>();
        let adjustments = elf.stack_adjustments.iter().filter(|site| owned.iter().any(|block| block.range.contains(site.address)));
        let mut rebound = valid.clone();
        let mut replacements = Vec::new();
        for site in adjustments {
            let instruction = elf.instructions.iter().find(|instruction| instruction.address == site.address).unwrap();
            assert_eq!(instruction.word & 0x000f_ffff, 0x00010113);
            let delta = site.delta + if site.delta < 0 { -8 } else { 8 };
            assert!((-2048..=2047).contains(&delta));
            replacements.push((site.address, (instruction.word & 0x000f_ffff) | ((delta as u32 & 0xfff) << 20)));
        }
        assert_eq!(replacements.len(), 2);
        // Keep the declared extent generously aligned and both adjustments
        // balanced: only machine alignment, not stale hashes/depth, rejects.
        rebound.record.entries.iter_mut().find(|item| item.id == entry.id).unwrap().frame_size_bytes += 16;
        for block in &mut rebound.record.blocks {
            if block.owner_entry == entry.id {
                block.frame_size_bytes += 16;
            }
        }
        rebound.replace_machine_words(&replacements);
        assert_code(&rebound, CheckerRejectionCode::V2407AbiOrStackInvalid);
    }
}

#[test]
fn script_out_point_and_witness_hash_effects_reject_rebound_helper_mutations() {
    let source = r#"module bounded_hash_effects
resource Token has store { amount: u64 }
fn inspect(view: InputView<Token>) -> bool {
    let lock = view.lock
    let script = view.type_script
    let point = view.out_point
    let code = lock.code_hash
    let args = lock.args32
    let type_code = script.code_hash
    let type_args = script.args32
    let transaction = point.tx_hash
    let witness = witness::args(0)
    let witness_lock = witness::lock_exact32(witness)
    let witness_input = witness::input_type_exact32(witness)
    let witness_output = witness::output_type_exact32(witness)
    return view.capacity > 0 && code == lock.code_hash && args == lock.args32
        && type_code == script.code_hash && type_args == script.args32
        && transaction == point.tx_hash
        && witness_lock == witness::lock_exact32(witness) && witness_input == witness::input_type_exact32(witness)
        && witness_output == witness::output_type_exact32(witness)
}
action check() -> u64 {
    require inspect(ckb::input<Token>(0))
    return 0
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_with_executable_surface_policy(
            source,
            CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let valid = Fixture::from_result(compiled);
        assert_bounded_admitted(&valid);
        for kind in ["ScriptView", "OutPoint"] {
            let handles = valid.metadata["runtime"]["transaction_view_handles"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|handle| handle["scope_name"] == "inspect" && handle["handle_type"] == kind)
                .collect::<Vec<_>>();
            assert!(!handles.is_empty(), "{kind} helper projections must retain provenance");
            for handle in handles {
                assert_eq!(handle["provenance"]["source"]["resolved_source"], "SourceView");
                assert_eq!(handle["provenance"]["source"]["origin"], "inherited-source-view");
                assert_eq!(handle["provenance"]["index"]["kind"], "dynamic");
                assert_eq!(handle["provenance"]["index"]["max_inclusive"], u32::MAX);
            }
        }
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for name in [
            "__ckb_cell_lock_code_hash",
            "__ckb_cell_type_code_hash",
            "__ckb_cell_lock_args32",
            "__ckb_cell_type_args32",
            "__ckb_input_out_point_tx_hash",
        ] {
            let owner = format!("runtime:{name}");
            let body = valid.record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
            assert!(!body.is_empty(), "{name} must be exercised at {diagnostics}");
            let out_point = name == "__ckb_input_out_point_tx_hash";
            let copy_counter_limit = if out_point { 6 } else { 7 };
            let byte_source = if out_point { 29 } else { 30 };
            let byte_destination = if out_point { 30 } else { 31 };
            let field = if out_point {
                0
            } else if name.contains("_lock_") {
                2
            } else {
                4
            };
            let mutations = [
                // Extend the caller-visible copy from 32 to 33 bytes.
                ((32 << 20) | (copy_counter_limit << 7) | 0x13, 1 << 20),
                // Redirect the byte write away from the checked output pointer.
                ((byte_source << 20) | (byte_destination << 15) | 0x23, 1 << 15),
                // Select a different syscall field while retaining all metadata.
                ((field << 20) | (15 << 7) | 0x13, 1 << 20),
            ];
            for (expected, bit) in mutations {
                let instruction = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        instruction.word == expected && body.iter().any(|block| block.range.contains(instruction.address))
                    })
                    .unwrap_or_else(|| panic!("missing {expected:#x} in {name}"));
                let mut rebound = valid.clone();
                rebound.replace_machine_word(instruction.address, instruction.word ^ bit);
                assert_code(&rebound, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
        for (name, field) in
            [("__ckb_witness_lock_exact32", 1), ("__ckb_witness_input_type_exact32", 2), ("__ckb_witness_output_type_exact32", 3)]
        {
            let owner = format!("runtime:{name}");
            let body = valid.record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
            assert!(!body.is_empty(), "{name} must be exercised at {diagnostics}");
            for (expected, bit) in [
                // Change the resolver's requested field or maximum size.
                ((field << 20) | (11 << 7) | 0x13, 1 << 20),
                ((32 << 20) | (12 << 7) | 0x13, 1 << 20),
                // Redirect the syscall output to a different saved slot.
                ((2 << 15) | (10 << 7) | 0x3003, 8 << 20),
            ] {
                let instruction = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        instruction.word == expected && body.iter().any(|block| block.range.contains(instruction.address))
                    })
                    .unwrap_or_else(|| panic!("missing {expected:#x} in {name}"));
                let mut rebound = valid.clone();
                rebound.replace_machine_word(instruction.address, instruction.word ^ bit);
                assert_code(&rebound, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn runtime_hash_helpers_preserve_aligned_frames_and_live_cell_views() {
    use ckb_testtool::ckb_types::bytes::Bytes;

    let source = r#"module aligned_hash_helpers
resource Token has store { amount: u64 }
fn hashes(view: InputView<Token>) -> bool {
    let lock = view.lock_hash
    let current = ckb::current_script_hash()
    let transaction = ckb::transaction_hash()
    return view.capacity > 0 && ckb::script_hash(current) == view.type_hash
        && lock == view.lock_hash
        && transaction == ckb::transaction_hash()
        && transaction != Hash::zero()
}
action inspect() -> u64 {
    require hashes(ckb::input<Token>(0))
    return 0
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_with_executable_surface_policy(
            source,
            CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let valid = Fixture::from_result(compiled);
        assert_bounded_admitted(&valid);
        let helper = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "hashes").unwrap();
        for target in ["__ckb_cell_lock_hash", "__ckb_cell_type_hash"] {
            let calls = helper
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .filter(|operation| operation.call.as_ref().is_some_and(|call| call.target == target))
                .collect::<Vec<_>>();
            assert!(!calls.is_empty());
            for call in calls {
                let local = helper.locals.iter().find(|local| call.destinations.as_slice() == [local.id]).unwrap();
                assert_eq!(local.ty, "ScriptHash");
            }
        }
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.current_type_script_input_indices = vec![0];
        transaction.inputs[0].data = vec![0; 8].into();
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);

        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for name in ["__ckb_current_script_hash", "__ckb_transaction_hash"] {
            let entry = valid.record.entries.iter().find(|entry| entry.name == name).unwrap();
            assert_eq!(entry.frame_size_bytes, 16);
            let block = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap();
            let adjustment = elf.stack_adjustments.iter().find(|adjustment| block.range.contains(adjustment.address)).unwrap();
            assert_eq!(adjustment.delta, -16);
            let instruction = elf.instructions.iter().find(|instruction| instruction.address == adjustment.address).unwrap();
            let mut misaligned = valid.clone();
            misaligned.replace_machine_word(instruction.address, (instruction.word & 0x000f_ffff) | (0xfe8 << 20));
            assert_code(&misaligned, CheckerRejectionCode::V2407AbiOrStackInvalid);

            let body = valid.record.blocks.iter().filter(|block| block.owner_entry == entry.id).collect::<Vec<_>>();
            let return_address_load = elf
                .instructions
                .iter()
                .find(|instruction| {
                    body.iter().any(|block| block.range.contains(instruction.address))
                        && instruction.word == (8 << 20) | (2 << 15) | (1 << 7) | 0x3003
                })
                .unwrap();
            let mut wrong_saved_slot = valid.clone();
            wrong_saved_slot.replace_machine_word(return_address_load.address, return_address_load.word ^ (8 << 20));
            assert_code(&wrong_saved_slot, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            let restoration = elf
                .stack_adjustments
                .iter()
                .find(|site| site.delta == 16 && body.iter().any(|block| block.range.contains(site.address)))
                .unwrap();
            let restore_word = elf.instructions.iter().find(|instruction| instruction.address == restoration.address).unwrap();
            let mut wrong_restore = valid.clone();
            wrong_restore.replace_machine_word(restoration.address, (restore_word.word & 0x000f_ffff) | (32 << 20));
            assert_code(&wrong_restore, CheckerRejectionCode::V2407AbiOrStackInvalid);
            let length_store = elf
                .instructions
                .iter()
                .find(|instruction| {
                    body.iter().any(|block| block.range.contains(instruction.address))
                        && instruction.word == (5 << 20) | (11 << 15) | 0x3023
                })
                .unwrap();
            let mut wrong_pointer = valid.clone();
            wrong_pointer.replace_machine_word(length_store.address, length_store.word ^ (1 << 15));
            assert_code(&wrong_pointer, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

            let length_load = elf
                .instructions
                .iter()
                .find(|instruction| {
                    body.iter().any(|block| block.range.contains(instruction.address))
                        && instruction.word == (31 << 15) | (5 << 7) | 0x3003
                })
                .unwrap();
            let mut wrong_length = valid.clone();
            wrong_length.replace_machine_word(length_load.address, length_load.word | (8 << 20));
            assert_code(&wrong_length, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn complete_bounded_programs_require_machine_evidence_after_strict_admission() {
    for (name, source, allocated) in [
        ("transfer", include_str!("../../examples/language/batches/multi_asset_transfer.cell"), true),
        ("settlement", include_str!("../../examples/language/batches/mixed_settlement.cell"), false),
        ("byte migration", include_str!("../../examples/language/batches/nested_migration.cell"), false),
        ("record migration", include_str!("../../examples/language/batches/record_migration.cell"), false),
    ] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = cellscript::compile_with_executable_surface_policy(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                cellscript::ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("{name} {diagnostics}: {error}"));
            let valid = Fixture::from_result(compiled);
            assert_bounded_admitted(&valid);

            // Outer hashes are rebound: the missing semantic proof, rather than
            // a stale checksum or the retired blanket flag, must cause rejection.
            let mut missing = valid.clone();
            let mut removed = 0;
            for block in &mut missing.record.blocks {
                let labels = if allocated { &mut block.allocated_init_labels } else { &mut block.exact_role_labels };
                removed += labels.len();
                labels.clear();
            }
            assert!(removed > 0, "{name} {diagnostics} must exercise its machine evidence");
            missing.rebind_sidecars();
            assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn strict_nested_successors_still_require_complete_output_comparisons() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_with_executable_surface_policy(
            NESTED_CELL_CODEC_SOURCE,
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let valid = Fixture::from_result(compiled);
        assert_bounded_admitted(&valid);
        let mut missing = valid.clone();
        assert!(missing.record.blocks.iter().any(|block| !block.nested_output_labels.is_empty()));
        for block in &mut missing.record.blocks {
            block.nested_output_labels.clear();
        }
        missing.rebind_sidecars();
        assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn scalar_getter_status_and_caller_guards_reject_rebound_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for method in ["input_out_point_index", "input_out_point_tx_hash_low", "cell_lock_hash_low", "cell_type_hash_low"] {
            let source = format!("module scalar_status\naction inspect() -> u64 {{ let value = ckb::{method}(source::input(0)) require value == value return 0 }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            valid.check().unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let owner = format!("runtime:__ckb_{method}");
            let entry = valid.record.entries.iter().find(|entry| entry.id == owner).unwrap();
            let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
            let blocks = valid.record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
            let call = elf.control_flow.iter().find(|edge| edge.target == start).unwrap();
            let word_at = |pc| elf.instructions.iter().find(|word| word.address == pc).unwrap().word;
            let body_word = |value| {
                elf.instructions
                    .iter()
                    .find(|word| word.word == value && blocks.iter().any(|block| block.range.contains(word.address)))
                    .unwrap()
                    .address
            };
            for (address, word) in [
                (call.address + 4, 0x00000013),
                (call.address + 4, word_at(call.address + 4) ^ (1 << 12)),
                (call.address + 8, 0x00050513),
                (body_word(0x00000593), 0x00100593),
                (body_word(0x02c00513), 0x00000513),
            ] {
                let mut rebound = valid.clone();
                rebound.replace_machine_words(&[(address, word)]);
                assert!(rebound.check().is_err(), "{method}, {diagnostics}, mutation at {address:#x}");
            }
        }
    }
}

#[test]
fn streaming_hashes_reject_rebound_bounds_source_capacity_output_and_restore_mutations() {
    for source_kind in ["cell_data", "witness"] {
        let source = format!("module streaming_hash_proof\naction inspect() -> u64 {{ verification\nlet value = {namespace}::{source_kind}_blake2b_span(source::input(0), 0, 600)\nrequire value == value return 0 }}", namespace = "ckb");
        // The witness entry has its own public namespace and shorter name.
        let source = source.replace("ckb::witness_blake2b_span", "witness::blake2b_span");
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            valid.check().unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let wrapper = format!("runtime:__ckb_{source_kind}_blake2b_span");
            for (owner, expected, mask) in [
                (wrapper.as_str(), 0x0062e063, 0x01ff_f07f), // unsigned total/offset or remaining/length check
                (wrapper.as_str(), 0x01013703, u32::MAX),    // saved caller output loaded into a4
                (wrapper.as_str(), 0x03813083, u32::MAX),    // saved return address
                ("runtime:__cellscript_blake2b_transaction_span", 0x08000e13, u32::MAX), // max chunk = 128
                ("runtime:__cellscript_blake2b_transaction_span", 0x0062e063, 0x01ff_f07f), // reported read length >= chunk
                ("runtime:__cellscript_blake2b_transaction_span", 0x005fb023, u32::MAX), // first output store uses checked t6
                ("runtime:__cellscript_blake2b_transaction_span", 0x18813083, u32::MAX), // restore on all exits
            ] {
                let ranges =
                    valid.record.blocks.iter().filter(|block| block.owner_entry == owner).map(|block| block.range).collect::<Vec<_>>();
                let instruction = elf
                    .instructions
                    .iter()
                    .find(|word| ranges.iter().any(|range| range.contains(word.address)) && word.word & mask == expected & mask)
                    .unwrap_or_else(|| panic!("missing {expected:#x} in {owner}"));
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, instruction.word ^ (1 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn raw_witness_bytes_reject_rebound_capacity_length_pointer_and_syscall_mutations() {
    let source = "module raw_witness_machine\naction inspect() -> u64 { verification\nlet bytes = witness::bytes32(source::input(0), 20)\nrequire bytes == bytes return 0 }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        valid.check().unwrap();
        let ranges = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == "runtime:__ckb_witness_bytes32")
            .map(|block| block.range)
            .collect::<Vec<_>>();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for (expected, mask, bit) in [
            (0x02000293, u32::MAX, 1 << 20),   // request size
            (0x0062e063, 0x01fff07f, 1 << 12), // short-read rejection relation
            (0x00813503, u32::MAX, 8 << 20),   // checked output pointer
            (0x01813083, u32::MAX, 8 << 20),   // saved return address
            (0x00000073, u32::MAX, 1 << 20),   // syscall
            (0x00400f13, u32::MAX, 1 << 20),   // source decoder direct-tag boundary
        ] {
            let word = elf
                .instructions
                .iter()
                .find(|word| ranges.iter().any(|range| range.contains(word.address)) && word.word & mask == expected & mask)
                .unwrap();
            let mut changed = valid.clone();
            if expected == 0x73 {
                changed.replace_machine_word(word.address, 0x13); // legal NOP, not an invalid instruction
                assert!(changed.check().is_err(), "omitting the actual witness syscall must reject");
            } else {
                changed.replace_machine_word(word.address, word.word ^ bit);
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn exact_witness_resolver_rejects_rebound_table_lengths_sources_owners_and_results() {
    let source = "module witness_resolver_proof\naction inspect() -> u64 { verification\nlet bytes = witness::lock_exact32(source::input(0))\nrequire bytes == bytes return 0 }";
    let valid =
        Fixture::from_result(compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap());
    valid.check().unwrap();
    let ranges = valid
        .record
        .blocks
        .iter()
        .filter(|block| block.owner_entry == "runtime:__cellscript_witness_bounded_resolve")
        .map(|block| block.range)
        .collect::<Vec<_>>();
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    for (expected, mask, bit) in [
        (0x01000313, u32::MAX, 1 << 20),   // canonical header size
        (0x00300293, u32::MAX, 1 << 20),   // owner/source limits and allowed syscall status
        (0x01df6063, 0x01fff07f, 1 << 12), // selected offsets must be monotonically ordered
        (0x01f2e063, 0x01fff07f, 1 << 12), // field end <= immutable witness total
        (0x00731063, 0x01fff07f, 1 << 12), // BytesOpt payload length must exactly match enclosing field
        (0x004e8513, u32::MAX, 1 << 20),   // return payload begins after four-byte length prefix
        (0x00030593, u32::MAX, 1 << 15),   // checked coordinate/length return source
        (0x06010113, u32::MAX, 16 << 20),  // exact frame restoration
    ] {
        let word = elf
            .instructions
            .iter()
            .find(|word| ranges.iter().any(|range| range.contains(word.address)) && word.word & mask == expected & mask)
            .unwrap_or_else(|| panic!("missing {expected:#x} in resolver"));
        let mut changed = valid.clone();
        changed.replace_machine_word(word.address, word.word ^ bit);
        assert!(changed.check().is_err(), "resolver mutation at {:#x} must reject", word.address);
    }
}

#[test]
fn raw_byte_calls_cannot_be_relabelled_as_hashes_or_retired_runtime_aliases() {
    fn rewrite(value: &mut Value, before: &str, after: &str, substring: bool) {
        match value {
            Value::String(text) if substring => *text = text.replace(before, after),
            Value::String(text) if text == before => *text = after.into(),
            Value::Array(items) => {
                for item in items {
                    rewrite(item, before, after, substring);
                }
            }
            Value::Object(fields) => {
                for item in fields.values_mut() {
                    rewrite(item, before, after, substring);
                }
            }
            _ => {}
        }
    }
    let source = "module raw_domain_binding\naction inspect() -> u64 { verification\nlet bytes = witness::bytes32(source::input(0), 0)\nrequire bytes == bytes return 0 }";
    let valid =
        Fixture::from_result(compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap());
    valid.check().unwrap();
    let mut changed = valid.clone();
    let mut typed = serde_json::to_value(&changed.record.typed_semantics).unwrap();
    for spelling in ["[u8; 32]", "[u8;32]"] {
        rewrite(&mut typed, spelling, "hash", false);
    }
    changed.record.typed_semantics = serde_json::from_value(typed).unwrap();
    changed.rebind_layered_identities();
    assert!(changed.check().is_err(), "coherent 32-byte width does not make a raw read a digest");
    for retired in [
        "__ckb_witness_raw",
        "__ckb_witness_lock",
        "__ckb_witness_input_type",
        "__ckb_witness_output_type",
        "__ckb_cell_data_hash",
        "__ckb_cell_data_hash_at",
        "__ckb_cell_lock_args_hash",
        "__ckb_cell_type_args_hash",
        "__ckb_require_cell_lock_args_hash",
        "__ckb_require_cell_type_args_hash",
        "__ckb_require_cell_lock_args_prefix_hash",
        "__ckb_require_cell_type_args_prefix_hash",
        "__ckb_require_cell_lock_args_suffix_hash",
        "__ckb_require_cell_type_args_suffix_hash",
    ] {
        let mut changed = valid.clone();
        let mut record = serde_json::to_value(&changed.record).unwrap();
        rewrite(&mut record, "__ckb_witness_bytes32", retired, true);
        changed.record = serde_json::from_value(record).unwrap();
        changed.rebind_layered_identities();
        assert!(changed.check().is_err(), "removed runtime identity {retired} must reject even with rebound identities");
    }
}
