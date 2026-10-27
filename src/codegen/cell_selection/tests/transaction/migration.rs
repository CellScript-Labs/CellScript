//! Complete local document transition with an independently executing registry.
use super::*;
use crate::nested_builder::{
    encode_nested_cell_data, encode_nested_values, NestedCellFieldInput as Field, NestedElementInput as Item,
};
use std::collections::BTreeMap;

const SOURCE: &str = include_str!("../../../../../examples/language/batches/nested_migration.cell");

fn compile_migration(opt_level: u8) -> crate::CompileResult {
    let mut source = SOURCE.to_string();
    for (placeholder, actual) in
        [("11", script(&code_identity(1), 0)), ("22", script(&code_identity(2), 0)), ("33", script(&code_identity(3), 0))]
    {
        source = source.replace(&placeholder.repeat(32), &hex::encode(hash(&actual)));
    }
    let ast = crate::frontend::parse(&source, crate::NEXT_EDITION).unwrap();
    let formatted = crate::fmt::format_default(&ast).unwrap();
    assert_eq!(crate::fmt::format_default(&crate::frontend::parse(&formatted, crate::NEXT_EDITION).unwrap()).unwrap(), formatted);
    let compiled = crate::compile(
        &source,
        crate::CompileOptions { edition: crate::NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
    )
    .unwrap_or_else(|error| panic!("migration O{opt_level}: {error}"));
    for blocker in ["nested-values-artifact-proof", "cell-data-view-artifact-proof", "exact-role-selection-artifact-proof"] {
        assert!(!compiled.metadata.runtime.fail_closed_runtime_features.iter().any(|feature| feature == blocker));
    }
    compiled
}

#[derive(Clone, Copy, Debug)]
enum Mutation {
    None,
    History,
    Note,
    Revision,
    Capacity,
    Missing,
    Duplicate,
    Custody,
    PeerReject,
    Malformed,
}

fn run_migration(
    compiled: &crate::CompileResult,
    count: usize,
    maximum: bool,
    mutation: Mutation,
) -> std::result::Result<u64, String> {
    let mut context = Context::new_with_deterministic_rng();
    let document = script(&code_identity(1), 0);
    let registry = script(&code_identity(2), 0);
    let lock = script(&code_identity(3), 0);
    deploy(&mut context, code_identity(1), crate::strip_vm_abi_trailer(&compiled.artifact_bytes).to_vec());
    let peer = crate::compile(
        include_str!("../../../../../tests/fixtures/nested_migration_registry.cell"),
        crate::CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
    )
    .unwrap();
    deploy(
        &mut context,
        code_identity(2),
        if matches!(mutation, Mutation::PeerReject) {
            exit_program(71)
        } else {
            crate::strip_vm_abi_trailer(&peer.artifact_bytes).to_vec()
        },
    );
    deploy(&mut context, code_identity(3), exit_program(0));
    let note = vec![0xab; if maximum { 256 } else { 2 }];
    let encode_document = |id: u64, revision: u64, messages: &[Vec<u8>]| {
        let messages: Vec<_> = messages.iter().map(|payload| Item { fixed_fields: &[], payload }).collect();
        encode_nested_cell_data(
            &compiled.metadata,
            "Document",
            &BTreeMap::from([
                ("messages".into(), Field::Nested(&messages)),
                ("id".into(), Field::FixedBytes(&id.to_le_bytes())),
                ("revision".into(), Field::FixedBytes(&revision.to_le_bytes())),
            ]),
        )
        .unwrap()
    };
    let cell = |script: Option<packed::Script>| {
        packed::CellOutput::new_builder().capacity(400_000_000_000u64).lock(lock.clone()).type_(script.pack()).build()
    };
    // Fee/change Cells are unrelated to both protected roles. The first Type
    // witness is consequently at global input index 1 rather than 0.
    let mut inputs = vec![(cell(None), vec![])];
    let mut outputs = vec![(cell(None), vec![])];
    for index in 0..count {
        let old: Vec<_> = (0..if maximum { 7 } else { index % 3 + 1 })
            .map(|item| vec![(index + item + 1) as u8; if maximum { 256 } else { item * 9 }])
            .collect();
        let mut updated = old.clone();
        updated.push(note.clone());
        if index + 1 == count {
            match mutation {
                Mutation::History => updated[0].push(1),
                Mutation::Note => updated.last_mut().unwrap()[0] ^= 1,
                _ => {}
            }
        }
        inputs.push((cell(Some(document.clone())), encode_document(index as u64, 4, &old)));
        let id = if index + 1 == count && matches!(mutation, Mutation::Duplicate) { 0 } else { index as u64 };
        let revision = if index + 1 == count && matches!(mutation, Mutation::Revision) { 6 } else { 5 };
        let mut data = encode_document(id, revision, &updated);
        if index + 1 == count && matches!(mutation, Mutation::Malformed) {
            data[4] ^= 1;
        }
        let mut output = cell(Some(document.clone()));
        if index + 1 == count {
            if matches!(mutation, Mutation::Capacity) {
                output = output.as_builder().capacity(1u64).build();
            }
            if matches!(mutation, Mutation::Custody) {
                output = output.as_builder().lock(script(&code_identity(3), 1)).build();
            }
        }
        if !(index + 1 == count && matches!(mutation, Mutation::Missing)) {
            outputs.push((output, data));
        }
    }
    let registry_data = |revision: u64| [7u64.to_le_bytes(), revision.to_le_bytes()].concat();
    inputs.push((cell(Some(registry.clone())), registry_data(4)));
    outputs.push((cell(Some(registry)), registry_data(5)));
    outputs.reverse();
    let mut tx = TransactionBuilder::default();
    for (cell, data) in inputs {
        let previous_output = context.create_cell(cell, Bytes::from(data));
        tx = tx.input(packed::CellInput::new_builder().previous_output(previous_output).build());
    }
    for (cell, data) in outputs {
        tx = tx.output(cell).output_data(Bytes::from(data).pack());
    }
    let wire = encode_nested_values(
        &compiled.metadata,
        &compiled.metadata.actions[0].params[0].ty,
        &[Item { fixed_fields: &[], payload: &note }],
    )
    .unwrap();
    let witness = compiled.metadata.actions[0].entry_witness_args(&[crate::EntryWitnessArg::Bytes(wire)]).unwrap();
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(witness)).pack()).build();
    tx = tx.witness(Bytes::new().pack()).witness(witness.as_bytes().pack());
    let tx = context.complete_tx(tx.build());
    context.verify_tx(&tx, 10_000_000).map_err(|error| format!("{error:?}"))
}

#[test]
fn nested_migration_business_transaction_preserves_history_and_checks_every_group() {
    for opt in 0..=3 {
        let compiled = compile_migration(opt);
        for count in [1, 3, 8] {
            let cycles = run_migration(&compiled, count, count == 8, Mutation::None).unwrap();
            eprintln!("[nested migration] O{opt} documents={count} cycles={cycles}");
        }
        // The changed final note remains canonically encoded and fails only
        // after the maximum batch's history comparisons have executed.
        let late = run_migration(&compiled, 8, true, Mutation::Note).unwrap_err();
        assert!(late.contains("error code 5"), "O{opt} late maximum failure: {late}");
        let over = run_migration(&compiled, 9, false, Mutation::None).unwrap_err();
        assert!(over.contains("error code 21"), "O{opt} over-count: {over}");
        for mutation in [
            Mutation::History,
            Mutation::Note,
            Mutation::Revision,
            Mutation::Capacity,
            Mutation::Missing,
            Mutation::Duplicate,
            Mutation::Custody,
            Mutation::PeerReject,
            Mutation::Malformed,
        ] {
            let error = run_migration(&compiled, 3, false, mutation).unwrap_err();
            let code = match mutation {
                Mutation::Missing => 7,
                Mutation::Duplicate => 21,
                Mutation::Custody => 41,
                Mutation::PeerReject => 71,
                Mutation::Malformed => 16,
                _ => 5,
            };
            assert!(error.contains(&format!("error code {code}")), "O{opt} {mutation:?}: {error}");
        }
    }
}
