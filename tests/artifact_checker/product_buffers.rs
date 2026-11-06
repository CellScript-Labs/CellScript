use super::*;

const SOURCE: &str = include_str!("../fixtures/document_product_sum_relation.cell");

fn compile_relation(source: &str, diagnostics: cellscript::CompilerDiagnostics) -> CompileResult {
    compile(
        source,
        CompileOptions {
            edition: CURRENT_EDITION,
            diagnostics,
            target: Some("riscv64-elf".into()),
            target_profile: Some("ckb".into()),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn typed_document_nested_product_sum_relation_has_a_bounded_runtime_closure() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (name, call) in [
            ("sum-eq", "c256::require_sum2_products_eq(quotient, denominator, remainder, one, a, b, zero, zero)"),
            ("sum-lte", "c256::require_sum2_products_lte(quotient, denominator, remainder, one, a, b, zero, zero)"),
            ("product-eq", "c256::require_product_eq(quotient, denominator, a, b)"),
            ("product-lte", "c256::require_product_lte(quotient, denominator, a, b)"),
        ] {
            let source = SOURCE
                .replace("c256::require_sum2_products_eq(quotient, denominator, remainder, one, a, b, zero, zero)", call)
                .replace("exact_relation", "check_document_arithmetic")
                .replace("check_document(", "read_revision(");
            let compiled = compile_relation(&source, diagnostics);
            let row = if name.starts_with("sum") { [2, 3, 4, 1, 2] } else { [2, 3, 3, 2, 0] };
            execute(&compiled, row, 0);
            let mut wrong = row;
            wrong[3] += 1;
            execute(&compiled, wrong, 49);
            if name.starts_with("sum") {
                let mut wrong = row;
                wrong[4] += 1;
                execute(&compiled, wrong, 49);
            }
            let mut zero = row;
            zero[2] = 0;
            execute(&compiled, zero, 5);
            execute(&compiled, [u128::MAX, u128::MAX, u128::MAX, u128::MAX, 0], 0);
            Fixture::from_result(compiled);
        }
        // Both mathematical sides are equal, so modular wraparound must not
        // accidentally accept two overflowing sums of full-width products.
        let overflow = SOURCE.replace(
            "c256::require_sum2_products_eq(quotient, denominator, remainder, one, a, b, zero, zero)",
            "c256::require_sum2_products_eq(a, b, a, b, quotient, denominator, quotient, denominator)",
        );
        let compiled = compile_relation(&overflow, diagnostics);
        execute(&compiled, [1, 1, 1, 1, 0], 0);
        execute(&compiled, [u128::MAX, u128::MAX, u128::MAX, u128::MAX, 0], 49);
        Fixture::from_result(compiled);
    }
}

#[test]
fn c256_fixed_witness_borrows_bind_split_words_staging_frames_and_exact_targets() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(compile_relation(SOURCE, diagnostics));
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let wrapper = valid.record.entries.iter().find(|e| e.name == "_cellscript_entry").unwrap();
        assert_eq!(wrapper.frame_size_bytes, 5408, "11 ABI words require the existing 32-byte outgoing extent");
        let owned = |pc| valid.record.blocks.iter().any(|b| b.owner_entry == wrapper.id && b.range.contains(pc));
        let words = elf.instructions.iter().filter(|i| owned(i.address)).collect::<Vec<_>>();
        let signed_store_offset = |word: u32| i64::from(((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20) >> 20);
        let stages = words
            .iter()
            .filter(|i| i.word & 0x707f == 0x3023 && i.word >> 15 & 31 == 2 && signed_store_offset(i.word) < 0)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(stages.len(), 3, "word7 pointer is in a7 while word8 length and word9/10 are staged");
        for store in stages {
            let mut changed = valid.clone();
            let offset = signed_store_offset(store.word) + 8;
            let word = (store.word & !((31 << 7) | (127 << 25))) | ((offset as u32 & 31) << 7) | (((offset as u32 >> 5) & 127) << 25);
            changed.replace_machine_word(store.address, word);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let split_pointer = words.iter().find(|i| i.word == addi(17, 2, 72)).unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(split_pointer.address, addi(17, 2, 88));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        for adjustment in words.iter().filter(|i| i.word == addi(2, 2, -32) || i.word == addi(2, 2, 32)) {
            let value = if adjustment.word == addi(2, 2, -32) { -16 } else { 16 };
            let mut changed = valid.clone();
            changed.replace_machine_word(adjustment.address, addi(2, 2, value));
            assert!(changed.check().is_err(), "outgoing reservation/restoration must be exact");
        }
        let mut changed = valid.clone();
        changed.record.entries.iter_mut().find(|e| e.id == wrapper.id).unwrap().frame_size_bytes += 16;
        assert!(changed.check().is_err(), "metadata cannot widen the certified outgoing frame");
        let entry = valid.record.entries.iter().find(|e| e.name == "verify").unwrap();
        let start = valid.record.blocks.iter().find(|b| b.id == entry.entry_block).unwrap().range.start;
        let incoming = elf.control_flow.iter().find(|e| e.target == start && owned(e.address)).unwrap();
        let delta = start as i64 + 4 - (incoming.address as i64 - 4);
        let upper = (delta + 0x800) >> 12;
        let lower = delta - (upper << 12);
        let mut changed = valid.clone();
        changed.replace_machine_words(&[
            (incoming.address - 4, ((upper as u32) << 12) | (1 << 7) | 0x17),
            (incoming.address, ((lower as u32 & 4095) << 20) | (1 << 15) | (1 << 7) | 0x67),
        ]);
        assert!(changed.check().is_err(), "decoded words must reach the exact checked entry prologue");
    }
}

fn execute(compiled: &CompileResult, values: [u128; 5], expected: u64) {
    use cellscript::EntryWitnessArg as A;
    use ckb_testtool::{
        ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
        context::Context,
    };
    let mut context = Context::new_with_deterministic_rng();
    let code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes)));
    let script = context.build_script(&code, Bytes::new()).unwrap();
    let cell = packed::CellOutput::new_builder().capacity::<packed::Uint64>(100_000_000_000u64.pack()).lock(script).build();
    let data = Bytes::copy_from_slice(&7u64.to_le_bytes());
    let previous = context.create_cell(cell.clone(), data.clone());
    let mut args = vec![A::U64(0)];
    args.extend(values.into_iter().map(A::U128));
    let payload = compiled.metadata.locks[0].entry_witness_args(&args).unwrap();
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
    let transaction = context.complete_tx(
        TransactionBuilder::default()
            .input(packed::CellInput::new_builder().previous_output(previous).build())
            .output(cell)
            .output_data(data.pack())
            .witness(witness.as_bytes().pack())
            .build(),
    );
    let result = context.verify_tx(&transaction, 10_000_000);
    if expected == 0 {
        assert!(result.unwrap() > 0);
    } else {
        let error = format!("{:?}", result.unwrap_err());
        assert!(error.contains("Inputs[0].Lock") && error.contains(&format!("error code {expected} ")), "{error}");
    }
}

fn addi(rd: u32, base: u32, immediate: i64) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 4095) << 20) | (base << 15) | (rd << 7) | 0x13
}

#[test]
fn c256_closure_rejects_machine_operand_output_carry_frame_and_branch_mutations() {
    let valid = Fixture::from_result(compile_relation(SOURCE, cellscript::CompilerDiagnostics::default()));
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let mut mutations = 0;
    for owner in [
        "runtime:__cellscript_load_u64_le",
        "runtime:__cellscript_mul_u128_to_u256",
        "runtime:__cellscript_add_u256",
        "runtime:__c256_require_u128_sum2_products_eq",
    ] {
        let owned = |pc| valid.record.blocks.iter().any(|b| b.owner_entry == owner && b.range.contains(pc));
        for instruction in elf.instructions.iter().filter(|i| owned(i.address)) {
            let word = instruction.word;
            // Every byte-load, multiply high/low, carry comparison, arithmetic
            // instruction and store is part of the finite closure. Replacing
            // any one with a NOP cannot silently inherit its summary.
            if matches!(word & 0x7f, 0x03 | 0x23 | 0x33) {
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, addi(0, 0, 0));
                assert!(changed.check().is_err(), "unchecked C256 machine instruction {owner}@{:#x}", instruction.address);
                mutations += 1;
            }
        }
        for edge in elf.control_flow.iter().filter(|edge| owned(edge.address)) {
            let word = elf.instructions.iter().find(|i| i.address == edge.address).unwrap().word;
            if word & 0x7f == 0x63 {
                let mut changed = valid.clone();
                changed.replace_machine_word(edge.address, (word & !(31 << 15)) | (2 << 15));
                assert!(changed.check().is_err(), "unchecked C256 condition {owner}@{:#x}", edge.address);
                mutations += 1;
            }
        }
    }
    assert!(mutations > 100, "complete closure families must execute nontrivial mutations");
    let owner = valid.record.entries.iter().find(|e| e.name == "__c256_require_u128_sum2_products_eq").unwrap();
    let target = valid.record.blocks.iter().find(|b| b.id == owner.entry_block).unwrap().range.start;
    let edge = elf.control_flow.iter().find(|e| e.target == target).unwrap();
    let caller = valid.record.blocks.iter().find(|b| b.range.contains(edge.address)).unwrap().owner_entry.clone();
    let frame = valid.record.entries.iter().find(|e| e.id == caller).unwrap().frame_size_bytes;
    let definition = elf
        .instructions
        .iter()
        .rev()
        .find(|i| {
            i.address < edge.address
                && valid.record.blocks.iter().any(|b| b.owner_entry == caller && b.range.contains(i.address))
                && i.word >> 7 & 31 == 10
                && matches!(i.word & 0x7f, 3 | 0x13 | 0x33 | 0x37)
        })
        .unwrap()
        .address;
    for (base, offset) in [(0, 0), (2, i64::from(frame) - 8)] {
        let mut changed = valid.clone();
        changed.replace_machine_word(definition, addi(10, base, offset));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
    let multiplication = valid.record.entries.iter().find(|e| e.name == "__cellscript_mul_u128_to_u256").unwrap();
    let product_start = valid.record.blocks.iter().find(|b| b.id == multiplication.entry_block).unwrap().range.start;
    let addition = valid.record.entries.iter().find(|e| e.name == "__cellscript_add_u256").unwrap();
    let sum_start = valid.record.blocks.iter().find(|b| b.id == addition.entry_block).unwrap().range.start;
    let call = elf
        .control_flow
        .iter()
        .find(|edge| {
            edge.target == product_start
                && valid.record.blocks.iter().any(|b| b.owner_entry == owner.id && b.range.contains(edge.address))
        })
        .unwrap();
    for target in [sum_start, product_start + 4] {
        let delta = target as i64 - (call.address as i64 - 4);
        let upper = (delta + 0x800) >> 12;
        let lower = delta - (upper << 12);
        let mut changed = valid.clone();
        changed.replace_machine_words(&[
            (call.address - 4, ((upper as u32) << 12) | (1 << 7) | 0x17),
            (call.address, ((lower as u32 & 4095) << 20) | (1 << 15) | (1 << 7) | 0x67),
        ]);
        assert!(changed.check().is_err(), "C256 dependency target cannot change or bypass its prologue");
    }
}

#[test]
fn c256_runtime_body_is_checked_even_without_a_typed_cell_read() {
    let source = SOURCE.replace("let revision = ckb::input<Document>(index).data.revision", "let revision: u64 = 7");
    let valid = Fixture::from_result(compile_relation(&source, cellscript::CompilerDiagnostics::default()));
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let high_product = elf
        .instructions
        .iter()
        .find(|i| {
            i.word & 0xfe00707f == 0x02003033
                && valid
                    .record
                    .blocks
                    .iter()
                    .any(|b| b.owner_entry == "runtime:__cellscript_mul_u128_to_u256" && b.range.contains(i.address))
        })
        .unwrap();
    let mut changed = valid.clone();
    changed.replace_machine_word(high_product.address, high_product.word & !(7 << 12));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}
