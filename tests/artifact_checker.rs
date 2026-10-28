use cellscript::{compile, CompileOptions, CompileResult, NEXT_EDITION};
use cellscript_artifact_checker::{
    canonical_bytes, canonical_hash, check_bundle, check_bundle_values, parse_elf, CheckerBudgets, CheckerRejectionCode, EdgeKind,
    SourceArtifactMap, TypedSemanticConstant, TypedSemanticOperation, TypedSemanticOperationDetail, VerifiedLoweringRecord,
    LOWERING_RECORD_SCHEMA, SOURCE_MAP_SCHEMA,
};
use serde_json::Value;

#[path = "support/collection_coordinate_helpers.rs"]
mod collection_coordinate_helpers;

#[path = "support/borrowed_collection_reads.rs"]
mod borrowed_collection_reads;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[path = "artifact_checker/allocated_arguments.rs"]
mod allocated_arguments;
#[path = "artifact_checker/allocated_calls.rs"]
mod allocated_calls;
#[path = "artifact_checker/allocated_copy_lifetime.rs"]
mod allocated_copy_lifetime;
#[path = "artifact_checker/allocated_isolation.rs"]
mod allocated_isolation;
#[path = "artifact_checker/allocated_parameters.rs"]
mod allocated_parameters;
#[path = "artifact_checker/allocated_paths.rs"]
mod allocated_paths;
#[path = "artifact_checker/allocated_queries.rs"]
mod allocated_queries;
#[path = "support/allocated_scalar_definitions.rs"]
mod allocated_scalar_definitions;
#[path = "artifact_checker/allocated_scalars.rs"]
mod allocated_scalars;
#[path = "artifact_checker/bounded_calls.rs"]
mod bounded_calls;
#[path = "artifact_checker/bounded_hash_properties.rs"]
mod bounded_hash_properties;
#[path = "artifact_checker/bounded_parameters.rs"]
mod bounded_parameters;
#[path = "artifact_checker/bounded_properties.rs"]
mod bounded_properties;
#[path = "artifact_checker/bounded_results.rs"]
mod bounded_results;
#[path = "artifact_checker/bounded_scalars.rs"]
mod bounded_scalars;

#[path = "artifact_checker/allocated_sorted_inputs.rs"]
mod allocated_sorted_inputs;

#[path = "artifact_checker/allocated_wide_transport.rs"]
mod allocated_wide_transport;

#[path = "artifact_checker/cell_data_results.rs"]
mod cell_data_results;

#[path = "artifact_checker/bounded_admission.rs"]
mod bounded_admission;

fn assert_bounded_admitted(fixture: &Fixture) {
    let features = fixture.metadata["runtime"]["fail_closed_runtime_features"].as_array().unwrap();
    for feature in [
        "allocated-values-artifact-proof",
        "nested-values-artifact-proof",
        "exact-role-selection-artifact-proof",
        "cell-data-view-artifact-proof",
    ] {
        assert!(!features.iter().any(|value| value == feature), "retired blanket blocker: {feature}");
    }
    fixture.check().unwrap();
}

const FIXTURE_SOURCE: &str = r#"
module artifact_checker_fixture

fn increment(value: u64) -> u64 {
    return value + 1
}

action main(value: u64) -> u64 {
    verification
        return increment(value)
}
"#;

fn cached_nested_source(hot: bool) -> String {
    let reads = (0..if hot { 48 } else { 2 })
        .map(|index| {
            let method = ["cell_data_u8", "cell_data_u32_le", "cell_data_u64_le"][index % 3];
            format!("require ckb::{method}(view, {index}) == 0\n")
        })
        .collect::<String>();
    format!(
        r#"
module nested_cache::proof
resource Token has store {{ id: u64 }}
action verify(witness items: NestedValues<Vec<u8>, 2, 8, 32>) -> u64 {{
    verification
    let first = items[0]
    let view = ckb::input<Token>(0)
    {reads}
    require view.type_hash == view.lock_hash
    require first[0] == 170
    return 0
}}
"#
    )
}

#[test]
fn exact_cached_reads_preserve_borrowed_caches_across_helpers_and_cfg_joins() {
    for hot in [false, true] {
        let source = cached_nested_source(hot)
            .replace("action verify", "fn read_one(index: u64) -> u64 { let view = ckb::input<Token>(index)\nreturn ckb::cell_data_u64_le(view, 0) }\nfn read_twice(index: u64) -> u64 { return read_one(index) + read_one(index) }\naction verify")
            .replace("let view = ckb::input<Token>(0)", "let view = ckb::input<Token>(0)\nif first.len() > 0 { require read_twice(0) == 0 }\nrequire read_one(0) == 0");
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                    .unwrap_or_else(|error| panic!("cache helper hot={hot} {diagnostics}: {error}")),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let helper = valid
                .record
                .entries
                .iter()
                .find(|entry| entry.kind == cellscript_artifact_checker::EntryKind::Helper && entry.name.ends_with("read_one"))
                .unwrap();
            let ranges = valid
                .record
                .blocks
                .iter()
                .filter(|block| block.owner_entry == helper.id)
                .map(|block| &block.range)
                .collect::<Vec<_>>();
            let setup = elf
                .instructions
                .iter()
                .find(|instruction| {
                    ranges.iter().any(|range| range.contains(instruction.address))
                        && instruction.word == ((27 << 15) | (12 << 7) | 0x13)
                })
                .unwrap();
            let mut changed = valid.clone();
            // A helper cannot substitute another incoming register for the
            // proved ambient cache, even if all outer hashes are recomputed.
            changed.replace_machine_word(setup.address, (26 << 15) | (12 << 7) | 0x13);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn exact_cached_reads_bind_initialization_regions_and_complete_helper_bodies() {
    for (diagnostics, hot) in
        [(cellscript::CompilerDiagnostics::without_optimizations(), false), (cellscript::CompilerDiagnostics::default(), true)]
    {
        let valid = Fixture::from_result(
            compile(
                &cached_nested_source(hot),
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap_or_else(|error| panic!("cache hot={hot}: {error}")),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let cache_blocks = valid
            .record
            .blocks
            .iter()
            .filter(|block| {
                block.owner_entry == "runtime:__cellscript_exact_read_cached"
                    || ["runtime:__ckb_cell_data_u8", "runtime:__ckb_cell_data_u32_le", "runtime:__ckb_cell_data_u64_le"]
                        .contains(&block.owner_entry.as_str())
            })
            .collect::<Vec<_>>();
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !cache_blocks.iter().any(|block| block.range.contains(instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let mask = match word & 0x7f {
                0x63 => 1 << 12,
                0x03 | 0x13 => 1 << 20,
                0x23 => 1 << 7,
                0x33 => 1 << 20,
                _ => continue,
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, word ^ mask);
            assert!(changed.check().is_err(), "cache hot={hot} mutation accepted at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 180);
        let entry = valid
            .record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.params.iter().any(|param| param.ty.contains("NestedValues<")))
            .unwrap();
        let owned =
            valid.record.blocks.iter().filter(|block| block.owner_entry == entry.id).map(|block| &block.range).collect::<Vec<_>>();
        let initialized = elf
            .instructions
            .iter()
            .find(|instruction| {
                owned.iter().any(|range| range.contains(instruction.address))
                    && instruction.word >> 7 & 31 == 27
                    && matches!(instruction.word & 0x7f, 0x13 | 0x33)
            })
            .unwrap()
            .address;
        let mut zero_stores = 0;
        for instruction in &elf.instructions {
            if instruction.address >= initialized || !owned.iter().any(|range| range.contains(instruction.address)) {
                continue;
            }
            if instruction.word & 0x707f == 0x3023 && instruction.word >> 20 & 31 == 0 {
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, instruction.word | (2 << 20));
                assert!(changed.check().is_err(), "cache initialization store was not proved");
                zero_stores += 1;
            }
        }
        assert_eq!(zero_stores, 6, "counter, last pointer and four valid flags");
        let setup = elf
            .instructions
            .iter()
            .find(|instruction| {
                owned.iter().any(|range| range.contains(instruction.address)) && instruction.word == ((27 << 15) | (12 << 7) | 0x13)
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(setup.address, setup.word | (8 << 20));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        if !hot {
            let mut missing_owner = valid.clone();
            let writes = elf
                .instructions
                .iter()
                .filter(|instruction| {
                    instruction.address < setup.address
                        && owned.iter().any(|range| range.contains(instruction.address))
                        && instruction.word >> 7 & 31 == 27
                        && matches!(instruction.word & 0x7f, 0x13 | 0x33 | 0x37)
                })
                .map(|instruction| (instruction.address, encode_addi(0, 0, 0)))
                .collect::<Vec<_>>();
            assert!(!writes.is_empty());
            missing_owner.replace_machine_words(&writes);
            let error = check_bundle_values(
                &missing_owner.artifact,
                &missing_owner.metadata,
                &missing_owner.record,
                &missing_owner.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert!(error.message.contains("public entry borrows an uninitialized"), "{error}");
        }
        let hash_entry = valid.record.entries.iter().find(|entry| entry.id == "runtime:__ckb_cell_type_hash").unwrap();
        let target = valid.record.blocks.iter().find(|block| block.id == hash_entry.entry_block).unwrap().range.start;
        let call = elf.control_flow.iter().find(|edge| edge.target == target).unwrap();
        let output = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < call.address
                    && instruction.address + 64 >= call.address
                    && instruction.word >> 7 & 31 == 11
                    && matches!(instruction.word & 0x7f, 0x13 | 0x33)
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(output.address, (27 << 15) | (11 << 7) | 0x13);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

fn selected_roles_source() -> String {
    let source = include_str!("../examples/language/ckb/exact_role_verification.cell")
        .replace("exact_cell_role<Receipt>", "exact_cell_role<Order>");
    let call = source.split_once("    ckb::verify_cell_roles(").unwrap().1.split_once("    return 0").unwrap().0;
    source.replacen("ckb::verify_cell_roles(", "let selected = ckb::select_cell_roles(", 1).replace(
        "return 0",
        &format!(
            r#"
    let other = ckb::select_cell_roles({call}
    let alias = selected
    let count = ckb::selected_input_count<Order>(alias, 0)
    let input = ckb::selected_input<Order>(alias, 0, count - 1)
    let output = ckb::selected_output<Order>(other, 0, 0)
    let partner = ckb::selected_successor<Order>(selected, 0, 0)
    require input.capacity == partner.capacity
    require output.capacity == partner.capacity
    return ckb::selected_output_count<Order>(selected, 0)
    "#
        ),
    )
}

#[test]
fn cell_data_field_instructions_reject_rebound_source_length_copy_and_branch_changes() {
    let source = "module typed_data_proof\nresource Token has store { prefix: [u8; 256], padding: [u8; 216], id: u64, owner: Hash }\naction inspect() -> u64 { verification\nlet input = ckb::input<Token>(0)\nrequire input.data.owner == Hash::zero()\nreturn input.data.id }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let ranges = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .exact_role_labels
                    .iter()
                    .filter(|label| label.starts_with(".Lexact_role_data_") && !label.ends_with("_done"))
                    .map(|label| {
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|block| block.exact_role_labels.contains(&format!("{label}_done")))
                            .unwrap()
                            .range
                            .start;
                        block.range.start..end
                    })
            })
            .collect::<Vec<_>>();
        assert!(ranges.len() >= 3);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !ranges.iter().any(|range| range.contains(&instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let mask = match word & 0x7f {
                0x03 | 0x13 | 0x1b => 1 << 20,
                0x23 => 1 << 7,
                0x33 => 1 << 20,
                0x37 => 1 << 12,
                0x63 => 1 << 12,
                _ => continue,
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, word ^ mask);
            assert!(changed.check().is_err(), "{diagnostics} accepted changed data field at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 150, "source tags, exact size, wide copy and scalar assembly must all be exercised");
    }
}

#[test]
fn cell_view_helper_parameters_admit_checked_transport_without_data_projection() {
    let source = include_str!("../examples/language/ckb/typed_cell_predicate.cell")
        .replace("before.data.id == after.data.id && before.data.owner == after.data.owner", "true");
    let admitted = Fixture::from_source(&source);
    assert_bounded_admitted(&admitted);
}

#[test]
fn cell_view_helpers_compose_with_branches_repeated_calls_and_wide_arguments() {
    let base = include_str!("../examples/language/ckb/typed_cell_predicate.cell");
    let branched = base.replace(
        "return matches(before, after)",
        "if before.capacity > 0 { return matches(before, after) && matches(before, after) }\nreturn matches(before, after)",
    );
    let wide = base
        .replace("before: InputView", "owner: Hash, before: InputView")
        .replace("return before.data.id", "return owner == before.data.owner && before.data.id")
        .replace("matches(before", "matches(owner, before")
        .replace("forward(ckb::input", "forward(ckb::input<Token>(1).data.owner, ckb::input");
    let same_kind = base.replace("OutputView<Token>", "InputView<Token>").replace("ckb::output<Token>(0)", "ckb::input<Token>(0)");
    let fixed_result = base
        .replace(
            "fn forward(before: InputView<Token>, after: OutputView<Token>) -> bool",
            "fn forward(before: InputView<Token>, after: OutputView<Token>) -> Check",
        )
        .replace("return matches(before, after)", "return Check { ok: matches(before, after) }")
        .replace("action inspect()", "struct Check { ok: bool }\naction inspect()")
        .replace("ckb::output<Token>(0))", "ckb::output<Token>(0)).ok");
    for (case, source) in [&branched, &wide, &same_kind, &fixed_result].into_iter().enumerate() {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            Fixture::from_result(
                compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                    .unwrap_or_else(|error| panic!("view composition case={case} {diagnostics}: {error}")),
            );
        }
    }
}

#[test]
fn cell_view_values_bind_constructors_aliases_and_caller_slot_lifetimes() {
    let source = "module coordinates\nresource Token has store { id: u64 }\nfn inspect(before: InputView<Token>, after: OutputView<Token>) -> bool { let input = before\nlet data = input.data\nlet alias = data\nreturn alias.id == after.data.id }\naction verify(index: u64, noise: u64) -> u64 { verification\nif noise == 0 { return 0 }\nlet first = ckb::input<Token>(index)\nlet second = ckb::input<Token>(0)\nlet before = first\nlet ordinary = index + 1\nrequire ordinary > 0\nrequire inspect(before, ckb::output<Token>(0))\nrequire second.capacity > 0\nreturn 0 }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let reject = |fixture: &Fixture| {
            let error = check_bundle_values(
                &fixture.artifact,
                &fixture.metadata,
                &fixture.record,
                &fixture.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
            assert!(error.message.contains("Cell view") || error.message.contains("exact-role"), "{error}");
        };
        // Alter the shared constructor's index range, source tag and packing.
        let helper = valid.record.blocks.iter().filter(|block| block.owner_entry == "runtime:__ckb_source_input").collect::<Vec<_>>();
        let start = helper.iter().map(|block| block.range.start).min().unwrap();
        let end = helper.iter().map(|block| block.range.end).max().unwrap();
        for instruction in elf.instructions.iter().filter(|i| start <= i.address && i.address < end) {
            let word = instruction.word;
            let replacement = match word & 0x7f {
                0x13 if word >> 7 & 31 != 10 => Some(word ^ (1 << 20)),
                0x33 => Some(word ^ (1 << 20)),
                0x63 => Some(word ^ (1 << 12)),
                _ => None,
            };
            if let Some(replacement) = replacement {
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, replacement);
                reject(&changed);
            }
        }
        // Every local operand load and result publication inside the marked
        // constructors/aliases must use its exact typed scalar slot.
        for block in &valid.record.blocks {
            for label in block.cell_view_value_labels.iter().filter(|label| !label.ends_with("_done")) {
                let done = format!("{label}_done");
                let end = valid.record.blocks.iter().find(|block| block.cell_view_value_labels.contains(&done)).unwrap().range.start;
                for instruction in elf.instructions.iter().filter(|i| block.range.start <= i.address && i.address < end) {
                    let word = instruction.word;
                    let replacement = match word & 0x707f {
                        0x3003 => Some(word ^ (8 << 20)),
                        0x3023 => Some(word ^ (1 << 20)),
                        _ => None,
                    };
                    if let Some(replacement) = replacement {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, replacement);
                        reject(&changed);
                    }
                }
            }
        }
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "verify").unwrap();
        let slot = |id| entry.locals.iter().find(|local| local.id == id).unwrap().source_id * 8;
        let arithmetic = entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add"))
            .unwrap();
        let call = entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|op| op.call.as_ref().is_some_and(|call| call.target == "inspect"))
            .unwrap();
        let ordinary = slot(arithmetic.destinations[0]);
        let destination = slot(call.operands[0].local.unwrap());
        assert!(ordinary < 2048 && destination < 2048);
        let store = elf
            .instructions
            .iter()
            .find(|i| {
                i.word & 0x707f == 0x3023
                    && i.word >> 15 & 31 == 2
                    && u64::from(((i.word >> 25) << 5) | (i.word >> 7 & 31)) == ordinary
                    && valid.record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(i.address))
            })
            .unwrap();
        for partial in [false, true] {
            let offset = destination as u32 + u32::from(partial) * 7;
            let word = (store.word & 0x01fff07f) | ((offset >> 5) << 25) | ((offset & 31) << 7);
            let word = if partial { word & !(7 << 12) } else { word };
            let mut changed = valid.clone();
            changed.replace_machine_word(store.address, word);
            reject(&changed);
        }
        let mut missing = valid.clone();
        for block in &mut missing.record.blocks {
            block.cell_view_value_labels.clear();
        }
        missing.rebind_sidecars();
        reject(&missing);
        let begin = valid
            .record
            .blocks
            .iter()
            .filter(|block| {
                block.owner_entry == entry.id && block.cell_view_value_labels.iter().any(|label| !label.ends_with("_done"))
            })
            .min_by_key(|block| block.range.start)
            .unwrap();
        let label = begin.cell_view_value_labels.iter().find(|label| !label.ends_with("_done")).unwrap();
        let end = valid.record.blocks.iter().find(|block| block.cell_view_value_labels.contains(&format!("{label}_done"))).unwrap();
        let branch = elf
            .instructions
            .iter()
            .filter(|i| {
                i.address < begin.range.start
                    && i.word & 0x7f == 0x63
                    && valid.record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(i.address))
            })
            .max_by_key(|i| i.address)
            .unwrap();
        let mut bypass = valid.clone();
        bypass.redirect_conditional_branch(branch.address, end.range.start);
        reject(&bypass);
    }
}

#[test]
fn cell_view_constructor_inventory_survives_erased_markers_without_data_reads() {
    let source = "module coordinates\nresource Token has store { id: u64 }\naction inspect() -> u64 { verification\nlet view = ckb::input<Token>(0)\nreturn view.capacity }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let mut changed = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        assert!(changed.record.blocks.iter().any(|block| !block.cell_view_value_labels.is_empty()));
        for block in &mut changed.record.blocks {
            block.cell_view_value_labels.clear();
        }
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn named_cell_roles_bind_role_projections_and_aliases_independently() {
    use cellscript_artifact_checker::cell_selection::Read;
    let source = include_str!("syntax_combo/seeds/named-role-selection.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition: cellscript::CellScriptEdition::Edition2027,
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        for case in 0..3 {
            let mut changed = valid.clone();
            let mut operations = changed
                .record
                .typed_semantics
                .entries
                .iter_mut()
                .flat_map(|entry| &mut entry.blocks)
                .flat_map(|block| &mut block.operations);
            match case {
                0 => {
                    let operation = operations
                        .filter(|operation| operation.opcode == "field-access")
                        .find(
                            |operation| matches!(&operation.detail, TypedSemanticOperationDetail::Field { name } if name == "orders"),
                        )
                        .unwrap();
                    operation.detail = TypedSemanticOperationDetail::Field { name: "refunds".into() };
                }
                1 => {
                    let operation = operations
                        .find(|operation| {
                            matches!(&operation.detail, TypedSemanticOperationDetail::ExactRoleRead { read: Read::Input, .. })
                        })
                        .unwrap();
                    operation.detail = TypedSemanticOperationDetail::ExactRoleRead { role: 1, read: Read::Input };
                }
                2 => {
                    let operation = operations.find(|operation| operation.opcode == "verify-exact-cell-roles").unwrap();
                    let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &mut operation.detail else { unreachable!() };
                    contract.names.as_mut().unwrap().roles.swap(0, 1);
                }
                _ => unreachable!(),
            }
            changed.rebind_typed_semantics();
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        }
        // Rewrite every instruction in projection/alias/read regions in turn;
        // hashes and lower-level instruction summaries are rebound by Fixture.
        let ranges: Vec<_> = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| {
                block.exact_role_labels.iter().filter_map(|label| {
                    if !label.starts_with(".Lexact_role_use_") || label.ends_with("_done") {
                        return None;
                    }
                    let end = valid.record.blocks.iter().find(|block| block.exact_role_labels.contains(&format!("{label}_done")))?;
                    Some(block.range.start..end.range.start)
                })
            })
            .collect();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        for instruction in
            elf.instructions.iter().filter(|instruction| ranges.iter().any(|range| range.contains(&instruction.address)))
        {
            let mask = match instruction.word & 0x7f {
                0x03 | 0x13 => 1 << 20,
                0x23 => 1 << 7,
                _ => continue,
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, instruction.word ^ mask);
            assert!(changed.check().is_err(), "{diagnostics} accepted named-role machine substitution at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 40, "exercise projections, aliases and reads");
    }
}

#[test]
fn named_cell_roles_verify_with_independent_machine_checks() {
    let source = include_str!("syntax_combo/seeds/named-role-verification.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition: cellscript::CellScriptEdition::Edition2027,
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
    }
}

#[test]
fn cell_view_helper_calls_reject_same_schema_source_substitution() {
    let source = include_str!("../examples/language/ckb/typed_cell_predicate.cell")
        .replace("OutputView<Token>", "InputView<Token>")
        .replace("ckb::output<Token>(0)", "ckb::input<Token>(0)");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let mut changed = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let forward = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name.ends_with("forward")).unwrap();
        let call = forward
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.operations)
            .find(|operation| operation.call.as_ref().is_some_and(|call| call.target.ends_with("matches")))
            .unwrap();
        assert_eq!(call.operands[0].ty, call.operands[1].ty);
        assert_ne!(call.operands[0].local, call.operands[1].local);
        call.operands.swap(0, 1);
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn cell_view_helpers_bind_register_and_stack_transport_to_typed_slots() {
    let base = include_str!("../examples/language/ckb/typed_cell_predicate.cell");
    for padding in [0, 7, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let arguments = "0, ".repeat(padding);
        let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
        let source = base
            .replace("before: InputView", &format!("{params}before: InputView"))
            .replace("matches(before", &format!("matches({forwarded}before"))
            .replace("forward(ckb::input", &format!("forward({arguments}ckb::input"));
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            for name in ["matches", "forward"] {
                let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with(name)).unwrap();
                let local = typed.locals.iter().find(|local| local.id == typed.params[padding].binding_id).unwrap();
                let slot = local.source_id as u32 * 8;
                let lowering = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
                let start = valid.record.blocks.iter().find(|block| block.id == lowering.entry_block).unwrap().range.start;
                let body = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| block.owner_entry == typed.id && block.lowering_block_id.is_some())
                    .map(|block| block.range.start)
                    .min()
                    .unwrap();
                let spill = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        let word = instruction.word;
                        let offset = (word >> 7 & 31) | (word >> 25 << 5);
                        (start..body).contains(&instruction.address)
                            && word & 0x707f == 0x3023
                            && word >> 15 & 31 == 2
                            && offset == slot
                    })
                    .expect("coordinate parameter spill");
                let mut changed = valid.clone();
                changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                if name == "forward" {
                    let load = elf
                        .instructions
                        .iter()
                        .find(|instruction| {
                            let word = instruction.word;
                            instruction.address >= body
                                && valid
                                    .record
                                    .blocks
                                    .iter()
                                    .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                                && word & 0x707f == 0x3003
                                && word >> 15 & 31 == 2
                                && word >> 20 == slot
                        })
                        .expect("forwarded coordinate load");
                    let mut changed = valid.clone();
                    changed.replace_machine_word(load.address, load.word ^ (8 << 20));
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if padding == 8 {
                        let outgoing = elf
                            .instructions
                            .iter()
                            .find(|instruction| {
                                let word = instruction.word;
                                let offset = ((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20) >> 20;
                                instruction.address >= body
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                                    && word & 0x707f == 0x3023
                                    && word >> 15 & 31 == 2
                                    && offset == -32
                            })
                            .expect("coordinate outgoing stack word");
                        let mut changed = valid.clone();
                        changed.replace_machine_word(outgoing.address, outgoing.word ^ (8 << 7));
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
            }
        }
    }
}

#[test]
fn cell_view_helper_coordinates_cannot_be_overwritten_by_body_stores() {
    for padding in [0, 7, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
        let arguments = "0, ".repeat(padding);
        let source = include_str!("../examples/language/ckb/typed_cell_predicate.cell")
            .replace("before: InputView", &format!("delta: u64, {params}before: InputView"))
            .replace("return before.data.id", "let adjusted = delta + 1\nif adjusted <= delta { return false }\nreturn before.data.id")
            .replace("matches(before", &format!("matches(delta, {forwarded}before"))
            .replace("action inspect()", "action inspect(witness delta: u64)")
            .replace("forward(ckb::input", &format!("forward(delta, {arguments}ckb::input"));
        for data_projection in [false, true] {
            let source = if data_projection {
                source.clone()
            } else {
                source.replace(
                    "before.data.id == after.data.id && before.data.owner == after.data.owner",
                    "before.capacity <= after.capacity",
                )
            };
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                        .unwrap_or_else(|error| panic!("padding={padding} data={data_projection} {diagnostics}: {error}")),
                );
                let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("matches")).unwrap();
                let binary = typed.blocks.iter().flat_map(|block| &block.operations).find(|op| op.opcode == "binary").unwrap();
                let result = typed.locals.iter().find(|local| local.id == binary.destinations[0]).unwrap().source_id as u32 * 8;
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let store = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        let word = instruction.word;
                        word & 0x707f == 0x3023
                            && word >> 15 & 31 == 2
                            && ((word >> 7 & 31) | (word >> 25 << 5)) == result
                            && valid.record.blocks.iter().any(|block| {
                                block.owner_entry == typed.id
                                    && block.lowering_block_id.is_some()
                                    && block.range.contains(instruction.address)
                            })
                    })
                    .expect("arithmetic result has a body store");
                for param in &typed.params[padding + 1..padding + 3] {
                    let coordinate = typed.locals.iter().find(|local| local.id == param.binding_id).unwrap().source_id as u32 * 8;
                    assert_ne!(coordinate, result);
                    assert!(coordinate < 2040 && result < 2048);
                    // Every write width, including a final-byte overlap, must
                    // preserve the coordinate for subsequent reads or calls.
                    for width in 0..=3 {
                        for offset in [coordinate, coordinate + 7] {
                            let mut changed = valid.clone();
                            let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                                | ((offset & 31) << 7)
                                | (width << 12)
                                | ((offset >> 5) << 25);
                            changed.replace_machine_word(store.address, word);
                            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn nested_migration_mixed_helper_parameters_keep_coordinate_spills_bound() {
    let source = include_str!("../examples/language/batches/nested_migration.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("migrate_document")).unwrap();
        let lowering = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == lowering.entry_block).unwrap().range.start;
        let body = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == typed.id && block.lowering_block_id.is_some())
            .map(|block| block.range.start)
            .min()
            .unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for parameter in &typed.params[..2] {
            let local = typed.locals.iter().find(|local| local.id == parameter.binding_id).unwrap();
            let slot = local.source_id as u32 * 8;
            let spill = elf
                .instructions
                .iter()
                .find(|instruction| {
                    let word = instruction.word;
                    (start..body).contains(&instruction.address)
                        && word & 0x707f == 0x3023
                        && word >> 15 & 31 == 2
                        && ((word >> 7 & 31) | (word >> 25 << 5)) == slot
                })
                .unwrap();
            let mut changed = valid.clone();
            changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn record_migration_helpers_reject_rebound_paired_payload_substitution() {
    let source = include_str!("../examples/language/batches/record_migration.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        valid.check().unwrap();
        let mut changed = valid.clone();
        let caller = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "same_message").unwrap();
        let caller_id = caller.id.clone();
        let call = caller
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.operations)
            .find(|operation| operation.call.as_ref().is_some_and(|call| call.target.ends_with("same_payload")))
            .unwrap();
        assert_ne!(call.operands[0].local, call.operands[1].local);
        call.operands[1] = call.operands[0].clone();
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let callee = valid.record.entries.iter().find(|entry| entry.id == "helper:same_payload").unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == callee.entry_block).unwrap().range.start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let call = elf
            .control_flow
            .iter()
            .find(|edge| {
                edge.target == start
                    && valid.record.blocks.iter().any(|block| block.owner_entry == caller_id && block.range.contains(edge.address))
            })
            .unwrap();
        let body = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == caller_id && block.lowering_block_id.is_some())
            .map(|block| block.range.start)
            .min()
            .unwrap();
        let load = |register| {
            elf.instructions
                .iter()
                .rev()
                .find(|instruction| {
                    let word = instruction.word;
                    (body..call.address).contains(&instruction.address)
                        && word & 0x707f == 0x3003
                        && word >> 7 & 31 == register
                        && word >> 15 & 31 == 2
                })
                .unwrap()
        };
        let mut changed = valid.clone();
        for (target, substitute) in [(10, 12), (11, 13)] {
            let (target, substitute) = (load(target), load(substitute));
            assert_ne!(target.word >> 20, substitute.word >> 20);
            changed.replace_machine_word(target.address, replace_i_immediate(target.word, (substitute.word as i32) >> 20));
        }
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn cell_data_helper_fields_preserve_independent_machine_checks() {
    let source = include_str!("../examples/language/ckb/typed_cell_predicate.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let helper = valid.record.entries.iter().find(|entry| entry.name.ends_with("matches")).unwrap();
        let ranges = valid.record.blocks.iter().filter(|block| block.owner_entry == helper.id).collect::<Vec<_>>();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let syscalls = elf
            .instructions
            .iter()
            .filter(|instruction| ranges.iter().any(|block| block.range.contains(instruction.address)) && instruction.word == 0x73)
            .collect::<Vec<_>>();
        assert!(syscalls.len() >= 4);
        for syscall in syscalls {
            let mut changed = valid.clone();
            changed.replace_machine_word(syscall.address, encode_addi(0, 0, 0));
            assert_code(&changed, CheckerRejectionCode::V2417SyscallContractInvalid);
        }
        let byte = elf
            .instructions
            .iter()
            .find(|instruction| {
                ranges.iter().any(|block| block.range.contains(instruction.address)) && instruction.word & 0x707f == 0x4003
            })
            .expect("helper contains full-width owner reads");
        let mut changed = valid.clone();
        changed.replace_machine_word(byte.address, byte.word ^ (1 << 20));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn cell_data_views_keep_nominal_fields_bound_after_admission() {
    let source = selected_roles_source().replace(
        "require input.capacity == partner.capacity",
        "require input.data.id == partner.data.id\nrequire input.data.amount == partner.data.amount",
    );
    let valid = Fixture::from_source(&source);
    assert_bounded_admitted(&valid);
    for case in 1..3 {
        let mut changed = valid.clone();
        {
            let operation = changed
                .record
                .typed_semantics
                .entries
                .iter_mut()
                .flat_map(|entry| &mut entry.blocks)
                .flat_map(|block| &mut block.operations)
                .find(|operation| {
                    operation.opcode == "field-access"
                        && operation.operands.first().is_some_and(|operand| operand.ty.starts_with("CellDataView<"))
                })
                .unwrap();
            if case == 1 {
                operation.detail = TypedSemanticOperationDetail::Field { name: "missing_field".into() };
            } else {
                operation.operands[0].ty = "CellDataView<Receipt>".into();
            }
            changed.rebind_typed_semantics();
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        }
    }
}

#[test]
fn exact_role_lifetime_rejects_entry_into_a_read_before_construction() {
    let base = include_str!("../examples/language/ckb/exact_role_verification.cell")
        .replace("exact_cell_role<Receipt>", "exact_cell_role<Order>");
    let (prefix, tail) = base.split_once("action verify()").unwrap();
    let call = tail.split_once("ckb::verify_cell_roles(").unwrap().1.split_once("    return 0").unwrap().0;
    let source = format!("{prefix}action verify(witness at: u64) -> u64 {{\nverification\nif at > 0 {{\nlet selected = ckb::select_cell_roles({call}\nif at > 1 {{ return ckb::selected_input_count<Order>(selected, 0) }}\n}}\nreturn 0\n}}");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let producer = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.exact_role_labels.iter().any(|label| label.starts_with(".Lexact_role_check_") && !label.ends_with("_done"))
            })
            .unwrap();
        let consumer = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.exact_role_labels.iter().any(|label| label.starts_with(".Lexact_role_use_") && !label.ends_with("_done"))
            })
            .unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let flow = elf
            .control_flow
            .iter()
            .find(|flow| {
                flow.address < producer.range.start
                    && flow.target > producer.range.start
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == producer.owner_entry && block.range.contains(flow.address))
                    && elf
                        .instructions
                        .iter()
                        .any(|instruction| instruction.address == flow.address && instruction.word & 0xfff == 0x6f)
            })
            .expect("the outer false path jumps over the large selection body");
        let origin = valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap();
        let previous_target = valid.record.blocks.iter().find(|block| block.range.start == flow.target).unwrap();
        let mut changed = valid.clone();
        changed.record.edges.iter_mut().find(|edge| edge.from == origin.id && edge.to == previous_target.id).unwrap().to =
            consumer.id.clone();
        let mut reached = std::collections::BTreeSet::new();
        let mut pending = changed.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if reached.insert(id.clone()) {
                pending.extend(changed.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
            }
        }
        for block in &mut changed.record.blocks {
            block.reachable = reached.contains(&block.id);
        }
        changed.record.canonicalize();
        changed.replace_machine_word(flow.address, encode_jal((consumer.range.start as i64 - flow.address as i64) as i32));
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert!(error.message.contains("selection read has no constructed handle provenance"), "{diagnostics}: {error}");
    }
}

#[test]
fn exact_role_lifetime_preserves_handles_and_tables_across_calls_and_branches() {
    let source = selected_roles_source()
        .replace("action verify()", "fn bump(value: u64) -> u64 { return value + 7 }\naction verify(witness at: u64)")
        .replace(
            "let alias = selected",
            "let marker = at + 7\nrequire marker > at\nif at > 0 { require bump(at) > at }\nlet alias = selected",
        );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                .unwrap_or_else(|error| panic!("selection lifetime {diagnostics}: {error}")),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut ranges = Vec::new();
        let mut use_ranges = Vec::new();
        let mut constructor_stores = Vec::new();
        let mut tables = Vec::new();
        let mut last_constructor_end = 0;
        for block in &valid.record.blocks {
            for label in &block.exact_role_labels {
                if label.ends_with("_done") {
                    continue;
                }
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.exact_role_labels.contains(&format!("{label}_done")))
                    .unwrap()
                    .range
                    .start;
                ranges.push(block.range.start..end);
                if let Some(parts) = label.strip_prefix(".Lexact_role_check_") {
                    tables.push(parts.split('_').nth(2).unwrap().parse::<i32>().unwrap());
                    last_constructor_end = last_constructor_end.max(end);
                    constructor_stores.push(end - 4);
                } else {
                    use_ranges.push(block.range.start..end);
                }
            }
        }
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "verify").unwrap();
        let handles = typed
            .locals
            .iter()
            .filter(|local| local.ty.starts_with("ExactCellSelection<"))
            .map(|local| local.source_id as i32 * 8)
            .collect::<Vec<_>>();
        assert!(handles.len() >= 2 && tables.len() == 2, "{diagnostics}: handles {handles:?}, tables {tables:?}");
        // Rebind a second typed handle and all its actual pointer loads/stores
        // to the first one's slot. Both individual instruction grammars still
        // match; the enclosing proof must reject the physical alias.
        let (original, replacement) = (handles[1], handles[0]);
        let mut mutations = Vec::new();
        for instruction in &elf.instructions {
            if !constructor_stores.contains(&instruction.address)
                && !use_ranges.iter().any(|range| range.contains(&instruction.address))
            {
                continue;
            }
            let word = instruction.word;
            if word >> 15 & 31 != 2 {
                continue;
            }
            if word & 0x707f == 0x3003 && (word as i32) >> 20 == original {
                mutations.push((instruction.address, replace_i_immediate(word, replacement)));
            } else if word & 0x707f == 0x3023 && ((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20 >> 20) == original {
                mutations.push((
                    instruction.address,
                    (word & !0xfe000f80) | ((replacement as u32 & 0xfe0) << 20) | ((replacement as u32 & 31) << 7),
                ));
            }
        }
        assert!(mutations.len() >= 2);
        let mut aliased = valid.clone();
        let entry = aliased.record.typed_semantics.entries.iter_mut().find(|entry| entry.id == typed.id).unwrap();
        entry.locals.iter_mut().find(|local| local.source_id * 8 == original as u64).unwrap().source_id = replacement as u64 / 8;
        aliased.rebind_typed_semantics();
        aliased.replace_machine_words(&mutations);
        let error = check_bundle_values(
            &aliased.artifact,
            &aliased.metadata,
            &aliased.record,
            &aliased.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert!(error.message.contains("selection pointer slot aliases"), "{diagnostics}: {error}");
        let store = elf
            .instructions
            .iter()
            .find(|instruction| {
                instruction.address >= last_constructor_end
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                    && !ranges.iter().any(|range| range.contains(&instruction.address))
                    && instruction.word & 0x707f == 0x3023
                    && instruction.word >> 15 & 31 == 2
            })
            .unwrap();
        let targets = handles.iter().copied().chain([tables[0], tables[0] + 768, tables[0] + 896]);
        for offset in targets {
            for kind in [0, 3] {
                assert!((0..2048).contains(&offset));
                let word = (store.word & !0xfe0fff80)
                    | ((offset as u32 & 0xfe0) << 20)
                    | ((offset as u32 & 31) << 7)
                    | (2 << 15)
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
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                assert!(error.message.contains("unverified write overlaps"), "{diagnostics} offset={offset}: {error}");
            }
        }
        // A computed alias must retain the same protection as an SP-relative write.
        let mut changed = valid.clone();
        changed.replace_machine_words(&[
            (store.address - 4, ((handles[0] as u32) << 20) | (2 << 15) | (3 << 12) | (30 << 7) | 3),
            (store.address, (store.word & !0xfe0f8f80) | (30 << 15)),
        ]);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn exact_role_selected_reads_reject_rebound_words_and_typed_operand_substitutions() {
    use cellscript_artifact_checker::cell_selection::Read;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &selected_roles_source(),
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap_or_else(|error| panic!("{diagnostics}: {error}")),
        );
        let mut ranges = Vec::new();
        for block in &valid.record.blocks {
            for label in &block.exact_role_labels {
                if label.starts_with(".Lexact_role_use_") && !label.ends_with("_done") {
                    let end =
                        valid.record.blocks.iter().find(|block| block.exact_role_labels.contains(&format!("{label}_done"))).unwrap();
                    ranges.push(block.range.start..end.range.start);
                }
            }
        }
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for block in &valid.record.blocks {
            if block.exact_role_labels.iter().any(|label| label.starts_with(".Lexact_role_check_") && label.ends_with("_done")) {
                // The retained pointer's address and destination store are
                // inside the constructor proof, after the restored SP.
                for address in [block.range.start - 8, block.range.start - 4] {
                    let word = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
                    let mask = if word & 0x7f == 0x23 { 1 << 7 } else { 1 << 15 };
                    let mut changed = valid.clone();
                    changed.replace_machine_word(address, word ^ mask);
                    assert!(changed.check().is_err(), "retained pointer mutation accepted at {address:#x}");
                }
            }
        }
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !ranges.iter().any(|range| range.contains(&instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let mask = match word & 0x7f {
                0x63 => 1 << 12,
                0x03 | 0x13 => 1 << 20,
                0x23 => 1 << 7,
                0x33 => 1 << 20,
                _ => continue,
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, word ^ mask);
            assert!(changed.check().is_err(), "{diagnostics} accepted changed read at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 60, "all five reads and local aliases must be exercised");
        for case in 0..4 {
            let mut changed = valid.clone();
            let entry = changed
                .record
                .typed_semantics
                .entries
                .iter_mut()
                .find(|entry| {
                    entry.blocks.iter().flat_map(|block| &block.operations).any(|operation| operation.opcode == "read-exact-cell-role")
                })
                .unwrap();
            let other = entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .find(|operation| matches!(operation.detail, TypedSemanticOperationDetail::ExactRoleRead { read: Read::Output, .. }))
                .unwrap()
                .operands[0]
                .local
                .unwrap();
            let operation = entry
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.operations)
                .find(|operation| {
                    matches!(operation.detail, TypedSemanticOperationDetail::ExactRoleRead { read: Read::Successor, .. })
                })
                .unwrap();
            match case {
                0 => operation.detail = TypedSemanticOperationDetail::ExactRoleRead { role: 1, read: Read::Successor },
                1 => operation.detail = TypedSemanticOperationDetail::ExactRoleRead { role: 0, read: Read::Output },
                2 => operation.operands[0].local = Some(other),
                3 => operation.operands[1].constant = Some(TypedSemanticConstant::U64("1".into())),
                _ => unreachable!(),
            }
            changed.rebind_typed_semantics();
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn selected_hash_reads_prove_caller_writes_while_nested_views_remain_live() {
    let source = selected_roles_source()
        .replace("verify()", "verify(witness items: NestedValues<Vec<u8>, 2, 8, 32>)")
        .replace("let alias = selected", "let first = items[0]\nlet alias = selected")
        .replace(
            "require input.capacity == partner.capacity",
            "require input.type_hash == partner.type_hash\nrequire input.lock_hash == partner.lock_hash\nrequire input.data_hash == partner.data_hash\nrequire first[0] == 170",
        );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                .unwrap_or_else(|error| panic!("hash with nested {diagnostics}: {error}")),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let blocks =
            valid.record.blocks.iter().filter(|block| block.owner_entry == "runtime:__ckb_cell_type_hash").collect::<Vec<_>>();
        assert!(!blocks.is_empty());
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !blocks.iter().any(|block| block.range.contains(instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let mask = match word & 0x7f {
                0x63 => 1 << 12,
                0x03 | 0x13 => 1 << 20,
                0x23 => 1 << 7,
                0x33 => 1 << 20,
                _ => continue,
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, word ^ mask);
            assert!(changed.check().is_err(), "hash helper mutation accepted at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 45);
        let start = blocks.iter().map(|block| block.range.start).min().unwrap();
        let call = elf.control_flow.iter().find(|edge| edge.target == start).unwrap();
        let owner = &valid.record.blocks.iter().find(|block| block.range.contains(call.address)).unwrap().owner_entry;
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| &entry.id == owner).unwrap();
        let nested_slot = entry.locals.iter().find(|local| local.ty.starts_with("NestedValues<")).unwrap().source_id * 8;
        assert!(nested_slot < 2048);
        for register in [11, 12] {
            let setup = elf
                .instructions
                .iter()
                .rev()
                .find(|instruction| {
                    instruction.address < call.address
                        && instruction.address + 64 >= call.address
                        && instruction.word >> 7 & 31 == register
                        && matches!(instruction.word & 0x7f, 0x13 | 0x33)
                })
                .unwrap();
            let mut changed = valid.clone();
            // Rebind the real caller output or length pointer into a live
            // nested root slot; the helper itself is unchanged and well formed.
            changed.replace_machine_word(setup.address, ((nested_slot as u32) << 20) | (2 << 15) | (register << 7) | 0x13);
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert!(error.message.contains("overlaps protected runtime storage"), "{error}");
        }
    }
}

#[test]
fn nested_transaction_data_views_bind_complete_tables_and_retained_results() {
    let source = include_str!("syntax_combo/seeds/named-nested-data-views.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = compile(
            source,
            CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap_or_else(|error| panic!("nested data {diagnostics}: {error}"));
        let valid = Fixture::from_result(result);
        assert_bounded_admitted(&valid);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let (start, end) = valid
            .record
            .blocks
            .iter()
            .find_map(|block| {
                let label = block.exact_role_labels.iter().find(|label| {
                    label.starts_with(".Lexact_role_data_")
                        && !label.ends_with("_done")
                        && label.strip_prefix(".Lexact_role_data_").unwrap().split('_').count() == 6
                        && label.strip_prefix(".Lexact_role_data_").unwrap().split('_').nth(4) != Some("0")
                })?;
                let end =
                    valid.record.blocks.iter().find(|block| block.exact_role_labels.contains(&format!("{label}_done")))?.range.start;
                Some((block.range.start, end))
            })
            .unwrap();
        let body = elf.instructions.iter().filter(|i| (start..end).contains(&i.address)).collect::<Vec<_>>();
        let pointer = body.iter().find(|i| i.word == ((30 << 20) | (29 << 15) | (5 << 7) | 0x33)).unwrap();
        let length = body.iter().find(|i| i.word & 0x7f == 0x23 && i.word >> 12 & 7 == 3 && i.word >> 20 & 31 == 6).unwrap();
        let source = body.iter().find(|i| i.word == ((7 << 15) | (14 << 7) | 0x13)).unwrap();
        for (address, word) in [
            (pointer.address, pointer.word ^ (1 << 15)),
            (length.address, length.word ^ (1 << 10)),
            (source.address, source.word ^ (1 << 15)),
        ] {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, word);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn nested_data_view_buffers_cannot_alias_after_coherent_machine_and_marker_rebinding() {
    let source = include_str!("syntax_combo/seeds/named-nested-data-views.cell")
        .split("fn total")
        .next()
        .unwrap()
        .replace("encoded_bytes: 4096", "encoded_bytes: 128");
    let source = format!("{source}\naction verify() -> u64 {{ let before = ckb::input<Batch>(1).data.messages\nlet after = ckb::output<Batch>(0).data.messages\nrequire before[0][0] == 1\nrequire after[0][0] == 4\nreturn 0\n}}");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let mut ranges = Vec::new();
        for block in &valid.record.blocks {
            for label in &block.exact_role_labels {
                let Some(parts) = label.strip_prefix(".Lexact_role_data_") else { continue };
                let parts = parts.split('_').collect::<Vec<_>>();
                if parts.len() != 6 || parts[4] == "0" {
                    continue;
                }
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.exact_role_labels.contains(&format!("{label}_done")))
                    .unwrap()
                    .range
                    .start;
                ranges.push((label.clone(), block.range.start..end, parts[3].parse::<i32>().unwrap()));
            }
        }
        assert_eq!(ranges.len(), 2);
        let (label, range, original) = &ranges[1];
        let replacement = ranges[0].2;
        assert!(*original + 520 < 2048, "fixture must exercise directly encoded offsets");
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut rewrites = Vec::new();
        for instruction in elf.instructions.iter().filter(|i| range.contains(&i.address)) {
            let word = instruction.word;
            if word >> 15 & 31 != 2 {
                continue;
            }
            let opcode = word & 0x7f;
            let offset = if opcode == 0x23 {
                (((word >> 25) << 5) | ((word >> 7) & 31)) as i32
            } else if opcode == 3 || opcode == 0x13 {
                (word as i32) >> 20
            } else {
                continue;
            };
            if (*original..*original + 520).contains(&offset) {
                let offset = offset - original + replacement;
                rewrites.push((
                    instruction.address,
                    if opcode == 0x23 { replace_s_immediate(word, offset) } else { replace_i_immediate(word, offset) },
                ));
            }
        }
        assert!(rewrites.len() >= 10);
        let mut changed = valid.clone();
        let mut parts = label.strip_prefix(".Lexact_role_data_").unwrap().split('_').map(str::to_string).collect::<Vec<_>>();
        parts[3] = replacement.to_string();
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
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert!(error.message.contains("share a retained buffer"), "{error}");
    }
}

#[test]
fn nested_role_selection_checks_profiles_layouts_and_complete_machine_code() {
    let source = include_str!("syntax_combo/seeds/named-nested-selection.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap_or_else(|error| panic!("nested selection {diagnostics}: {error}")),
        );
        let profiles = valid
            .record
            .typed_semantics
            .entries
            .iter()
            .flat_map(|entry| &entry.blocks)
            .flat_map(|block| &block.operations)
            .filter_map(|op| match &op.detail {
                TypedSemanticOperationDetail::ExactRoleSelection { contract } => Some(contract.profile.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            profiles.iter().filter(|profile| **profile == cellscript_artifact_checker::cell_selection::BOUNDED_PROFILE).count(),
            2
        );
        assert_eq!(profiles.iter().filter(|profile| **profile == cellscript_artifact_checker::cell_selection::PROFILE).count(), 1);
        assert_bounded_admitted(&valid);
        let mut downgraded = valid.clone();
        for op in downgraded
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
        {
            if let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &mut op.detail {
                contract.profile = cellscript_artifact_checker::cell_selection::PROFILE.into();
            }
        }
        downgraded.rebind_typed_semantics();
        assert_code(&downgraded, CheckerRejectionCode::V2419TypedSemanticsInvalid);

        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        // Change selected identity offset, actual-byte accounting, nested
        // key normalization after rebinding hashes.
        // Each pattern is specific to the admitted selection's instruction body.
        let patterns = [
            ((8 << 20) | (29 << 15) | (4 << 12) | (30 << 7) | 3, (4 << 20) | (29 << 15) | (4 << 12) | (30 << 7) | 3),
            ((16 << 20) | (2 << 15) | (3 << 12) | (7 << 7) | 3, (24 << 20) | (2 << 15) | (3 << 12) | (7 << 7) | 3),
            ((30 << 20) | (29 << 15) | (29 << 7) | 0x33, (7 << 20) | (29 << 15) | (29 << 7) | 0x33),
        ];
        for (original, replacement) in patterns {
            let instruction =
                elf.instructions.iter().find(|instruction| instruction.word == original).expect("nested selection obligation");
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, replacement);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let mut reordered = valid.clone();
        let ty = reordered.record.typed_semantics.types.iter_mut().find(|ty| ty.name == "Order").unwrap();
        for field in &mut ty.fields {
            if field.molecule_field_index == Some(0) {
                field.molecule_field_index = Some(2);
            } else if field.molecule_field_index == Some(2) {
                field.molecule_field_index = Some(0);
            }
        }
        reordered.rebind_typed_semantics();
        assert!(check_bundle_values(
            &reordered.artifact,
            &reordered.metadata,
            &reordered.record,
            &reordered.source_map,
            &CheckerBudgets::default()
        )
        .is_err());
    }
}

#[test]
fn exact_role_declarations_reject_rebound_substitutions_after_admission() {
    let valid = Fixture::from_source(include_str!("../examples/language/ckb/exact_role_verification.cell"));
    assert_bounded_admitted(&valid);

    // These validate the declaration boundary. The separate machine test also
    // rejects well-formed catalog substitutions after hashes are rebound.
    for case in 0..7 {
        let mut changed = valid.clone();
        let operation = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| op.opcode == "verify-exact-cell-roles")
            .unwrap();
        let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &mut operation.detail else { panic!() };
        match case {
            0 => contract.input_scan = 65,
            1 => contract.roles[1].type_hash = contract.roles[0].type_hash,
            2 => contract.roles[0].type_name = "UnknownCell".into(),
            3 => contract.roles[0].identity_field = "unknown".into(),
            4 => contract.roles[0].outputs = (2, 1),
            5 => contract.roles[0].correspondence = cellscript_artifact_checker::cell_selection::Correspondence::Merge(vec![0]),
            6 => contract.roles.clear(),
            _ => unreachable!(),
        }
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn exact_role_instruction_contract_checks_each_lifecycle_and_local_authority() {
    let source = include_str!("../examples/language/ckb/exact_role_verification.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for mode in ["preserve", "create", "burn", "merge"] {
            let mut source = source.replace("\"preserve\"", &format!("\"{mode}\""));
            if mode == "merge" {
                source = source.replace("1, 16, 1, 16", "2, 16, 1, 1, b\"0123456789abcdef\"");
            }
            source = source.replacen("\"observe\"", "\"local-type\"", 1);
            let valid = compile(&source, CompileOptions { target: Some("riscv64-elf".into()), diagnostics, ..Default::default() })
                .unwrap_or_else(|error| panic!("{mode} {diagnostics}: {error}"));
            Fixture::from_result(valid);
        }
    }
}

#[test]
fn exact_role_machine_rejects_rebound_predicate_load_store_and_policy_mutations() {
    let valid = Fixture::from_source(include_str!("../examples/language/ckb/exact_role_verification.cell"));
    let start = valid
        .record
        .blocks
        .iter()
        .find(|block| block.exact_role_labels.iter().any(|label| !label.ends_with("_done")))
        .unwrap()
        .range
        .start;
    let end = valid
        .record
        .blocks
        .iter()
        .find(|block| block.exact_role_labels.iter().any(|label| label.ends_with("_done")))
        .unwrap()
        .range
        .start;
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let mut mutations = 0;
    for instruction in elf.instructions.iter().filter(|instruction| (start..end).contains(&instruction.address)) {
        let word = instruction.word;
        let opcode = word & 0x7f;
        let mask = match opcode {
            0x63 => 1 << 12, // Invert a predicate without changing its CFG destinations.
            0x03 => 1 << 20, // Read the wrong frame, hash or key byte.
            0x23 => 1 << 7,  // Write another counter/record byte.
            0x13 if word >> 12 & 7 == 0 && word >> 7 & 31 != 2 => 1 << 20,
            _ => continue,
        };
        let mut changed = valid.clone();
        changed.replace_machine_word(instruction.address, word ^ mask);
        assert!(changed.check().is_err(), "rebound selection instruction accepted at {:#x}", instruction.address);
        mutations += 1;
    }
    assert!(mutations > 500, "exercise both complete scans, every role, key bytes and correspondence loops");
    for case in 0..7 {
        let mut changed = valid.clone();
        let operation = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| op.opcode == "verify-exact-cell-roles")
            .unwrap();
        let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &mut operation.detail else { panic!() };
        match case {
            0 => contract.roles[0].type_hash[31] ^= 1,
            1 => contract.roles[0].lock_hash[31] ^= 1,
            2 => contract.roles[0].identity_field = "amount".into(),
            3 => contract.input_scan -= 1,
            4 => contract.selected_per_side -= 1,
            5 => contract.roles[0].inputs.0 = 0,
            6 => contract.roles[0].local_type = true,
            _ => unreachable!(),
        }
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
    let mut missing = valid.clone();
    for block in &mut missing.record.blocks {
        block.exact_role_labels.clear();
    }
    missing.rebind_sidecars();
    assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn exact_role_machine_binds_verification_order_and_conditional_block_ownership() {
    let source = include_str!("../examples/language/ckb/exact_role_verification.cell");
    let begin = source.find("    ckb::verify_cell_roles(").unwrap();
    let end = source.rfind("    return 0").unwrap();
    let repeated = format!("{}{}{}{}", &source[..begin], &source[begin..end], &source[begin..end], &source[end..]);
    let valid = Fixture::from_source(&repeated);
    let starts = valid
        .record
        .blocks
        .iter()
        .flat_map(|block| &block.exact_role_labels)
        .filter(|label| !label.ends_with("_done"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2);
    let mut reversed = valid.clone();
    for block in &mut reversed.record.blocks {
        for label in &mut block.exact_role_labels {
            let done = label.ends_with("_done");
            let base = label.strip_suffix("_done").unwrap_or(label);
            let replacement = if base == starts[0] {
                &starts[1]
            } else if base == starts[1] {
                &starts[0]
            } else {
                continue;
            };
            *label = format!("{replacement}{}", if done { "_done" } else { "" });
        }
        block.exact_role_labels.sort();
    }
    reversed.rebind_sidecars();
    assert_code(&reversed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let conditional = format!("{}if flag {{\n{}}}\n{}", &source[..begin], &source[begin..end], &source[end..])
        .replace("verify()", "verify(witness flag: bool)");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(&conditional, CompileOptions { target: Some("riscv64-elf".into()), diagnostics, ..Default::default() })
            .unwrap_or_else(|error| panic!("conditional {diagnostics}: {error}"));
        Fixture::from_result(compiled);
    }
}

#[test]
fn exact_role_scratch_preserves_nested_views_and_rejects_rebound_aliases() {
    let source = include_str!("../examples/language/ckb/exact_role_verification.cell")
        .replace("verify()", "verify(witness items: NestedValues<Vec<u8>, 2, 8, 32>)")
        .replace("    ckb::verify_cell_roles(", "    let first = items[0]\n    let before = first.len()\n    ckb::verify_cell_roles(")
        .replace(
            "    return 0",
            "    require items.len() == 1\n    require first.len() == before\n    require first[0] == 170\n    return 0",
        );
    let cell_source = source
        .replace("witness items: NestedValues<Vec<u8>, 2, 8, 32>", "input document: Document")
        .replace("action verify(", "resource Document has store, consume { entries: NestedValues<Vec<u8>, 2, 8, 32> }\naction verify(")
        .replace("let first = items[0]", "let items = document.entries\n    let first = items[0]")
        .replace("return 0", "consume document\n    return 0");
    for (source, cell) in [(&source, false), (&cell_source, true)] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                    .unwrap_or_else(|error| panic!("nested selection {diagnostics}: {error}")),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let (block, label) = valid
                .record
                .blocks
                .iter()
                .find_map(|block| block.exact_role_labels.iter().find(|label| !label.ends_with("_done")).map(|label| (block, label)))
                .unwrap();
            let end_label = format!("{label}_done");
            let end = valid.record.blocks.iter().find(|block| block.exact_role_labels.contains(&end_label)).unwrap().range.start;
            let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.id == block.owner_entry).unwrap();
            let scalar_end = typed.locals.iter().map(|local| (local.source_id + 1) * 8).max().unwrap();
            let sizes = valid
                .record
                .blocks
                .iter()
                .flat_map(|block| {
                    block
                        .nested_read_labels
                        .iter()
                        .filter(|label| label.starts_with(".Lnested_use_length_") && !label.ends_with("_done"))
                        .map(|label| {
                            let done = format!("{label}_done");
                            let end =
                                valid.record.blocks.iter().find(|block| block.nested_read_labels.contains(&done)).unwrap().range.start;
                            (block.range.start, end)
                        })
                })
                .collect::<Vec<_>>();
            let size = if cell {
                let label = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| &block.nested_cell_labels)
                    .find(|label| !label.ends_with("_done") && !label.ends_with("_validate"))
                    .unwrap();
                let buffer = label.strip_prefix(".Lnested_cell_codec_").unwrap().split('_').nth(2).unwrap().parse::<i32>().unwrap();
                // Start inside the loaded Cell's bytes, after its scalar pointer.
                (buffer + 15) & !15
            } else {
                elf.instructions
                    .iter()
                    .filter(|instruction| {
                        sizes.iter().any(|(start, end)| (*start..*end).contains(&instruction.address))
                            && instruction.word & 0x707f == 0x3003
                            && instruction.word >> 15 & 31 == 2
                            && instruction.word >> 7 & 31 == 11
                    })
                    .map(|instruction| (instruction.word as i32) >> 20)
                    .find(|offset| *offset >= scalar_end as i32 && offset % 16 == 0)
                    .expect("a nested length slot outside scalar locals can alias an aligned selection scratch")
            };
            assert!(size >= scalar_end as i32 && size < 2048);
            let start_word = elf.instructions.iter().find(|instruction| instruction.address == block.range.start).unwrap().word;
            let end_word = elf.instructions.iter().find(|instruction| instruction.address == end - 4).unwrap().word;
            assert_eq!(start_word & 0xfffff, 0x10113, "small positive SP adjustment");
            assert_eq!(end_word & 0xfffff, 0x10113, "small negative SP adjustment");
            let mut changed = valid.clone();
            changed.replace_machine_words(&[
                (block.range.start, replace_i_immediate(start_word, size)),
                (end - 4, replace_i_immediate(end_word, -size)),
            ]);
            let mut parts = label.strip_prefix(".Lexact_role_check_").unwrap().split('_').map(str::to_owned).collect::<Vec<_>>();
            parts[2] = size.to_string();
            let replacement = format!(".Lexact_role_check_{}", parts.join("_"));
            for block in &mut changed.record.blocks {
                for candidate in &mut block.exact_role_labels {
                    if candidate == label {
                        *candidate = replacement.clone();
                    } else if candidate == &end_label {
                        *candidate = format!("{replacement}_done");
                    }
                }
                block.exact_role_labels.sort();
            }
            changed.rebind_sidecars();
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert!(error.message.contains("overlaps protected runtime storage"), "{diagnostics}: {error:?}");
        }
    }
}

#[test]
fn exact_role_nested_lifetime_handles_empty_branches_and_rejects_helper_escape() {
    let source = include_str!("../examples/language/ckb/exact_role_verification.cell")
        .replace("verify()", "verify(witness items: NestedValues<Vec<u8>, 2, 8, 32>)");
    let start = source.find("    ckb::verify_cell_roles(").unwrap();
    let end = source.rfind("    return 0").unwrap();
    let selection = &source[start..end];
    let prefix = format!("{}let first = items[0]\nlet before = first.len()\n", &source[..start]);
    let suffix = format!("require first.len() == before\nrequire items.len() == 1\n{}", &source[end..]);
    let conditional = format!("{prefix}if before == 2 {{\n{selection}}}\n{suffix}");
    let helper = format!("{prefix}verify_roles()\n{suffix}")
        .replace("action verify(", &format!("fn verify_roles() {{\n{selection}}}\naction verify("));
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let options = CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() };
        let compiled = compile(&conditional, options.clone())
            .unwrap_or_else(|error| panic!("nested selection control flow {diagnostics}: {error}"));
        let valid = Fixture::from_result(compiled);
        let selection_end = valid
            .record
            .blocks
            .iter()
            .find(|block| block.exact_role_labels.iter().any(|label| label.ends_with("_done")))
            .unwrap()
            .range
            .start;
        let read = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.range.start >= selection_end)
            .filter(|block| {
                block.nested_read_labels.iter().any(|label| label.starts_with(".Lnested_use_length_") && !label.ends_with("_done"))
            })
            .min_by_key(|block| block.range.start)
            .unwrap();
        let empty = valid
            .record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.id == read.owner_entry)
            .unwrap()
            .blocks
            .iter()
            .find(|block| block.operations.is_empty())
            .unwrap();
        assert_ne!(read.lowering_block_id, Some(empty.id));
        let mut changed = valid.clone();
        let target = changed.record.blocks.iter_mut().find(|block| block.id == read.id).unwrap();
        target.lowering_block_id = Some(empty.id);
        target.typed_block_hash = Some(canonical_hash("cellscript-typed-block-v1", empty).unwrap());
        for binding in &mut changed.record.entries.iter_mut().find(|entry| entry.id == read.owner_entry).unwrap().typed_blocks {
            binding.machine_block_ids.retain(|id| id != &read.id);
            if binding.id == empty.id {
                binding.machine_block_ids.push(read.id.clone());
            }
        }
        changed.record.canonicalize();
        changed.rebind_sidecars();
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert!(error.message.contains("nested read crosses its owning typed block"), "{error}");
        let error = compile(&helper, options).unwrap_err();
        assert!(error.message.contains("exact-role verification requires an action or lock entry"), "{error}");
    }
}

fn large_policy_fixture(count: u32) -> CompileResult {
    policy_fixture_with_layouts(count, false)
}

fn policy_fixture_with_layouts(count: u32, distinct_layouts: bool) -> CompileResult {
    use cellscript::artifact::{compile_artifact, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    let mut source = "module policy_error_labels\nresource Token has store, consume { amount: u64\nexpected: [u8; 8] }\n".to_string();
    for index in 0..count {
        let extra = if distinct_layouts { format!(", witness extra: [u8; {}]", index + 1) } else { String::new() };
        source.push_str(&format!("action check_{index}(input before: Token, witness payload: [u8; 8]{extra}) {{ verification\nrequire before.amount == {}\nrequire payload == before.expected\nconsume before\n}}\n",index+1));
    }
    compile_artifact(
        &source,
        CompileOptions {
            edition: NEXT_EDITION,
            target: Some("riscv64-elf".into()),
            target_profile: Some("ckb".into()),
            diagnostics: cellscript::CompilerDiagnostics::default(),
            ..Default::default()
        },
        ArtifactDeclaration {
            name: "LabelOrdinalPolicy".into(),
            context: ArtifactContext::TypeGroup { resource: "Token".into() },
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            actions: (0..count).map(|index| ArtifactAction { tag: index, action: format!("check_{index}") }).collect(),
            common_checks: vec![],
        },
        cellscript::ExecutableSurfacePolicy::DenyFailClosed,
    )
    .expect("large policy must not interpret label ordinals as errors")
}

#[test]
fn policy_failure_label_ordinals_are_not_runtime_error_codes() {
    let mut reaches_large_ordinal = false;
    for count in [32, 64] {
        let compiled = policy_fixture_with_layouts(count, true);
        compiled.validate().expect("independent checker accepts the large policy");
        let record = compiled.verified_lowering_record.as_ref().unwrap();
        reaches_large_ordinal |= record.blocks.iter().any(|block| {
            block.machine_label.as_deref().is_some_and(|label| {
                label.rsplit_once("_fail_").and_then(|(_, suffix)| suffix.parse::<u32>().ok()).is_some_and(|ordinal| ordinal > 255)
            })
        });
        assert!(record.runtime_error_exits.iter().all(|exit| (1..=255).contains(&exit.code)));
        assert!(!record.verifier_failure_exits.is_empty(), "actual failure sites must remain bound");
    }
    assert!(reaches_large_ordinal, "fixtures reach ordinals beyond the runtime exit-code range");
}

#[test]
fn relaxed_policy_branches_reject_condition_skip_and_call_mutations() {
    let valid = Fixture::from_result(large_policy_fixture(64));
    assert_eq!(valid.check(), Ok(()));
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let pair = elf
        .instructions
        .windows(2)
        .find(|pair| {
            pair[0].word & 0x7f == 0x63
                && pair[1].word & 0xfff == 0x6f
                && valid
                    .record
                    .blocks
                    .iter()
                    .any(|block| block.range.contains(pair[0].address) && block.range.contains(pair[1].address))
                && elf.control_flow.iter().any(|flow| flow.address == pair[0].address && flow.target == pair[0].address + 8)
        })
        .expect("64 actions exercise a relaxed policy branch");
    for (address, word) in [
        (pair[0].address, pair[0].word ^ (1 << 12)), // Invert the accepted condition.
        (pair[0].address, pair[0].word ^ (1 << 9)),  // Skip twelve bytes instead of eight.
        (pair[1].address, pair[1].word | (1 << 7)),  // Turn the jump into a call.
    ] {
        let mut changed = valid.clone();
        changed.replace_machine_word(address, word);
        assert!(changed.check().is_err(), "hash-rebound relaxed-branch mutation at {address:#x} must reject");
    }
}

#[test]
fn compact_policy_adapter_rejects_frame_capacity_and_saved_return_address_mutations() {
    let valid = Fixture::from_result(large_policy_fixture(1));
    let adapter = valid.record.entries.iter().find(|entry| entry.name.starts_with(".Lpolicy_action_adapter_")).unwrap();
    assert_eq!(adapter.frame_size_bytes, 32, "eight-byte witness needs only a 32-byte private frame");
    let base = valid.record.blocks.iter().find(|block| block.id == adapter.entry_block).unwrap().range.start;
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let word = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
    // These change stack ownership or the bound before copying, even after
    // outer hashes and block digests are rebound by the mutation helper.
    for (address, replacement) in
        [(base, word(base) ^ (16 << 20)), (base + 4, word(base + 4) ^ (1 << 10)), (base + 8, word(base + 8) ^ (1 << 20))]
    {
        let mut changed = valid.clone();
        changed.replace_machine_word(address, replacement);
        assert!(changed.check().is_err(), "compact adapter mutation must be independently rejected");
    }
    let mut old = valid.clone();
    old.record.schema = "cellscript-verified-lowering-record-v8".into();
    old.record.version = 8;
    old.rebind_sidecars();
    assert!(old.check().is_err(), "v8 cannot certify the v9 private-frame contract");
}

#[test]
fn shared_policy_decoder_has_only_bound_callers_and_cannot_clobber_its_borrowed_frame() {
    let valid = Fixture::from_result(large_policy_fixture(4));
    assert_eq!(valid.check(), Ok(()));
    let decoders: Vec<_> = valid.record.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")).collect();
    assert_eq!(decoders.len(), 1, "four identical contracts use one decoder");
    let decoder = decoders[0];
    assert_eq!(decoder.frame_size_bytes, 0, "the direct stub owns the borrowed frame");
    let start = valid.record.blocks.iter().find(|block| block.id == decoder.entry_block).unwrap().range.start;
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let callers: Vec<_> = elf.control_flow.iter().filter(|flow| flow.target == start).collect();
    assert_eq!(callers.len(), 4);
    let word = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
    let copy = valid
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == decoder.id
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lpolicy_args_copy_"))
        })
        .unwrap();
    let done = valid
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == decoder.id
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_witness_done_"))
        })
        .unwrap();
    for (address, replacement) in [
        (callers[0].address, word(callers[0].address) & !(31 << 7)),
        (start, word(start) ^ (8 << 20)),
        (copy.range.start + 8, (word(copy.range.start + 8) & !(31 << 7)) | (1 << 7)),
        (done.range.start, 0x01010113), // addi sp, sp, 16 instead of ret.
    ] {
        let mut changed = valid.clone();
        changed.replace_machine_word(address, replacement);
        assert!(changed.check().is_err(), "shared decoder mutation at {address:#x} must reject after rebinding");
    }
    let distinct = policy_fixture_with_layouts(4, true);
    assert!(
        !distinct.verified_lowering_record.unwrap().entries.iter().any(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")),
        "different payload contracts retain dedicated decoders"
    );
}

const RUNTIME_PROVENANCE_SOURCE: &str = r#"
module artifact_checker_runtime_provenance

resource Token has store { amount: u64 }

action inspect(witness source_index: u64, witness expected_data_hash: Hash) -> u64 {
    let input = ckb::input<Token>(source_index)
    let dep = ckb::cell_dep(source_index)
    let transaction_hash = ckb::transaction_hash()
    require input.capacity > 0
    require dep.data_hash == expected_data_hash
    require transaction_hash != Hash::zero()
    return 0
}
"#;

const BOUNDED_WITNESS_SOURCE: &str = r#"
module artifact_checker_bounded_witness

action inspect() -> u64 {
    verification
        let witness_args = witness::args(0)
        let bytes = witness::bounded_lock(witness_args, 64)
        require bytes.size <= 64
        require witness::byte(bytes, 0) >= 0
        require witness::blake2b(bytes) != Hash::zero()
        return 0
}
"#;

const SIGHASH_ZERO_LOCK_SOURCE: &str = r#"
module artifact_checker_sighash_zero_lock

action inspect() -> u64 {
    verification
        let digest = env::sighash_all_zero_lock(4, 8, 4, 4096)
        return 0
}
"#;

const HEADER_DEP_SOURCE: &str = r#"
module artifact_checker_header_dep

action inspect(witness source_index: u64) -> u64 {
    let header = ckb::header_dep(source_index)
    return ckb::epoch_number_to_u64(header.epoch_number)
        + ckb::block_number_to_u64(header.epoch_start_block_number)
        + ckb::epoch_length_to_u64(header.epoch_length)
        + ckb::block_number_to_u64(header.block_number)
        + ckb::timestamp_millis_to_u64(header.timestamp)
}
"#;

const SCRIPT_HASH_SOURCE: &str = r#"
module artifact_checker_script_hash

action inspect() -> u64 {
    let value = script::new(Hash::zero(), script::hash_type_data2(), script::args(b"ckb!"))
    let complete: ScriptHash = script::hash(value)
    require complete != ckb::script_hash(Hash::zero())
    return 0
}
"#;

const COMMITTED_STATE_SOURCE: &str = r#"
module artifact_checker_committed_state

struct State { counter: u64 }

action inspect(witness expected: Commitment<State>, witness opening: Opening<State>) -> u64 {
    verification
        let state = commitment::open(expected, opening)
        let successor = State { counter: state.counter + 1 }
        let next = commitment::commit(successor)
        require next != expected
        return 0
}
"#;

const COMMITTED_STATE_FIELD_SOURCE: &str = r#"
module artifact_checker_committed_state_field

struct State { counter: u64 }

struct Envelope {
    nonce: u64,
    state_commitment: Commitment<State>,
}

action inspect(witness envelope: Envelope, witness opening: Opening<State>) -> u64 {
    verification
        let state = commitment::open(envelope.state_commitment, opening)
        return state.counter
}
"#;

const EXACT_HANDLE_SOURCE: &str = r#"
module artifact_checker_exact_handle

resource Token has store { amount: u64 }

action inspect(witness handle: ExactScriptHandle) -> u64 {
    let dep = ckb::cell_dep(0)
    ckb::require_cell_dep_exact_verifier_handle(
        dep,
        handle,
        Hash::from_bytes(b"0123456789abcdef0123456789abcdef")
    )
    return 0
}
"#;

const DEPLOYMENT_LINE_HANDLE_SOURCE: &str = r#"
module artifact_checker_deployment_line_handle

resource Token has store { amount: u64 }

action inspect(witness line: DeploymentLineHandle) -> u64 {
    let admission = ckb::cell_dep(0)
    let code = ckb::cell_dep(1)
    ckb::require_cell_dep_deployment_line_verifier_handle(
        admission,
        code,
        line,
        Hash::from_bytes(b"0123456789abcdef0123456789abcdef")
    )
    return 0
}
"#;

#[derive(Clone)]
struct Fixture {
    artifact: Vec<u8>,
    metadata: Value,
    record: VerifiedLoweringRecord,
    source_map: SourceArtifactMap,
}

impl Fixture {
    fn new() -> Self {
        let result =
            compile(FIXTURE_SOURCE, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        Self::from_result(result)
    }

    fn from_result(result: CompileResult) -> Self {
        let fixture = Self {
            artifact: result.artifact_bytes,
            metadata: serde_json::to_value(result.metadata).unwrap(),
            record: result.verified_lowering_record.unwrap(),
            source_map: result.source_artifact_map.unwrap(),
        };
        fixture.check().unwrap();
        fixture
    }

    fn from_source(source: &str) -> Self {
        let result = compile(source, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        Self::from_result(result)
    }

    fn check(&self) -> Result<(), CheckerRejectionCode> {
        check_bundle_values(&self.artifact, &self.metadata, &self.record, &self.source_map, &CheckerBudgets::default())
            .map(|_| ())
            .map_err(|error| error.code)
    }

    fn rebind_sidecars(&mut self) {
        let record_hash = canonical_hash(LOWERING_RECORD_SCHEMA, &self.record).unwrap();
        self.source_map.lowering_record_hash = record_hash.clone();
        let source_map_hash = canonical_hash(SOURCE_MAP_SCHEMA, &self.source_map).unwrap();
        let verified_bundle_id = canonical_hash(
            "cellscript-verified-bundle-id-v1",
            &(
                self.record.artifact_hash.as_str(),
                self.record.typed_semantics_hash.as_str(),
                self.record.compatibility_profile_hash.as_str(),
                record_hash.as_str(),
                source_map_hash.as_str(),
                self.source_map.source_digest.as_str(),
            ),
        )
        .unwrap();
        self.metadata["verified_artifact"]["lowering_record_hash"] = Value::String(record_hash);
        self.metadata["verified_artifact"]["source_map_hash"] = Value::String(source_map_hash);
        self.metadata["verified_artifact"]["verified_bundle_id"] = Value::String(verified_bundle_id);
    }

    fn rebind_typed_semantics(&mut self) {
        self.record.typed_semantics_hash =
            canonical_hash(cellscript_artifact_checker::TYPED_SEMANTICS_SCHEMA, &self.record.typed_semantics).unwrap();
        self.metadata["typed_semantics"] = serde_json::to_value(&self.record.typed_semantics).unwrap();
        self.metadata["typed_semantics_hash"] = Value::String(self.record.typed_semantics_hash.clone());
        self.rebind_sidecars();
    }

    fn rebind_layered_identities(&mut self) {
        let typed = &mut self.record.typed_semantics;
        let foundation = &mut typed.foundation;
        foundation.identities.core_semantic_id = canonical_hash(
            "cellscript-core-semantic-id-v2",
            &(
                typed.failure_semantics,
                &typed.types,
                &foundation.roles,
                &foundation.dispositions,
                &foundation.claims,
                &foundation.legacy_nodes,
            ),
        )
        .unwrap();
        let roots = foundation
            .provenance
            .nodes
            .iter()
            .filter(|node| !matches!(node.provenance, cellscript_artifact_checker::ValueProvenance::Derived { .. }))
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>();
        foundation.identities.entry_contract_id = canonical_hash(
            "cellscript-entry-contract-id-v1",
            &(
                foundation.identities.core_semantic_id.as_str(),
                &foundation.entry_contract,
                roots,
                foundation.entry_contract.entry_payload_abi.as_str(),
                foundation.entry_contract.witness_placement_abi.as_str(),
            ),
        )
        .unwrap();
        foundation.identities.artifact_contract_id = canonical_hash(
            "cellscript-artifact-contract-id-v1",
            &(foundation.identities.entry_contract_id.as_str(), &foundation.artifact_contract),
        )
        .unwrap();
        for (key, value) in [
            ("core_semantic_id", &foundation.identities.core_semantic_id),
            ("entry_contract_id", &foundation.identities.entry_contract_id),
            ("artifact_contract_id", &foundation.identities.artifact_contract_id),
        ] {
            self.metadata["verified_artifact"][key] = Value::String(value.clone());
        }
        self.rebind_typed_semantics();
    }

    fn bind_artifact_identity(&mut self) {
        let artifact_hash = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&self.artifact));
        self.record.artifact_hash.clone_from(&artifact_hash);
        self.record.artifact_size_bytes = self.artifact.len() as u64;
        self.source_map.artifact_hash.clone_from(&artifact_hash);
        self.metadata["artifact_hash"] = Value::String(artifact_hash);
        self.metadata["artifact_size_bytes"] = Value::from(self.artifact.len() as u64);
        self.metadata["verified_artifact"]["deployable_artifact_id"] = Value::String(self.record.artifact_hash.clone());
        self.rebind_sidecars();
    }

    fn replace_machine_word(&mut self, address: u64, word: u32) {
        self.replace_machine_words(&[(address, word)]);
    }

    fn redirect_conditional_branch(&mut self, address: u64, target: u64) {
        let elf = parse_elf(&self.artifact, CheckerBudgets::default().instructions).unwrap();
        let word = elf.instructions.iter().find(|i| i.address == address).unwrap().word;
        assert_eq!(word & 0x7f, 0x63);
        let origin = self.record.blocks.iter().find(|block| block.range.contains(address)).unwrap().id.clone();
        let destination = self.record.blocks.iter().find(|block| block.range.start == target).unwrap().id.clone();
        self.record.edges.iter_mut().find(|edge| edge.from == origin && edge.kind == EdgeKind::ConditionalTaken).unwrap().to =
            destination;
        let mut reachable = std::collections::BTreeSet::new();
        let mut pending = self.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if reachable.insert(id.clone()) {
                pending.extend(self.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
            }
        }
        for block in &mut self.record.blocks {
            block.reachable = reachable.contains(&block.id);
        }
        self.record.canonicalize();
        let displacement = target as i64 - address as i64;
        assert!((-4096..4096).contains(&displacement) && displacement % 2 == 0);
        let immediate = displacement as u32;
        let branch = (word & 0x01fff07f)
            | (((immediate >> 12) & 1) << 31)
            | (((immediate >> 5) & 0x3f) << 25)
            | (((immediate >> 1) & 15) << 8)
            | (((immediate >> 11) & 1) << 7);
        self.replace_machine_word(address, branch);
    }

    fn replace_machine_words(&mut self, replacements: &[(u64, u32)]) {
        let elf = parse_elf(&self.artifact, CheckerBudgets::default().instructions).unwrap();
        for &(address, word) in replacements {
            let offset = (elf.text.offset + address - elf.text.address) as usize;
            self.artifact[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        }
        for block in &mut self.record.blocks {
            let start = (elf.text.offset + block.range.start - elf.text.address) as usize;
            let end = (elf.text.offset + block.range.end - elf.text.address) as usize;
            block.byte_digest =
                cellscript_artifact_checker::domain_hash_bytes("cellscript-machine-block-v1", &self.artifact[start..end]);
        }
        self.bind_artifact_identity();
    }
}

#[test]
fn instance_runtime_checks_and_transitive_dependencies_reject_rehashed_machine_changes() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("foreign.cell"),
        "module foreign::documents\npublic struct Marker has copy, drop { value: u64 }\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("main.cell"), "module observer\nuse foreign::documents::Marker\nresource Document has store { digest: Hash }\npublic action verify(witness handle: InstanceScriptHandle<foreign::documents>) { verification ckb::require_instance_script_participant_from_args(ckb::cell_dep(0), handle, ckb::input<Document>(1)) }\n").unwrap();
    let valid = Fixture::from_result(
        cellscript::compile_path_with_executable_surface_policy(
            camino::Utf8PathBuf::from_path_buf(directory.path().join("main.cell")).unwrap(),
            CompileOptions {
                edition: NEXT_EDITION,
                target: Some("riscv64-elf".into()),
                target_profile: Some("ckb".into()),
                source_contracts: true,
                ..Default::default()
            },
            None,
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap(),
    );
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let owner = "runtime:__ckb_require_instance_script_participant_from_args";
    let mut changed_count = 0;
    for instruction in &elf.instructions {
        if !valid.record.blocks.iter().any(|block| block.owner_entry == owner && block.range.contains(instruction.address)) {
            continue;
        }
        let word = instruction.word;
        let conditional = word & 0x7f == 0x63;
        let byte_load = word & 0x7f == 0x03 && (word >> 12) & 7 == 4;
        let constant = word & 0x7f == 0x13
            && (word >> 12) & 7 == 0
            && (word >> 15) & 31 == 0
            && matches!((word >> 7) & 31, 5 | 6 | 7 | 12 | 13 | 14 | 15);
        if !conditional && !byte_load && !constant {
            continue;
        }
        let mut changed = valid.clone();
        changed.replace_machine_word(instruction.address, word ^ if conditional { 1 << 12 } else { 1 << 20 });
        assert!(changed.check().is_err(), "rehashed instance check accepted at {:#x}", instruction.address);
        changed_count += 1;
    }
    assert!(changed_count > 200, "all canonical-field, bounds, predicate, subject and failure branches must be exercised");
    for helper in [
        "__ckb_require_cell_dep_open_script_handle",
        "__ckb_hash_blake2b_var",
        "__cellscript_blake2b_compress",
        "__cellscript_memcpy_fixed",
        "__cellscript_memcmp_fixed",
    ] {
        let owner = format!("runtime:{helper}");
        let instruction = elf
            .instructions
            .iter()
            .find(|instruction| {
                valid.record.blocks.iter().any(|block| block.owner_entry == owner && block.range.contains(instruction.address))
                    && instruction.word & 0x7f == 0x03
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(instruction.address, instruction.word ^ (1 << 20));
        assert!(changed.check().is_err(), "unchecked runtime dependency {helper}");
    }
    // Reclassification must not remove the machine profile obligation.
    let mut changed = valid.clone();
    let operation = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|op| op.call.as_ref().is_some_and(|call| call.target == "__ckb_require_instance_script_participant_from_args"))
        .unwrap();
    operation.call.as_mut().unwrap().target = "__ckb_require_open_script_participant_from_args".into();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn exact_zk_machine_mutations_reject_after_hash_rebinding() {
    let hash = |byte: u8| format!("Hash::from_bytes(b\"{}\")", format!("\\x{byte:02x}").repeat(32));
    let source = format!(
        r#"module zk_mutations
action update(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle) -> u64 {{
    verification
    zk::require_valid("transition", proof, ckb::cell_dep(0), verifier, {}, {}, {}, {})
    return 0
}}
"#,
        hash(1),
        hash(2).replace("Hash::", "VerificationKeyCommitment::"),
        hash(3),
        hash(4)
    );
    let valid = Fixture::from_result(
        compile(
            &source,
            CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
        )
        .unwrap(),
    );
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let begin = valid
        .record
        .blocks
        .iter()
        .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lzk_begin_")))
        .unwrap()
        .range
        .start;
    let end = valid
        .record
        .blocks
        .iter()
        .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lzk_verified_")))
        .unwrap()
        .range
        .start;
    let instructions: Vec<_> =
        elf.instructions.iter().filter(|instruction| begin <= instruction.address && instruction.address < end).collect();
    // Request identities, context source/index, read/write lengths, mandatory
    // result conditions and error exits. Rehashing must not bless any change.
    let mut changed_count = 0;
    for instruction in instructions {
        let word = instruction.word;
        let opcode = word & 0x7f;
        let rd = (word >> 7) & 31;
        let source1 = (word >> 15) & 31;
        let function = (word >> 12) & 7;
        let is_constant = opcode == 0x13 && function == 0 && source1 == 0 && matches!(rd, 5 | 6 | 13 | 14 | 15);
        let is_checked_branch = opcode == 0x63 && function == 0;
        let is_dependency_shift = opcode == 0x13 && matches!(function, 1 | 5) && rd == 10 && source1 == 10;
        if !is_constant && !is_checked_branch && !is_dependency_shift {
            continue;
        }
        let mut changed = valid.clone();
        changed.replace_machine_word(instruction.address, word ^ if is_checked_branch { 1 << 12 } else { 1 << 20 });
        assert!(changed.check().is_err(), "ZK machine mutation accepted at {:#x}: {word:#x}", instruction.address);
        changed_count += 1;
    }
    let stack_store = elf
        .instructions
        .iter()
        .find(|instruction| {
            let word = instruction.word;
            begin <= instruction.address
                && instruction.address < end
                && word & 0x7f == 0x23
                && (word >> 12) & 7 == 3
                && (word >> 15) & 31 == 2
                && (word >> 20) & 31 == 5
        })
        .expect("ZK canonical request has a stack store");
    let mut changed_stack = valid.clone();
    changed_stack.replace_machine_word(stack_store.address, stack_store.word ^ (1 << 10));
    assert!(changed_stack.check().is_err(), "accepted changed request stack slot");
    assert!(changed_count > 150);
    let mut changed = valid.clone();
    changed.metadata["runtime"]["zk_verifiers"][0]["verification_key_hash"] = Value::String("ff".repeat(32));
    changed.rebind_sidecars();
    assert!(changed.check().is_err());
    for (field, value) in [
        ("request_bytes", serde_json::json!(465)),
        ("max_calls", serde_json::json!(2)),
        ("max_cycles", serde_json::json!(250_000_001)),
        ("context_binding", serde_json::json!("witness-selected")),
    ] {
        let mut changed = valid.clone();
        changed.metadata["runtime"]["zk_verifiers"][0][field] = value;
        changed.rebind_sidecars();
        assert!(changed.check().is_err(), "accepted changed ZK {field}");
    }
    let mut changed = valid.clone();
    changed.metadata["runtime"]["zk_verifiers"][0]["source"]["fields"][0] = serde_json::json!("domain=witness");
    changed.rebind_sidecars();
    assert!(changed.check().is_err());
}

fn bounded_lock_handle(metadata: &mut Value) -> &mut Value {
    metadata["runtime"]["transaction_view_handles"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|handle| handle["handle_type"] == "WitnessBytesView<lock,64>")
        .expect("bounded lock witness handle")
}

fn exact_handle_operation(fixture: &mut Fixture) -> &mut TypedSemanticOperation {
    fixture
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "__ckb_require_cell_dep_exact_verifier_handle"))
        .expect("exact verifier handle operation")
}

fn deployment_line_handle_operation(fixture: &mut Fixture) -> &mut TypedSemanticOperation {
    fixture
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| {
            operation.call.as_ref().is_some_and(|call| call.target == "__ckb_require_cell_dep_deployment_line_verifier_handle")
        })
        .expect("deployment line verifier handle operation")
}

#[test]
fn checker_rejects_vm2_isa_or_data2_contract_tampering() {
    let valid = Fixture::new();
    for pointer in ["/target_profile/minimum_vm_version", "/constraints/ckb/profile_abi_contract/minimum_vm_version"] {
        let mut changed = valid.clone();
        *changed.metadata.pointer_mut(pointer).unwrap() = Value::from(1);
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    changed.metadata["target_profile"]["riscv_isa"] = Value::String("rv64imac".to_string());
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    changed.metadata["target_profile"]["deployment_hash_types"] = serde_json::json!(["data1"]);
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
}

#[test]
fn checker_accepts_type_hash_profile_and_rejects_cross_profile_hash_type_tampering() {
    let valid = Fixture::from_result(
        compile(
            FIXTURE_SOURCE,
            CompileOptions {
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb-type-hash".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.metadata["target_profile"]["deployment_hash_types"], serde_json::json!(["type"]));
    assert_eq!(
        valid.metadata["constraints"]["ckb"]["hash_type_policy"]["default_script_hash_type"],
        Value::String("type".to_string())
    );

    let mut changed = valid.clone();
    changed.metadata["target_profile"]["deployment_hash_types"] = serde_json::json!(["data2"]);
    changed.metadata["constraints"]["ckb"]["profile_abi_contract"]["deployment_hash_types"] = serde_json::json!(["data2"]);
    changed.metadata["constraints"]["ckb"]["hash_type_policy"]["default_script_hash_type"] = Value::String("data2".to_string());
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
}

#[test]
fn checker_rejects_runtime_access_provenance_tampering_after_hash_rebinding() {
    let valid = Fixture::from_result(
        compile(
            RUNTIME_PROVENANCE_SOURCE,
            CompileOptions {
                edition: NEXT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.metadata["metadata_schema_version"], Value::from(72));
    assert_eq!(
        valid.metadata["runtime"]["ckb_runtime_access_provenance_contract"],
        Value::String("cellscript-ckb-runtime-access-provenance-v1".to_string())
    );

    let mut changed = valid.clone();
    changed.metadata["runtime"]["ckb_runtime_access_provenance_contract"] = Value::String("tampered".to_string());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mutate_matching_accesses = |metadata: &mut Value, mutation: fn(&mut Value)| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let accesses = metadata.pointer_mut(pointer).and_then(Value::as_array_mut).unwrap();
            let access = accesses
                .iter_mut()
                .find(|access| access["operation"] == "cell-data-hash-field")
                .expect("dynamic CellDep data-hash access");
            mutation(access);
        }
    };

    let mut changed = valid.clone();
    mutate_matching_accesses(&mut changed.metadata, |access| {
        access["provenance"]["index"]["max_inclusive"] = Value::from(u64::from(u32::MAX) - 1);
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    mutate_matching_accesses(&mut changed.metadata, |access| {
        access["provenance"]["source"]["resolved_source"] = Value::String("HeaderDep".to_string());
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    mutate_matching_accesses(&mut changed.metadata, |access| {
        access["provenance"]["range"]["length"]["value"] = Value::from(0);
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    changed.metadata["actions"][0]["ckb_runtime_accesses"][0]["binding"] = Value::String("tampered".to_string());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let set_transaction_hash_access_field = |metadata: &mut Value, field: &str, value: &Value| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let accesses = metadata.pointer_mut(pointer).and_then(Value::as_array_mut).unwrap();
            let access = accesses
                .iter_mut()
                .find(|access| access["operation"] == "transaction-hash")
                .expect("canonical transaction-hash access");
            access[field] = value.clone();
        }
    };

    for mutation in [
        ("operation", Value::String("transaction-hash-unbound".to_string())),
        ("syscall", Value::String("LOAD_TRANSACTION".to_string())),
        ("binding", Value::String("ckb::transaction_bytes".to_string())),
    ] {
        let mut changed = valid.clone();
        set_transaction_hash_access_field(&mut changed.metadata, mutation.0, &mutation.1);
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
        let access = changed
            .metadata
            .pointer_mut(pointer)
            .and_then(Value::as_array_mut)
            .unwrap()
            .iter_mut()
            .find(|access| access["operation"] == "transaction-hash")
            .expect("canonical transaction-hash access");
        access["provenance"]["range"]["length"]["value"] = Value::from(31);
        access["provenance"]["range"]["length"]["max_inclusive"] = Value::from(31);
    }
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    let handle = changed.metadata["runtime"]["transaction_view_handles"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|handle| handle["handle_type"] == "InputView<Token>")
        .expect("typed Input view handle");
    handle["provenance"]["index"]["binding"] = Value::String(String::new());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
}

#[test]
fn checker_binds_header_dep_fields_widths_syscalls_and_terminal_errors_to_machine_code() {
    let valid = Fixture::from_result(
        compile(
            HEADER_DEP_SOURCE,
            CompileOptions {
                edition: NEXT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );

    let mutate_access = |metadata: &mut Value, operation: &str, mutation: &dyn Fn(&mut Value)| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let access = metadata
                .pointer_mut(pointer)
                .and_then(Value::as_array_mut)
                .unwrap()
                .iter_mut()
                .find(|access| access["operation"] == operation)
                .unwrap_or_else(|| panic!("missing HeaderDep access {operation}"));
            mutation(access);
        }
    };
    for (operation, mutation) in [
        ("header-dep-epoch-number", ("syscall", Value::String("LOAD_HEADER".to_string()))),
        ("header-dep-epoch-start-block-number", ("binding", Value::String("HeaderDepView.timestamp".to_string()))),
        ("header-dep-epoch-length", ("source", Value::String("Input".to_string()))),
    ] {
        let mut changed = valid.clone();
        mutate_access(&mut changed.metadata, operation, &|access| access[mutation.0] = mutation.1.clone());
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    mutate_access(&mut changed.metadata, "header-dep-block-number", &|access| {
        access["provenance"]["range"]["length"]["value"] = Value::from(207);
        access["provenance"]["range"]["length"]["max_inclusive"] = Value::from(207);
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    let timestamp_call = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "__ckb_header_dep_timestamp_millis"))
        .expect("timestamp typed helper call");
    timestamp_call.call.as_mut().unwrap().return_type = "BlockNumber".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let scalar_site = valid
        .record
        .syscall_sites
        .iter()
        .find(|site| site.contract == "ckb-header-dep-epoch-number-v1")
        .expect("epoch-number syscall site")
        .clone();
    let full_site = valid
        .record
        .syscall_sites
        .iter()
        .find(|site| site.contract == "ckb-header-dep-block-number-v1")
        .expect("block-number syscall site")
        .clone();

    for mutate in [
        |site: &mut cellscript_artifact_checker::SyscallSite| site.syscall_number = Some(2081),
        |site: &mut cellscript_artifact_checker::SyscallSite| site.source_domain = "HeaderDep".to_string(),
        |site: &mut cellscript_artifact_checker::SyscallSite| site.index_domain = "unbounded".to_string(),
        |site: &mut cellscript_artifact_checker::SyscallSite| site.return_code_checked = false,
        |site: &mut cellscript_artifact_checker::SyscallSite| site.buffer_limit_bytes = 7,
    ] {
        let mut changed = valid.clone();
        let site = changed.record.syscall_sites.iter_mut().find(|site| site.address == scalar_site.address).unwrap();
        mutate(site);
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2417SyscallContractInvalid);
    }

    for delta in [-48_i64, -40, -12, -4, 4, 20] {
        let mut changed = valid.clone();
        let address = scalar_site.address.checked_add_signed(delta).unwrap();
        let word = parse_elf(&changed.artifact, CheckerBudgets::default().instructions)
            .unwrap()
            .instructions
            .iter()
            .find(|instruction| instruction.address == address)
            .unwrap()
            .word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        assert_eq!(changed.check(), Err(CheckerRejectionCode::V2417SyscallContractInvalid), "scalar delta {delta}");
    }

    for delta in [-44_i64, -36, -4, 4, 20, 28] {
        let mut changed = valid.clone();
        let address = full_site.address.checked_add_signed(delta).unwrap();
        let word = parse_elf(&changed.artifact, CheckerBudgets::default().instructions)
            .unwrap()
            .instructions
            .iter()
            .find(|instruction| instruction.address == address)
            .unwrap()
            .word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        assert_eq!(changed.check(), Err(CheckerRejectionCode::V2417SyscallContractInvalid), "full delta {delta}");
    }

    for (site, delta) in [(&scalar_site, 8_i64), (&full_site, 8), (&full_site, 40)] {
        let mut changed = valid.clone();
        let address = site.address.checked_add_signed(delta).unwrap();
        let word = parse_elf(&changed.artifact, CheckerBudgets::default().instructions)
            .unwrap()
            .instructions
            .iter()
            .find(|instruction| instruction.address == address)
            .unwrap()
            .word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        assert_eq!(changed.check(), Err(CheckerRejectionCode::V2414ControlFlowInvalid), "terminal error delta {delta}");
    }

    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::from_result(
            compile(
                HEADER_DEP_SOURCE,
                CompileOptions {
                    edition: NEXT_EDITION,
                    target: Some("riscv64-elf".to_string()),
                    target_profile: Some("ckb".to_string()),
                    diagnostics,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        assert_eq!(
            fixture.record.syscall_sites.iter().filter(|site| site.contract.starts_with("ckb-header-dep-")).count(),
            5,
            "strategy {diagnostics} must retain every specialized HeaderDep syscall site"
        );
    }

    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let invalid_source_error = valid
        .record
        .blocks
        .iter()
        .filter(|block| block.owner_entry == "runtime:__ckb_header_dep_epoch_number")
        .flat_map(|block| elf.instructions.iter().filter(move |instruction| block.range.contains(instruction.address)))
        .find(|instruction| {
            instruction.word & 0x7f == 0x13
                && (instruction.word >> 7) & 0x1f == 10
                && (instruction.word >> 15) & 0x1f == 0
                && (instruction.word as i32) >> 20 == 44
        })
        .expect("invalid HeaderDep source error")
        .address;
    let mut changed = valid;
    let word = elf.instructions.iter().find(|instruction| instruction.address == invalid_source_error).unwrap().word;
    changed.replace_machine_word(invalid_source_error, word ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);
}

#[test]
fn checker_binds_canonical_script_hash_metadata_types_and_machine_serialization() {
    let valid = Fixture::from_result(
        compile(
            SCRIPT_HASH_SOURCE,
            CompileOptions {
                edition: NEXT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.record.version, 9);

    let mutate_access = |metadata: &mut Value, mutation: fn(&mut Value)| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let access = metadata
                .pointer_mut(pointer)
                .and_then(Value::as_array_mut)
                .unwrap()
                .iter_mut()
                .find(|access| access["operation"] == "script-hash-v1")
                .expect("canonical Script hash access");
            mutation(access);
        }
    };
    for mutation in [
        |access: &mut Value| access["syscall"] = Value::String("SHA256".to_string()),
        |access: &mut Value| access["source"] = Value::String("Expression".to_string()),
        |access: &mut Value| access["provenance"]["source"]["origin"] = Value::String("implicit-lowering".to_string()),
        |access: &mut Value| {
            access["provenance"]["range"]["length"]["value"] = Value::from(56);
            access["provenance"]["range"]["length"]["max_inclusive"] = Value::from(56);
        },
    ] {
        let mut changed = valid.clone();
        mutate_access(&mut changed.metadata, mutation);
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    let call = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| operation.call.as_mut().filter(|call| call.target == "__ckb_script_hash"))
        .expect("typed Script hash call");
    call.return_type = "ScriptHash".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let script_entry =
        valid.record.entries.iter().find(|entry| entry.id == "runtime:__ckb_script_hash").expect("Script hash runtime entry");
    let base =
        valid.record.blocks.iter().find(|block| block.id == script_entry.entry_block).expect("Script hash entry block").range.start;
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    for delta in [36_u64, 108, 144, 188, 200, 252, 280, 300] {
        let mut changed = valid.clone();
        let address = base + delta;
        let word = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        assert_eq!(changed.check(), Err(CheckerRejectionCode::V2417SyscallContractInvalid), "machine delta {delta}");
    }

    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::from_result(
            compile(
                SCRIPT_HASH_SOURCE,
                CompileOptions {
                    edition: NEXT_EDITION,
                    target: Some("riscv64-elf".to_string()),
                    target_profile: Some("ckb".to_string()),
                    diagnostics,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        assert_eq!(fixture.check(), Ok(()), "strategy {diagnostics} must preserve the Script hash machine contract");
    }
}

#[test]
fn checker_binds_bounded_witness_owner_limit_range_and_typed_retyping() {
    let valid = Fixture::from_result(
        compile(
            BOUNDED_WITNESS_SOURCE,
            CompileOptions {
                edition: NEXT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.metadata["metadata_schema_version"], Value::from(72));

    let mutate_bounded_accesses = |metadata: &mut Value, mutation: fn(&mut Value)| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let accesses = metadata.pointer_mut(pointer).and_then(Value::as_array_mut).unwrap();
            for access in accesses
                .iter_mut()
                .filter(|access| access["operation"].as_str().is_some_and(|operation| operation.starts_with("witness-bounded-lock-")))
            {
                mutation(access);
            }
        }
    };

    let mut changed = valid.clone();
    bounded_lock_handle(&mut changed.metadata)["witness_owner"] = Value::String("entry".to_string());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    bounded_lock_handle(&mut changed.metadata)["max_bytes"] = Value::from(63);
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    bounded_lock_handle(&mut changed.metadata)["provenance"]["range"]["length"]["max_inclusive"] = Value::from(63);
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    mutate_bounded_accesses(&mut changed.metadata, |access| {
        access["provenance"]["range"]["offset"]["max_inclusive"] = Value::from(63);
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid;
    let local = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.locals)
        .find(|local| local.ty == "WitnessBytesView<lock,64>")
        .expect("typed bounded witness local");
    local.ty = "WitnessBytesView<signer,64>".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_binds_zero_lock_signing_domain_to_runtime_access_and_typed_call() {
    let valid = Fixture::from_result(
        compile(
            SIGHASH_ZERO_LOCK_SOURCE,
            CompileOptions {
                edition: NEXT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.metadata["metadata_schema_version"], Value::from(72));
    assert_eq!(valid.metadata["runtime"]["signing_message_domains"].as_array().unwrap().len(), 1);

    for (field, tampered) in [
        ("contract", "tampered-signing-domain"),
        ("digest_type", "Hash"),
        ("first_witness_lock_transform", "preserve-prefix-and-zero-signatures"),
        ("runtime_helper", "__tampered_sighash"),
    ] {
        let mut changed = valid.clone();
        changed.metadata["runtime"]["signing_message_domains"][0][field] = Value::String(tampered.to_string());
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    changed.metadata["runtime"]["signing_message_domains"] = serde_json::json!([]);
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid;
    changed.metadata["runtime"]["signing_message_domains"][0]["max_group_inputs"] = Value::from(5);
    for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
        let access = changed
            .metadata
            .pointer_mut(pointer)
            .and_then(Value::as_array_mut)
            .unwrap()
            .iter_mut()
            .find(|access| access["operation"] == "sighash-all-zero-lock-v1")
            .expect("bounded sighash runtime access");
        access["provenance"]["index"]["max_inclusive"] = Value::from(4);
    }
    changed.rebind_sidecars();
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .expect_err("metadata and access bounds cannot diverge from the typed call");
    assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
    assert!(error.message.contains("typed bounded sighash call"), "{error}");
}

#[test]
fn terminal_verifier_failures_reject_hash_rebound_machine_and_record_mutations() {
    let source = "module fatal_checker\naction main(value: u64) { verification require value > 0 }";
    for edition in [cellscript::CellScriptEdition::Edition2026, NEXT_EDITION] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition,
                    target: Some("riscv64-elf".into()),
                    diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let failure = valid.record.verifier_failure_exits.iter().find(|exit| exit.code == 5).unwrap().clone();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        // Small failure codes now materialize with a single ADDI, so the
        // recorded failure address points directly at the constant.
        let constant_last = elf.instructions.iter().find(|instruction| instruction.address == failure.address).unwrap();
        assert_eq!(constant_last.word >> 20, 5, "fixture uses the assembler's single-ADDI materialization");
        let sink = valid.record.entries.iter().find(|entry| entry.name == "__cellscript_abort").unwrap();
        let sink_block = valid.record.blocks.iter().find(|block| block.id == sink.entry_block).unwrap();
        let sink_start = sink_block.range.start;

        let mut changed = valid.clone();
        changed.replace_machine_word(failure.address, constant_last.word & 0x000f_ffff);
        assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);

        let mut changed = valid.clone();
        // The sink's exit-code constant is a single ADDI now, so the ECALL
        // sits directly at sink_start + 4: corrupting it trips the
        // instruction allowlist (V2413) instead of the constant check.
        let syscall_word = elf.instructions.iter().find(|instruction| instruction.address == sink_start + 4).unwrap().word;
        changed.replace_machine_word(sink_start + 4, syscall_word ^ (1 << 20));
        assert_code(&changed, CheckerRejectionCode::V2413InstructionInvalid);

        let mut changed = valid.clone();
        changed.replace_machine_word(sink_start + 8, 0x0000_8067); // ret instead of EXIT
        assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);

        let mut changed = valid.clone();
        changed.record.verifier_failure_exits.retain(|exit| exit.address != failure.address);
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);

        let mut changed = valid.clone();
        changed.record.verifier_failure_exits.iter_mut().find(|exit| exit.address == failure.address).unwrap().address += 4;
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);

        let mut changed = valid.clone();
        let success = changed
            .record
            .blocks
            .iter()
            .find(|block| {
                block.owner_entry == "action:main" && block.terminator == cellscript_artifact_checker::MachineTerminator::Return
            })
            .unwrap();
        let success_id = success.id.clone();
        let target = success.range.start;
        let jump_address = failure.address + 8;
        for edge in &mut changed.record.edges {
            if edge.from == failure.block_id && edge.kind == EdgeKind::Jump {
                edge.to = success_id.clone();
            }
        }
        changed.record.canonicalize();
        changed.replace_machine_word(jump_address, encode_jal((target as i64 - jump_address as i64) as i32));
        assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);

        // Keep the claimed CFG edge intact: its target block still contains the
        // forged address. The checker must reject entering after the error load.
        let incoming = elf.control_flow.iter().find(|edge| edge.target == failure.address).unwrap();
        let word = elf.instructions.iter().find(|instruction| instruction.address == incoming.address).unwrap().word;
        assert_eq!(word & 0x7f, 0x63, "require fixture branches to failure");
        let offset = (failure.address + 8) as i64 - incoming.address as i64;
        assert!((-4096..4096).contains(&offset));
        let immediate = offset as u32;
        let branch = (word & 0x01ff_f07f)
            | (((immediate >> 12) & 1) << 31)
            | (((immediate >> 5) & 0x3f) << 25)
            | (((immediate >> 1) & 0xf) << 8)
            | (((immediate >> 11) & 1) << 7);
        let mut changed = valid.clone();
        changed.replace_machine_word(incoming.address, branch);
        assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);

        let mut changed = valid.clone();
        let block = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .find(|block| block.runtime_error.is_some())
            .unwrap();
        block.runtime_error = None;
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

        assert_eq!(valid.metadata["typed_semantics"]["failure_semantics"], "current-vm-process-exit-v1");
        let mut malformed = valid.metadata["typed_semantics"].clone();
        malformed.as_object_mut().unwrap().remove("failure_semantics");
        assert!(serde_json::from_value::<cellscript_artifact_checker::TypedSemanticRecord>(malformed).is_err());
    }
}

#[test]
fn terminal_verifier_operations_cannot_be_inserted_before_continuation_after_rebinding() {
    let source = "module fatal_operation_checker\naction main(value: u64) { verification require value > 0 }";
    for edition in [cellscript::CellScriptEdition::Edition2026, NEXT_EDITION] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition,
                    target: Some("riscv64-elf".into()),
                    diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let failure = valid
            .record
            .typed_semantics
            .entries
            .iter()
            .flat_map(|entry| &entry.blocks)
            .find(|block| block.runtime_error.is_some())
            .unwrap()
            .operations
            .last()
            .unwrap()
            .clone();
        for (terminator, code) in [("return", 0), ("return", 20), ("verifier-failure", 5)] {
            let mut changed = valid.clone();
            let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "main").unwrap();
            let entry_id = entry.id.clone();
            let block = entry.blocks.iter_mut().find(|block| block.terminator == terminator).unwrap();
            let mut inserted = failure.clone();
            inserted.operands[0].constant = Some(TypedSemanticConstant::U64(code.to_string()));
            block.operations.insert(block.operations.len() - 1, inserted);
            for (index, operation) in block.operations.iter_mut().enumerate() {
                operation.index = u32::try_from(index).unwrap();
            }
            let block_id = block.id;
            let block_hash = canonical_hash("cellscript-typed-block-v1", block).unwrap();
            // Rebind even the per-block typed/machine hashes: rejection must
            // come from terminal semantics, not a stale sidecar digest.
            changed
                .record
                .entries
                .iter_mut()
                .find(|entry| entry.id == entry_id)
                .unwrap()
                .typed_blocks
                .iter_mut()
                .find(|binding| binding.id == block_id)
                .unwrap()
                .hash
                .clone_from(&block_hash);
            for machine_block in &mut changed.record.blocks {
                if machine_block.owner_entry == entry_id && machine_block.lowering_block_id == Some(block_id) {
                    machine_block.typed_block_hash = Some(block_hash.clone());
                }
            }
            changed.rebind_typed_semantics();
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        }
    }
}

#[test]
fn implicit_verifier_exit_inventory_cannot_be_hidden_by_renaming_the_sink() {
    let valid = Fixture::from_source("module implicit_failure\nfn divide(value: u64) -> u64 { return 7 / value }\naction main(value: u64) -> u64 { verification return divide(value) }");
    assert!(valid.record.typed_semantics.entries.iter().flat_map(|entry| &entry.blocks).all(|block| block.runtime_error.is_none()));
    let mut changed = valid.clone();
    changed.record.entries.iter_mut().find(|entry| entry.name == "__cellscript_abort").unwrap().name = "renamed_abort".into();
    changed.record.verifier_failure_exits.clear();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);
}

#[test]
fn common_policy_calls_keep_a_bounded_retained_body_contract_after_rebinding() {
    use cellscript::artifact::{compile_artifact, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    let source = r#"
module common_calls
resource Token has consume { amount: u64 }
fn checked(value: u64) -> u64 { return 7 / value }
action common() { verification require checked(7) == 1 }
action burn(input token: Token) { verification consume token }
"#;
    for edition in [cellscript::CURRENT_EDITION, NEXT_EDITION] {
        let valid = Fixture::from_result(
            compile_artifact(
                source,
                CompileOptions {
                    edition,
                    target: Some("riscv64-elf".into()),
                    diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                    ..Default::default()
                },
                ArtifactDeclaration {
                    name: "TokenPolicy".into(),
                    context: ArtifactContext::TypeGroup { resource: "Token".into() },
                    dispatch: ArtifactDispatch::PolicyWitnessV1,
                    actions: vec![ArtifactAction { tag: 7, action: "burn".into() }],
                    common_checks: vec!["common".into()],
                },
                cellscript::ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap(),
        );
        for mutation in
            ["unknown", "foreign-contract", "field", "wide", "reference-return", "physical-cell", "loop", "unsupported-error"]
        {
            let mut changed = valid.clone();
            let typed = &mut changed.record.typed_semantics;
            let binding = typed.entries.iter().find(|entry| entry.name == "burn").unwrap().cell_bindings[0].clone();
            if matches!(mutation, "unknown" | "foreign-contract") {
                let call = typed
                    .entries
                    .iter_mut()
                    .find(|entry| entry.name == "common")
                    .unwrap()
                    .blocks
                    .iter_mut()
                    .flat_map(|block| &mut block.operations)
                    .find_map(|operation| operation.call.as_mut())
                    .unwrap();
                if mutation == "unknown" {
                    call.target = "absent".into();
                } else {
                    call.contract = "raw-status".into();
                }
            } else {
                let callee = typed.entries.iter_mut().find(|entry| entry.name == "checked").unwrap();
                match mutation {
                    "field" => callee.blocks[0].operations[0].opcode = "field-access".into(),
                    "wide" => callee.params[0].ty = "u128".into(),
                    "reference-return" => callee.return_type = "&Token".into(),
                    "physical-cell" => callee.cell_bindings.push(binding),
                    "loop" => {
                        let id = callee.blocks[0].id;
                        callee.blocks[0].successors = vec![id];
                        callee.blocks[0].terminator = "jump".into();
                    }
                    "unsupported-error" => {
                        callee.blocks[0].runtime_error =
                            Some(cellscript_artifact_checker::TypedSemanticRuntimeError { code: 1, name: "syscall-failed".into() });
                        callee.blocks[0].terminator = "verifier-failure".into();
                    }
                    _ => unreachable!(),
                }
            }
            if mutation == "reference-return" {
                let call = typed
                    .entries
                    .iter_mut()
                    .find(|entry| entry.name == "common")
                    .unwrap()
                    .blocks
                    .iter_mut()
                    .flat_map(|block| &mut block.operations)
                    .find_map(|operation| operation.call.as_mut())
                    .unwrap();
                call.return_type = "&Token".into();
            }
            changed.rebind_typed_semantics();
            let error =
                cellscript_artifact_checker::validate_policy_metadata(&changed.metadata, &changed.record.typed_semantics).unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{edition:?}: {mutation}: {error}");
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        }
    }
}

const REFERENCE_ENTRY_SOURCE: &str = r#"
module artifact_checker_reference_fixture

resource Token has consume {
    amount: u64,
}

fn inspect(token: &Token) -> u64 {
    return token.amount
}

action main() -> u64 {
    verification
        return 0
}
"#;

const NATIVE_EDITION_2027_SOURCE: &str = r#"
module artifact_checker_native_2027

resource Token has store, replace, relock {
    owner: Address,
    amount: u64,
}

type_script TokenTransfer on type_group<Token> {
    entry transfer(
        input token: Token from group_input[0],
        witness recipient: Address from group_witness.input_type,
        output next: Token from group_output[0],
    ) {
        verify {
            enforce token.amount > 0
        }

        effects {
            replace token -> next {
                data {
                    owner = same
                    amount = same
                }
                identity = same
                type_script = same
                lock_script = exact_hash(recipient)
                capacity = same
                cardinality = one_to_one
            }
        }
    }
}
"#;

const NATIVE_FRESH_AUDIT_EDITION_2027_SOURCE: &str = r#"
module artifact_checker_native_fresh_2027
#[type_id("artifact-checker::Token:v1")]
resource Token has store, create, burn identity(ckb_type_id) { amount: u64 }
type_script TokenMint on type_group<Token> {
    entry mint(
        witness amount: u64 from group_witness.input_type,
        witness recipient: Address from group_witness.input_type,
        output next: Token from group_output[0],
    ) {
        verify { enforce amount > 0 }
        audit issuance_policy {
            expected_evidence = external_policy(recipient)
        }
        effects {
            fresh next {
                data { amount = amount }
                identity = ckb_type_id
                type_script = declared
                lock_script = exact_hash(recipient)
                capacity = builder_computed
                cardinality = one
            }
        }
    }
}
"#;

const NATIVE_RETIRE_EDITION_2027_SOURCE: &str = r#"
module artifact_checker_native_retire_2027
resource Note has store, consume, burn identity(field(note_id)) { note_id: u64, amount: u64 }
type_script NoteRetirement on type_group<Note> {
    entry retire_note(input note: Note from group_input[0]) {
        verify { enforce note.amount == 0 }
        effects {
            retire note {
                absence = field(note_id)
                data = discarded
                lock_script = none
                type_script = absent
                capacity = released
                cardinality = one
            }
        }
    }
}
"#;

const NATIVE_POOL_EDITION_2027_SOURCE: &str = r#"
module artifact_checker_native_pool_2027
resource Token has store, create, consume { owner: Address, amount: u64 }
type_script TokenPool on type_group<Token> {
    entry merge(
        input left: Token from group_input[0],
        input right: Token from group_input[1],
        witness recipient: Address from group_witness.input_type,
        output merged: Token from group_output[0],
    ) {
        verify { enforce left.amount > 0 }
        effects {
            pool value_flow {
                inputs { left, right }
                outputs { merged }
                data {
                    owner { merged = recipient }
                    amount = conserve
                }
                identity = pooled
                type_script = same
                lock_script { merged = exact_hash(recipient) }
                capacity = builder_computed
                cardinality = declared
            }
        }
    }
}
"#;

const LEGACY_LOCK_SOURCE: &str = r#"
module artifact_checker_lock_2027
resource Vault has store { owner: Address }
lock unlock(protected vault: Vault, lock_args owner: Address, witness claimed_owner: Address) -> bool {
    verification
        require vault.owner == owner
        require claimed_owner == owner
}
"#;

const NATIVE_LOCK_EDITION_2027_SOURCE: &str = r#"
module artifact_checker_lock_2027
resource Vault has store { owner: Address }
lock_script VaultOwner on lock_group {
    entry unlock(
        protected vault: Vault from group_input[0],
        lock_args owner: Address from current_script.args,
        witness claimed_owner: Address from group_witness.input_type,
    ) {
        verify {
            enforce vault.owner == owner
            enforce claimed_owner == owner
        }
    }
}
"#;

#[track_caller]
fn assert_code(fixture: &Fixture, expected: CheckerRejectionCode) {
    match fixture.check() {
        Ok(()) => panic!("mutation unexpectedly passed; expected {}", expected.as_str()),
        Err(actual) => assert_eq!(actual, expected),
    }
}

#[test]
fn signed_literal_rejects_unsigned_overflow_after_rebinding() {
    let source = "module signed_literal\naction inspect() -> u64 { let value: i32 = 7\nlet negative = -value\nrequire negative < value\nreturn 0 }";
    let valid = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                edition: NEXT_EDITION,
                target: Some("riscv64-elf".into()),
                diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    valid.check().unwrap();
    for value in ["2147483648", "4294967295", "4294967296", "-1"] {
        let mut changed = valid.clone();
        let constant = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find_map(|op| match &mut op.detail {
                TypedSemanticOperationDetail::Constant { value: TypedSemanticConstant::U32(value) } => Some(value),
                _ => None,
            })
            .expect("contextual signed literal");
        *constant = value.to_string();
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn allocated_record_contracts_retain_unused_layouts_and_reject_forged_types() {
    let source = r#"
module record_proof
struct Inner { amount: u64 }
struct Value { inner: Inner }
struct Unused { bytes: [u8; 32] }
collections Work { limits { bytes: 512, allocations: 4 }
values: Vec<Value> { items: 4 }
unused: OrderedMap<u64, Unused> { items: 4 }
}
fn read(view: &Work) -> Value { return view.values.get(0) }
action inspect(witness value: Value) -> u64 {
    let mut work = Work::new()
    work.values.push(value)
    let copied = read(&work)
    work.drop()
    require copied.inner.amount == value.inner.amount
    return 0
}
"#;
    let valid = Fixture::from_result(
        compile(source, CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
    );
    valid.check().unwrap();
    for name in ["Inner", "Value", "Unused"] {
        assert!(valid.record.typed_semantics.types.iter().any(|ty| ty.name == name), "lost {name}");
    }
    // Rebind all uses of the contract together so owner-consistency alone
    // cannot reject these missing, oversized, borrowed or Cell-like layouts.
    for forged in ["Unknown", "[u8; 65]", "&Value", "Work"] {
        let mut changed = valid.clone();
        for op in changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
        {
            if let TypedSemanticOperationDetail::AllocatedValue { contract, .. } = &mut op.detail {
                let cellscript_artifact_checker::allocated_values::MemberKind::OrderedMap { value, .. } =
                    &mut contract.members[1].shape
                else {
                    unreachable!()
                };
                *value = forged.into();
            }
        }
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn allocated_allocations_reject_rebound_budget_state_and_zeroing_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [64, 65536] {
            let source = format!("module allocation\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 2 }}\nvalues: Vec<u64> {{ items: 4 }} }}\naction inspect() -> u64 {{ let mut first = Work::new()\nfirst.values.reserve(1)\nlet mut moved = first\nmoved.values.push(7)\nlet mut second = Work::new()\nsecond.values.push(8)\nrequire moved.values.get(0) == 7\nreturn second.values.get(0) }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let begin = valid
                .record
                .blocks
                .iter()
                .filter(|block| block.allocated_allocation_labels.iter().any(|label| label.ends_with("_begin")))
                .min_by_key(|block| block.range.start)
                .unwrap();
            let label = begin.allocated_allocation_labels.iter().find(|label| label.ends_with("_begin")).unwrap();
            let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
            let end = valid.record.blocks.iter().find(|block| block.allocated_allocation_labels.contains(&end_label)).unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let body =
                elf.instructions.iter().filter(|i| begin.range.start <= i.address && i.address < end.range.start).collect::<Vec<_>>();
            let first_store = body.iter().find(|i| i.word & 0x7f == 0x23).unwrap().address;
            let reject = |fixture: &Fixture| {
                let error = check_bundle_values(
                    &fixture.artifact,
                    &fixture.metadata,
                    &fixture.record,
                    &fixture.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.message.contains("allocated allocation"), "{error}");
            };
            let mut mutations = 0;
            for instruction in &body {
                let word = instruction.word;
                let opcode = word & 0x7f;
                // Preserve branch addresses and CFG while reversing predicates.
                // Mutate the actual record/state/zero-byte writes, excluding
                // the spill subframe, and arithmetic outside guard wrappers.
                let replacement = if opcode == 0x63 {
                    Some(word ^ (1 << 12))
                } else if opcode == 0x23 && word >> 15 & 31 == 16 {
                    Some(word ^ (1 << 20))
                } else if instruction.address < first_store && matches!(opcode, 0x13 | 0x33 | 0x37 | 0x1b) && word >> 7 & 31 != 2 {
                    Some(0x00000013)
                } else if [0x020f8513, 0x00050293, 0x00128293, 0xfff38393].contains(&word) {
                    // Result pointer +32, zero-loop pointer, stride and count.
                    Some(word ^ (1 << 20))
                } else {
                    None
                };
                if let Some(replacement) = replacement {
                    let mut changed = valid.clone();
                    changed.replace_machine_word(instruction.address, replacement);
                    reject(&changed);
                    mutations += 1;
                }
            }
            assert!(mutations >= 40, "insufficient allocation mutation inventory: {mutations}");
            for all in [false, true] {
                let mut missing = valid.clone();
                for block in &mut missing.record.blocks {
                    block.allocated_allocation_labels.retain(|candidate| !all && candidate != label && candidate != &end_label);
                }
                missing.rebind_sidecars();
                reject(&missing);
            }
            // A marker must not manufacture a second alias of the same bytes.
            let mut duplicate = valid.clone();
            let duplicate_block = duplicate.record.blocks.iter_mut().find(|block| block.id == begin.id).unwrap();
            duplicate_block.allocated_allocation_labels.push(label.replace("_begin", "0_begin"));
            duplicate_block.allocated_allocation_labels.sort();
            duplicate.rebind_sidecars();
            reject(&duplicate);

            let loop_branch = body.iter().find(|i| i.word & 0x707f == 0x1063 && i.word >> 15 & 31 == 7).unwrap();
            let zero_loop = elf.control_flow.iter().find(|edge| edge.address == loop_branch.address).unwrap().target;
            let outside = elf
                .instructions
                .iter()
                .filter(|i| i.address < begin.range.start && i.word & 0x7f == 0x63)
                .max_by_key(|i| i.address)
                .unwrap()
                .address;
            for address in [begin.range.start, outside] {
                // Keep machine CFG, reachability and all digests consistent;
                // reject skipped admission and external entry into zeroing.
                let mut bypass = valid.clone();
                bypass.redirect_conditional_branch(address, zero_loop);
                reject(&bypass);
            }
        }
    }
}

#[test]
fn allocated_lifecycle_rejects_rebound_lookup_copy_and_retirement_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!("module lifecycle\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 8 }}\nvalues: Vec<u64> {{ items: 8 }}\nseen: OrderedSet<u64> {{ items: 8 }}\ntotals: OrderedMap<u64, u128> {{ items: 8 }} }}\naction inspect() -> u64 {{ let mut first = Work::new()\nfirst.values.push(7)\nlet mut second = Work::new()\nsecond.values.push(8)\nsecond.values.push(9)\nsecond.values.reserve(8)\nrequire second.values.get(0) == 8\nrequire second.values.get(1) == 9\nsecond.drop()\nfirst.drop()\nreturn 0 }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let reject = |fixture: &Fixture| {
                let error = check_bundle_values(
                    &fixture.artifact,
                    &fixture.metadata,
                    &fixture.record,
                    &fixture.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.message.contains("allocated allocation"), "{error}");
            };
            for prefix in [".Lallocated_resize_", ".Lallocated_release_"] {
                let candidates = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .filter(|(_, label)| label.starts_with(prefix) && label.ends_with("_begin"))
                    .collect::<Vec<_>>();
                let largest_offset = candidates
                    .iter()
                    .map(|(_, label)| label.strip_prefix(prefix).unwrap().split('_').next().unwrap().parse::<u32>().unwrap())
                    .max()
                    .unwrap();
                let exact_prefix = format!("{prefix}{largest_offset}_");
                let (begin, label) = candidates
                    .into_iter()
                    .filter(|(_, label)| label.starts_with(&exact_prefix))
                    .min_by_key(|(block, _)| block.range.start)
                    .unwrap();
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let end = valid.record.blocks.iter().find(|block| block.allocated_lifecycle_labels.contains(&end_label)).unwrap();
                let allocations = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| begin.range.start <= block.range.start && block.range.start < end.range.start)
                    .flat_map(|block| {
                        block
                            .allocated_allocation_labels
                            .iter()
                            .filter(|label| label.ends_with("_begin"))
                            .map(move |label| (block, label))
                    })
                    .map(|(block, label)| {
                        let finish = format!("{}_end", label.strip_suffix("_begin").unwrap());
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|block| block.allocated_allocation_labels.contains(&finish))
                            .unwrap()
                            .range
                            .start;
                        (block.range.start, end)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(allocations.len(), if prefix.contains("resize") { 2 } else { 0 });
                if bytes == 65536 {
                    assert!(largest_offset > 65536);
                }
                let body = elf
                    .instructions
                    .iter()
                    .filter(|i| {
                        begin.range.start <= i.address
                            && i.address < end.range.start
                            && !allocations.iter().any(|(start, end)| (*start..*end).contains(&i.address))
                    })
                    .collect::<Vec<_>>();
                let mut mutations = 0;
                for instruction in &body {
                    let word = instruction.word;
                    let opcode = word & 0x7f;
                    let base = word >> 15 & 31;
                    let source = word >> 20 & 31;
                    let replacement = match opcode {
                        0x63 => Some(word ^ (1 << 12)),
                        0x03 if base != 2 => Some(word ^ (8 << 20)),
                        0x23 if base != 2 || matches!(source, 6 | 10) => Some(word ^ (1 << 20)),
                        0x13 | 0x1b | 0x33 if word >> 7 & 31 != 2 => Some(word ^ (1 << 20)),
                        0x37 => Some(word ^ (1 << 12)),
                        _ => None,
                    };
                    if let Some(replacement) = replacement {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, replacement);
                        reject(&changed);
                        mutations += 1;
                    }
                }
                assert!(mutations >= 45, "insufficient lifecycle mutations: {mutations}");
                let mut missing = valid.clone();
                for block in &mut missing.record.blocks {
                    block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                }
                missing.rebind_sidecars();
                reject(&missing);
                if let Some(&(child, _)) = allocations.first() {
                    let outside = elf
                        .instructions
                        .iter()
                        .filter(|i| i.address < begin.range.start && i.word & 0x7f == 0x63)
                        .max_by_key(|i| i.address)
                        .unwrap()
                        .address;
                    for address in [begin.range.start, outside] {
                        let mut bypass = valid.clone();
                        bypass.redirect_conditional_branch(address, child);
                        reject(&bypass);
                    }
                }
            }
            let mut missing = valid.clone();
            for block in &mut missing.record.blocks {
                block.allocated_lifecycle_labels.clear();
            }
            missing.rebind_sidecars();
            reject(&missing);
        }
    }
}

#[test]
fn allocated_descriptors_bind_member_growth_publication_and_drop() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!("module descriptors\nstruct Record {{ amount: u128, flag: u64 }}\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 8 }}\nvalues: Vec<Record> {{ items: 4 }}\ntotals: OrderedMap<u64, Record> {{ items: 6 }}\nseen: OrderedSet<Hash> {{ items: 5 }} }}\naction inspect(flag: bool) -> u64 {{ if flag {{ return 0 }}\nlet mut first = Work::new()\nlet mut second = Work::new()\nsecond.values.reserve(4)\nsecond.totals.reserve(6)\nsecond.seen.reserve(5)\nsecond.drop()\nfirst.drop()\nreturn 0 }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let reject = |fixture: &Fixture| {
                let error = check_bundle_values(
                    &fixture.artifact,
                    &fixture.metadata,
                    &fixture.record,
                    &fixture.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.message.contains("allocated allocation"), "{error}");
            };
            for prefix in [".Lallocated_reserve_", ".Lallocated_drop_"] {
                let candidates = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .filter(|(_, label)| label.starts_with(prefix) && label.ends_with("_begin"))
                    .collect::<Vec<_>>();
                let owner = candidates
                    .iter()
                    .map(|(_, label)| label.strip_prefix(prefix).unwrap().split('_').next().unwrap().parse::<u32>().unwrap())
                    .max()
                    .unwrap();
                let descriptor = owner + 32 + bytes + 32; // The second, differently shaped member.
                let (begin, label) =
                    candidates.into_iter().find(|(_, label)| label.starts_with(&format!("{prefix}{owner}_{descriptor}_"))).unwrap();
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                    .unwrap()
                    .range
                    .start;
                let child_prefix = if prefix.contains("reserve") { ".Lallocated_resize_" } else { ".Lallocated_release_" };
                let (child, child_label) = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| begin.range.start < block.range.start && block.range.start < end)
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .find(|(_, label)| label.starts_with(child_prefix) && label.ends_with("_begin"))
                    .unwrap();
                let child_end_label = format!("{}_end", child_label.strip_suffix("_begin").unwrap());
                let child_end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.allocated_lifecycle_labels.contains(&child_end_label))
                    .unwrap()
                    .range
                    .start;
                let mut mutations = 0;
                for instruction in elf.instructions.iter().filter(|i| {
                    begin.range.start <= i.address && i.address < end && !(child.range.start..child_end).contains(&i.address)
                }) {
                    let word = instruction.word;
                    let replacement = match word & 0x7f {
                        0x63 => Some(word ^ (1 << 12)),
                        0x03 => Some(word ^ (8 << 20)),
                        0x23 => Some(word ^ (1 << 20)),
                        0x13 | 0x1b | 0x33 => Some(word ^ (1 << 20)),
                        0x37 => Some(word ^ (1 << 12)),
                        _ => None,
                    };
                    if let Some(replacement) = replacement {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, replacement);
                        reject(&changed);
                        mutations += 1;
                    }
                }
                assert!(mutations >= 60, "descriptor checks and publication must both be exercised: {mutations}");
                if bytes == 65536 {
                    assert!(owner > 65536);
                }
                let mut missing = valid.clone();
                for block in &mut missing.record.blocks {
                    block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                }
                missing.rebind_sidecars();
                reject(&missing);
                // Swap two valid member coordinates without changing the
                // inventory counts or machine bytes. Equal entry widths must
                // not erase their distinct declared item limits.
                let mut swapped = valid.clone();
                let left = format!("{prefix}{owner}_{descriptor}_");
                let right = format!("{prefix}{owner}_{}_", descriptor + 32);
                for block in &mut swapped.record.blocks {
                    for label in &mut block.allocated_lifecycle_labels {
                        if let Some(suffix) = label.strip_prefix(&left) {
                            *label = format!("{right}{suffix}");
                        } else if let Some(suffix) = label.strip_prefix(&right) {
                            *label = format!("{left}{suffix}");
                        }
                    }
                    block.allocated_lifecycle_labels.sort();
                }
                swapped.rebind_sidecars();
                reject(&swapped);
                let branch = elf
                    .instructions
                    .iter()
                    .find(|i| begin.range.start <= i.address && i.address < child.range.start && i.word & 0x7f == 0x63)
                    .unwrap();
                let mut bypass = valid.clone();
                bypass.redirect_conditional_branch(branch.address, child.range.start);
                reject(&bypass);
            }
            let mut missing = valid.clone();
            for block in &mut missing.record.blocks {
                block
                    .allocated_lifecycle_labels
                    .retain(|label| !label.starts_with(".Lallocated_reserve_") && !label.starts_with(".Lallocated_drop_"));
            }
            missing.rebind_sidecars();
            reject(&missing);
            let tail = valid.record.blocks.iter().find(|block| block.machine_label.as_deref() == Some(".Linspect_epilogue")).unwrap();
            let restore = elf.instructions.iter().find(|i| tail.range.contains(i.address) && i.word & 0x7fff == 0x3083).unwrap();
            let mut skipped_restore = valid.clone();
            skipped_restore.replace_machine_word(restore.address, restore.word ^ (1 << 7));
            assert_code(&skipped_restore, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn allocated_elements_bind_copy_direction_extent_and_length_transitions() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!("module elements\nstruct Item {{ amount: u128, tag: u64 }}\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 16 }}\nvalues: Vec<Item> {{ items: 4 }}\ntotals: OrderedMap<u64, u128> {{ items: 4 }}\nseen: OrderedSet<u64> {{ items: 4 }} }}\naction inspect(flag: bool) -> u64 {{ if flag {{ return 0 }}\nlet mut first = Work::new()\nlet mut second = Work::new()\nsecond.values.push(Item {{ amount: 3, tag: 4 }})\nsecond.values.insert(0, Item {{ amount: 5, tag: 6 }})\nsecond.values.set(1, Item {{ amount: 7, tag: 8 }})\nlet removed = second.values.remove(0)\nrequire removed.tag == 6\nlet popped = second.values.pop()\nrequire popped.tag == 8\nsecond.totals.insert(1, 2)\nsecond.totals.insert(1, 3)\nsecond.totals.remove(1)\nsecond.seen.insert(1)\nsecond.seen.remove(1)\nsecond.drop()\nfirst.drop()\nreturn 0 }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let reject = |fixture: &Fixture| {
                let error = check_bundle_values(
                    &fixture.artifact,
                    &fixture.metadata,
                    &fixture.record,
                    &fixture.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.message.contains("allocated allocation"), "{error}");
            };
            let prefixes = [".Lallocated_push_", ".Lallocated_insert_", ".Lallocated_set_", ".Lallocated_remove_"];
            for prefix in prefixes {
                let candidates = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .filter(|(_, label)| label.starts_with(prefix) && label.ends_with("_begin"))
                    .collect::<Vec<_>>();
                let owner = candidates
                    .iter()
                    .map(|(_, label)| label.strip_prefix(prefix).unwrap().split('_').next().unwrap().parse::<u32>().unwrap())
                    .max()
                    .unwrap();
                let descriptor = owner + 32 + bytes;
                let (begin, label) = candidates
                    .into_iter()
                    .filter(|(_, label)| label.starts_with(&format!("{prefix}{owner}_{descriptor}_")))
                    .min_by_key(|(block, _)| block.range.start)
                    .unwrap();
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                    .unwrap()
                    .range
                    .start;
                let body = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| {
                        begin.range.start <= block.range.start
                            && block.range.start < end
                            && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lvalue_vector_checked_"))
                    })
                    .map(|block| block.range.start)
                    .min()
                    .unwrap();
                let child = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| begin.range.start < block.range.start && block.range.start < end)
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .find(|(_, label)| label.starts_with(".Lallocated_reserve_") && label.ends_with("_begin"))
                    .map(|(block, label)| {
                        let finish = format!("{}_end", label.strip_suffix("_begin").unwrap());
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|block| block.allocated_lifecycle_labels.contains(&finish))
                            .unwrap()
                            .range
                            .start;
                        block.range.start..end
                    });
                let mut mutations = 0;
                for instruction in elf.instructions.iter().filter(|i| {
                    body <= i.address && i.address < end && !child.as_ref().is_some_and(|child| child.contains(&i.address))
                }) {
                    let word = instruction.word;
                    let rd = word >> 7 & 31;
                    let replacement = match word & 0x7f {
                        0x63 => Some(word ^ (1 << 12)),
                        0x03 if word >> 12 & 7 == 4 => Some(word ^ (1 << 15)),
                        0x23 if word >> 12 & 7 == 0 || word >> 20 & 31 == 6 => Some(word ^ (1 << 20)),
                        0x13 | 0x1b | 0x33 if matches!(rd, 5 | 6 | 7 | 11 | 14 | 28 | 29) => Some(word ^ (1 << 20)),
                        0x37 => Some(word ^ (1 << 12)),
                        _ => None,
                    };
                    if let Some(replacement) = replacement {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, replacement);
                        reject(&changed);
                        mutations += 1;
                    }
                }
                assert!(mutations >= 10, "missing element mutation coverage: {prefix} {mutations}");
                if bytes == 65536 {
                    assert!(owner > 65536);
                }
                let mut missing = valid.clone();
                for block in &mut missing.record.blocks {
                    block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                }
                missing.rebind_sidecars();
                reject(&missing);
                if let Some(child) = child {
                    let branch = elf
                        .instructions
                        .iter()
                        .find(|i| begin.range.start <= i.address && i.address < child.start && i.word & 0x7f == 0x63)
                        .unwrap();
                    let mut bypass = valid.clone();
                    bypass.redirect_conditional_branch(branch.address, child.start);
                    reject(&bypass);
                }
            }
            let mut missing = valid.clone();
            for block in &mut missing.record.blocks {
                block.allocated_lifecycle_labels.retain(|label| !prefixes.iter().any(|prefix| label.starts_with(prefix)));
            }
            missing.rebind_sidecars();
            reject(&missing);
        }
    }
}

#[test]
fn allocated_ordered_searches_bind_key_order_bounds_and_update_selection() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!(
                r#"module ordered_proof
collections Work {{
    limits {{ bytes: {bytes}, allocations: 16 }}
    totals: OrderedMap<u64, u128> {{ items: 4 }}
    seen: OrderedSet<Hash> {{ items: 4 }}
}}
fn lookup(view: &Work, key: u64) -> u128 {{
    borrow view as alias {{ return alias.totals.get_or(key, 0) }}
}}
action inspect(flag: bool, hash: Hash) -> u64 {{
    if flag {{ return 0 }}
    let first = Work::new()
    let mut second = Work::new()
    let added = second.totals.insert(256, 3)
    require added
    let replaced = second.totals.insert(256, 5)
    require !replaced
    require second.totals.contains_key(256)
    require second.totals.get(256) == 5
    require second.totals.get_or(255, 8) == 8
    borrow second as view {{
        require view.totals.contains_key(256)
        require lookup(view, 256) == 5
    }}
    let removed = second.totals.remove(256)
    require removed
    let missing = second.totals.remove(256)
    require !missing
    let fresh = second.seen.insert(hash)
    require fresh
    let duplicate = second.seen.insert(hash)
    require !duplicate
    require second.seen.contains(hash)
    let erased = second.seen.remove(hash)
    require erased
    let absent = second.seen.remove(hash)
    require !absent
    second.drop()
    first.drop()
    return 0
}}"#
            );
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let reject = |fixture: &Fixture| {
                let error = check_bundle_values(
                    &fixture.artifact,
                    &fixture.metadata,
                    &fixture.record,
                    &fixture.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.message.contains("allocated allocation"), "{error}");
            };
            let prefixes = [".Lallocated_find_", ".Lallocated_put_", ".Lallocated_erase_"];
            // Both owners have the same contract. Rebinding the lexical loan
            // to the other live owner must not certify reads from the old one.
            let mut wrong_owner = valid.clone();
            let entry = wrong_owner.record.typed_semantics.entries.iter_mut().find(|entry| entry.kind == "action").unwrap();
            entry.borrows.iter_mut().find(|borrow| borrow.root == "second").unwrap().root = "first".into();
            wrong_owner.rebind_typed_semantics();
            reject(&wrong_owner);
            for prefix in prefixes {
                let mut first_per_member = std::collections::BTreeMap::new();
                for block in &valid.record.blocks {
                    for label in &block.allocated_lifecycle_labels {
                        if let Some(suffix) = label.strip_prefix(prefix).filter(|_| label.ends_with("_begin")) {
                            let parts = suffix.split('_').collect::<Vec<_>>();
                            let descriptor = parts[1].parse::<u32>().unwrap();
                            first_per_member.entry(descriptor).or_insert((block, label));
                        }
                    }
                }
                assert_eq!(first_per_member.len(), 2);
                for (descriptor, (begin, label)) in first_per_member {
                    if bytes == 65536 {
                        assert!(descriptor > 131072);
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
                    let body = if prefix == ".Lallocated_find_" {
                        valid
                            .record
                            .blocks
                            .iter()
                            .filter(|block| {
                                begin.range.start <= block.range.start
                                    && block.range.start < end
                                    && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lvalue_vector_checked_"))
                            })
                            .map(|block| block.range.start)
                            .min()
                            .unwrap()
                    } else {
                        begin.range.start
                    };
                    let mut children = Vec::new();
                    if prefix != ".Lallocated_find_" {
                        for block in &valid.record.blocks {
                            if !(begin.range.start..end).contains(&block.range.start) {
                                continue;
                            }
                            for label in &block.allocated_lifecycle_labels {
                                if [".Lallocated_find_", ".Lallocated_set_", ".Lallocated_insert_", ".Lallocated_remove_"]
                                    .iter()
                                    .any(|prefix| label.starts_with(prefix))
                                    && label.ends_with("_begin")
                                {
                                    let finish = format!("{}_end", label.strip_suffix("_begin").unwrap());
                                    let child_end = valid
                                        .record
                                        .blocks
                                        .iter()
                                        .find(|block| block.allocated_lifecycle_labels.contains(&finish))
                                        .unwrap()
                                        .range
                                        .start;
                                    children.push(block.range.start..child_end);
                                }
                            }
                        }
                    }
                    let mut mutations = 0;
                    for instruction in elf
                        .instructions
                        .iter()
                        .filter(|i| body <= i.address && i.address < end && !children.iter().any(|range| range.contains(&i.address)))
                    {
                        let word = instruction.word;
                        let replacement = match word & 0x7f {
                            0x63 => Some(word ^ (1 << 12)),
                            0x03 => Some(word ^ (1 << 15)),
                            0x13 | 0x1b | 0x33 => Some(word ^ (1 << 20)),
                            0x37 => Some(word ^ (1 << 12)),
                            _ => None,
                        };
                        if let Some(replacement) = replacement {
                            let mut changed = valid.clone();
                            changed.replace_machine_word(instruction.address, replacement);
                            reject(&changed);
                            mutations += 1;
                        }
                    }
                    assert!(mutations >= if prefix == ".Lallocated_find_" { 25 } else { 2 });
                    let mut missing = valid.clone();
                    for block in &mut missing.record.blocks {
                        block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                    }
                    missing.rebind_sidecars();
                    reject(&missing);
                    if prefix != ".Lallocated_find_" {
                        let find = children.iter().min_by_key(|range| range.start).unwrap();
                        let branch = elf.instructions.iter().find(|i| i.address == find.end).unwrap();
                        let update = children.iter().filter(|range| range.start > find.end).min_by_key(|range| range.start).unwrap();
                        let mut bypass = valid.clone();
                        let interior = valid
                            .record
                            .blocks
                            .iter()
                            .filter(|block| update.start < block.range.start && block.range.start < update.end)
                            .map(|block| block.range.start)
                            .min()
                            .unwrap();
                        bypass.redirect_conditional_branch(branch.address, interior);
                        reject(&bypass);
                    }
                }
            }
            let mut missing = valid.clone();
            for block in &mut missing.record.blocks {
                block.allocated_lifecycle_labels.retain(|label| !prefixes.iter().any(|prefix| label.starts_with(prefix)));
            }
            missing.rebind_sidecars();
            reject(&missing);
        }
    }
}

#[test]
fn allocated_ordered_search_inventory_accepts_all_key_profiles_and_local_borrows() {
    for key in ["u8", "u16", "u32", "u64", "Hash", "Address", "[u8; 1]", "[u8; 8]", "[u8; 32]"] {
        for member in [format!("OrderedMap<{key}, u64>"), format!("OrderedSet<{key}>")] {
            let args = if member.starts_with("OrderedMap") { "key, 7" } else { "key" };
            let contains = if member.starts_with("OrderedMap") { "contains_key" } else { "contains" };
            let source = format!(
                r#"module key_profile
collections Work {{
    limits {{ bytes: 512, allocations: 4 }}
    values: {member} {{ items: 4 }}
}}
action inspect(key: {key}) -> u64 {{
    let mut work = Work::new()
    let added = work.values.insert({args})
    require added
    borrow work as view {{ borrow view as nested {{ require nested.values.{contains}(key) }} }}
    let removed = work.values.remove(key)
    require removed
    work.drop()
    return 0
}}"#
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
                let starts = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| &block.allocated_lifecycle_labels)
                    .filter(|label| label.starts_with(".Lallocated_find_") && label.ends_with("_begin"))
                    .count();
                assert_eq!(starts, 3, "{member} at {diagnostics}");
            }
        }
    }
}

#[test]
fn allocated_helper_parameters_bind_both_abi_words_and_preserve_slots() {
    for padding in [0, 7, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
        let arguments = "0, ".repeat(padding);
        let source = format!(
            r#"module arena_parameters
collections Work {{
    limits {{ bytes: 512, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
fn read({params}first: &Work, second: &Work, delta: u64) -> u64 {{
    let adjusted = delta + 1
    if adjusted <= delta {{ return 0 }}
    return first.values.get(0) + second.values.get(0) + adjusted
}}
fn forward({params}first: &Work, second: &Work, delta: u64) -> u64 {{
    borrow first as alias {{ borrow alias as nested {{
        return read({forwarded}nested, second, delta)
    }} }}
}}
action inspect(witness delta: u64) -> u64 {{
    let mut first = Work::new()
    let mut second = Work::new()
    first.values.push(3)
    second.values.push(7)
    borrow first as a {{ borrow second as b {{
        require forward({arguments}a, b, delta) > 0
    }} }}
    second.drop()
    first.drop()
    return 0
}}"#
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            for name in ["read", "forward"] {
                let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with(name)).unwrap();
                let entry = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
                let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| block.owner_entry == typed.id && block.lowering_block_id.is_some())
                    .map(|block| block.range.start)
                    .min()
                    .unwrap();
                let spills = elf
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        (start..end).contains(&instruction.address)
                            && instruction.word & 0x707f == 0x3023
                            && instruction.word >> 15 & 31 == 2
                    })
                    .collect::<Vec<_>>();
                let offset = |word: u32| (word >> 7 & 31) | (word >> 25 << 5);
                for param in &typed.params[padding..padding + 2] {
                    let pointer = typed.locals.iter().find(|local| local.id == param.binding_id).unwrap().source_id as u32 * 8;
                    let index = spills.iter().position(|instruction| offset(instruction.word) == pointer).unwrap();
                    let length = offset(spills[index + 1].word);
                    assert!(length > pointer && length < 2040);
                    for spill in [spills[index], spills[index + 1]] {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                    if name == "read" {
                        let binary = typed.blocks.iter().flat_map(|block| &block.operations).find(|op| op.opcode == "binary").unwrap();
                        let result =
                            typed.locals.iter().find(|local| local.id == binary.destinations[0]).unwrap().source_id as u32 * 8;
                        let store = elf
                            .instructions
                            .iter()
                            .find(|instruction| {
                                instruction.address >= end
                                    && instruction.word & 0x707f == 0x3023
                                    && instruction.word >> 15 & 31 == 2
                                    && offset(instruction.word) == result
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                            })
                            .unwrap();
                        // Full stores and partial final-byte overlaps must not
                        // replace either incoming word in the helper body.
                        for slot in [pointer, length] {
                            for width in 0..=3 {
                                for destination in [slot, slot + 7] {
                                    let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                                        | ((destination & 31) << 7)
                                        | (width << 12)
                                        | ((destination >> 5) << 25);
                                    let mut changed = valid.clone();
                                    changed.replace_machine_word(store.address, word);
                                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn allocated_borrowed_reads_bind_descriptors_searches_and_copied_results() {
    let prefix = ".Lallocated_borrowed_";
    for padding in [0, 7, 8] {
        for bytes in [512, 65536] {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(
                        &borrowed_collection_reads::source(padding, bytes),
                        CompileOptions {
                            edition: NEXT_EDITION,
                            diagnostics,
                            target: Some("riscv64-elf".into()),
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                );
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let mut kinds = std::collections::BTreeSet::new();
                let mut mutations = 0;
                for block in &valid.record.blocks {
                    for label in &block.allocated_lifecycle_labels {
                        let Some(suffix) = label.strip_prefix(prefix).filter(|suffix| suffix.ends_with("_begin")) else { continue };
                        let kind = suffix.split('_').next().unwrap();
                        if !kinds.insert(kind) {
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
                        let body =
                            elf.instructions.iter().filter(|i| (block.range.start..end).contains(&i.address)).collect::<Vec<_>>();
                        for opcode in [0x03, 0x13, 0x1b, 0x33, 0x37, 0x23, 0x63] {
                            let candidates = body
                                .iter()
                                .filter(|i| i.word & 0x7f == opcode && (opcode == 0x23 || i.word >> 7 & 31 != 2))
                                .collect::<Vec<_>>();
                            for instruction in candidates.first().into_iter().chain(candidates.last()).copied() {
                                let word = instruction.word;
                                let replacement = word
                                    ^ match opcode {
                                        0x37 | 0x63 => 1 << 12,
                                        0x03 => 8 << 20,
                                        0x13 if word >> 12 & 7 == 3 => 1 << 15,
                                        _ => 1 << 20,
                                    };
                                let mut changed = valid.clone();
                                changed.replace_machine_word(instruction.address, replacement);
                                let error = check_bundle_values(
                                    &changed.artifact,
                                    &changed.metadata,
                                    &changed.record,
                                    &changed.source_map,
                                    &CheckerBudgets::default(),
                                )
                                .unwrap_err();
                                assert_eq!(
                                    error.code,
                                    CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                                    "{kind} at {:#x}: {word:#x} -> {replacement:#x}: {error}",
                                    instruction.address
                                );
                                mutations += 1;
                            }
                        }
                        let mut missing = valid.clone();
                        for block in &mut missing.record.blocks {
                            block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                        }
                        missing.rebind_sidecars();
                        assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        if kind == "read" {
                            // Rebind the CFG as well: the extent check cannot
                            // be redirected into an already checked read body.
                            let branch = elf
                                .instructions
                                .iter()
                                .rev()
                                .find(|i| {
                                    i.address < block.range.start
                                        && block.range.start - i.address < 4096
                                        && i.word & 0x7f == 0x63
                                        && valid
                                            .record
                                            .blocks
                                            .iter()
                                            .any(|b| b.owner_entry == block.owner_entry && b.range.contains(i.address))
                                })
                                .unwrap();
                            let target = valid
                                .record
                                .blocks
                                .iter()
                                .find(|b| b.range.start > block.range.start && b.range.start < end)
                                .unwrap()
                                .range
                                .start;
                            let mut changed = valid.clone();
                            changed.redirect_conditional_branch(branch.address, target);
                            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        }
                    }
                }
                assert_eq!(
                    kinds,
                    std::collections::BTreeSet::from(["extent", "len", "capacity", "empty", "find", "read", "lookup", "required"])
                );
                assert!(mutations > 55);
                let mut erased = valid.clone();
                for block in &mut erased.record.blocks {
                    block.allocated_lifecycle_labels.retain(|label| !label.starts_with(prefix));
                }
                erased.rebind_sidecars();
                assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn allocated_transport_binds_indices_results_and_adjacent_primitives() {
    for bytes in [512, 65536] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let source = format!(
                r#"module collection_transport
collections Work {{ limits {{ bytes: {bytes}, allocations: 8 }}
    values: Vec<u64> {{ items: 4 }}
    totals: OrderedMap<u64, u64> {{ items: 4 }}
}}
fn read(view: &Work, index: u64) -> u64 {{ return view.values.get(index) }}
action inspect(witness index: u64, witness other: u64, witness flag: bool) -> u64 {{
    if flag {{ return 0 }}
    let mut work = Work::new()
    work.values.reserve(4)
    work.values.insert(index, 7)
    work.values.set(other, 9)
    if flag {{ return 0 }}
    let copied = work.values.get(index)
    require read(&work, other) == copied
    let removed = work.values.remove(other)
    work.values.push(removed)
    let popped = work.values.pop()
    work.totals.insert(3, popped)
    require work.totals.get(3) == copied
    require work.totals.get_or(4, 11) == 11
    for (key, value) in work.totals {{ require key == 3
        require value == copied }}
    work.drop()
    return 0
}}"#
            );
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let mut counts = [0, 0];
            for (kind, prefix) in [".Lallocated_index_", ".Lallocated_result_"].iter().enumerate() {
                let mut mutated_classes = std::collections::BTreeSet::new();
                let mut bypass_checked = false;
                for block in &valid.record.blocks {
                    for label in &block.allocated_lifecycle_labels {
                        if !label.starts_with(prefix) || !label.ends_with("_begin") {
                            continue;
                        }
                        counts[kind] += 1;
                        let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|b| b.allocated_lifecycle_labels.contains(&end_label))
                            .unwrap()
                            .range
                            .start;
                        for instruction in elf.instructions.iter().filter(|i| (block.range.start..end).contains(&i.address)) {
                            let word = instruction.word;
                            let opcode = word & 0x7f;
                            let class = (opcode, word >> 12 & 7);
                            if !mutated_classes.insert(class) {
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
                            let error = check_bundle_values(
                                &changed.artifact,
                                &changed.metadata,
                                &changed.record,
                                &changed.source_map,
                                &CheckerBudgets::default(),
                            )
                            .unwrap_err();
                            assert_eq!(
                                error.code,
                                CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                                "{prefix} {diagnostics} bytes={bytes} {word:#x}->{replacement:#x}: {error}"
                            );
                        }
                        if counts[kind] == 1 {
                            let mut missing = valid.clone();
                            for block in &mut missing.record.blocks {
                                block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                            }
                            missing.rebind_sidecars();
                            assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                            let mut forged = valid.clone();
                            for block in &mut forged.record.blocks {
                                for candidate in &mut block.allocated_lifecycle_labels {
                                    if candidate != label && candidate != &end_label {
                                        continue;
                                    }
                                    let mut parts =
                                        candidate.strip_prefix(prefix).unwrap().split('_').map(str::to_owned).collect::<Vec<_>>();
                                    // The owner source or result scratch coordinate
                                    // cannot be changed by rebinding label evidence.
                                    let field = if kind == 0 { 0 } else { 1 };
                                    parts[field] = (parts[field].parse::<u64>().unwrap() + 8).to_string();
                                    *candidate = format!("{prefix}{}", parts.join("_"));
                                }
                            }
                            forged.rebind_sidecars();
                            assert_code(&forged, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        }
                        // Rebind an earlier source branch to skip the index or
                        // producer entirely, including all CFG/hash sidecars.
                        if !bypass_checked
                            && valid
                                .record
                                .typed_semantics
                                .entries
                                .iter()
                                .any(|entry| entry.id == block.owner_entry && entry.kind == "action")
                        {
                            let target = if kind == 0 { end } else { block.range.start };
                            let before = if kind == 0 {
                                block.range.start
                            } else {
                                valid
                                    .record
                                    .blocks
                                    .iter()
                                    .filter(|b| {
                                        b.owner_entry == block.owner_entry
                                            && b.range.start < block.range.start
                                            && b.allocated_lifecycle_labels
                                                .iter()
                                                .any(|label| label.starts_with(".Lallocated_index_") && label.ends_with("_begin"))
                                    })
                                    .map(|b| b.range.start)
                                    .max()
                                    .unwrap()
                            };
                            let branch = elf
                                .instructions
                                .iter()
                                .rev()
                                .find(|i| {
                                    i.word & 0x7f == 0x63
                                        && i.address < before
                                        && target - i.address < 4096
                                        && valid
                                            .record
                                            .blocks
                                            .iter()
                                            .any(|b| b.owner_entry == block.owner_entry && b.range.contains(i.address))
                                })
                                .unwrap();
                            let mut bypass = valid.clone();
                            bypass.redirect_conditional_branch(branch.address, target);
                            assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                            bypass_checked = true;
                        }
                    }
                }
                // O3 may forward every variable index with ADDI instead of LD.
                assert!(
                    mutated_classes.len() >= if kind == 0 { 1 } else { 5 },
                    "missing transport instruction coverage: {prefix} {diagnostics}: {mutated_classes:?}"
                );
                assert!(bypass_checked, "missing transport bypass mutation");
                let mut erased = valid.clone();
                for block in &mut erased.record.blocks {
                    block.allocated_lifecycle_labels.retain(|label| !label.starts_with(prefix));
                }
                erased.rebind_sidecars();
                assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
            assert!(counts[0] >= 8 && counts[1] >= 8, "{counts:?}");
        }
    }
}

#[test]
fn allocated_transport_checks_scalar_widths_and_signed_publication() {
    for ty in ["bool", "u8", "u16", "u32", "i32", "u64"] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let literal = if ty == "bool" { "true" } else { "7" };
            let source = format!(
                r#"module scalar_collection_transport
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<{ty}> {{ items: 4 }}
}}
fn read(view: &Work, index: u64) -> {ty} {{ return view.values.get(index) }}
action inspect(witness index: u64) -> u64 {{
    let value: {ty} = {literal}
    let mut work = Work::new()
    work.values.push(value)
    require read(&work, index) == value
    require work.values.get(index) == value
    work.drop()
    return 0
}}"#
            );
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let mut count = 0;
            for block in &valid.record.blocks {
                for label in &block.allocated_lifecycle_labels {
                    if !label.starts_with(".Lallocated_result_") || !label.ends_with("_begin") {
                        continue;
                    }
                    count += 1;
                    let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                    let end =
                        valid.record.blocks.iter().find(|b| b.allocated_lifecycle_labels.contains(&end_label)).unwrap().range.start;
                    let body = elf.instructions.iter().filter(|i| (block.range.start..end).contains(&i.address)).collect::<Vec<_>>();
                    let store = body.iter().find(|i| i.word & 0x7f == 0x23).unwrap();
                    let mut changed = valid.clone();
                    changed.replace_machine_word(store.address, store.word ^ (8 << 7));
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if ty == "i32" {
                        let sign = body.iter().find(|i| i.word & 0x7f == 0x13 && i.word >> 12 & 7 == 5).unwrap();
                        let mut unsigned = valid.clone();
                        unsigned.replace_machine_word(sign.address, sign.word ^ (1 << 30));
                        assert_code(&unsigned, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
            }
            assert_eq!(count, 2);
        }
    }
}

#[test]
fn allocated_borrowed_reads_preserve_reborrowed_parameter_storage() {
    let source = r#"module borrowed_alias_provenance
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn read(view: &Work, delta: u64) -> u64 {
    borrow view as alias {
        let noise = delta + 1
        if noise <= delta { return 0 }
        return alias.values.get(0)
    }
}
action inspect(witness delta: u64) -> u64 {
    let mut work = Work::new()
    work.values.push(7)
    require read(&work, delta) == 7
    work.drop()
    return 0
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "read").unwrap();
        let slot = |local| entry.locals.iter().find(|value| value.id == local).unwrap().source_id as u32 * 8;
        let alias = entry.locals.iter().find(|value| value.name == "alias").unwrap();
        let pointer = slot(alias.id);
        let arithmetic = entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add"))
            .unwrap();
        let result = slot(arithmetic.destinations[0]);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let store = elf
            .instructions
            .iter()
            .find(|instruction| {
                instruction.word & 0x707f == 0x3023
                    && instruction.word >> 15 & 31 == 2
                    && ((instruction.word >> 7 & 31) | (instruction.word >> 25 << 5)) == result
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
            })
            .unwrap();
        for width in 0..=3 {
            for destination in [pointer, pointer + 7] {
                let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                    | ((destination & 31) << 7)
                    | (width << 12)
                    | ((destination >> 5) << 25);
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
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn allocated_helper_calls_bind_owner_identity_extent_and_intervening_writes() {
    for padding in [0, 7, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
        let arguments = "0, ".repeat(padding);
        for bytes in [512, 65536] {
            let source = format!(
                r#"module arena_call_provenance
collections Work {{ limits {{ bytes: {bytes}, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
fn read({params}first: &Work, second: &Work) -> u64 {{
    return first.values.get(0) * 10 + second.values.get(0)
}}
fn forward({params}first: &Work, second: &Work) -> u64 {{
    borrow first as alias {{ borrow alias as nested {{
        return read({forwarded}nested, second)
    }} }}
}}
action inspect(witness delta: u64) -> u64 {{
    let mut first = Work::new()
    let mut second = Work::new()
    first.values.push(3)
    second.values.push(7)
    let mut noise = 0
    if delta > 0 {{ noise = delta + 1 }} else {{ noise = delta + 2 }}
    require noise > delta
    require forward({arguments}&first, &second) == 37
    second.drop()
    first.drop()
    return 0
}}"#
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
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                for (caller_name, callee_name) in [("inspect", "forward"), ("forward", "read")] {
                    let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == caller_name).unwrap();
                    let callee = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == callee_name).unwrap();
                    let lowering = valid.record.entries.iter().find(|entry| entry.id == callee.id).unwrap();
                    let target = valid.record.blocks.iter().find(|block| block.id == lowering.entry_block).unwrap().range.start;
                    let call = elf
                        .control_flow
                        .iter()
                        .find(|flow| {
                            flow.target == target
                                && valid
                                    .record
                                    .blocks
                                    .iter()
                                    .any(|block| block.owner_entry == typed.id && block.range.contains(flow.address))
                        })
                        .unwrap()
                        .address;
                    let operation = typed
                        .blocks
                        .iter()
                        .flat_map(|block| &block.operations)
                        .find(|op| op.call.as_ref().is_some_and(|call| call.target == callee_name))
                        .unwrap();
                    let slot = |local| typed.locals.iter().find(|value| value.id == local).unwrap().source_id as u32 * 8;
                    let pointers =
                        [slot(operation.operands[padding].local.unwrap()), slot(operation.operands[padding + 1].local.unwrap())];
                    let body_load = |offset| {
                        elf.instructions
                            .iter()
                            .rev()
                            .find(|instruction| {
                                instruction.address < call
                                    && instruction.word & 0x707f == 0x3003
                                    && instruction.word >> 15 & 31 == 2
                                    && instruction.word >> 20 == offset
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                            })
                            .expect("arena argument reload")
                    };
                    let load = body_load(pointers[0]);
                    let mut changed = valid.clone();
                    changed.replace_machine_word(load.address, replace_i_immediate(load.word, pointers[1] as i32));
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if caller_name == "forward" {
                        let lowering = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
                        let start = valid.record.blocks.iter().find(|block| block.id == lowering.entry_block).unwrap().range.start;
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .filter(|block| block.owner_entry == typed.id && block.lowering_block_id.is_some())
                            .map(|block| block.range.start)
                            .min()
                            .unwrap();
                        let spills = elf
                            .instructions
                            .iter()
                            .filter(|instruction| {
                                (start..end).contains(&instruction.address)
                                    && instruction.word & 0x707f == 0x3023
                                    && instruction.word >> 15 & 31 == 2
                            })
                            .collect::<Vec<_>>();
                        let offset = |word: u32| (word >> 7 & 31) | (word >> 25 << 5);
                        let lengths = [padding, padding + 1].map(|position| {
                            let pointer = slot(typed.params[position].binding_id);
                            let index = spills.iter().position(|instruction| offset(instruction.word) == pointer).unwrap();
                            offset(spills[index + 1].word)
                        });
                        let load = body_load(lengths[0]);
                        for replacement in [replace_i_immediate(load.word, lengths[1] as i32), encode_addi(load.word >> 7 & 31, 0, 0)]
                        {
                            let mut changed = valid.clone();
                            changed.replace_machine_word(load.address, replacement);
                            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        }
                    } else {
                        let length_word = padding as u32 + 1;
                        let (register, before) = if length_word < 8 {
                            (10 + length_word, call)
                        } else {
                            let outgoing = ((padding as u32 + 4).saturating_sub(8) * 8).next_multiple_of(16);
                            let offset = i64::from(length_word - 8) * 8 - i64::from(outgoing);
                            let store =
                                elf.instructions
                                    .iter()
                                    .rev()
                                    .find(|instruction| {
                                        let word = instruction.word;
                                        let immediate = i64::from(((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20) >> 20);
                                        instruction.address < call
                                            && word & 0x707f == 0x3023
                                            && word >> 15 & 31 == 2
                                            && immediate == offset
                                            && valid.record.blocks.iter().any(|block| {
                                                block.owner_entry == typed.id && block.range.contains(instruction.address)
                                            })
                                    })
                                    .expect("owned extent outgoing stack word");
                            (store.word >> 20 & 31, store.address)
                        };
                        let producer = elf
                            .instructions
                            .iter()
                            .rev()
                            .find(|instruction| {
                                instruction.address < before
                                    && instruction.word >> 7 & 31 == register
                                    && matches!(instruction.word & 0x7f, 0x03 | 0x13 | 0x1b | 0x33 | 0x37)
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                            })
                            .expect("owned extent constant producer");
                        assert_ne!(register, 2);
                        let mut changed = valid.clone();
                        changed.replace_machine_word(producer.address, encode_addi(register, 0, 0));
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        let binaries = typed.blocks.iter().flat_map(|block| &block.operations).filter(|op| {
                            matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add")
                        }).collect::<Vec<_>>();
                        assert_eq!(binaries.len(), 2, "both normal predecessors must be exercised");
                        let stores = binaries
                            .iter()
                            .map(|binary| {
                                let result = slot(binary.destinations[0]);
                                elf.instructions
                                    .iter()
                                    .find(|instruction| {
                                        instruction.word & 0x707f == 0x3023
                                            && instruction.word >> 15 & 31 == 2
                                            && ((instruction.word >> 7 & 31) | (instruction.word >> 25 << 5)) == result
                                            && valid.record.blocks.iter().any(|block| {
                                                block.owner_entry == typed.id && block.range.contains(instruction.address)
                                            })
                                    })
                                    .unwrap()
                            })
                            .collect::<Vec<_>>();
                        let owner_use = typed
                            .blocks
                            .iter()
                            .flat_map(|block| &block.operations)
                            .find(|op| {
                                matches!(
                                    op.detail,
                                    TypedSemanticOperationDetail::AllocatedValue {
                                        operation: cellscript_artifact_checker::allocated_values::Operation::Push,
                                        ..
                                    }
                                )
                            })
                            .unwrap();
                        // Mutate the retained binding consumed by the call;
                        // the constructor's initial temporary can already be dead.
                        let pointer = slot(owner_use.operands[0].local.unwrap());
                        for store in stores {
                            for width in 0..=3 {
                                for destination in [pointer, pointer + 7] {
                                    let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                                        | ((destination & 31) << 7)
                                        | (width << 12)
                                        | ((destination >> 5) << 25);
                                    let mut changed = valid.clone();
                                    changed.replace_machine_word(store.address, word);
                                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn allocated_helper_calls_recheck_pointer_provenance_on_loop_backedges() {
    for bytes in [512, 65536] {
        let source = format!(
            r#"module arena_call_backedge
collections Work {{ limits {{ bytes: {bytes}, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
fn read(work: &Work) -> u64 {{ return work.values.get(0) }}
action inspect(witness delta: u64) -> u64 {{
    let mut work = Work::new()
    work.values.push(7)
    for i in 0..2 {{
        require read(&work) == 7
        let noise = delta ^ i
        require noise <= delta + 2
    }}
    work.drop()
    return 0
}}"#
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
            let slot = |local| entry.locals.iter().find(|value| value.id == local).unwrap().source_id as u32 * 8;
            let operations = entry.blocks.iter().flat_map(|block| &block.operations).collect::<Vec<_>>();
            let arithmetic = operations
                .iter()
                .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "bit-xor"))
                .unwrap();
            let push = operations
                .iter()
                .find(|op| {
                    matches!(
                        op.detail,
                        TypedSemanticOperationDetail::AllocatedValue {
                            operation: cellscript_artifact_checker::allocated_values::Operation::Push,
                            ..
                        }
                    )
                })
                .unwrap();
            let result = slot(arithmetic.destinations[0]);
            let pointer = slot(push.operands[0].local.unwrap());
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let store = elf
                .instructions
                .iter()
                .find(|instruction| {
                    instruction.word & 0x707f == 0x3023
                        && instruction.word >> 15 & 31 == 2
                        && ((instruction.word >> 7 & 31) | (instruction.word >> 25 << 5)) == result
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
                })
                .unwrap();
            // The first helper call precedes this store and has valid facts.
            // A poisoned retained binding must still invalidate the next call
            // when its incoming state is merged through the loop backedge.
            let helper = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "read").unwrap();
            let call = elf
                .control_flow
                .iter()
                .find(|flow| {
                    valid.record.blocks.iter().any(|block| block.owner_entry == helper.id && block.range.start == flow.target)
                })
                .unwrap()
                .address;
            assert!(call < store.address);
            assert!(elf.control_flow.iter().any(|flow| flow.address > store.address
                && flow.target <= call
                && valid.record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(flow.address))));
            for width in [0, 3] {
                for destination in [pointer, pointer + 7] {
                    let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                        | ((destination & 31) << 7)
                        | (width << 12)
                        | ((destination >> 5) << 25);
                    let mut changed = valid.clone();
                    changed.replace_machine_word(store.address, word);
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
        }
    }
}

#[test]
fn allocated_coordinate_calls_bind_guard_spills_bounds_and_restoration() {
    for padding in [0, 5, 6] {
        for bytes in [512, 65536] {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(
                        &collection_coordinate_helpers::source(padding, bytes),
                        CompileOptions {
                            edition: NEXT_EDITION,
                            diagnostics,
                            target: Some("riscv64-elf".into()),
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                );
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let words = elf
                    .instructions
                    .iter()
                    .map(|instruction| (instruction.address, instruction.word))
                    .collect::<std::collections::BTreeMap<_, _>>();
                for name in ["inspect", "read"] {
                    let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == name).unwrap();
                    let guards = elf
                        .instructions
                        .iter()
                        .filter(|instruction| {
                            instruction.word == 0xfe010113
                                && valid
                                    .record
                                    .blocks
                                    .iter()
                                    .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                                && (1..=3).all(|index| {
                                    words.get(&(instruction.address + index * 4)).is_some_and(|word| word & 0x707f == 0x3023)
                                })
                        })
                        .map(|instruction| instruction.address)
                        .collect::<Vec<_>>();
                    assert!(!guards.is_empty());
                    for start in [guards[0], *guards.last().unwrap()] {
                        let guard = (start + 16..start + 144)
                            .step_by(4)
                            .find(|pc| words.get(pc).is_some_and(|word| word & 0x7f == 0x63))
                            .unwrap();
                        let restore = guard + 12;
                        assert_eq!(words[&(restore + 12)], 0x02010113);
                        let mut mutations = vec![
                            (start, encode_addi(2, 2, -16)),
                            (guard, words[&guard] ^ (1 << 12)),
                            (guard + 4, words[&(guard + 4)] ^ (1 << 12)),
                            (guard + 8, words[&(guard + 8)] | (1 << 7)),
                            (restore + 12, encode_addi(2, 2, 16)),
                        ];
                        if name == "inspect" {
                            // Transport relies on exact restoration across the
                            // sequence. The callee's separate isolation proof
                            // may accept changes to otherwise dead scratch
                            // registers; complete read-value proof is pending.
                            for index in 0..3u64 {
                                let spill = start + 4 + index * 4;
                                mutations.push((spill, words[&spill] & !(31 << 20)));
                                let load = restore + index * 4;
                                mutations.push((load, words[&load] ^ (8 << 20)));
                            }
                        }
                        // Bind the lower/upper bound registers too, not merely
                        // the branch opcode and its error destination.
                        mutations.push((guard, (words[&guard] & !(31 << 20)) | (2 << 20)));
                        mutations.push((guard + 4, (words[&(guard + 4)] & !(31 << 15)) | (2 << 15)));
                        for (pc, word) in mutations {
                            let mut changed = valid.clone();
                            changed.replace_machine_word(pc, word);
                            let code = match changed.check() {
                                Err(code) => code,
                                Ok(()) => panic!("accepted {name} {diagnostics}, padding={padding}, bytes={bytes}, start={start:#x}, pc={pc:#x}, old={:#x}, new={word:#x}", words[&pc]),
                            };
                            assert!(
                                matches!(
                                    code,
                                    CheckerRejectionCode::V2420TypedMachineBindingInvalid
                                        | CheckerRejectionCode::V2407AbiOrStackInvalid
                                ),
                                "{name} at {pc:#x}, {word:#x}: {code:?}"
                            );
                        }
                        // Coherently rebound control flow cannot jump into the
                        // middle of a spill/restore sequence.
                        let mut changed = valid.clone();
                        changed.replace_machine_word(start + 4, encode_jal((guard - start - 4) as i32));
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
                let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
                let operation = typed
                    .blocks
                    .iter()
                    .flat_map(|block| &block.operations)
                    .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "forward"))
                    .unwrap();
                let local = operation.operands[padding + 1].local.unwrap();
                let coordinate = typed.locals.iter().find(|value| value.id == local).unwrap().source_id as u32 * 8;
                let callee = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "forward").unwrap();
                let callee = valid.record.entries.iter().find(|entry| entry.id == callee.id).unwrap();
                let target = valid.record.blocks.iter().find(|block| block.id == callee.entry_block).unwrap().range.start;
                let call = elf.control_flow.iter().find(|flow| flow.target == target).unwrap().address;
                let load = elf
                    .instructions
                    .iter()
                    .rev()
                    .find(|instruction| {
                        instruction.address < call
                            && instruction.word & 0x707f == 0x3003
                            && instruction.word >> 15 & 31 == 2
                            && instruction.word >> 20 == coordinate
                            && valid
                                .record
                                .blocks
                                .iter()
                                .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                    })
                    .expect("coordinate reload before the helper call");
                let mut changed = valid.clone();
                changed.replace_machine_word(load.address, load.word ^ (8 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn allocated_helper_parameters_compose_with_nested_and_coordinate_decoders() {
    let source = r#"module mixed_arena_parameters
resource Token has store { amount: u64 }
collections Work {
    limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn read(view: &Work, before: InputView<Token>, items: NestedValues<Vec<u8>, 2, 8, 32>) -> u64 {
    if items.len() == 0 { return 0 }
    return view.values.get(0) + before.capacity
}
fn forward(view: &Work, before: InputView<Token>, items: NestedValues<Vec<u8>, 2, 8, 32>) -> u64 {
    return read(view, before, items)
}
action inspect(witness items: NestedValues<Vec<u8>, 2, 8, 32>) -> u64 {
    verification
    let mut work = Work::new()
    work.values.push(3)
    let before = ckb::input<Token>(0)
    require forward(&work, before, items) > 0
    return 0
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for name in ["read", "forward"] {
            let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with(name)).unwrap();
            let entry = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
            let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
            let end = valid
                .record
                .blocks
                .iter()
                .filter(|block| block.owner_entry == typed.id && block.lowering_block_id.is_some())
                .map(|block| block.range.start)
                .min()
                .unwrap();
            // The owner is the first ABI pair. Corrupt both incoming spills
            // before the nested decoder, whose verified seed must retain them.
            for register in [10, 11] {
                let spill = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        (start..end).contains(&instruction.address)
                            && instruction.word & 0x707f == 0x3023
                            && instruction.word >> 20 & 31 == register
                    })
                    .unwrap();
                let mut changed = valid.clone();
                changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn allocated_reads_bind_copy_buffers_pop_and_missing_key_behavior() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!(
                r#"module read_proof
struct Item {{ amount: u128, tag: u64 }}
collections Work {{
    limits {{ bytes: {bytes}, allocations: 16 }}
    values: Vec<Item> {{ items: 4 }}
    totals: OrderedMap<u64, u128> {{ items: 4 }}
}}
fn copied(view: &Work) -> Item {{ return view.values.get(0) }}
action inspect(flag: bool) -> u64 {{
    if flag {{ return 0 }}
    let first = Work::new()
    let mut second = Work::new()
    second.values.push(Item {{ amount: 3, tag: 4 }})
    second.values.push(Item {{ amount: 5, tag: 6 }})
    let initial = second.values.get(0)
    borrow second as view {{ require copied(view).tag == 4 }}
    let removed = second.values.remove(0)
    let popped = second.values.pop()
    second.totals.insert(256, 9)
    require second.totals.get(256) == 9
    require second.totals.get_or(255, 17) == 17
    require initial.tag == 4
    require removed.tag == 4
    require popped.tag == 6
    second.drop()
    first.drop()
    return 0
}}"#
            );
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            for (prefix, children, minimum) in [
                (".Lallocated_read_", &[][..], 10),
                (".Lallocated_pop_", &[".Lallocated_read_", ".Lallocated_remove_"][..], 1),
                (".Lallocated_take_", &[".Lallocated_read_", ".Lallocated_remove_"][..], 0),
                (".Lallocated_lookup_", &[".Lallocated_find_", ".Lallocated_read_"][..], 2),
                (".Lallocated_required_", &[".Lallocated_lookup_"][..], 1),
            ] {
                allocated_body_mutations(&valid, prefix, children, minimum, false);
            }
        }
    }
}

#[test]
fn allocated_sorted_bodies_bind_counts_order_overlap_and_last_value_copy() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!(
                r#"module sorted_proof
collections Work {{
    limits {{ bytes: {bytes}, allocations: 8 }}
    totals: OrderedMap<u64, u128> {{ items: 4 }}
    seen: OrderedSet<Hash> {{ items: 4 }}
}}
action inspect(flag: bool, witness entries: [(u64, u128); 4], witness keys: [Hash; 4]) -> u64 {{
    if flag {{ return 0 }}
    let first = Work::new()
    let mut second = Work::new()
    second.totals.load_sorted(entries)
    second.seen.load_sorted(keys)
    require second.totals.len() <= 4
    require second.seen.len() <= 4
    second.drop()
    first.drop()
    return 0
}}"#
            );
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            allocated_body_mutations(&valid, ".Lallocated_sorted_4_", &[".Lallocated_reserve_"], 45, true);
            let mut wrong_count = valid.clone();
            for block in &mut wrong_count.record.blocks {
                for label in &mut block.allocated_lifecycle_labels {
                    *label = label.replace(".Lallocated_sorted_4_", ".Lallocated_sorted_3_");
                }
            }
            wrong_count.rebind_sidecars();
            assert_code(&wrong_count, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
    for input in ["[]", "[(1, 7)]"] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let source = format!("module short_sorted\ncollections Work {{ limits {{ bytes: 256, allocations: 2 }}\nvalues: OrderedMap<u64, u64> {{ items: 4 }} }}\naction inspect() -> u64 {{ let mut work = Work::new()\nwork.values.reserve(4)\nwork.values.load_sorted({input})\nreturn 0 }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            allocated_body_mutations(&valid, ".Lallocated_sorted_", &[".Lallocated_reserve_"], 1, true);
        }
    }
}

/// Mutate core instructions outside already-checked children. Guard spills are
/// covered by dedicated tests; retain byte-store mutations inside those guards.
fn allocated_body_mutations(valid: &Fixture, prefix: &str, children: &[&str], minimum: usize, all: bool) {
    let ranges = |prefix: &str| {
        valid
            .record
            .blocks
            .iter()
            .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
            .filter(|(_, label)| label.starts_with(prefix) && label.ends_with("_begin"))
            .map(|(block, label)| {
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                    .unwrap()
                    .range
                    .start;
                (label.clone(), end_label, block.range.start..end)
            })
            .collect::<Vec<_>>()
    };
    let candidates = ranges(prefix);
    assert!(!candidates.is_empty(), "{prefix}");
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let reject = |fixture: &Fixture| {
        let error = check_bundle_values(
            &fixture.artifact,
            &fixture.metadata,
            &fixture.record,
            &fixture.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.message.contains("allocated allocation"), "{error}");
    };
    for (begin_label, end_label, range) in candidates.iter().take(if all { candidates.len() } else { 1 }) {
        let child_ranges = children
            .iter()
            .flat_map(|prefix| ranges(prefix))
            .map(|(_, _, range)| range)
            .filter(|child| range.start <= child.start && child.end <= range.end)
            .collect::<Vec<_>>();
        let body = valid
            .record
            .blocks
            .iter()
            .filter(|block| {
                range.contains(&block.range.start)
                    && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lvalue_vector_checked_"))
            })
            .map(|block| block.range.start)
            .min()
            .unwrap_or(range.start);
        let mut in_guard = false;
        let mut mutated = 0;
        for instruction in elf
            .instructions
            .iter()
            .filter(|i| body <= i.address && i.address < range.end && !child_ranges.iter().any(|child| child.contains(&i.address)))
        {
            let word = instruction.word;
            if word == 0xfe010113 {
                in_guard = true;
            }
            if word == 0x02010113 {
                in_guard = false;
                continue;
            }
            let replacement = if word & 0x707f == 0x23 {
                Some(word ^ (1 << 20))
            } else if in_guard {
                None
            } else {
                match word & 0x7f {
                    0x63 => Some(word ^ (1 << 12)),
                    0x03 => Some(word ^ (1 << 15)),
                    0x13 | 0x1b | 0x33 => Some(word ^ (1 << 20)),
                    0x37 => Some(word ^ (1 << 12)),
                    0x23 => Some(word ^ (1 << 20)),
                    _ => None,
                }
            };
            if let Some(replacement) = replacement {
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, replacement);
                reject(&changed);
                mutated += 1;
            }
        }
        assert!(mutated >= minimum, "{prefix}: {mutated} mutations");
        let mut missing = valid.clone();
        for block in &mut missing.record.blocks {
            block.allocated_lifecycle_labels.retain(|label| label != begin_label && label != end_label);
        }
        missing.rebind_sidecars();
        reject(&missing);
        if let Some(child) = child_ranges.iter().filter(|child| child.start > range.start).min_by_key(|child| child.start) {
            let branch = elf
                .instructions
                .iter()
                .find(|i| range.start <= i.address && i.address < child.start && i.word & 0x7f == 0x63)
                .unwrap();
            let interior = valid
                .record
                .blocks
                .iter()
                .filter(|block| child.start < block.range.start && block.range.start < child.end)
                .map(|block| block.range.start)
                .min()
                .unwrap();
            let mut bypass = valid.clone();
            bypass.redirect_conditional_branch(branch.address, interior);
            reject(&bypass);
        }
    }
    let mut erased = valid.clone();
    for block in &mut erased.record.blocks {
        block.allocated_lifecycle_labels.retain(|label| !label.starts_with(prefix));
    }
    erased.rebind_sidecars();
    reject(&erased);
}

#[test]
fn allocated_initializers_bind_every_state_word_and_reject_rebound_bypasses() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!("module initialization\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 8 }}\nvalues: Vec<u64> {{ items: 4 }}\ntotals: OrderedMap<Hash, u128> {{ items: 4 }}\nseen: OrderedSet<u64> {{ items: 4 }} }}\naction inspect(flag: bool) -> u64 {{ if flag {{ return 0 }}\nlet mut first = Work::new()\nfirst.values.push(7)\nlet mut second = Work::new()\nsecond.values.push(8)\nrequire first.values.get(0) == 7\nrequire second.values.get(0) == 8\nreturn 0 }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let begin = valid
                .record
                .blocks
                .iter()
                .filter(|block| block.allocated_init_labels.iter().any(|label| label.contains("_begin_")))
                .min_by_key(|block| block.range.start)
                .unwrap();
            let label = begin.allocated_init_labels.iter().find(|label| label.contains("_begin_")).unwrap();
            let prefix = format!("{}_end_", label.split("_begin_").next().unwrap());
            let end = valid
                .record
                .blocks
                .iter()
                .find(|block| block.allocated_init_labels.iter().any(|label| label.starts_with(&prefix)))
                .unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let stores = elf
                .instructions
                .iter()
                .filter(|instruction| {
                    begin.range.start <= instruction.address
                        && instruction.address < end.range.start
                        && instruction.word & 0x707f == 0x3023
                })
                .collect::<Vec<_>>();
            assert_eq!(stores.len(), 17); // Four state words, four per member, owner pointer.
            for store in &stores {
                let mut changed = valid.clone();
                changed.replace_machine_word(store.address, 0x00000013);
                let error = check_bundle_values(
                    &changed.artifact,
                    &changed.metadata,
                    &changed.record,
                    &changed.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.message.contains("allocated initialization"), "{error}");
            }
            for index in [7, 11, 15, 16] {
                // Live flags and published owner pointer must not be zero.
                let mut changed = valid.clone();
                changed.replace_machine_word(stores[index].address, stores[index].word & !(31 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
            let mut missing = valid.clone();
            for block in &mut missing.record.blocks {
                block.allocated_init_labels.clear();
            }
            missing.rebind_sidecars();
            assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

            // Rebind the machine CFG as well as all byte/outer hashes. A
            // structurally valid branch must not enter after initialization.
            let flow = elf
                .control_flow
                .iter()
                .find(|flow| {
                    flow.address < begin.range.start
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == begin.owner_entry && block.range.contains(flow.address))
                        && elf
                            .instructions
                            .iter()
                            .any(|instruction| instruction.address == flow.address && instruction.word & 0x7f == 0x63)
                })
                .unwrap();
            let origin = valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap();
            let word = elf.instructions.iter().find(|instruction| instruction.address == flow.address).unwrap().word;
            let offset = end.range.start as i64 - flow.address as i64;
            assert!((-4096..4096).contains(&offset));
            let immediate = offset as u32;
            let branch = (word & 0x01fff07f)
                | (((immediate >> 12) & 1) << 31)
                | (((immediate >> 5) & 0x3f) << 25)
                | (((immediate >> 1) & 15) << 8)
                | (((immediate >> 11) & 1) << 7);
            let mut bypass = valid.clone();
            bypass
                .record
                .edges
                .iter_mut()
                .find(|edge| edge.from == origin.id && edge.kind == EdgeKind::ConditionalTaken)
                .unwrap()
                .to = end.id.clone();
            let mut reachable = std::collections::BTreeSet::new();
            let mut pending = bypass.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
            while let Some(id) = pending.pop() {
                if reachable.insert(id.clone()) {
                    pending.extend(bypass.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
                }
            }
            for block in &mut bypass.record.blocks {
                block.reachable = reachable.contains(&block.id);
            }
            bypass.record.canonicalize();
            bypass.replace_machine_word(flow.address, branch);
            let error = check_bundle_values(
                &bypass.artifact,
                &bypass.metadata,
                &bypass.record,
                &bypass.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
            // The source-branch recognizer can reject the redirected successor
            // before the generic initialization-path coverage check reaches it.
            assert!(error.message.contains("bypass") || error.message.contains("control-flow target differs"), "{error}");
        }
    }
}

#[test]
fn allocated_values_owner_lifetimes_reject_rebound_stale_and_borrowed_handles() {
    use cellscript_artifact_checker::allocated_values::Operation as O;
    let source = "module lifetime\ncollections Work { limits { bytes: 256, allocations: 4 }\nvalues: Vec<u64> { items: 8 } }\nfn count(view: &Work) -> u64 { return view.values.len() }\naction inspect() -> u64 { let mut original = Work::new()\noriginal.values.push(7)\nlet moved = original\nlet observed = count(&moved)\nmoved.drop()\nreturn observed }";
    let reject = |mut fixture: Fixture, signature_rejection: bool| {
        fixture.rebind_typed_semantics();
        let error = check_bundle_values(
            &fixture.artifact,
            &fixture.metadata,
            &fixture.record,
            &fixture.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{error}");
        let boundary = if signature_rejection { "invalid signature contract" } else { "allocated owner lifetime" };
        assert!(error.to_string().contains(boundary), "mutation missed {boundary}: {error}");
    };
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        for mutation in 0..6 {
            let mut changed = valid.clone();
            let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "inspect").unwrap();
            let original = entry.locals.iter().find(|local| local.name == "original").unwrap().id;
            let constructor = entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .find(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::New, .. }))
                .unwrap()
                .destinations[0];
            let reference = entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::UnaryOperator { operator } if operator == "ref"))
                .unwrap()
                .destinations[0];
            let op = entry
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.operations)
                .find(|op| match mutation {
                    0 | 3 => matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::Push, .. }),
                    1 | 4 => matches!(&op.detail, TypedSemanticOperationDetail::UnaryOperator { operator } if operator == "ref"),
                    _ => matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::Drop, .. }),
                })
                .unwrap();
            match mutation {
                0 => op.operands[0].local = Some(constructor), // Constructor result has moved into original.
                1 | 2 => op.operands[0].local = Some(original), // Original has moved into moved.
                3 => {
                    // Retire original before the later move and read.
                    let TypedSemanticOperationDetail::AllocatedValue { operation, member, .. } = &mut op.detail else {
                        unreachable!()
                    };
                    *operation = O::Drop;
                    *member = None;
                    op.operands.truncate(1);
                }
                4 => {
                    // Same canonical ABI type does not grant a read-only call an owned argument.
                    op.opcode = "move".into();
                    op.detail = TypedSemanticOperationDetail::None;
                }
                _ => op.operands[0].local = Some(reference), // Ref temporaries carry Work in the ABI, but cannot drop it.
            }
            reject(changed, mutation == 4);
        }
        let evaluation = source
            .replace(
                "fn count(view: &Work) -> u64 { return view.values.len() }",
                "fn count(view: &Work, n: u64) -> u64 { return view.values.len() + n }",
            )
            .replace(
                "let observed = count(&moved)",
                "let mut other = Work::new()\nlet observed = count(&moved, { other.values.push(2)\n3 })",
            );
        let mut changed = Fixture::from_result(
            compile(
                &evaluation,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "inspect").unwrap();
        let moved = entry.locals.iter().find(|local| local.name == "moved").unwrap().id;
        let op = entry
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.operations)
            .filter(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::Push, .. }))
            .nth(1)
            .unwrap();
        op.operands[0].local = Some(moved); // Invalidate the first argument while evaluating the second.
        reject(changed, false);
    }
}

#[test]
fn allocated_values_lifetimes_follow_joins_backedges_and_lexical_loans() {
    use cellscript_artifact_checker::allocated_values::Operation as O;
    let prefix = "module lifetime_cfg\ncollections Work { limits { bytes: 256, allocations: 4 }\nvalues: Vec<u64> { items: 8 } }\n";
    let bodies = [
        "action inspect(flag: bool) -> u64 { let mut work = Work::new()\nif flag { work.values.reserve(1) } else { work.values.reserve(2) }\nreturn work.values.len() }",
        "action inspect() -> u64 { let mut work = Work::new()\nfor i in 0..2 { work.values.reserve(i) }\nreturn work.values.len() }",
        "action inspect() -> u64 { let work = Work::new()\nlet mut other = Work::new()\nborrow work as view { other.values.push(9)\nrequire view.values.len() == 0 }\nreturn 0 }",
        "action inspect() -> u64 { let work = Work::new()\nlet mut other = Work::new()\nborrow work as view { borrow view as inner { other.values.push(9)\nrequire inner.values.len() == 0 } }\nreturn 0 }",
        "action inspect(flag: bool) -> u64 { let mut work = Work::new()\nif flag { borrow work as view { require view.values.len() == 0 } }\nwork.values.push(1)\nreturn 0 }",
    ];
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (case, body) in bodies.iter().enumerate() {
            let source = format!("{prefix}{body}");
            let mut changed = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "inspect").unwrap();
            let work = entry.locals.iter().find(|local| local.name == "work").unwrap().id;
            let (block_id, op) = entry
                .blocks
                .iter_mut()
                .flat_map(|block| {
                    let id = block.id;
                    block.operations.iter_mut().map(move |op| (id, op))
                })
                .find(|(_, op)| {
                    matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::Reserve | O::Push, .. })
                })
                .unwrap();
            if case < 2 {
                let TypedSemanticOperationDetail::AllocatedValue { operation, member, .. } = &mut op.detail else { unreachable!() };
                *operation = O::Drop;
                *member = None;
                op.operands.truncate(1);
            } else if case < 4 {
                op.operands[0].local = Some(work);
            } else {
                // Keep a lexical loan alive on only one predecessor. Joining
                // available handles by intersection must not discard the loan.
                let position = op.index + 1;
                entry.borrows[0].end_block = Some(block_id);
                entry.borrows[0].end_operation = Some(position);
            }
            changed.rebind_typed_semantics();
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "case {case}/{diagnostics}: {error}");
            assert!(error.to_string().contains("allocated owner lifetime"), "case {case}/{diagnostics}: {error}");
        }
    }
}

#[test]
fn allocated_values_keep_structured_contract_after_admission_and_rebinding() {
    let source = "module allocated_proof\ncollections Work { limits { bytes: 128, allocations: 2 }\nvalues: Vec<u64> { items: 8 } }\nfn count(view: &Work) -> u64 { return view.values.len() }\naction inspect() -> u64 { let mut work = Work::new()\nwork.values.push(9)\nrequire work.values.get(0) == 9\nrequire count(&work) == 1\nreturn 0 }";
    let valid = Fixture::from_result(
        compile(source, CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
    );
    valid.check().unwrap();
    assert_bounded_admitted(&valid);
    for mutation in 0..4 {
        let mut changed = valid.clone();
        let op = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { .. }))
            .unwrap();
        let TypedSemanticOperationDetail::AllocatedValue { contract, member, operation } = &mut op.detail else { unreachable!() };
        match mutation {
            0 => contract.bytes = 63,
            1 => contract.allocations = 0,
            2 => *member = Some(99),
            _ => *operation = cellscript_artifact_checker::allocated_values::Operation::Drop,
        }
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
    let mut drift = valid.clone();
    let op = drift
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { .. }))
        .unwrap();
    if let TypedSemanticOperationDetail::AllocatedValue { contract, .. } = &mut op.detail {
        contract.bytes = 256;
    }
    drift.rebind_typed_semantics();
    assert_code(&drift, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    let mut callee_drift = valid;
    let helper = callee_drift.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "count").unwrap();
    for op in helper.blocks.iter_mut().flat_map(|block| &mut block.operations) {
        if let TypedSemanticOperationDetail::AllocatedValue { contract, .. } = &mut op.detail {
            contract.bytes = 256;
        }
    }
    callee_drift.rebind_typed_semantics();
    assert_code(&callee_drift, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn fixed_byte_result_contract_rejects_rebound_width_and_missing_copy() {
    let source = "module byte_copy_proof\nfn bytes() -> [u8; 9] { return b\"constant!\" }\naction inspect() -> u64 { require bytes() == b\"constant!\"\nreturn 0 }";
    let valid = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                edition: NEXT_EDITION,
                diagnostics: cellscript::CompilerDiagnostics::default().without(cellscript::OptimizationPass::Inlining),
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    valid.check().unwrap();
    for mutation in 0..4 {
        let mut changed = valid.clone();
        let entry = changed.record.entries.iter_mut().find(|entry| entry.name == "bytes").unwrap();
        match mutation {
            0 => entry.fixed_result_abi = None,
            1 => entry.fixed_result_abi.as_mut().unwrap().width_bytes = 8,
            2 => entry.fixed_result_abi.as_mut().unwrap().copy_ranges.clear(),
            _ => entry.fixed_result_abi.as_mut().unwrap().type_name = "[u8; 8]".into(),
        }
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);
    }
}

#[test]
fn explicit_hash_constructor_rejects_implicit_retyping_extra_fields_and_wrong_copy_source() {
    let source = "module hash_constructor\nfn make(bytes: [u8; 32]) -> Hash { return Hash::from_bytes(bytes) }\naction inspect(witness bytes: [u8; 32]) -> u64 { verification require make(bytes) == Hash::from_bytes(b\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\") return 0 }";
    let valid = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                diagnostics: cellscript::CompilerDiagnostics::default().without(cellscript::OptimizationPass::Inlining),
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let constructor = valid
        .record
        .typed_semantics
        .entries
        .iter()
        .enumerate()
        .find_map(|(entry_index, entry)| {
            entry.blocks.iter().enumerate().find_map(|(block_index, block)| {
                block.operations.iter().enumerate().find_map(|(operation_index, operation)| {
                    (operation.opcode == "tuple"
                        && operation.destinations.len() == 1
                        && entry.locals.iter().any(|local| local.id == operation.destinations[0] && local.ty == "hash"))
                    .then_some((entry_index, block_index, operation_index))
                })
            })
        })
        .expect("explicit constructor retained in typed semantics");
    for extra_field in [false, true] {
        let mut changed = valid.clone();
        let operation = &mut changed.record.typed_semantics.entries[constructor.0].blocks[constructor.1].operations[constructor.2];
        if extra_field {
            operation.operands.push(operation.operands[0].clone());
        } else {
            operation.opcode = "move".into();
        }
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
    let abi = valid.record.entries.iter().find(|entry| entry.name == "make").unwrap().fixed_result_abi.as_ref().unwrap();
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let load = elf
        .instructions
        .iter()
        .find(|instruction| {
            abi.copy_sources.iter().any(|source| source.range.contains(instruction.address))
                && instruction.word & 0x7f == 0x03
                && (instruction.word >> 7) & 31 == 10
        })
        .expect("owned result source-pointer load");
    let mut changed = valid.clone();
    changed.replace_machine_word(load.address, load.word & !(31 << 15));
    assert!(changed.check().is_err(), "absolute memory cannot replace the proved owned source pointer");
}

#[test]
fn committed_state_typed_call_contracts_cannot_be_relabelled_after_rebinding() {
    let valid = Fixture::from_source(COMMITTED_STATE_SOURCE);
    let calls = valid
        .record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .filter_map(|operation| operation.call.as_ref())
        .map(|call| call.target.as_str())
        .collect::<Vec<_>>();
    assert!(calls.contains(&"__ckb_commitment_open"));
    assert!(calls.contains(&"__ckb_hash_blake2b_packed"));

    let mut changed_opening = valid.clone();
    let opening = changed_opening
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| operation.call.as_mut().filter(|call| call.target == "__ckb_commitment_open"))
        .expect("typed commitment opening call");
    opening.params[1] = "Opening<u64>".to_string();
    changed_opening.rebind_typed_semantics();
    assert_code(&changed_opening, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed_commit = valid;
    let commit = changed_commit
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| {
            operation
                .call
                .as_mut()
                .filter(|call| call.target == "__ckb_hash_blake2b_packed" && call.return_type.starts_with("Commitment<"))
        })
        .expect("typed commitment construction call");
    commit.params[0] = "u64".to_string();
    changed_commit.rebind_typed_semantics();
    assert_code(&changed_commit, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn committed_state_machine_blocks_reject_rebound_removal_reordering_and_instruction_mutations() {
    let valid = Fixture::from_source(COMMITTED_STATE_SOURCE);
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let marker = |prefix: &str| {
        valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.owner_entry == "action:inspect" && block.machine_label.as_deref().is_some_and(|label| label.starts_with(prefix))
            })
            .expect("committed-state marker")
    };
    let opening = marker(".Lcommitment_opening_header_ready_");
    let authenticated = marker(".Lcommitment_opening_verified_");
    let recommit = marker(".Lcommitment_commit_header_ready_");
    assert!(opening.range.start < authenticated.range.start && authenticated.range.start < recommit.range.start);

    let runtime_target = |name: &str| {
        let entry = valid.record.entries.iter().find(|entry| entry.id == format!("runtime:{name}")).expect("runtime entry");
        valid.record.blocks.iter().find(|block| block.id == entry.entry_block).expect("runtime entry block").range.start
    };
    let calls = |name: &str, start: u64, end: u64| {
        let target = runtime_target(name);
        elf.control_flow
            .iter()
            .filter(|flow| start <= flow.address && flow.address < end && flow.target == target)
            .map(|flow| flow.address)
            .collect::<Vec<_>>()
    };
    let opening_hash = calls("__ckb_hash_blake2b_var", opening.range.start, authenticated.range.start)[0];
    let opening_compare = calls("__cellscript_memcmp_fixed", opening.range.start, authenticated.range.start)[0];
    let recommit_hash = calls("__ckb_hash_blake2b_var", recommit.range.start, recommit.range.end + 512)[0];
    let word_at = |address| elf.instructions.iter().find(|instruction| instruction.address == address).expect("machine word").word;
    let last_immediate_definition = |address: u64, register: u32| {
        elf.instructions
            .iter()
            .filter(|instruction| instruction.address < address)
            .rev()
            .find(|instruction| {
                instruction.word & 0x7f == 0x13
                    && (instruction.word >> 12) & 0x7 == 0
                    && (instruction.word >> 7) & 0x1f == register
                    && (instruction.word >> 15) & 0x1f == 0
            })
            .expect("immediate argument definition")
            .address
    };

    let mut changed = valid.clone();
    changed.record.blocks.iter_mut().find(|block| block.id == opening.id).unwrap().machine_label = None;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let opening_index = changed.record.blocks.iter().position(|block| block.id == opening.id).unwrap();
    let recommit_index = changed.record.blocks.iter().position(|block| block.id == recommit.id).unwrap();
    let opening_label = changed.record.blocks[opening_index].machine_label.take();
    let recommit_label = changed.record.blocks[recommit_index].machine_label.take();
    changed.record.blocks[opening_index].machine_label = recommit_label;
    changed.record.blocks[recommit_index].machine_label = opening_label;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    for marker in [opening, recommit] {
        let mut changed = valid.clone();
        let address = marker.range.start - 8;
        changed.replace_machine_word(address, word_at(address) ^ (1 << 20));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }

    for (call, register) in [(opening_hash, 11), (opening_compare, 12), (recommit_hash, 11)] {
        let mut changed = valid.clone();
        let address = last_immediate_definition(call, register);
        let immediate = (word_at(address) as i32) >> 20;
        changed.replace_machine_word(address, replace_i_immediate(word_at(address), immediate + 1));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }

    let mut changed = valid.clone();
    changed.replace_machine_word(opening_compare + 4, word_at(opening_compare + 4) ^ (1 << 12));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let opening_destination = valid
        .record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "__ckb_commitment_open"))
        .and_then(|operation| operation.destinations.first())
        .copied()
        .unwrap();
    let destination_offset = i32::try_from(opening_destination * 8).unwrap();
    let destination_store = elf
        .instructions
        .iter()
        .find(|instruction| {
            authenticated.range.start <= instruction.address
                && instruction.address < recommit.range.start
                && instruction.word & 0x7f == 0x23
                && (instruction.word >> 12) & 0x7 == 3
                && (instruction.word >> 20) & 0x1f == 5
                && (instruction.word >> 15) & 0x1f == 2
                && decode_s_immediate(instruction.word) == destination_offset
        })
        .expect("authenticated opening destination store");
    let mut changed = valid;
    changed.replace_machine_word(destination_store.address, replace_s_immediate(destination_store.word, destination_offset + 8));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn committed_state_machine_binds_schema_field_pointer_and_offset() {
    let valid = Fixture::from_source(COMMITTED_STATE_FIELD_SOURCE);
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let opening = valid
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == "action:inspect"
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lcommitment_opening_header_ready_"))
        })
        .expect("committed-state field marker");
    let authenticated = valid
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == "action:inspect"
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lcommitment_opening_verified_"))
        })
        .expect("committed-state field authenticated marker");
    let runtime = valid.record.entries.iter().find(|entry| entry.id == "runtime:__cellscript_memcmp_fixed").unwrap();
    let target = valid.record.blocks.iter().find(|block| block.id == runtime.entry_block).unwrap().range.start;
    let compare = elf
        .control_flow
        .iter()
        .find(|flow| opening.range.start <= flow.address && flow.address < authenticated.range.start && flow.target == target)
        .expect("committed-state field comparison")
        .address;
    let field_offset = elf
        .instructions
        .iter()
        .filter(|instruction| opening.range.start <= instruction.address && instruction.address < compare)
        .rev()
        .find(|instruction| {
            instruction.word & 0x7f == 0x13
                && (instruction.word >> 12) & 0x7 == 0
                && (instruction.word >> 7) & 0x1f == 11
                && (instruction.word >> 15) & 0x1f == 11
                && (instruction.word as i32 >> 20) == 8
        })
        .expect("typed field pointer offset");

    let mut changed = valid;
    changed.replace_machine_word(field_offset.address, replace_i_immediate(field_offset.word, 16));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn verified_artifact_sidecars_are_deterministic_and_canonical() {
    let first = Fixture::new();
    let second = Fixture::new();
    assert_eq!(first.artifact, second.artifact);
    assert_eq!(canonical_bytes(&first.record).unwrap(), canonical_bytes(&second.record).unwrap());
    assert_eq!(canonical_bytes(&first.source_map).unwrap(), canonical_bytes(&second.source_map).unwrap());
    assert!(first.source_map.intervals.iter().all(|interval| interval.source_path == "<memory>"));
    let foundation = &first.record.typed_semantics.foundation;
    assert_eq!(foundation.schema, cellscript_artifact_checker::SEMANTIC_FOUNDATION_SCHEMA);
    assert!(!foundation.identities.core_semantic_id.is_empty());
    assert!(!foundation.identities.entry_contract_id.is_empty());
    assert!(!foundation.identities.artifact_contract_id.is_empty());
    assert_eq!(first.metadata["verified_artifact"]["deployable_artifact_id"], Value::String(first.record.artifact_hash.clone()));
    assert_eq!(
        first.metadata["verified_artifact"]["boundary_schema"],
        Value::String(cellscript_artifact_checker::VERIFIED_ARTIFACT_BOUNDARY_SCHEMA.to_string())
    );
    let mapped = first
        .source_map
        .semantic_mappings
        .iter()
        .map(|mapping| mapping.semantic_node_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(foundation.provenance.nodes.iter().all(|node| mapped.contains(node.id.as_str())));
}

#[test]
fn checker_accepts_native_edition_2027_type_script_trigger_and_disposition() {
    let result = compile(
        NATIVE_EDITION_2027_SOURCE,
        CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let fixture = Fixture::from_result(result);
    let foundation = &fixture.record.typed_semantics.foundation;
    assert_eq!(foundation.entry_contract.trigger, "type-group<Token>");
    assert_eq!(foundation.entry_contract.exact_entry, "action:transfer");
    assert!(foundation.dispositions.iter().any(|disposition| {
        matches!(
            &disposition.input,
            Some(cellscript_artifact_checker::InputDisposition::Successor { output_role })
                if output_role.ends_with(":next:output[0]")
        ) && disposition.envelope.completeness == "exhaustive"
    }));
    let enforced = foundation.claims.iter().find(|claim| claim.execution.is_some()).expect("native enforce claim");
    assert_eq!(enforced.statement, "require token.amount > 0");
    assert_eq!(enforced.enforcement, "checked-runtime");
    assert!(enforced.on_chain_checked);
    let mapping = fixture
        .source_map
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == enforced.semantic_node_id)
        .expect("native enforce claim must have a source mapping");
    let mapped_source = &NATIVE_EDITION_2027_SOURCE[mapping.source_start as usize..mapping.source_end as usize];
    assert_eq!(mapped_source, "enforce token.amount > 0");

    let mut invalid = fixture.clone();
    invalid
        .record
        .typed_semantics
        .foundation
        .claims
        .iter_mut()
        .find_map(|claim| claim.execution.as_mut())
        .expect("native enforce execution binding")
        .condition_node_id = "00".repeat(32);
    invalid.rebind_typed_semantics();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut invalid = fixture.clone();
    let execution = invalid
        .record
        .typed_semantics
        .foundation
        .claims
        .iter_mut()
        .find_map(|claim| claim.execution.as_mut())
        .expect("native enforce execution binding");
    std::mem::swap(&mut execution.success_block, &mut execution.failure_block);
    invalid.rebind_typed_semantics();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut invalid = fixture;
    invalid
        .record
        .typed_semantics
        .foundation
        .claims
        .iter_mut()
        .find_map(|claim| claim.execution.as_mut())
        .expect("native enforce execution binding")
        .failure_error_code = 1;
    invalid.rebind_typed_semantics();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_accepts_fresh_retire_and_audit_but_rejects_reclassified_evidence() {
    let fresh = compile(
        NATIVE_FRESH_AUDIT_EDITION_2027_SOURCE,
        CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let fixture = Fixture::from_result(fresh);
    let foundation = &fixture.record.typed_semantics.foundation;
    assert!(matches!(
        foundation.dispositions[0].output,
        Some(cellscript_artifact_checker::OutputOrigin::Fresh { ref identity_policy }) if identity_policy == "ckb-type-id"
    ));
    let fresh_mapping = fixture
        .source_map
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == foundation.dispositions[0].semantic_node_id)
        .expect("fresh disposition source mapping");
    assert!(NATIVE_FRESH_AUDIT_EDITION_2027_SOURCE[fresh_mapping.source_start as usize..fresh_mapping.source_end as usize]
        .starts_with("fresh next"));
    let audit = foundation.claims.iter().find(|claim| claim.category == "audit").expect("audit claim");
    assert_eq!(audit.enforcement, "metadata-only");
    assert!(!audit.on_chain_checked);
    assert!(audit.execution.is_none());
    let audit_mapping = fixture
        .source_map
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == audit.semantic_node_id)
        .expect("audit claim source mapping");
    assert!(NATIVE_FRESH_AUDIT_EDITION_2027_SOURCE[audit_mapping.source_start as usize..audit_mapping.source_end as usize]
        .starts_with("audit issuance_policy"));

    let mut invalid = fixture.clone();
    invalid.record.typed_semantics.foundation.claims.iter_mut().find(|claim| claim.category == "audit").unwrap().evidence_reference =
        "proof-plan:pretend-on-chain".to_string();
    invalid.rebind_typed_semantics();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut invalid = fixture;
    invalid.record.typed_semantics.foundation.claims.iter_mut().find(|claim| claim.category == "audit").unwrap().on_chain_checked =
        true;
    invalid.rebind_typed_semantics();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let retired = compile(
        NATIVE_RETIRE_EDITION_2027_SOURCE,
        CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let fixture = Fixture::from_result(retired);
    assert!(matches!(
        fixture.record.typed_semantics.foundation.dispositions[0].input,
        Some(cellscript_artifact_checker::InputDisposition::Retired { ref absence_policy })
            if absence_policy == "same-field-identity-output-absent:note_id"
    ));

    let mut invalid = fixture;
    invalid.record.typed_semantics.foundation.dispositions[0].envelope.completeness = "partial".to_string();
    invalid.rebind_typed_semantics();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_accepts_checked_pool_and_rejects_divergent_accounting() {
    let result = compile(
        NATIVE_POOL_EDITION_2027_SOURCE,
        CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let fixture = Fixture::from_result(result);
    let foundation = &fixture.record.typed_semantics.foundation;
    assert_eq!(
        foundation
            .dispositions
            .iter()
            .filter(|disposition| matches!(disposition.input, Some(cellscript_artifact_checker::InputDisposition::Pooled { .. })))
            .count(),
        2
    );
    assert!(foundation.dispositions.iter().any(|disposition| {
        matches!(
            disposition.output,
            Some(cellscript_artifact_checker::OutputOrigin::PoolResult {
                ref accounting_obligation,
                ..
            }) if accounting_obligation == "checked-u128-field-sum-equality:amount"
        )
    }));
    let pool_result = foundation
        .dispositions
        .iter()
        .find(|disposition| matches!(disposition.output, Some(cellscript_artifact_checker::OutputOrigin::PoolResult { .. })))
        .unwrap();
    let pool_mapping = fixture
        .source_map
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == pool_result.semantic_node_id)
        .expect("pool disposition source mapping");
    assert!(NATIVE_POOL_EDITION_2027_SOURCE[pool_mapping.source_start as usize..pool_mapping.source_end as usize]
        .starts_with("pool value_flow"));
    assert!(foundation.claims.iter().any(|claim| {
        claim.statement == "require left.amount as u128 + right.amount as u128 == merged.amount as u128"
            && claim.on_chain_checked
            && claim.execution.is_some()
    }));

    let mut invalid = fixture.clone();
    let Some(cellscript_artifact_checker::OutputOrigin::PoolResult { accounting_obligation, .. }) =
        invalid.record.typed_semantics.foundation.dispositions.iter_mut().find_map(|disposition| disposition.output.as_mut())
    else {
        panic!("pool result disposition");
    };
    *accounting_obligation = "checked-u128-field-sum-equality:forged".to_string();
    invalid.rebind_typed_semantics();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut invalid = fixture;
    for disposition in &mut invalid.record.typed_semantics.foundation.dispositions {
        if let Some(cellscript_artifact_checker::InputDisposition::Pooled { accounting_obligation, .. }) = &mut disposition.input {
            accounting_obligation.clear();
        }
        if let Some(cellscript_artifact_checker::OutputOrigin::PoolResult { accounting_obligation, .. }) = &mut disposition.output {
            accounting_obligation.clear();
        }
    }
    invalid.rebind_typed_semantics();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_accepts_native_lock_script_with_byte_identical_legacy_lowering() {
    let legacy =
        compile(LEGACY_LOCK_SOURCE, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
    let native = compile(
        NATIVE_LOCK_EDITION_2027_SOURCE,
        CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    assert_eq!(legacy.artifact_bytes, native.artifact_bytes);
    assert_eq!(legacy.metadata.typed_semantics.foundation.identities, native.metadata.typed_semantics.foundation.identities);
    let legacy_claim = legacy
        .metadata
        .typed_semantics
        .foundation
        .claims
        .iter()
        .find(|claim| claim.execution.is_some())
        .expect("legacy require claim");
    let legacy_mapping = legacy
        .source_artifact_map
        .as_ref()
        .unwrap()
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == legacy_claim.semantic_node_id)
        .expect("legacy require claim must have a source mapping");
    assert_eq!(
        &LEGACY_LOCK_SOURCE[legacy_mapping.source_start as usize..legacy_mapping.source_end as usize],
        "require vault.owner == owner"
    );

    let fixture = Fixture::from_result(native);
    let contract = &fixture.record.typed_semantics.foundation.entry_contract;
    assert_eq!(contract.script_role, "lock");
    assert_eq!(contract.trigger, "lock-group");
    assert_eq!(contract.exact_entry, "lock:unlock");

    let mut invalid = fixture;
    let disposition = invalid.record.typed_semantics.foundation.dispositions.first_mut().expect("lock disposition");
    let Some(cellscript_artifact_checker::InputDisposition::AuthorizationOnly { disposition_owner }) = &mut disposition.input else {
        panic!("native Lock Script must emit AuthorizationOnly");
    };
    disposition_owner.clear();
    invalid.rebind_typed_semantics();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn semantic_foundation_and_source_mapping_mutations_are_rejected() {
    let valid = Fixture::new();

    let mut changed = valid.clone();
    changed.record.typed_semantics.foundation.provenance.nodes[0].id = "00".repeat(32);
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    changed.record.typed_semantics.foundation.identities.core_semantic_id = "11".repeat(32);
    changed.metadata["verified_artifact"]["core_semantic_id"] = Value::String("11".repeat(32));
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid;
    changed.source_map.semantic_mappings[0].semantic_node_id = "22".repeat(32);
    changed.source_map.canonicalize();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2416SourceMapInvalid);
}

#[test]
fn fixed_cell_binding_mutations_are_rejected_after_outer_hash_rebinding() {
    use cellscript_artifact_checker::{CellBindingMembership, CellBindingSource};
    let valid = Fixture::from_source(
        r#"
module fixed_binding_mutations
resource Token has consume { amount: u64 }
shared Config { value: u64 }
action inspect(input token: Token, read config: Config, witness expected: u64) -> u64 {
    verification
        require token.amount == expected
        require config.value == expected
        consume token
        return 0
}
"#,
    );
    assert_eq!(valid.record.typed_semantics.entries[0].cell_bindings.len(), 2);

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].cell_bindings[0].ordinal += 1;
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let binding = changed.record.typed_semantics.entries[0]
        .cell_bindings
        .iter_mut()
        .find(|binding| binding.source == CellBindingSource::Input)
        .unwrap();
    binding.source = CellBindingSource::GroupInput;
    binding.membership = CellBindingMembership::CurrentTypeGroup;
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let binding = changed.record.typed_semantics.entries[0]
        .cell_bindings
        .iter_mut()
        .find(|binding| binding.source == CellBindingSource::CellDep)
        .unwrap();
    binding.membership = CellBindingMembership::CurrentTypeGroup;
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].cell_bindings.remove(0);
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let entry = &mut changed.record.typed_semantics.entries[0];
    let removed = entry
        .cell_bindings
        .remove(entry.cell_bindings.iter().position(|binding| binding.source == CellBindingSource::CellDep).unwrap());
    let removed_role = removed.role_id(&entry.id);
    changed.record.typed_semantics.foundation.roles.retain(|role| role.role_id != removed_role);
    changed.rebind_typed_semantics();
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    assert!(error.message.contains("fixed Cell parameter 'config' has no resolved binding"), "{error:?}");

    let mut changed = valid;
    let duplicate = changed.record.typed_semantics.entries[0].cell_bindings[0].clone();
    changed.record.typed_semantics.entries[0].cell_bindings.insert(0, duplicate);
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn deferred_sighash_effect_cannot_be_reclassified_after_hash_rebinding() {
    let source = r#"
module deferred_sighash_checker
action main() -> u64 {
    verification
    let digest = env::sighash_all(source::group_input(0))
    return 0
}
"#;
    let fixture = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                target: Some("riscv64-elf".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    for effect in ["runtime-contract", "deferred-runtime-fail-closed:0:ckb-sighash-all-deferred", "Pure"] {
        let mut changed = fixture.clone();
        let call = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .filter_map(|operation| operation.call.as_mut())
            .find(|call| call.target == "__ckb_sighash_all")
            .expect("deferred call is present");
        call.effect = effect.to_string();
        changed.rebind_typed_semantics();
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .expect_err("deferred failure classification cannot be stripped");
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        assert!(error.message.contains("deferred sighash call"), "{error}");
    }
}

#[test]
fn exact_handle_contract_cannot_be_reclassified_after_hash_rebinding() {
    let fixture = Fixture::from_result(
        compile(
            EXACT_HANDLE_SOURCE,
            CompileOptions {
                edition: NEXT_EDITION,
                diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );

    let mut changed = fixture.clone();
    exact_handle_operation(&mut changed).call.as_mut().unwrap().target = "__foreign_exact_handle".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture.clone();
    exact_handle_operation(&mut changed).call.as_mut().unwrap().effect = "Pure".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture.clone();
    let operation = exact_handle_operation(&mut changed);
    operation.operands[2].ty = "address".to_string();
    operation.operands[2].constant = Some(TypedSemanticConstant::Address("00".repeat(32)));
    operation.call.as_mut().unwrap().params[2] = "address".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture;
    let operation = exact_handle_operation(&mut changed);
    let Some(TypedSemanticConstant::Hash(hash)) = operation.operands[2].constant.as_mut() else {
        panic!("expected exact-handle hash constant")
    };
    hash.replace_range(0..2, "ff");
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn deployment_line_handle_contract_cannot_be_reclassified_after_hash_rebinding() {
    let fixture = Fixture::from_result(
        compile(
            DEPLOYMENT_LINE_HANDLE_SOURCE,
            CompileOptions {
                edition: NEXT_EDITION,
                diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert!(check_bundle_values(
        &fixture.artifact,
        &fixture.metadata,
        &fixture.record,
        &fixture.source_map,
        &CheckerBudgets::default(),
    )
    .is_ok());

    let mut changed = fixture.clone();
    deployment_line_handle_operation(&mut changed).call.as_mut().unwrap().target = "__foreign_deployment_line".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture.clone();
    deployment_line_handle_operation(&mut changed).call.as_mut().unwrap().effect = "Pure".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture.clone();
    let operation = deployment_line_handle_operation(&mut changed);
    operation.operands[2].ty = "ExactScriptHandle".to_string();
    operation.call.as_mut().unwrap().params[2] = "ExactScriptHandle".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture;
    let operation = deployment_line_handle_operation(&mut changed);
    operation.operands[3].ty = "address".to_string();
    operation.operands[3].constant = Some(TypedSemanticConstant::Address("00".repeat(32)));
    operation.call.as_mut().unwrap().params[3] = "address".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_accepts_an_explicit_versioned_dispatch_contract() {
    let mut fixture = Fixture::new();
    let foundation = &mut fixture.record.typed_semantics.foundation;
    let selector_node_id = foundation
        .provenance
        .nodes
        .iter()
        .find(|node| !matches!(node.provenance, cellscript_artifact_checker::ValueProvenance::Derived { .. }))
        .expect("fixture must have a provenance root")
        .id
        .clone();
    let exact_entry = foundation.entry_contract.exact_entry.clone();
    let previous_contract_node = foundation.entry_contract.semantic_node_id.clone();
    foundation.entry_contract.dispatch = cellscript_artifact_checker::EntryDispatchContract::ExplicitVersionedDispatch {
        selector_node_id,
        selector_type: "u32-le".to_string(),
        variants: vec![cellscript_artifact_checker::EntryDispatchVariant { tag: "0".to_string(), entry_id: exact_entry.clone() }],
        unknown_selector: "reject".to_string(),
    };
    foundation.entry_contract.semantic_node_id = canonical_hash(
        "cellscript-semantic-node-entry-contract-v1",
        &(
            foundation.entry_contract.script_role.as_str(),
            foundation.entry_contract.trigger.as_str(),
            exact_entry.as_str(),
            "explicit-versioned-dispatch",
            foundation.entry_contract.entry_payload_abi.as_str(),
            foundation.entry_contract.witness_placement_abi.as_str(),
            foundation.entry_contract.witness_placement_field.as_str(),
            foundation.entry_contract.witness_placement_source.as_str(),
        ),
    )
    .unwrap();
    let roots = foundation
        .provenance
        .nodes
        .iter()
        .filter(|node| !matches!(node.provenance, cellscript_artifact_checker::ValueProvenance::Derived { .. }))
        .map(|node| node.id.as_str())
        .collect::<Vec<_>>();
    foundation.identities.entry_contract_id = canonical_hash(
        "cellscript-entry-contract-id-v1",
        &(
            foundation.identities.core_semantic_id.as_str(),
            &foundation.entry_contract,
            roots,
            foundation.entry_contract.entry_payload_abi.as_str(),
            foundation.entry_contract.witness_placement_abi.as_str(),
        ),
    )
    .unwrap();
    foundation.identities.artifact_contract_id = canonical_hash(
        "cellscript-artifact-contract-id-v1",
        &(foundation.identities.entry_contract_id.as_str(), &foundation.artifact_contract),
    )
    .unwrap();
    let replacement_contract_node = foundation.entry_contract.semantic_node_id.clone();
    for mapping in &mut fixture.source_map.semantic_mappings {
        if mapping.semantic_node_id == previous_contract_node {
            mapping.semantic_node_id.clone_from(&replacement_contract_node);
        }
    }
    fixture.source_map.canonicalize();
    fixture.metadata["verified_artifact"]["entry_contract_id"] = Value::String(foundation.identities.entry_contract_id.clone());
    fixture.metadata["verified_artifact"]["artifact_contract_id"] = Value::String(foundation.identities.artifact_contract_id.clone());
    fixture.rebind_typed_semantics();

    assert!(fixture.check().is_ok());
}

#[test]
fn stable_rejection_codes_cover_json_budget_graph_abi_proof_and_binding_mutations() {
    let valid = Fixture::new();
    let budgets = CheckerBudgets::default();
    let metadata_bytes = serde_json::to_vec(&valid.metadata).unwrap();
    let record_bytes = canonical_bytes(&valid.record).unwrap();
    let source_map_bytes = canonical_bytes(&valid.source_map).unwrap();

    let mut tiny = budgets.clone();
    tiny.artifact_bytes = 1;
    assert_eq!(
        check_bundle(&valid.artifact, &metadata_bytes, &record_bytes, &source_map_bytes, &tiny).unwrap_err().code,
        CheckerRejectionCode::V2400BudgetExceeded,
    );
    assert_eq!(
        check_bundle(&valid.artifact, &metadata_bytes, b"{", &source_map_bytes, &budgets).unwrap_err().code,
        CheckerRejectionCode::V2401MalformedJson,
    );
    assert_eq!(
        check_bundle(
            &valid.artifact,
            &metadata_bytes,
            &serde_json::to_vec_pretty(&valid.record).unwrap(),
            &source_map_bytes,
            &budgets,
        )
        .unwrap_err()
        .code,
        CheckerRejectionCode::V2402NonCanonicalJson,
    );

    let mut changed = valid.clone();
    changed.record.schema = "future-schema".to_string();
    assert_code(&changed, CheckerRejectionCode::V2403UnsupportedSchema);

    let mut changed = valid.clone();
    changed.record.entries[0].id = "zz-noncanonical".to_string();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2404CanonicalOrder);

    let mut changed = valid.clone();
    changed.record.entries[0].entry_block = "missing:block".to_string();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2405ReferentialIntegrity);

    let mut changed = valid.clone();
    let index = changed
        .record
        .blocks
        .iter()
        .position(|block| changed.record.edges.iter().any(|edge| edge.from == block.id && edge.kind != EdgeKind::Call))
        .expect("fixture must contain a non-return CFG edge");
    changed.record.blocks[index].terminator = cellscript_artifact_checker::MachineTerminator::Return;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2406CfgInvalid);

    let mut changed = valid.clone();
    changed.record.runtime_error_exits.push(cellscript_artifact_checker::RuntimeErrorExit {
        block_id: changed.record.blocks[0].id.clone(),
        address: changed.record.blocks[0].range.end,
        code: 5,
        name: "assertion-failed".to_string(),
    });
    changed.record.canonicalize();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2406CfgInvalid);

    let mut changed = valid.clone();
    changed.record.blocks[0].reachable = !changed.record.blocks[0].reachable;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2406CfgInvalid);

    let mut changed = valid.clone();
    let param = changed.record.entries.iter_mut().find_map(|entry| entry.params.first_mut()).unwrap();
    param.alignment_bytes = 3;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);

    let mut changed = valid.clone();
    let framed_entry =
        changed.record.entries.iter().position(|entry| entry.frame_size_bytes > 0).expect("fixture must contain a stack-framed entry");
    let owner = changed.record.entries[framed_entry].id.clone();
    changed.record.entries[framed_entry].frame_size_bytes = 0;
    changed.record.entries[framed_entry].outgoing_argument_bytes = 0;
    for block in changed.record.blocks.iter_mut().filter(|block| block.owner_entry == owner) {
        block.frame_size_bytes = 0;
        block.outgoing_argument_bytes = 0;
        block.stack_slots.clear();
    }
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);

    let mut changed = valid.clone();
    changed.record.entries[0].proof_ids.push("zz-missing-proof".to_string());
    changed.record.entries[0].proof_ids.sort();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2408ProofCoverageInvalid);

    let mut changed = valid.clone();
    changed.artifact[0] ^= 1;
    assert_code(&changed, CheckerRejectionCode::V2409ArtifactIdentityMismatch);

    let mut changed = valid.clone();
    changed.metadata["module"] = Value::String("tampered".to_string());
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].effect = "tampered".to_string();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let typed_param = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .find_map(|entry| entry.params.first_mut())
        .expect("fixture must contain a typed parameter");
    typed_param.ty = "u128".to_string();
    let binding_id = typed_param.binding_id;
    let typed_local = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| entry.locals.iter_mut())
        .find(|local| local.id == binding_id)
        .expect("typed parameter must bind a local");
    typed_local.ty = "u128".to_string();
    changed.record.typed_semantics_hash =
        canonical_hash(cellscript_artifact_checker::TYPED_SEMANTICS_SCHEMA, &changed.record.typed_semantics).unwrap();
    changed.metadata["typed_semantics"] = serde_json::to_value(&changed.record.typed_semantics).unwrap();
    changed.metadata["typed_semantics_hash"] = Value::String(changed.record.typed_semantics_hash.clone());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    if let Some(interval) = changed.source_map.intervals.first_mut() {
        interval.source_path = "../escape.cell".to_string();
    } else {
        changed.source_map.schema = "bad-map".to_string();
    }
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2416SourceMapInvalid);

    let mut changed = valid.clone();
    if let Some(site) = changed.record.syscall_sites.first_mut() {
        site.contract.clear();
    } else {
        changed.record.syscall_sites.push(cellscript_artifact_checker::SyscallSite {
            block_id: changed.record.blocks[0].id.clone(),
            address: changed.record.blocks[0].range.start,
            syscall_number: None,
            contract: "declared-but-not-present".to_string(),
            source_domain: "test".to_string(),
            index_domain: "test".to_string(),
            return_code_checked: true,
            buffer_limit_bytes: 1,
        });
    }
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2417SyscallContractInvalid);

    let mut changed = valid.clone();
    let distinct = changed
        .record
        .entries
        .iter()
        .enumerate()
        .find_map(|(left, a)| changed.record.entries.iter().enumerate().find(|(_, b)| b.id != a.id).map(|(right, _)| (left, right)))
        .unwrap();
    let left = changed.record.entries[distinct.0].entry_block.clone();
    let right = changed.record.entries[distinct.1].entry_block.clone();
    changed.record.edges.push(cellscript_artifact_checker::LoweringEdge {
        from: left.clone(),
        to: right.clone(),
        kind: EdgeKind::Call,
    });
    changed.record.edges.push(cellscript_artifact_checker::LoweringEdge { from: right, to: left, kind: EdgeKind::Call });
    changed.record.canonicalize();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2418RecursionPolicyInvalid);
}

#[test]
fn stable_rejection_codes_cover_elf_sections_instructions_flow_and_digests() {
    let valid = Fixture::new();

    let mut changed = valid.clone();
    changed.artifact[0] = 0;
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2411ElfFormatInvalid);

    let mut changed = valid.clone();
    let section_table = u64::from_le_bytes(changed.artifact[40..48].try_into().unwrap()) as usize;
    let rodata_type = section_table + 2 * 64 + 4;
    changed.artifact[rodata_type..rodata_type + 4].copy_from_slice(&6_u32.to_le_bytes());
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2412ElfSectionInvalid);

    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let text_offset = elf.text.offset as usize;

    let mut changed = valid.clone();
    changed.artifact[text_offset..text_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2413InstructionInvalid);

    let mut changed = valid.clone();
    changed.artifact[text_offset..text_offset + 4].copy_from_slice(&encode_jal(1_048_574).to_le_bytes());
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);

    let mut changed = valid.clone();
    let candidate = elf
        .instructions
        .windows(2)
        .find(|pair| {
            let word = pair[0].word;
            let rd = (word >> 7) & 0x1f;
            let next = pair[1].word;
            let next_uses_rd_for_sp =
                next & 0x7f == 0x33 && (next >> 7) & 0x1f == 2 && (next >> 15) & 0x1f == 2 && (next >> 20) & 0x1f == rd;
            valid.record.text_range.contains(pair[0].address)
                && word & 0x7f == 0x13
                && rd != 2
                && !next_uses_rd_for_sp
                && valid
                    .record
                    .blocks
                    .iter()
                    .any(|block| block.range.contains(pair[0].address) && pair[0].address + 4 < block.range.end)
        })
        .map(|pair| pair[0])
        .expect("fixture must contain a non-terminating add-immediate instruction");
    let block_offset = elf.text.offset as usize + (candidate.address - elf.text.address) as usize;
    changed.artifact[block_offset..block_offset + 4].copy_from_slice(&(candidate.word ^ (1 << 20)).to_le_bytes());
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2415BlockDigestMismatch);
}

#[test]
fn named_data_shapes_retain_independent_codec_bounds_and_source_checks() {
    let unused = Fixture::from_source("module unused_nested_schema\nresource Batch has store { entries: NestedValues<Vec<u8>, 3, 8, 128> }\naction inspect() -> u64 { verification return 0 }");
    assert!(
        unused.record.blocks.iter().all(|block| block.nested_cell_labels.is_empty() && block.nested_read_labels.is_empty()),
        "a declaration alone must not imply runtime nested decoding"
    );
    for source in [
        NESTED_CELL_CODEC_SOURCE,
        "module unused_nested_field\nresource Batch has store, consume { entries: NestedValues<Vec<u8>, 2, 3, 32> }\naction remove(input before: Batch) { verification consume before }",
    ] {
        assert_bounded_admitted(&Fixture::from_source(source));
    }
    let source = include_str!("syntax_combo/seeds/named-data-shapes.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition: cellscript::NEXT_EDITION,
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        for old in ["NestedValues<Vec<u8>, 3, 8, 128>", "NestedValues<Entry, 3, 8, 128>"] {
            for new in [old.replace(", 3,", ", 2,"), old.replace(", 8,", ", 7,"), old.replace("128", "127")] {
                let mut changed = valid.clone();
                let mut parameters_changed = 0;
                for entry in &mut changed.record.typed_semantics.entries {
                    for param in &mut entry.params {
                        if param.ty == old {
                            param.ty = new.clone();
                            parameters_changed += 1;
                        }
                    }
                    for local in &mut entry.locals {
                        if local.ty == old {
                            local.ty = new.clone();
                        }
                    }
                    for operation in entry.blocks.iter_mut().flat_map(|block| &mut block.operations) {
                        for operand in &mut operation.operands {
                            if operand.ty == old {
                                operand.ty = new.clone();
                            }
                        }
                    }
                }
                assert!(parameters_changed > 0, "the resolved parameter shape must be present: {old}");
                changed.rebind_typed_semantics();
                assert!(changed.check().is_err(), "accepted changed resolved shape {new}");
            }
        }
        let start = valid
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_codec_")))
            .unwrap()
            .range
            .start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let word = elf.instructions.iter().find(|instruction| instruction.address == start).unwrap().word;
        let mut changed = valid.clone();
        changed.replace_machine_word(start, word ^ (1 << 20));
        assert!(changed.check().is_err(), "named shape must not bypass the codec instruction proof");
        assert_bounded_admitted(&valid);
    }
}

#[test]
fn nested_value_helpers_bind_validated_spans_through_calls() {
    let source = include_str!("syntax_combo/seeds/named-nested-helpers.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition: cellscript::NEXT_EDITION,
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
    }
}

#[test]
fn nested_derived_helpers_bind_validated_elements_through_calls() {
    let source = include_str!("syntax_combo/seeds/named-derived-helpers.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition: cellscript::NEXT_EDITION,
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
    }
}

#[test]
fn nested_value_helpers_bind_cell_fields_alongside_borrowed_cells() {
    let shape = "NestedValues<Vec<u8>, 32, 256, 4096>";
    for padding in [0, 5, 7, 9] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let args = "0, ".repeat(padding);
        let source = format!(
            "module mixed_nested_helpers\nresource Document has store, consume {{ entries: {shape} }}\n#[effect(ReadOnly)]\nfn count({params}values: {shape}, document: &Document) -> u64 {{ let alias = values\nreturn alias.len() + document.entries.len() }}\n#[effect(ReadOnly)]\nfn forward(document: &Document) -> u64 {{ let values = document.entries\nreturn count({args}values, document) }}\naction inspect(input before: Document) {{ verification\nlet values = before.entries\nrequire count({args}values, &before) == 2\nrequire forward(&before) == 2\nconsume before }}"
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
        }
    }
}

#[test]
fn nested_value_helpers_reject_rebound_sources_spills_and_paired_arguments() {
    for (shape, parameter, read, argument, declaration) in [
        ("NestedValues<Vec<u8>, 2, 3, 32>", "NestedValues<Vec<u8>, 2, 3, 32>", "values[0][0]", "", ""),
        ("NestedValues<Vec<u8>, 2, 3, 32>", "NestedBytesView<3>", "values[0]", "[0]", ""),
        (
            "NestedValues<Record, 2, 3, 64>",
            "NestedRecordView<Record, 3>",
            "values.id",
            "[0]",
            "struct Record { id: u8, payload: Vec<u8> }",
        ),
    ] {
        for padding in [0, 7, 8] {
            let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
            let args = "0, ".repeat(padding);
            let source = format!("module nested_helper_binding\n{declaration}\nfn first({params}values: {parameter}) -> u64 {{ return {read} as u64 }}\nfn forward(a: {shape}, b: {shape}) -> u64 {{ let alias = a\nlet left_value = alias{argument}\nlet right_value = b{argument}\nlet left = first({args}left_value)\nlet right = first({args}right_value)\nreturn left + right }}\naction inspect(witness a: {shape}, witness b: {shape}, witness expected: u64) {{ verification require forward(a, b) == expected }}");
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                        .unwrap(),
                );
                let forward = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("forward")).unwrap();
                let mut changed = valid.clone();
                let mutated = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.id == forward.id).unwrap();
                let calls = mutated
                    .blocks
                    .iter_mut()
                    .flat_map(|block| &mut block.operations)
                    .filter(|op| op.call.as_ref().is_some_and(|call| call.target.ends_with("first")))
                    .collect::<Vec<_>>();
                let [first, second]: [_; 2] = calls.try_into().unwrap();
                assert_ne!(first.operands[padding].local, second.operands[padding].local);
                first.operands[padding] = second.operands[padding].clone();
                changed.rebind_typed_semantics();
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let helper = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("first")).unwrap();
                let parameter = helper.locals.iter().find(|local| local.id == helper.params[padding].binding_id).unwrap();
                let slot = parameter.source_id as u32 * 8;
                let entry = valid.record.entries.iter().find(|entry| entry.id == helper.id).unwrap();
                let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
                let body = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| block.owner_entry == helper.id && block.lowering_block_id.is_some())
                    .map(|block| block.range.start)
                    .min()
                    .unwrap();
                let spill = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        let word = instruction.word;
                        let offset = (word >> 7 & 31) | (word >> 25 << 5);
                        (start..body).contains(&instruction.address)
                            && word & 0x707f == 0x3023
                            && word >> 15 & 31 == 2
                            && offset == slot
                    })
                    .unwrap();
                let mut changed = valid.clone();
                changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                // Widening a callee's element payload bound must fail even
                // though its callers also validate their complete values.
                let bound = elf
                    .instructions
                    .iter()
                    .find(|instruction| (start..body).contains(&instruction.address) && instruction.word == (3 << 20 | 28 << 7 | 0x13))
                    .unwrap();
                let mut changed = valid.clone();
                changed.replace_machine_word(bound.address, 4 << 20 | 28 << 7 | 0x13);
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                if padding == 0 {
                    let calls = elf
                        .control_flow
                        .iter()
                        .filter(|flow| {
                            flow.target == start
                                && valid
                                    .record
                                    .blocks
                                    .iter()
                                    .any(|block| block.owner_entry == forward.id && block.range.contains(flow.address))
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(calls.len(), 2);
                    let mut calls = calls;
                    calls.sort_by_key(|flow| flow.address);
                    let forward_start = valid
                        .record
                        .blocks
                        .iter()
                        .filter(|block| block.owner_entry == forward.id && block.lowering_block_id.is_some())
                        .map(|block| block.range.start)
                        .min()
                        .unwrap();
                    let loads = |from, until, register| {
                        elf.instructions
                            .iter()
                            .rev()
                            .find(|instruction| {
                                let word = instruction.word;
                                (from..until).contains(&instruction.address)
                                    && word & 0x707f == 0x3003
                                    && word >> 7 & 31 == register
                                    && word >> 15 & 31 == 2
                            })
                            .unwrap()
                    };
                    // Substitute both words from another completely valid value of
                    // identical type and bounds. Revalidating the callee's codec
                    // alone cannot detect that its caller selected the wrong root.
                    let mut changed = valid.clone();
                    for register in [10, 11] {
                        let first = loads(forward_start, calls[0].address, register);
                        let second = loads(calls[0].address + 4, calls[1].address, register);
                        assert_ne!(first.word >> 20, second.word >> 20);
                        changed.replace_machine_word(first.address, (first.word & 0x000f_ffff) | (second.word & 0xfff0_0000));
                    }
                    let error = check_bundle_values(
                        &changed.artifact,
                        &changed.metadata,
                        &changed.record,
                        &changed.source_map,
                        &CheckerBudgets::default(),
                    )
                    .unwrap_err();
                    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    assert!(error.to_string().contains("pointer/length do not match"), "{error}");
                }
            }
        }
    }
}

#[test]
fn nested_witness_decoder_rejects_rehashed_instruction_and_marker_changes() {
    for (element, declarations) in [
        ("Vec<u8>", String::new()),
        (
            "Record",
            format!(
                "struct Record {{ {} payload: Vec<u8> }}",
                (0..8).map(|index| format!("id{index}: [u8; 256], ")).collect::<String>()
            ),
        ),
    ] {
        let source = format!("module nested_machine\n{declarations}\naction inspect(witness values: NestedValues<{element}, 32, 256, 4096>) -> u64 {{ verification return 0 }}");
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            let start = valid
                .record
                .blocks
                .iter()
                .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_codec_")))
                .unwrap();
            let end = valid
                .record
                .blocks
                .iter()
                .filter(|block| {
                    block.range.start > start.range.start
                        && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_ready_"))
                })
                .map(|block| block.range.start)
                .min()
                .unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let mut count = 0;
            for instruction in &elf.instructions {
                if !(start.range.start..end).contains(&instruction.address) || instruction.word & 0x7f == 0x6f {
                    continue;
                }
                let mut changed = valid.clone();
                let word = instruction.word ^ if instruction.word & 0x7f == 0x63 { 1 << 12 } else { 1 << 20 };
                changed.replace_machine_word(instruction.address, word);
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                count += 1;
            }
            assert!(count >= 80, "complete decoder mutation coverage for {element} {diagnostics}");
            let mut changed = valid.clone();
            changed.record.blocks.iter_mut().find(|block| block.id == start.id).unwrap().machine_label = None;
            changed.rebind_sidecars();
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn nested_witness_machine_contract_covers_zero_bounds_and_multiple_parameters() {
    let source = "module nested_multiple\naction inspect(witness empty: NestedValues<Vec<u8>, 0, 0, 4>, witness nonempty: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 { verification return 0 }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
    }
}

#[test]
fn nested_witness_inputs_bind_register_stack_and_mixed_width_arguments() {
    let prefixes = [
        String::new(),
        (0..7).map(|index| format!("witness p{index}: u64, ")).collect(),
        (0..9).map(|index| format!("witness p{index}: u64, ")).collect(),
        "witness hash: Hash, witness wide: u128, witness tag: [u8; 17], witness flag: u64, ".into(),
        "witness flag: bool, witness delta: i32, witness empty: (), witness small: [u8; 3], ".into(),
    ];
    for prefix in prefixes {
        let source = format!("module nested_abi\naction inspect({prefix}witness first: NestedValues<Vec<u8>, 2, 3, 32>, witness second: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 {{ verification return 0 }}");
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let decoders = valid
                .record
                .blocks
                .iter()
                .filter(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_codec_")))
                .collect::<Vec<_>>();
            assert_eq!(decoders.len(), 2);
            for decoder in &decoders {
                // The two final reloads supply the pointer and length. Changing
                // either to an adjacent initialized slot keeps the CFG and all
                // hashes valid but changes which incoming ABI word is checked.
                for distance in [4, 8] {
                    let at = decoder.range.start - distance;
                    let instruction = elf.instructions.iter().find(|instruction| instruction.address == at).unwrap();
                    assert_eq!(instruction.word & 0x707f, 0x3003);
                    let mut changed = valid.clone();
                    changed.replace_machine_word(at, replace_i_immediate(instruction.word, ((instruction.word as i32) >> 20) + 8));
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
            // Equal codec bounds do not make two parameter identities fungible.
            let mut changed = valid.clone();
            let left = decoders[0].machine_label.clone();
            let right = decoders[1].machine_label.clone();
            changed.record.blocks.iter_mut().find(|block| block.id == decoders[0].id).unwrap().machine_label = right;
            changed.record.blocks.iter_mut().find(|block| block.id == decoders[1].id).unwrap().machine_label = left;
            changed.rebind_sidecars();
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

            // Redirect the real caller past the complete decoder prefix, and
            // update both its machine target and declared CFG edge together.
            let entry = valid.record.entries.iter().find(|entry| entry.id == "action:inspect").unwrap();
            let entry_block = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap();
            let caller = elf.control_flow.iter().find(|flow| flow.target == entry_block.range.start).unwrap();
            let caller_block = valid.record.blocks.iter().find(|block| block.range.contains(caller.address)).unwrap();
            let ready = valid
                .record
                .blocks
                .iter()
                .filter(|block| {
                    block.owner_entry == entry.id
                        && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_ready_"))
                })
                .max_by_key(|block| block.range.start)
                .unwrap();
            let old_word = elf.instructions.iter().find(|instruction| instruction.address == caller.address).unwrap().word;
            let mut changed = valid.clone();
            for edge in &mut changed.record.edges {
                if edge.from == caller_block.id && edge.to == entry_block.id {
                    edge.to = ready.id.clone();
                }
            }
            changed.record.canonicalize();
            if old_word & 0x7f == 0x6f {
                let jump = encode_jal((ready.range.start as i64 - caller.address as i64) as i32) | (old_word & (31 << 7));
                changed.replace_machine_word(caller.address, jump);
            } else {
                assert_eq!(old_word & 0x7f, 0x67);
                let high_address = caller.address - 4;
                let high_word = elf.instructions.iter().find(|instruction| instruction.address == high_address).unwrap().word;
                assert_eq!(high_word & 0x7f, 0x17);
                let offset = ready.range.start as i64 - high_address as i64;
                let high = (offset + 0x800) >> 12;
                let low = offset - (high << 12);
                changed.replace_machine_words(&[
                    (high_address, (high_word & 0xfff) | ((high as u32) << 12)),
                    (caller.address, replace_i_immediate(old_word, low as i32)),
                ]);
            }
            // The general ABI boundary already rejects a call into an entry's
            // interior before the feature-specific prefix checker runs.
            assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);
        }
    }
}

#[test]
fn nested_witness_transport_rejects_rehashed_envelope_and_argument_mutations() {
    let source = "module nested_transport\naction inspect(witness salt: u64, witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness tail: Hash) -> u64 { verification return 0 }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let wrapper = valid.record.entries.iter().find(|entry| entry.name == "_cellscript_entry").unwrap();
        let blocks = valid.record.blocks.iter().filter(|block| block.owner_entry == wrapper.id).collect::<Vec<_>>();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !blocks.iter().any(|block| block.range.contains(instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let opcode = word & 0x7f;
            let rd = (word >> 7) & 31;
            // Invert each conditional without changing its two CFG successors;
            // shift each byte read; change each register argument's origin.
            let changed_word = if opcode == 0x63 {
                word ^ (1 << 12)
            } else if word & 0x707f == 0x4003 {
                replace_i_immediate(word, ((word as i32) >> 20) + 1)
            } else if word & 0x707f == 0x13 && (10..=17).contains(&rd) && ((word >> 15) & 31) != 0 {
                word ^ (1 << 15)
            } else {
                continue;
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, changed_word);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            mutations += 1;
        }
        assert!(mutations >= 60, "complete envelope and positional source checks at {diagnostics}");
    }
}

#[test]
fn nested_mixed_script_args_transport_binds_disjoint_buffers_and_live_cursors() {
    let source = "module mixed_transport\nlock inspect(lock_args domain: u64, witness a: u64, witness b: u64, witness c: u64, witness d: u64, witness e: u64, witness f: u64, witness values: NestedValues<Vec<u8>, 2, 3, 32>, lock_args owner: Hash, lock_args nonce: u16, lock_args delta: i32) -> bool { verification true }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let wrapper = valid.record.entries.iter().find(|entry| entry.name == "_cellscript_entry").unwrap();
        let blocks = valid.record.blocks.iter().filter(|block| block.owner_entry == wrapper.id).collect::<Vec<_>>();
        let fail = blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_witness_fail_")))
            .unwrap()
            .range
            .start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if instruction.address >= fail || !blocks.iter().any(|block| block.range.contains(instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let kind = word & 0x707f;
            let rd = (word >> 7) & 31;
            // Disturb Script buffer/size/cursor constants, real byte reads,
            // cursor guards and outgoing argument sources after rebinding.
            let changed_word = if word & 0x7f == 0x63 {
                word ^ (1 << 12)
            } else if kind == 0x4003 || (kind == 0x13 && (5..=31).contains(&rd) && ((word as i32) >> 20) >= 8) {
                replace_i_immediate(word, ((word as i32) >> 20) + 1)
            } else if kind == 0x3023 && decode_s_immediate(word) < 0 {
                word ^ (1 << 20)
            } else {
                continue;
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, changed_word);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            mutations += 1;
        }
        assert!(mutations >= 100, "mixed transport mutations at {diagnostics}");
        // Rewrite all uses of one address-helper scratch register together.
        // Its address calculation remains internally consistent, but t3 still
        // owns the Script total length and must survive the args-offset read.
        let header = blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_script_table_header_ok_")))
            .unwrap();
        let helper = header.range.start + 4;
        let words = (0..4)
            .map(|index| elf.instructions.iter().find(|instruction| instruction.address == helper + index * 4).unwrap().word)
            .collect::<Vec<_>>();
        assert_eq!(words[0] & 0x7f, 0x37);
        assert_eq!(words[1] & 0x707f, 0x13);
        assert_eq!(words[2] & 0x7f, 0x33);
        assert_eq!(words[3] & 0x707f, 0x4003);
        let mut changed = valid.clone();
        changed.replace_machine_words(&[
            (helper, (words[0] & !(31 << 7)) | (28 << 7)),
            (helper + 4, (words[1] & !((31 << 7) | (31 << 15))) | (28 << 7) | (28 << 15)),
            (helper + 8, (words[2] & !((31 << 7) | (31 << 20))) | (28 << 7) | (28 << 20)),
            (helper + 12, (words[3] & !(31 << 15)) | (28 << 15)),
        ]);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn nested_view_type_rules_reject_rehashed_mutation_and_pointer_operations() {
    let valid = Fixture::from_source(
        r#"
module checked_nested_views
struct Record { id: u64, payload: Vec<u8> }
action inspect(witness values: NestedValues<Record, 3, 8, 128>) -> u64 {
    verification
        require values.len() == 1
        let record = values[0]
        require record.id == 42
        require record.payload.len() == 1
        require record.payload[0] == 7
        return 0
}
"#,
    );
    for opcode in ["collection-clear", "collection-reverse", "collection-capacity", "type-hash", "return"] {
        let mut changed = valid.clone();
        let operation = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|operation| operation.opcode == "length")
            .unwrap();
        operation.opcode = opcode.into();
        if matches!(opcode, "collection-clear" | "collection-reverse" | "return") {
            operation.destinations.clear();
        }
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }

    for replacement in ["NestedRecordView<Record, 9>", "NestedBytesView<8>", "Record"] {
        let mut changed = valid.clone();
        // Rebind every use and declaration of the derived value together so
        // rejection must enforce the owner's declared element/payload shape.
        for entry in &mut changed.record.typed_semantics.entries {
            for local in &mut entry.locals {
                if local.ty == "NestedRecordView<Record, 8>" {
                    local.ty = replacement.into();
                }
            }
            for operation in entry.blocks.iter_mut().flat_map(|block| &mut block.operations) {
                for operand in &mut operation.operands {
                    if operand.ty == "NestedRecordView<Record, 8>" {
                        operand.ty = replacement.into();
                    }
                }
            }
        }
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn typed_semantics_rejects_operator_and_constant_mutations_after_rebinding() {
    let valid = Fixture::new();

    let mut changed = valid.clone();
    let binary = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| operation.opcode == "binary")
        .expect("fixture must contain a binary operation");
    binary.detail = TypedSemanticOperationDetail::BinaryOperator { operator: "and".to_string() };
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let constant = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .flat_map(|operation| &mut operation.operands)
        .find_map(|operand| operand.constant.as_mut())
        .expect("fixture must contain a constant operand");
    *constant = TypedSemanticConstant::U64("01".to_string());
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let constant = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .flat_map(|operation| &mut operation.operands)
        .find_map(|operand| match &mut operand.constant {
            Some(TypedSemanticConstant::U64(value)) => Some(value),
            _ => None,
        })
        .expect("fixture must contain a u64 constant operand");
    *constant = "2".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn typed_semantics_rejects_cross_domain_epoch_since_comparison() {
    let valid = Fixture::from_source(
        r#"
module checker::temporal

action main() -> bool {
    verification
        let left = ckb::since_absolute_epoch(42, 3, 10)
        let right = ckb::since_absolute_epoch(43, 0, 10)
        return left < right
}
"#,
    );
    let binary = valid
        .record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(&operation.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "lt"))
        .expect("fixture must contain the temporal comparison");
    assert!(binary.operands.iter().all(|operand| operand.ty == "AbsoluteEpochSince"));

    let mut changed = valid.clone();
    let binary = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| matches!(&operation.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "lt"))
        .expect("fixture must contain the temporal comparison");
    binary.operands[1].ty = "RelativeEpochSince".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let valid = Fixture::from_source(
        r#"
module checker::temporal_scalar

action main() -> bool {
    verification
        let left = ckb::since_absolute_block(42)
        let right = ckb::since_absolute_block(43)
        return left < right
}
"#,
    );
    let mut changed = valid.clone();
    let binary = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| matches!(&operation.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "lt"))
        .expect("fixture must contain the temporal comparison");
    assert!(binary.operands.iter().all(|operand| operand.ty == "AbsoluteBlockSince"));
    binary.operands[1].ty = "AbsoluteTimestampSince".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let valid = Fixture::from_source(
        r#"
module checker::header_timestamp

action main() -> bool {
    verification
        let header = ckb::header_dep(0)
        return header.timestamp <= header.timestamp
}
"#,
    );
    let mut changed = valid.clone();
    let binary = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| matches!(&operation.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "le"))
        .expect("fixture must contain the timestamp comparison");
    assert!(binary.operands.iter().all(|operand| operand.ty == "TimestampMillis"));
    binary.operands[1].ty = "BlockNumber".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn typed_semantics_accepts_declared_vec_constructors_and_unsigned_widening() {
    let widened = Fixture::from_source(
        r#"
module checker::widening

action multiply(amount: u64, basis_points: u16) -> u64 {
    verification
        return amount * basis_points
}
"#,
    );
    assert!(widened.check().is_ok());

    let empty_array = Fixture::from_source(
        r#"
module checker::empty_array

action empty() -> [u8; 0] {
    verification
        return []
}
"#,
    );
    assert!(empty_array.check().is_ok());

    let script_tuple = Fixture::from_source(
        r#"
module checker::script_tuple

action script_value() -> u64 {
    verification
        let args = script::args_empty()
        let value = script::new(Hash::zero(), 0, args)
        return 0
}
"#,
    );
    assert!(script_tuple.check().is_ok());

    let collection = Fixture::from_source(include_str!("../examples/language/collections/order_book.cell"));
    assert!(collection.check().is_ok());

    let mut changed = collection;
    let declared_type = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match &mut operation.detail {
            TypedSemanticOperationDetail::Collection { declared_type } => Some(declared_type),
            _ => None,
        })
        .expect("fixture must contain a collection constructor");
    *declared_type = "Map".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn bounded_cell_runtime_contract_is_bound_to_typed_and_machine_evidence() {
    let valid = Fixture::from_source(
        r#"
module checker::bounded_cells

resource Token has store, consume { amount: u64 }

action verify(input inputs: BoundedCellSet<Token, 2>) -> u64 {
    verification
        consume_each token in inputs {
            require token.amount > 0
        }
        return 0
}
"#,
    );
    assert!(valid.check().is_ok());

    let mut changed = valid.clone();
    let declared_type = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match (&*operation.opcode, &mut operation.detail) {
            ("bounded-cell-load", TypedSemanticOperationDetail::Collection { declared_type }) => Some(declared_type),
            _ => None,
        })
        .expect("fixture must contain the bounded Cell load contract");
    *declared_type = "BoundedCellSet<Token, 0>".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let (entry_id, typed_block_id) = changed
        .record
        .typed_semantics
        .entries
        .iter()
        .find_map(|entry| {
            entry
                .blocks
                .iter()
                .find(|block| block.operations.iter().any(|operation| operation.opcode == "bounded-cell-load"))
                .map(|block| (entry.id.clone(), block.id))
        })
        .expect("fixture must bind the bounded Cell load to a typed block");
    let binding = changed
        .record
        .entries
        .iter_mut()
        .find(|entry| entry.id == entry_id)
        .and_then(|entry| entry.typed_blocks.iter_mut().find(|binding| binding.id == typed_block_id))
        .expect("fixture must contain a machine binding for the bounded Cell load");
    binding.machine_block_ids.pop().expect("bounded Cell load must map to machine code");
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let machine_block = |prefix: &str| {
        valid
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(prefix)))
            .expect("bounded Cell machine block")
    };
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let word_at = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
    let loaded = machine_block(".Lbounded_cell_loaded_");
    let out_of_bound = machine_block(".Lbounded_cell_out_of_bound_");
    let lock_distinct = machine_block(".Lbounded_cell_lock_is_distinct_");
    let data_site = valid
        .record
        .syscall_sites
        .iter()
        .filter(|site| site.address < loaded.range.start)
        .max_by_key(|site| site.address)
        .expect("bounded data syscall")
        .address;
    let identity_sites = valid
        .record
        .syscall_sites
        .iter()
        .filter(|site| loaded.range.start < site.address && site.address < out_of_bound.range.start)
        .map(|site| site.address)
        .collect::<Vec<_>>();
    assert_eq!(identity_sites.len(), 3);

    let mut changed = valid.clone();
    let count_address = machine_block(".Lbounded_cell_loaded_").range.start + 4;
    changed.replace_machine_word(count_address, word_at(count_address) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let decode_address = machine_block(".Lbounded_cell_count_ok_").range.start + 4;
    changed.replace_machine_word(decode_address, word_at(decode_address) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let identity_address = machine_block(".Lbounded_cell_type_word_ok_").range.start - 12;
    changed.replace_machine_word(identity_address, word_at(identity_address) ^ (3 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let data_index_address = elf
        .instructions
        .iter()
        .rev()
        .find(|instruction| instruction.address < data_site && instruction.word & 0x7f == 0x13 && (instruction.word >> 7) & 0x1f == 13)
        .expect("bounded data syscall a3 definition")
        .address;
    changed.replace_machine_word(data_index_address, encode_addi(13, 0, 7));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut type_word_blocks = valid
        .record
        .blocks
        .iter()
        .filter(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lbounded_cell_type_word_ok_")))
        .collect::<Vec<_>>();
    type_word_blocks.sort_by_key(|block| block.range.start);
    let repeated_type_word_address = type_word_blocks[1].range.start - 20;
    let repeated_type_word = word_at(repeated_type_word_address);
    let mut changed = valid.clone();
    changed.replace_machine_word(
        repeated_type_word_address,
        replace_i_immediate(repeated_type_word, ((repeated_type_word as i32) >> 20) - 8),
    );
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(lock_distinct.range.start - 60, encode_addi(7, 0, 1));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(identity_sites[1] + 16, encode_addi(11, 0, 31));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let success_pointer_address = lock_distinct.range.start;
    let success_pointer = word_at(success_pointer_address);
    let mut changed = valid.clone();
    changed.replace_machine_word(success_pointer_address, replace_i_immediate(success_pointer, ((success_pointer as i32) >> 20) + 8));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let absent_destination_address = out_of_bound.range.start;
    let absent_destination = word_at(absent_destination_address);
    let mut changed = valid.clone();
    changed.replace_machine_word(
        absent_destination_address,
        replace_s_immediate(absent_destination, decode_s_immediate(absent_destination) + 8),
    );
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid;
    let (entry_id, predicate_block_id) = changed
        .record
        .typed_semantics
        .entries
        .iter()
        .find_map(|entry| {
            entry
                .blocks
                .iter()
                .find(|block| {
                    block.operations.iter().any(|operation| {
                        operation.opcode == "binary"
                            && matches!(
                                &operation.detail,
                                TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "gt"
                            )
                    })
                })
                .map(|block| (entry.id.clone(), block.id))
        })
        .expect("fixture must bind its per-element predicate");
    let binding = changed
        .record
        .entries
        .iter_mut()
        .find(|entry| entry.id == entry_id)
        .and_then(|entry| entry.typed_blocks.iter_mut().find(|binding| binding.id == predicate_block_id))
        .expect("predicate must have a machine binding");
    binding.machine_block_ids.clear();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn bounded_output_plan_contract_is_bound_to_typed_and_machine_evidence() {
    let valid = Fixture::from_source(
        r#"
module checker::bounded_outputs

struct Plan { owner: Address, amount: u64 }
resource Token has store, create
with_capacity_floor(10000000000)
{ amount: u64 }

action verify(witness plans: BoundedList<Plan, 2>) -> u64 {
    verification
        create_each plan in plans {
            require plan.amount > 0
            create Token { amount: plan.amount } with_lock(plan.owner)
        }
        return 0
}
"#,
    );
    assert!(valid.check().is_ok());

    let mut changed = valid.clone();
    changed.record.compatibility_profile.metadata_schema_version = 71;
    changed.record.compatibility_profile.id = changed.record.compatibility_profile.id.replace("metadata-72-", "metadata-71-");
    changed.record.compatibility_profile_hash =
        canonical_hash("cellscript-compatibility-profile-identity-v1", &changed.record.compatibility_profile).unwrap();
    changed.metadata["metadata_schema_version"] = Value::from(71);
    changed.metadata["compatibility_profile"] = serde_json::to_value(&changed.record.compatibility_profile).unwrap();
    changed.metadata["public_interface"]["runtime_contract"]["compatibility_profile_id"] =
        Value::String(changed.record.compatibility_profile.id.clone());
    changed.metadata["constraints"]["compatibility_profile"] = Value::String(changed.record.compatibility_profile.id.clone());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    let declared_type = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match (&*operation.opcode, &mut operation.detail) {
            ("bounded-plan-load", TypedSemanticOperationDetail::Collection { declared_type }) => Some(declared_type),
            _ => None,
        })
        .expect("fixture must contain the bounded output plan decoder");
    *declared_type = "BoundedList<Plan, 0>".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let (entry_id, typed_block_id) = changed
        .record
        .typed_semantics
        .entries
        .iter()
        .find_map(|entry| {
            entry
                .blocks
                .iter()
                .find(|block| block.operations.iter().any(|operation| operation.opcode == "bounded-output-verify"))
                .map(|block| (entry.id.clone(), block.id))
        })
        .expect("fixture must bind output verification to a typed block");
    let binding = changed
        .record
        .entries
        .iter_mut()
        .find(|entry| entry.id == entry_id)
        .and_then(|entry| entry.typed_blocks.iter_mut().find(|binding| binding.id == typed_block_id))
        .expect("fixture must contain a machine binding for bounded output verification");
    binding.machine_block_ids.clear();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    for (pointer, replacement) in [
        ("/runtime/collection_instantiations/0/bounded_output_plan/ordering", Value::String("reverse-plan-order".to_string())),
        ("/runtime/collection_instantiations/0/bounded_output_plan/identity_policy", Value::String("content-hash".to_string())),
        ("/runtime/collection_instantiations/0/bounded_output_plan/field_bindings/0/plan_offset_bytes", Value::from(0)),
    ] {
        let mut changed = valid.clone();
        *changed.metadata.pointer_mut(pointer).expect("bounded output contract field") = replacement;
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }

    let machine_block = |prefix: &str| {
        valid
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(prefix)))
            .expect("bounded output machine block")
    };
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let word_at = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
    let exact = machine_block(".Lbounded_output_count_exact_");
    let type_only = machine_block(".Lbounded_output_type_only_");
    let capacity_ok = machine_block(".Lbounded_output_capacity_ok_");
    let output_sites = valid
        .record
        .syscall_sites
        .iter()
        .filter(|site| {
            valid
                .record
                .blocks
                .iter()
                .find(|block| block.id == site.block_id)
                .is_some_and(|block| block.owner_entry == "action:verify" && exact.range.end <= site.address)
        })
        .collect::<Vec<_>>();
    assert_eq!(output_sites.len(), 4);

    let mut changed = valid.clone();
    let first_magic = machine_block(".Lbounded_plan_magic_ok_").range.start;
    changed.replace_machine_word(first_magic - 16, word_at(first_magic - 16) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(exact.range.start - 8, word_at(exact.range.start - 8) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let data_site = output_sites[0].address;
    let data_index_move = elf
        .instructions
        .iter()
        .filter(|instruction| instruction.address < data_site)
        .rev()
        .find(|instruction| {
            let word = instruction.word;
            word & 0x7f == 0x13 && (word >> 7) & 0x1f == 13 && (word >> 15) & 0x1f == 28
        })
        .expect("GroupOutput data ordinal move")
        .address;
    changed.replace_machine_word(data_index_move, word_at(data_index_move) ^ (1 << 15));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(data_site + 16, word_at(data_site + 16) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(type_only.range.start + 12, word_at(type_only.range.start + 12) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(type_only.range.start - 8, word_at(type_only.range.start - 8) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(capacity_ok.range.start - 8, word_at(capacity_ok.range.start - 8) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn typed_semantics_requires_reference_coercion_for_reference_calls() {
    let valid = Fixture::from_source(
        r#"
module checker::reference_call

struct Wallet {
    amount: u64,
}

fn inspect(wallet: &Wallet) -> u64 {
    return wallet.amount
}

action main(wallet: Wallet) -> u64 {
    verification
        return inspect(&wallet)
}
"#,
    );
    assert!(valid.check().is_ok());

    let mut changed = valid;
    let operator = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match &mut operation.detail {
            TypedSemanticOperationDetail::UnaryOperator { operator } if operator == "ref" => Some(operator),
            _ => None,
        })
        .expect("fixture must contain a reference coercion");
    *operator = "deref".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn typed_semantics_requires_exact_guard_for_unsigned_narrowing() {
    let valid = Fixture::from_source(
        r#"
module checker::narrowing

action narrow(value: u64) -> u8 {
    verification
        return value as u8
}
"#,
    );
    assert!(valid.check().is_ok());

    let mut changed = valid;
    let bound = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .flat_map(|operation| &mut operation.operands)
        .find_map(|operand| match &mut operand.constant {
            Some(TypedSemanticConstant::U64(value)) if value == "255" => Some(value),
            _ => None,
        })
        .expect("fixture must contain the u8 narrowing bound");
    *bound = "256".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn typed_semantics_rejects_field_enum_layout_and_instantiation_mutations() {
    let valid = Fixture::from_source(include_str!("syntax_combo/seeds/generic-value.cell"));

    let mut changed = valid.clone();
    let field = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match &mut operation.detail {
            TypedSemanticOperationDetail::Field { name } => Some(name),
            _ => None,
        })
        .expect("fixture must contain a field access");
    *field = "missing".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let variant = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match &mut operation.detail {
            TypedSemanticOperationDetail::EnumConstruct { variant, .. } => Some(variant),
            _ => None,
        })
        .expect("fixture must contain an enum constructor");
    *variant = "Missing".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let field_index = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match &mut operation.detail {
            TypedSemanticOperationDetail::EnumPayload { field_index, .. } => Some(field_index),
            _ => None,
        })
        .expect("fixture must contain an enum payload read");
    *field_index = u32::MAX;
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    changed.record.typed_semantics.types[0].layout_hash = "00".repeat(32);
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    changed.record.typed_semantics.instantiations[0].identity.push_str("::tampered");
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn typed_semantics_rejects_borrow_ownership_cfg_and_reference_mutations() {
    let valid = Fixture::from_source(include_str!("syntax_combo/seeds/explicit-borrow.cell"));

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].borrows[0].start_operation = u32::MAX;
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].ownership[0].final_state = "available".to_string();
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let block = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .find(|block| !block.successors.is_empty())
        .expect("fixture must contain a CFG edge");
    block.successors[0] = u32::MAX;
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    // This mutation inspects an unused helper ABI; retain it in this
    // diagnostic fixture instead of disabling normal dead-code elimination.
    let mut changed = Fixture::from_result(
        compile(
            REFERENCE_ENTRY_SOURCE,
            CompileOptions {
                diagnostics: cellscript::CompilerDiagnostics::default().without(cellscript::OptimizationPass::DeadCode),
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let entry = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .find(|entry| entry.name == "inspect")
        .expect("fixture must contain the reference helper");
    let binding_id = entry.params[0].binding_id;
    entry.params[0].ty = "Token".to_string();
    entry.locals.iter_mut().find(|local| local.id == binding_id).unwrap().ty = "Token".to_string();
    for operand in entry.blocks.iter_mut().flat_map(|block| &mut block.operations).flat_map(|operation| &mut operation.operands) {
        if operand.local == Some(binding_id) {
            operand.ty = "Token".to_string();
        }
    }
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

fn encode_jal(offset: i32) -> u32 {
    let immediate = offset as u32;
    (((immediate >> 20) & 1) << 31)
        | (((immediate >> 1) & 0x03ff) << 21)
        | (((immediate >> 11) & 1) << 20)
        | (((immediate >> 12) & 0x00ff) << 12)
        | 0x6f
}

fn encode_addi(rd: u32, rs1: u32, immediate: i32) -> u32 {
    ((immediate as u32 & 0x0fff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

fn replace_i_immediate(word: u32, immediate: i32) -> u32 {
    (word & 0x000f_ffff) | ((immediate as u32 & 0x0fff) << 20)
}

fn decode_s_immediate(word: u32) -> i32 {
    let immediate = (((word >> 25) & 0x7f) << 5) | ((word >> 7) & 0x1f);
    ((immediate as i32) << 20) >> 20
}

fn replace_s_immediate(word: u32, immediate: i32) -> u32 {
    let immediate = immediate as u32 & 0x0fff;
    (word & !((0x7f << 25) | (0x1f << 7))) | ((immediate >> 5) << 25) | ((immediate & 0x1f) << 7)
}

#[test]
fn nested_policy_transport_binds_each_selected_record_and_outgoing_argument() {
    use cellscript::artifact::{compile_artifact, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    let source = r#"
module nested_policy_transport
resource Token has store, consume { amount: u64 }
action first(input token: Token, witness a: u64, witness b: u64, witness c: u64, witness d: u64, witness e: u64, witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness tail: Hash) {
    verification
    require values.len() == 1
    require token.amount == a
    consume token
}
action second(input token: Token, witness values: NestedValues<Vec<u8>, 3, 5, 64>, witness other: NestedValues<Vec<u8>, 2, 3, 32>, witness nonce: u16) {
    verification
    require values.len() == 2
    require other.len() == 1
    consume token
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile_artifact(
                source,
                CompileOptions {
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    target_profile: Some("ckb".into()),
                    ..Default::default()
                },
                ArtifactDeclaration {
                    name: "nested-policy".into(),
                    context: ArtifactContext::TypeGroup { resource: "Token".into() },
                    dispatch: ArtifactDispatch::PolicyWitnessV1,
                    actions: vec![
                        ArtifactAction { tag: 40, action: "first".into() },
                        ArtifactAction { tag: 10, action: "second".into() },
                    ],
                    common_checks: vec![],
                },
                cellscript::ExecutableSurfacePolicy::AllowFailClosed,
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let adapters =
            valid.record.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_action_adapter_")).collect::<Vec<_>>();
        assert_eq!(adapters.len(), 2);
        for adapter in adapters {
            let blocks = valid.record.blocks.iter().filter(|block| block.owner_entry == adapter.id).collect::<Vec<_>>();
            let label = |prefix: &str| {
                blocks
                    .iter()
                    .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(prefix)))
                    .unwrap()
                    .range
                    .start
            };
            let copied = label(".Lpolicy_args_copied_");
            let fail = label(".Lentry_witness_fail_");
            let mut mutations = 0;
            for instruction in &elf.instructions {
                if instruction.address < copied
                    || instruction.address >= fail
                    || !blocks.iter().any(|block| block.range.contains(instruction.address))
                {
                    continue;
                }
                let word = instruction.word;
                let kind = word & 0x707f;
                let rd = word >> 7 & 31;
                let changed_word = if word & 0x7f == 0x63 {
                    word ^ (1 << 12)
                } else if kind == 0x4003 {
                    replace_i_immediate(word, ((word as i32) >> 20) + 1)
                } else if (kind == 0x13 || kind == 0x33) && (10..=17).contains(&rd) {
                    word ^ (1 << 15)
                } else if kind == 0x3023 && decode_s_immediate(word) < 0 {
                    word ^ (1 << 20)
                } else {
                    continue;
                };
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, changed_word);
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                mutations += 1;
            }
            assert!(mutations >= 35, "selected adapter mutations at {diagnostics}: {mutations}");
        }
    }
}

#[test]
fn nested_read_operations_reject_rebound_ranges_fields_and_aliases() {
    let source = r#"
module nested_read_contract
struct Row { a: u8, b: u16, c: u32, d: u64, big: u128, hash: Hash, pad: [u8; 17], payload: Vec<u8> }
action inspect(witness values: NestedValues<Row, 3, 8, 512>, witness at: u64, witness big: u128, witness hash: Hash, witness pad: [u8; 17]) -> u64 {
    verification
    let row = values[at]
    let alias = row
    require row.a == 1
    require alias.b == 2
    require row.c == 3
    require row.d == 4
    require alias.big == big
    require row.hash == hash
    require alias.pad == pad
    let bytes = alias.payload
    require !bytes.is_empty()
    require bytes[at] == 171
    return values.len()
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        let mut sites = 0;
        for block in &valid.record.blocks {
            for label in block.nested_read_labels.iter().filter(|label| !label.ends_with("_done")) {
                sites += 1;
                let end_name = format!("{label}_done");
                let end = valid.record.blocks.iter().find(|block| block.nested_read_labels.contains(&end_name)).unwrap().range.start;
                for instruction in
                    elf.instructions.iter().filter(|instruction| block.range.start <= instruction.address && instruction.address < end)
                {
                    let word = instruction.word;
                    let changed_word = match word & 0x7f {
                        0x63 => word ^ (1 << 12),
                        0x03 => replace_i_immediate(word, ((word as i32) >> 20) + 1),
                        0x13 | 0x33 => word ^ (1 << 15),
                        0x23 => word ^ (1 << 20),
                        _ => continue,
                    };
                    let mut changed = valid.clone();
                    changed.replace_machine_word(instruction.address, changed_word);
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    mutations += 1;
                }
                let mut missing = valid.clone();
                missing
                    .record
                    .blocks
                    .iter_mut()
                    .find(|candidate| candidate.id == block.id)
                    .unwrap()
                    .nested_read_labels
                    .retain(|candidate| candidate != label);
                missing.rebind_sidecars();
                assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
        assert!(sites >= 12, "nested read coverage at {diagnostics}: {sites}");
        assert!(mutations >= 100, "nested read mutations at {diagnostics}: {mutations}");
    }
}

#[test]
fn nested_read_contract_preserves_large_offsets_and_high_unsigned_indexes() {
    let fields = (0..8).map(|index| format!("f{index}: [u8; 256]")).collect::<Vec<_>>().join(", ");
    let source = format!("module wide_nested_read\nstruct Row {{ {fields}, payload: Vec<u8> }}\naction inspect(witness values: NestedValues<Row, 1, 3, 4096>, witness expected: [u8; 256]) -> u64 {{ verification\nlet row = values[0]\nrequire row.f7 == expected\nlet payload = row.payload\nreturn payload.len() }}");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        for index in [305419896u64, u64::MAX] {
            let source = format!("module high_nested_index\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 {{ verification\nlet item = values[{index}]\nreturn item.len() }}");
            Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
        }
    }
}

#[test]
fn nested_independent_views_cannot_share_a_coherently_rewritten_length_slot() {
    let source = "module nested_view_slots\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 { verification\nlet first = values[0]\nlet second = values[1]\nreturn first.len() + second.len() }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let indexes = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .nested_read_labels
                    .iter()
                    .filter(|label| label.starts_with(".Lnested_use_index_") && !label.ends_with("_done"))
                    .map(move |label| (block, label))
            })
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 2);
        let size_slot = |block: &cellscript_artifact_checker::LoweringBlock, label: &str| {
            let done = format!("{label}_done");
            let end = valid.record.blocks.iter().find(|block| block.nested_read_labels.contains(&done)).unwrap().range.start;
            let store = elf
                .instructions
                .iter()
                .find(|instruction| {
                    block.range.start <= instruction.address
                        && instruction.address < end
                        && instruction.word & 0x707f == 0x3023
                        && instruction.word >> 15 & 31 == 2
                        && instruction.word >> 20 & 31 == 29
                })
                .unwrap();
            decode_s_immediate(store.word)
        };
        let first = size_slot(indexes[0].0, indexes[0].1);
        let second = size_slot(indexes[1].0, indexes[1].1);
        assert_ne!(first, second);
        let owned = valid.record.blocks.iter().filter(|block| block.owner_entry == indexes[0].0.owner_entry).collect::<Vec<_>>();
        let changes = elf
            .instructions
            .iter()
            .filter(|instruction| owned.iter().any(|block| block.range.contains(instruction.address)))
            .filter_map(|instruction| {
                let word = instruction.word;
                if word >> 15 & 31 != 2 {
                    return None;
                }
                if word & 0x707f == 0x3003 && (word as i32) >> 20 == second {
                    Some((instruction.address, replace_i_immediate(word, first)))
                } else if word & 0x707f == 0x3023 && decode_s_immediate(word) == second {
                    let changed = (word & !0xfe000f80) | ((first as u32 & 0xfe0) << 20) | ((first as u32 & 31) << 7);
                    Some((instruction.address, changed))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert!(changes.len() >= 2, "rewrite both producer and consumer");
        let mut changed = valid.clone();
        changed.replace_machine_words(&changes);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn nested_lifetime_rejects_ordinary_full_partial_and_aliased_stack_clobbers() {
    let source = "module nested_lifetime\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness at: u64) -> u64 { verification\nlet bytes = values[at]\nlet marker = at + 7\nrequire marker > at\nreturn bytes.len() + values.len() }";
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
    let source = "module nested_dominance\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness at: u64) -> u64 { verification\nrequire at < 2\nlet bytes = values[at]\nrequire bytes.len() > 0\nreturn bytes.len() }";
    let cell_source =
        NESTED_CELL_CODEC_SOURCE.replace("let values = before.entries", "require before.id == 42\nlet values = before.entries");
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
        let source = format!("module nested_result\nstruct Pair {{ left: u64, right: u64 }}\nfn make({params}) -> Pair {{ Pair {{ left: {value}, right: {value} + 1 }} }}\nfn wrap(at: u64) -> Pair {{ make({args}) }}\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 {{ verification\nlet bytes = values[0]\nlet pair = wrap(bytes.len())\nrequire pair.right > pair.left\nreturn bytes.len() }}");
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
module nested_cell_codec
resource Document has store, consume, create {
    id: u64,
    entries: NestedValues<Vec<u8>, 2, 3, 32>,
}
action roll(input before: Document, witness recipient: Address) -> after: Document {
    verification
    let values = before.entries
    require values.len() == 1
    let id = before.id
    consume before
    create after = Document { id: id, entries: values } with_lock(recipient)
}
"#;

fn nested_cell_two_fields_source() -> String {
    NESTED_CELL_CODEC_SOURCE
        .replace(
            "entries: NestedValues<Vec<u8>, 2, 3, 32>,",
            "entries: NestedValues<Vec<u8>, 2, 3, 32>, spare: NestedValues<Vec<u8>, 2, 3, 32>,",
        )
        .replace("let id = before.id", "let spare = before.spare\nrequire spare.len() == 1\nlet id = before.id")
        .replace("entries: values }", "entries: values, spare: spare }")
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
                .replace("consume before", if alias { "let copy = bytes\nconsume before" } else { "consume before" })
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
        .replace("with_lock(recipient)", "with_lock(recipient)\nreturn 0");
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

#[test]
fn nested_cell_borrow_calls_bind_register_stack_and_forwarded_views() {
    for count in [0, 1, 6, 7, 8, 9] {
        let params = (0..count).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let arguments = "0, ".repeat(count);
        let forwarded = (0..count).map(|i| format!("p{i}, ")).collect::<String>();
        let source = format!("{}\n#[effect(ReadOnly)]\nfn count({params}document: &Document) -> u64 {{ return document.entries.len() }}\n#[effect(ReadOnly)]\nfn wrap({params}document: &Document) -> u64 {{ return count({forwarded}document) }}\n",
            NESTED_CELL_CODEC_SOURCE.replace("require values.len() == 1", &format!("require wrap({arguments}&before) == 1\nrequire count({arguments}&before) == 1")));
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            if count > 0 {
                let mut changed = valid.clone();
                let helper = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "wrap").unwrap();
                let scalar = helper.locals.iter().find(|local| local.id == helper.params[0].binding_id).unwrap().source_id;
                let borrowed = helper.params.last().unwrap().binding_id;
                helper.locals.iter_mut().find(|local| local.id == borrowed).unwrap().source_id = scalar;
                changed.rebind_typed_semantics();
                let error = check_bundle_values(
                    &changed.artifact,
                    &changed.metadata,
                    &changed.record,
                    &changed.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.to_string().contains("borrowed Cell parameter aliases"), "{error}");
            }
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let helpers =
                valid.record.entries.iter().filter(|entry| ["count", "wrap"].contains(&entry.name.as_str())).collect::<Vec<_>>();
            let mut mutations = std::collections::BTreeSet::new();
            for helper in helpers {
                let start = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
                for flow in elf.control_flow.iter().filter(|flow| flow.target == start) {
                    let owner =
                        valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap().owner_entry.as_str();
                    // The latest ABI-register definitions include optimized
                    // register moves. Stack arguments share t0, so inspect both.
                    for register in if count < 7 {
                        vec![10 + count, 11 + count]
                    } else if count == 7 {
                        vec![17, 5]
                    } else {
                        vec![5]
                    } {
                        let loads = elf
                            .instructions
                            .iter()
                            .rev()
                            .filter(|i| {
                                i.address < flow.address
                                    && matches!(i.word & 0x7f, 0x03 | 0x13 | 0x33 | 0x37 | 0x1b | 0x3b)
                                    && i.word >> 7 & 31 == register as u32
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == owner && block.range.contains(i.address))
                            })
                            .take(if register == 5 && count >= 8 { 2 } else { 1 })
                            .collect::<Vec<_>>();
                        assert!(!loads.is_empty(), "caller {diagnostics}, {count} args");
                        for load in loads {
                            mutations.insert((load.address, ((register as u32) << 7) | 0x13));
                        }
                    }
                }
                let first_use = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| {
                        block.owner_entry == helper.id && (!block.nested_read_labels.is_empty() || block.lowering_block_id.is_some())
                    })
                    .map(|block| block.range.start)
                    .filter(|address| *address > start)
                    .min()
                    .unwrap();
                for i in elf.instructions.iter().filter(|i| i.address >= start && i.address < first_use) {
                    let word = i.word;
                    let register = word >> 20 & 31;
                    if word & 0x707f == 0x3023
                        && ((count < 8 && register == 10 + count as u32) || (count < 7 && register == 11 + count as u32))
                    {
                        mutations.insert((i.address, word & !(31 << 20)));
                    } else if word & 0x707f == 0x3003
                        && word >> 15 & 31 == 8
                        && [count, count + 1]
                            .into_iter()
                            .filter(|index| *index >= 8)
                            .any(|index| ((word as i32) >> 20) == ((index - 8) * 8) as i32)
                    {
                        mutations.insert((i.address, word ^ (8 << 20)));
                    }
                }
            }
            assert!(mutations.len() >= 6, "{diagnostics}, {count} args: {}", mutations.len());
            for (address, word) in mutations {
                let mut changed = valid.clone();
                changed.replace_machine_word(address, word);
                let error = check_bundle_values(
                    &changed.artifact,
                    &changed.metadata,
                    &changed.record,
                    &changed.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(
                    error.code,
                    CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                    "{diagnostics}, {count} args, {address:#x}: {error}"
                );
            }
        }
    }
}

#[test]
fn nested_cell_lifetime_rejects_buffer_pointer_and_size_clobbers() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                NESTED_CELL_CODEC_SOURCE,
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let projection = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.nested_read_labels.iter().any(|label| label.starts_with(".Lnested_use_cellfield_") && !label.ends_with("_done"))
            })
            .unwrap();
        let codec = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| &block.nested_cell_labels)
            .filter(|label| !label.ends_with("_done") && !label.ends_with("_validate"))
            .min_by_key(|label| label.strip_prefix(".Lnested_cell_codec_").unwrap().split('_').next().unwrap().parse::<u32>().unwrap())
            .unwrap();
        let coordinates = codec
            .strip_prefix(".Lnested_cell_codec_")
            .unwrap()
            .split('_')
            .map(|value| value.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        let [root, size, buffer, _] = coordinates.as_slice() else { unreachable!() };
        let ranges = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| {
                block.nested_read_labels.iter().filter(|label| !label.ends_with("_done")).map(|label| {
                    let done = format!("{label}_done");
                    (
                        block.range.start,
                        valid.record.blocks.iter().find(|block| block.nested_read_labels.contains(&done)).unwrap().range.start,
                    )
                })
            })
            .collect::<Vec<_>>();
        let store = elf
            .instructions
            .iter()
            .find(|i| {
                i.address > projection.range.start
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == projection.owner_entry && block.range.contains(i.address))
                    && !ranges.iter().any(|(start, end)| *start <= i.address && i.address < *end)
                    && i.word & 0x707f == 0x3023
                    && i.word >> 15 & 31 == 2
            })
            .unwrap();
        // The first ordinary scalar store after projection is redirected to
        // each protected region, including one-byte corruption at its edge.
        for offset in [root * 8, *size, *buffer, buffer + 511] {
            for kind in [0, 3] {
                assert!((0..2048).contains(&offset));
                let word = (store.word & !0xfe007f80) | ((offset as u32 & 0xfe0) << 20) | ((offset as u32 & 31) << 7) | (kind << 12);
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
fn nested_cell_borrow_rejects_another_valid_cell_with_the_same_layout() {
    let source = format!(
        "{}\n#[effect(ReadOnly)]\nfn count(document: &Document) -> u64 {{ return document.entries.len() }}\n",
        NESTED_CELL_CODEC_SOURCE
            .replace("input before: Document", "input before: Document, input peer: Document")
            .replace("require values.len() == 1", "require count(&before) == 1")
            .replace("consume before", "consume peer\nconsume before")
    );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "roll").unwrap();
        let peer = typed.params.iter().find(|param| param.name == "peer").unwrap();
        let peer_id = typed.locals.iter().find(|local| local.id == peer.binding_id).unwrap().source_id;
        let label = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == typed.id)
            .flat_map(|block| &block.nested_cell_labels)
            .find(|label| {
                label.starts_with(&format!(".Lnested_cell_codec_{peer_id}_"))
                    && !label.ends_with("_done")
                    && !label.ends_with("_validate")
            })
            .unwrap();
        let size = label.strip_prefix(".Lnested_cell_codec_").unwrap().split('_').nth(1).unwrap().parse::<i32>().unwrap();
        let helper = valid.record.entries.iter().find(|entry| entry.name == "count").unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let call = elf.control_flow.iter().find(|flow| flow.target == start).unwrap();
        let mut changes = Vec::new();
        for (register, offset) in [(10, (peer_id * 8) as i32), (11, size)] {
            let load = elf
                .instructions
                .iter()
                .rev()
                .find(|i| {
                    i.address < call.address
                        && matches!(i.word & 0x7f, 0x03 | 0x13 | 0x33 | 0x37 | 0x1b | 0x3b)
                        && i.word >> 7 & 31 == register
                })
                .unwrap();
            assert!((-2048..2048).contains(&offset));
            changes.push((load.address, ((offset as u32 & 0xfff) << 20) | (2 << 15) | (3 << 12) | (register << 7) | 3));
        }
        // Both buffers have already passed their own complete codec. Swapping
        // pointer AND length must still reject the wrong declared Cell source.
        let mut changed = valid.clone();
        changed.replace_machine_words(&changes);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("do not match their typed source"), "{error}");
    }
}

#[test]
fn nested_cell_field_views_bind_spans_lengths_and_load_initialization() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &nested_cell_two_fields_source(),
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = std::collections::BTreeSet::new();
        let mut projections = 0;
        for block in &valid.record.blocks {
            for label in &block.nested_read_labels {
                if !label.starts_with(".Lnested_use_cellfield_") || label.ends_with("_done") {
                    continue;
                }
                projections += 1;
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.nested_read_labels.contains(&format!("{label}_done")))
                    .unwrap()
                    .range
                    .start;
                for instruction in elf.instructions.iter().filter(|i| i.address >= block.range.start && i.address < end) {
                    let word = instruction.word;
                    let replacement = match word & 0x7f {
                        3 => Some(word ^ (1 << 23)),    // Different byte/word displacement.
                        0x23 => Some(word ^ (1 << 10)), // Different scalar or size slot.
                        0x63 | 0x33 => Some(word ^ (1 << 15)),
                        _ => None,
                    };
                    if let Some(word) = replacement {
                        mutations.insert((instruction.address, word));
                    }
                }
            }
        }
        assert_eq!(projections, 2);
        let done = valid
            .record
            .blocks
            .iter()
            .find(|block| block.nested_cell_labels.iter().any(|label| label.ends_with("_done")))
            .unwrap()
            .range
            .start;
        let status = elf.instructions.iter().find(|i| i.address == done).unwrap();
        mutations.insert((done, status.word ^ (1 << 12)));
        let initialization = elf.instructions.iter().find(|i| i.address > done && i.word & 0x7f == 0x23).unwrap();
        mutations.insert((initialization.address, initialization.word ^ (1 << 10)));
        for (address, word) in mutations {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, word);
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{diagnostics} at {address:#x}: {error}");
        }
    }
}

#[test]
fn nested_cell_field_order_is_explicit_and_authenticated() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &nested_cell_two_fields_source(),
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let definition = valid.record.typed_semantics.types.iter().find(|ty| ty.name == "Document").unwrap();
        for (name, index) in [("id", 0), ("entries", 1), ("spare", 2)] {
            assert_eq!(definition.fields.iter().find(|field| field.name == name).unwrap().molecule_field_index, Some(index));
        }
        for mutation in 0..3 {
            let mut changed = valid.clone();
            let ty = changed.record.typed_semantics.types.iter_mut().find(|ty| ty.name == "Document").unwrap();
            for field in &mut ty.fields {
                field.molecule_field_index = match mutation {
                    0 => None,
                    1 => Some(0),
                    _ => field.molecule_field_index.map(|index| if index == 0 { 0 } else { 3 - index }),
                };
            }
            ty.layout_hash = canonical_hash(
                "cellscript-typed-layout-v2",
                &(
                    ty.kind.as_str(),
                    ty.encoded_size,
                    &ty.fields,
                    ty.tag_width_bytes,
                    &ty.variants,
                    &ty.capabilities,
                    &ty.identity_policy,
                ),
            )
            .unwrap();
            let layout_hash = ty.layout_hash.clone();
            // Rebind schema-dependent roles and their source-map identities as
            // well; swapped field indices must reach the machine proof.
            for role in changed.record.typed_semantics.foundation.roles.iter_mut().filter(|role| role.ty == "Document") {
                let old_node = role.semantic_node_id.clone();
                role.schema_identity.clone_from(&layout_hash);
                role.semantic_node_id = canonical_hash(
                    "cellscript-semantic-node-role-v1",
                    &(
                        role.role_id.as_str(),
                        role.entry_id.as_str(),
                        role.binding.as_str(),
                        role.ty.as_str(),
                        role.direction.as_str(),
                        role.source.as_str(),
                        role.selector.as_str(),
                        role.cardinality.as_str(),
                        role.lock_or_type_role.as_str(),
                        role.script_identity_policy.as_str(),
                        role.schema_identity.as_str(),
                        role.correspondence_policy.as_str(),
                    ),
                )
                .unwrap();
                for mapping in &mut changed.source_map.semantic_mappings {
                    if mapping.semantic_node_id == old_node {
                        mapping.semantic_node_id.clone_from(&role.semantic_node_id);
                    }
                }
            }
            changed.source_map.canonicalize();
            changed.rebind_layered_identities();
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(
                error.code,
                if mutation < 2 {
                    CheckerRejectionCode::V2419TypedSemanticsInvalid
                } else {
                    CheckerRejectionCode::V2420TypedMachineBindingInvalid
                },
                "{error}"
            );
        }
    }
}

#[test]
fn nested_cell_codec_rejects_rehashed_load_table_and_decoder_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                NESTED_CELL_CODEC_SOURCE,
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = std::collections::BTreeSet::new();
        let mut sites = 0;
        for block in &valid.record.blocks {
            for label in &block.nested_cell_labels {
                if label.ends_with("_done") || label.ends_with("_validate") {
                    continue;
                }
                sites += 1;
                let validation = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.nested_cell_labels.contains(&format!("{label}_validate")))
                    .unwrap()
                    .range
                    .start;
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.nested_cell_labels.contains(&format!("{label}_done")))
                    .unwrap()
                    .range
                    .start;
                for instruction in elf.instructions.iter().filter(|i| i.address >= block.range.start && i.address < end) {
                    let word = instruction.word;
                    let rd = word >> 7 & 31;
                    // Every table/element condition and byte load; transport
                    // constants bind the capacity, source and ordinal too.
                    let changed = if word & 0x7f == 0x63 {
                        Some(word ^ (1 << 15))
                    } else if word & 0x707f == 0x4003
                        || (instruction.address < validation && word & 0x707f == 0x13 && [5, 10, 11, 13, 14].contains(&rd))
                    {
                        Some(word ^ (1 << 20))
                    } else {
                        None
                    };
                    if let Some(changed) = changed {
                        mutations.insert((instruction.address, changed));
                    }
                    if instruction.address < validation && word & 0x707f == 0x13 && word >> 15 & 31 == 0 && [5, 13, 14].contains(&rd) {
                        mutations.insert((instruction.address, word | (rd << 15)));
                    }
                }
            }
        }
        assert!(sites >= 2, "input and output codecs must both be bound");
        assert!(mutations.len() > 100);
        for (address, word) in mutations {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, word);
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{diagnostics}, {address:#x}: {error}");
        }
    }
}

#[test]
fn nested_cell_codec_requires_markers_for_unused_fields_and_group_roles() {
    use cellscript::artifact::{compile_artifact, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    let source = "module unused_nested_cell\nresource Document has store, consume { id: u64, entries: NestedValues<Vec<u8>, 2, 3, 32> }\naction remove(input before: Document) { verification\nconsume before }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile_artifact(
                source,
                CompileOptions {
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    target_profile: Some("ckb".into()),
                    ..Default::default()
                },
                ArtifactDeclaration {
                    name: "nested-document".into(),
                    context: ArtifactContext::TypeGroup { resource: "Document".into() },
                    dispatch: ArtifactDispatch::PolicyWitnessV1,
                    actions: vec![ArtifactAction { tag: 1, action: "remove".into() }],
                    common_checks: vec![],
                },
                cellscript::ExecutableSurfacePolicy::AllowFailClosed,
            )
            .unwrap(),
        );
        assert!(valid.record.blocks.iter().any(|block| !block.nested_cell_labels.is_empty()));
        let mut changed = valid.clone();
        for block in &mut changed.record.blocks {
            block.nested_cell_labels.clear();
        }
        // Rebind the complete sidecar envelope without changing ELF bytes.
        let elf = parse_elf(&changed.artifact, CheckerBudgets::default().instructions).unwrap();
        let instruction = &elf.instructions[0];
        changed.replace_machine_word(instruction.address, instruction.word);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("lacks a complete load codec"), "{error}");
    }
}

#[test]
fn nested_cell_codec_rejects_a_rebound_entry_after_the_syscall() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                NESTED_CELL_CODEC_SOURCE,
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let target = valid
            .record
            .blocks
            .iter()
            .find(|block| block.nested_cell_labels.iter().any(|label| label.ends_with("_validate")))
            .unwrap();
        let label = target.nested_cell_labels.iter().find(|label| label.ends_with("_validate")).unwrap();
        let done_label = label.replace("_validate", "_done");
        let done = valid.record.blocks.iter().find(|block| block.nested_cell_labels.contains(&done_label)).unwrap().range.start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let flow = elf
            .control_flow
            .iter()
            .find(|flow| {
                flow.address >= done
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == target.owner_entry && block.range.contains(flow.address))
                    && valid.record.verifier_failure_exits.iter().any(|exit| exit.code == 5 && exit.address == flow.target)
            })
            .unwrap();
        let block = valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap();
        let word = elf.instructions.iter().find(|instruction| instruction.address == flow.address).unwrap().word;
        assert_eq!(word & 0x7f, 0x63);
        let offset = target.range.start as i64 - flow.address as i64;
        assert!((-4096..4096).contains(&offset));
        let immediate = offset as u32;
        let branch = (word & 0x01fff07f)
            | (((immediate >> 12) & 1) << 31)
            | (((immediate >> 5) & 0x3f) << 25)
            | (((immediate >> 1) & 15) << 8)
            | (((immediate >> 11) & 1) << 7);
        let mut changed = valid.clone();
        changed.record.edges.iter_mut().find(|edge| edge.from == block.id && edge.kind == EdgeKind::ConditionalTaken).unwrap().to =
            target.id.clone();
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
        assert!(error.to_string().contains("external control flow bypasses Cell load"), "{error}");
    }
}

#[test]
fn nested_lifetime_preserves_hidden_result_pointer_provenance() {
    let source = "module nested_result_write\nstruct Pair { left: u64, right: u64 }\nfn make(at: u64) -> Pair { Pair { left: at, right: at + 1 } }\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 { verification\nlet bytes = values[0]\nlet pair = make(bytes.len())\nrequire pair.right > pair.left\nreturn bytes.len() }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let callee = valid.record.entries.iter().find(|entry| entry.name == "make").unwrap();
        assert_eq!(callee.fixed_result_abi.as_ref().unwrap().hidden_argument_index, 1);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let spill = elf
            .instructions
            .iter()
            .find(|instruction| {
                valid.record.blocks.iter().any(|block| block.owner_entry == callee.id && block.range.contains(instruction.address))
                    && instruction.word & 0x707f == 0x3023
                    && instruction.word >> 20 & 31 == 10
            })
            .unwrap();
        let mut changed = valid.clone();
        // Replace the ordinary parameter spill with sd a0, 0(a1). The incoming
        // a1 is a valid result capability, but only the checked return copy may
        // exercise it. Rebinding hashes must not turn it into arbitrary writes.
        changed.replace_machine_word(spill.address, (10 << 20) | (11 << 15) | 0x3023);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("private stack provenance"), "{error}");

        let abi = callee.fixed_result_abi.as_ref().unwrap();
        let body_store = elf
            .instructions
            .iter()
            .find(|instruction| {
                valid.record.blocks.iter().any(|block| block.owner_entry == callee.id && block.range.contains(instruction.address))
                    && instruction.address >= abi.save_range.end
                    && instruction.word & 0x707f == 0x3023
            })
            .unwrap();
        // Corrupt the final byte of the saved hidden pointer through FP. The
        // copy range still contains its original load and exact width; only a
        // lifetime proof notices that the loaded capability was invalidated.
        let offset = (i64::from(abi.saved_pointer_offset) + 7 - i64::from(abi.owned_frame_bytes)) as u32 & 0xfff;
        let byte_store = ((offset >> 5) << 25) | (body_store.word & (31 << 20)) | (8 << 15) | ((offset & 31) << 7) | 0x23;
        let mut changed = valid.clone();
        changed.replace_machine_word(body_store.address, byte_store);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("private stack provenance"), "{error}");
    }
}

#[test]
fn nested_lifetime_checks_transitive_callee_memory_effects() {
    let source = "module nested_callee\nfn bump(at: u64) -> u64 { return at + 1 }\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness at: u64) -> u64 { verification\nlet bytes = values[at]\nrequire bump(at) > at\nreturn bytes.len() }";
    let valid = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let callee = valid.record.entries.iter().find(|entry| entry.name == "bump").unwrap();
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let store = elf
        .instructions
        .iter()
        .find(|instruction| {
            valid.record.blocks.iter().any(|block| block.owner_entry == callee.id && block.range.contains(instruction.address))
                && instruction.word & 0x707f == 0x3023
        })
        .unwrap();
    let mut changed = valid.clone();
    // An argument-derived write has no proof that it stays in the helper's own frame.
    changed.replace_machine_word(store.address, (store.word & !(31 << 15)) | (10 << 15));
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
    assert!(error.to_string().contains("private stack provenance"), "{error}");
}

#[test]
fn nested_lifetime_checks_fixed_copy_destinations_after_hash_rebinding() {
    use cellscript::artifact::{compile_artifact, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    let source = "module nested_copy_effects\nresource Token has store, consume { expected: Hash }\naction inspect(input token: Token, witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness expected: Hash) { verification\nrequire values.len() > 0\nrequire token.expected == expected\nconsume token }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile_artifact(
                source,
                CompileOptions {
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    target_profile: Some("ckb".into()),
                    ..Default::default()
                },
                ArtifactDeclaration {
                    name: "nested-copy".into(),
                    context: ArtifactContext::TypeGroup { resource: "Token".into() },
                    dispatch: ArtifactDispatch::PolicyWitnessV1,
                    actions: vec![ArtifactAction { tag: 1, action: "inspect".into() }],
                    common_checks: vec![],
                },
                cellscript::ExecutableSurfacePolicy::AllowFailClosed,
            )
            .unwrap(),
        );
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
        let pointer = typed.locals.iter().find(|local| local.ty.starts_with("NestedValues<")).unwrap().source_id as i32 * 8;
        let copy = valid.record.entries.iter().find(|entry| entry.name == "__cellscript_memcpy_fixed").unwrap();
        let target = valid.record.blocks.iter().find(|block| block.id == copy.entry_block).unwrap().range.start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let call = elf
            .control_flow
            .iter()
            .find(|flow| {
                flow.target == target
                    && valid.record.blocks.iter().any(|block| block.owner_entry == typed.id && block.range.contains(flow.address))
            })
            .unwrap();
        let destination = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < call.address
                    && instruction.address + 32 >= call.address
                    && instruction.word & 0x707f == 0x13
                    && instruction.word >> 7 & 31 == 11
                    && instruction.word >> 15 & 31 == 2
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(destination.address, replace_i_immediate(destination.word, pointer));
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
        let mut changed = valid.clone();
        let helper_store = elf.instructions.iter().find(|instruction| instruction.address == target + 8).unwrap();
        changed.replace_machine_word(helper_store.address, helper_store.word ^ (1 << 15));
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("copy effects"), "{error}");
    }
}

#[test]
fn nested_lifetime_rejects_coherent_derived_pointer_aliasing_of_a_root_slot() {
    let source = "module nested_physical_alias\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 { verification\nlet bytes = values[0]\nreturn bytes.len() + values.len() }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
        let root = typed.locals.iter().find(|local| local.id == typed.params[0].binding_id).unwrap().source_id;
        let destination = typed
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|operation| operation.opcode == "index")
            .unwrap()
            .destinations[0];
        let old = typed.locals.iter().find(|local| local.id == destination).unwrap().source_id;
        assert_ne!(old, root);
        let mut changed = valid.clone();
        changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .find(|entry| entry.id == typed.id)
            .unwrap()
            .locals
            .iter_mut()
            .find(|local| local.id == destination)
            .unwrap()
            .source_id = root;
        let prefix = format!(".Lnested_use_index_{old}_");
        for block in changed.record.blocks.iter_mut().filter(|block| block.owner_entry == typed.id) {
            for label in &mut block.nested_read_labels {
                if let Some(suffix) = label.strip_prefix(&prefix) {
                    *label = format!(".Lnested_use_index_{root}_{suffix}");
                }
            }
        }
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let replacements = elf
            .instructions
            .iter()
            .filter(|instruction| {
                valid.record.blocks.iter().any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
            })
            .filter_map(|instruction| {
                let word = instruction.word;
                if word >> 15 & 31 != 2 {
                    return None;
                }
                if word & 0x707f == 0x3003 && (word as i32) >> 20 == old as i32 * 8 {
                    Some((instruction.address, replace_i_immediate(word, root as i32 * 8)))
                } else if word & 0x707f == 0x3023 && decode_s_immediate(word) == old as i32 * 8 {
                    let offset = root as u32 * 8;
                    Some((instruction.address, (word & !0xfe000f80) | ((offset & 0xfe0) << 20) | ((offset & 31) << 7)))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert!(replacements.len() >= 2, "rewrite both derived pointer producer and consumer");
        changed.record.canonicalize();
        changed.rebind_typed_semantics();
        changed.replace_machine_words(&replacements);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("nested pointer slot aliases another typed local"), "{error}");
    }
}

#[test]
fn allocated_values_sorted_arrays_reject_rebound_shapes_after_admission() {
    let source = "module sorted_proof\ncollections Work { limits { bytes: 128, allocations: 2 }\ntotals: OrderedMap<u8, u16> { items: 4 } }\naction inspect() -> u64 { let mut work = Work::new()\nwork.totals.load_sorted([(1, 7), (1, 11), (2, 13)])\nreturn 0 }";
    let valid = Fixture::from_result(
        compile(source, CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
    );
    valid.check().unwrap();
    for mutation in 0..6 {
        let mut changed = valid.clone();
        let op = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| {
                matches!(
                    op.detail,
                    TypedSemanticOperationDetail::AllocatedValue {
                        operation: cellscript_artifact_checker::allocated_values::Operation::LoadSorted,
                        ..
                    }
                )
            })
            .unwrap();
        match mutation {
            0 => op.operands[1].ty = "[(u16, u8); 3]".into(),
            1 => op.operands[1].ty = "[(u8, u16); 1025]".into(),
            2 => op.operands[1].ty = "[(u8, u16); 2]".into(),
            3 => op.operands[0].ty = "&Work".into(),
            4 => {
                op.operands.pop();
            }
            _ => op.operands.push(op.operands[1].clone()),
        }
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
    for mutation in 0..3 {
        let mut changed = valid.clone();
        let op = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| op.opcode == "tuple" && op.operands.len() == 3)
            .unwrap();
        match mutation {
            0 => {
                op.operands.pop();
            }
            1 => op.operands.push(op.operands[0].clone()),
            _ => op.operands[0].ty = "(u16, u8)".into(),
        }
        changed.rebind_typed_semantics();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
    assert_bounded_admitted(&valid);
}

#[test]
fn allocated_writes_preserve_retained_cell_views_after_guard_mutations() {
    let source = r#"
module guarded_collections
resource Note has store { amount: u64 }
collections Work { limits { bytes: 128, allocations: 2 }
    values: Vec<u64> { items: 4 }
}
action inspect() -> u64 {
    verification
    let note = ckb::group_input<Note>(0).data
    let mut work = Work::new()
    work.values.push(note.amount)
    require work.values.get(0) == note.amount
    return 0
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let words = elf
            .instructions
            .iter()
            .map(|instruction| (instruction.address, instruction.word))
            .collect::<std::collections::BTreeMap<_, _>>();
        let guards = elf
            .instructions
            .iter()
            .filter_map(|instruction| {
                let pc = instruction.address;
                let word = instruction.word;
                let upper = *words.get(&(pc + 4))?;
                let store = *words.get(&(pc + 8))?;
                (word & 0x707f == 0x6063
                    && upper & 0x707f == 0x6063
                    && store & 0x7f == 0x23
                    && word >> 15 & 31 == upper >> 20 & 31
                    && word >> 15 & 31 == store >> 15 & 31)
                    .then_some((pc, word, upper, store))
            })
            .collect::<Vec<_>>();
        assert!(guards.len() >= 4);
        for mutation in 0..8 {
            let replacements = guards
                .iter()
                .map(|&(pc, lower, upper, store)| match mutation {
                    0 => (pc, lower & !0x7000),                        // equality is not an unsigned lower bound
                    1 => (pc + 4, upper & !0x7000),                    // equality is not an unsigned upper bound
                    2 => (pc + 8, (store & !0xfe000f80) | 0xfe000f80), // unchecked -1 address displacement
                    3 => (pc + 4, upper & !(31 << 15)),                // x0 is not a private-stack upper bound
                    4 => (pc + 8, (store & !(0xfe000f80 | (31 << 15))) | 0xfe000c00 | (2 << 15)), // spill below the allowed subframe
                    5 => (pc, encode_jal(8)),                          // bypass both guards and enter the store
                    6 => (pc - 4, 0xfff10013 | ((upper >> 15 & 31) << 7)), // upper bound below private frame
                    7 => (pc + 24, 0x01010113),                        // restore only half the temporary subframe
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>();
            let mut changed = valid.clone();
            changed.replace_machine_words(&replacements);
            assert_eq!(
                changed.check(),
                Err(match mutation {
                    5 => CheckerRejectionCode::V2414ControlFlowInvalid,
                    7 => CheckerRejectionCode::V2407AbiOrStackInvalid,
                    _ => CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                }),
                "{diagnostics}/guard mutation {mutation} did not fail independent machine binding"
            );
        }
    }
}

#[test]
fn mixed_settlement_shared_return_tails_reject_skipped_restores_and_field_checks() {
    let source = include_str!("../examples/language/batches/mixed_settlement.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        valid.check().unwrap();
        let helper = valid.record.entries.iter().find(|entry| entry.name.ends_with("transfer_leg")).unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        // Source-only evidence can share the epilogue address and become its
        // primary machine label. Locate the actual return rather than a label
        // spelling, while retaining the complete restoration mutation range.
        let tail = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.owner_entry == helper.id
                    && elf
                        .instructions
                        .iter()
                        .any(|instruction| block.range.contains(instruction.address) && instruction.word == 0x00008067)
            })
            .unwrap();
        assert!(
            elf.stack_adjustments
                .iter()
                .any(|adjustment| tail.range.contains(adjustment.address) && adjustment.delta == i64::from(helper.frame_size_bytes)),
            "return mutation range must include frame restoration"
        );
        for instruction in elf.instructions.iter().filter(|instruction| tail.range.contains(instruction.address)) {
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, instruction.word ^ (1 << 7));
            assert!(changed.check().is_err(), "altered return restoration at {:#x}", instruction.address);
        }
        let edge = elf
            .control_flow
            .iter()
            .find(|edge| {
                edge.target == tail.range.start
                    && elf
                        .instructions
                        .iter()
                        .any(|instruction| instruction.address == edge.address && instruction.word & 0xfff == 0x6f)
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(edge.address, encode_jal((tail.range.start as i64 + 4 - edge.address as i64) as i32));
        assert!(changed.check().is_err(), "entry into the middle of a return tail");
        let read = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.owner_entry == helper.id
                    && block.exact_role_labels.iter().any(|label| label.starts_with(".Lexact_role_data_") && !label.ends_with("_done"))
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(read.range.start, encode_jal((tail.range.start as i64 - read.range.start as i64) as i32));
        assert!(changed.check().is_err(), "return-tail recognition must not bypass a field check");
    }
}

#[test]
fn nominal_wide_result_contract_rejects_retyping_wrong_width_and_hidden_pointer() {
    for (ty, other) in [("ScriptHash", "SighashAllDigest"), ("SighashAllDigest", "ScriptHash")] {
        let source = format!("module nominal_result\nfn copy(value: {ty}) -> {ty} {{ let snapshot = value\nreturn snapshot }}\naction inspect(witness value: {ty}) -> u64 {{ verification require copy(value) == value return 0 }}");
        for diagnostics in [
            cellscript::CompilerDiagnostics::default().without(cellscript::OptimizationPass::Inlining),
            cellscript::CompilerDiagnostics::without_optimizations(),
        ] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            valid.check().unwrap();
            for mutation in 0..6 {
                let mut changed = valid.clone();
                let abi =
                    changed.record.entries.iter_mut().find(|entry| entry.name == "copy").unwrap().fixed_result_abi.as_mut().unwrap();
                match mutation {
                    0 => abi.width_bytes = 31,
                    1 => abi.hidden_argument_index += 1,
                    2 => abi.copy_ranges.clear(),
                    3 => abi.type_name = "hash".into(),
                    4 => abi.type_name = other.into(),
                    _ => abi.copy_sources[0].field_offset = 1,
                }
                changed.rebind_sidecars();
                assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);
            }
            let abi = valid.record.entries.iter().find(|entry| entry.name == "copy").unwrap().fixed_result_abi.as_ref().unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let load = elf
                .instructions
                .iter()
                .find(|word| {
                    abi.copy_sources.iter().any(|source| source.range.contains(word.address))
                        && word.word & 0x7f == 3
                        && word.word >> 7 & 31 == 10
                })
                .unwrap();
            let mut changed = valid.clone();
            changed.replace_machine_word(load.address, load.word & !(31 << 15));
            assert!(changed.check().is_err(), "{ty} result cannot read an unproved absolute pointer");
        }
    }
}

#[test]
fn computed_sighash_result_cannot_be_relabelled_as_an_ordinary_hash() {
    let valid = Fixture::from_source(SIGHASH_ZERO_LOCK_SOURCE);
    valid.check().unwrap();
    let mut changed = valid.clone();
    for entry in &mut changed.record.typed_semantics.entries {
        for local in &mut entry.locals {
            if local.ty == "SighashAllDigest" {
                local.ty = "hash".into();
            }
        }
        for operation in entry.blocks.iter_mut().flat_map(|block| &mut block.operations) {
            for operand in &mut operation.operands {
                if operand.ty == "SighashAllDigest" {
                    operand.ty = "hash".into();
                }
            }
            if let Some(call) = &mut operation.call {
                for param in &mut call.params {
                    if param == "SighashAllDigest" {
                        *param = "hash".into();
                    }
                }
                if call.return_type == "SighashAllDigest" {
                    call.return_type = "hash".into();
                }
            }
        }
    }
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}
