use super::*;

const PREFIX: &str = ".Lallocated_scalar_";

fn erase_markers(fixture: &mut Fixture) {
    for block in &mut fixture.record.blocks {
        block.allocated_lifecycle_labels.retain(|label| !label.starts_with(PREFIX));
    }
    fixture.rebind_sidecars();
}

#[test]
fn bounded_scalar_arithmetic_binds_cell_fields_and_named_nested_helpers() {
    let cell = r#"module scalar_cell_view
resource Token has store { amount: u64 }
fn adjusted(before: InputView<Token>, delta: u64) -> u64 {
    let value = before.data.amount
    if delta == 0 { return value }
    return value + delta
}
action inspect() -> u64 {
    verification
    require adjusted(ckb::input<Token>(0), 4) == 7
    return 0
}"#;
    let nested = r#"module scalar_nested_view
data_shape Messages { element: Vec<u8>, limits { items: 2, payload_bytes: 8, encoded_bytes: 32 } }
fn adjusted(message: Messages::Item, delta: u64) -> u64 {
    let value = message.len()
    if delta == 0 { return value }
    return value + delta
}
fn forward(messages: Messages) -> u64 { return adjusted(messages[0], 4) }
action inspect(witness messages: Messages) -> u64 {
    require forward(messages) == 7
    return 0
}"#;
    for (source, nested) in [(cell, false), (nested, true)] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap_or_else(|error| panic!("nested={nested}/{diagnostics}: {error}"));
            let valid = Fixture::from_result(compiled.clone());
            assert!(!valid
                .record
                .typed_semantics
                .entries
                .iter()
                .flat_map(|entry| &entry.blocks)
                .flat_map(|block| &block.operations)
                .any(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { .. })));
            use cellscript::EntryWitnessArg as A;
            use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
            let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            transaction.current_type_script_input_indices = vec![0];
            transaction.inputs[0].data = 3u64.to_le_bytes().to_vec().into();
            if nested {
                // Independent one-item Molecule DynVec containing Bytes[1,2,3].
                let value = [15u32.to_le_bytes().as_slice(), &8u32.to_le_bytes(), &3u32.to_le_bytes(), &[1, 2, 3]].concat();
                let payload = compiled.metadata.actions[0].entry_witness_args(&[A::Bytes(value)]).unwrap();
                transaction.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            }
            assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
            let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("adjusted")).unwrap();
            let (block, op) = entry
                .blocks
                .iter()
                .flat_map(|block| block.operations.iter().map(move |op| (block, op)))
                .find(|(_, op)| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add"))
                .unwrap();
            let prefix = format!("{PREFIX}{}_{}_", block.id, op.index);
            let begin = valid
                .record
                .blocks
                .iter()
                .find(|block| {
                    block.owner_entry == entry.id
                        && block.allocated_lifecycle_labels.iter().any(|label| label.starts_with(&prefix) && label.ends_with("_begin"))
                })
                .unwrap();
            let label =
                begin.allocated_lifecycle_labels.iter().find(|label| label.starts_with(&prefix) && label.ends_with("_begin")).unwrap();
            let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
            let end = valid
                .record
                .blocks
                .iter()
                .find(|block| block.owner_entry == entry.id && block.allocated_lifecycle_labels.contains(&end_label))
                .unwrap()
                .range
                .start;
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let add = elf
                .instructions
                .iter()
                .find(|instruction| (begin.range.start..end).contains(&instruction.address) && instruction.word == 0x006282b3)
                .unwrap();
            let mut changed = valid.clone();
            changed.replace_machine_word(add.address, add.word | 1 << 30);
            assert_ne!(ckb_script_runner::execute_cellscript_script(&changed.artifact, &transaction).exit_code, 0);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            let branch = elf
                .instructions
                .iter()
                .rev()
                .find(|instruction| instruction.address < begin.range.start && instruction.word & 0x7f == 0x63)
                .unwrap();
            let mut bypass = valid.clone();
            bypass.redirect_conditional_branch(branch.address, end);
            assert_ne!(ckb_script_runner::execute_cellscript_script(&bypass.artifact, &transaction).exit_code, 0);
            assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            let mut erased = valid.clone();
            erase_markers(&mut erased);
            assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn bounded_scalar_inventory_covers_named_catalogs_and_complete_programs() {
    for source in [
        include_str!("../syntax_combo/seeds/named-data-shapes.cell"),
        include_str!("../syntax_combo/seeds/named-role-selection.cell"),
        include_str!("../../examples/language/batches/mixed_settlement.cell"),
        include_str!("../../examples/language/batches/nested_migration.cell"),
        include_str!("../../examples/language/batches/record_migration.cell"),
        NESTED_CELL_CODEC_SOURCE,
    ] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(
                    source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap_or_else(|error| {
                    panic!("{diagnostics} {}: {error}", source.lines().find(|line| line.starts_with("module ")).unwrap_or("fixture"))
                }),
            );
            assert!(
                valid.record.blocks.iter().flat_map(|block| &block.allocated_lifecycle_labels).any(|label| label.starts_with(PREFIX)),
                "{} {diagnostics}",
                valid.record.module
            );
            let mut erased = valid.clone();
            erase_markers(&mut erased);
            assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}
