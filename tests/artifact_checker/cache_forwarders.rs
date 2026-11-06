use super::*;

const SOURCE: &str = include_str!("../fixtures/document_cache_scalar_forwarding.cell");

fn compile_forwarder(source: &str, diagnostics: cellscript::CompilerDiagnostics) -> CompileResult {
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

fn source(hot: bool, wide: bool) -> String {
    let mut source = SOURCE.to_string();
    if hot {
        let reads = std::iter::repeat_n("valid = valid && ckb::cell_data_u64_le(source::input(index), offset) == expected", 48)
            .collect::<Vec<_>>()
            .join("\n");
        source = source.replace(
            "return revision(index) == expected && ckb::cell_data_u64_le(source::input(index), offset) == expected",
            &format!("let mut valid = true\n{reads}\nreturn revision(index) == expected && valid"),
        );
    }
    if wide {
        source = source
            .replace(
                "witness offset: u64) -> bool",
                "witness offset: u64, witness p3: u64, witness p4: u64, witness p5: u64, witness p6: u64, witness p7: u64) -> bool",
            )
            .replace(
                "expected: u64, offset: u64) -> bool",
                "expected: u64, offset: u64, p3: u64, p4: u64, p5: u64, p6: u64, p7: u64) -> bool",
            )
            .replace("check(index, expected, offset)", "check(index, expected, offset, p3, p4, p5, p6, p7)")
            .replace("verify", "approve")
            .replace("check", "read_document");
    }
    source
}

#[test]
fn cache_forwarders_bind_actual_scalar_slots_and_execute_as_real_locks() {
    use cellscript::EntryWitnessArg as A;
    use ckb_testtool::{
        ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
        context::Context,
    };
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (hot, wide) in [(false, false), (true, false), (false, true)] {
            let compiled = compile_forwarder(&source(hot, wide), diagnostics);
            for expected in [7, 8] {
                let mut context = Context::new_with_deterministic_rng();
                let code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes)));
                let script = context.build_script(&code, Bytes::new()).unwrap();
                let cell =
                    packed::CellOutput::new_builder().capacity::<packed::Uint64>(100_000_000_000u64.pack()).lock(script).build();
                assert!(cell.type_().to_opt().is_none());
                let data = Bytes::copy_from_slice(&7u64.to_le_bytes());
                let input = context.create_cell(cell.clone(), data.clone());
                let mut arguments = vec![A::U64(0), A::U64(expected), A::U64(0)];
                if wide {
                    arguments.extend((3..8).map(A::U64));
                }
                let payload = compiled.metadata.locks[0].entry_witness_args(&arguments).unwrap();
                let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
                let tx = context.complete_tx(
                    TransactionBuilder::default()
                        .input(packed::CellInput::new_builder().previous_output(input).build())
                        .output(cell)
                        .output_data(data.pack())
                        .witness(witness.as_bytes().pack())
                        .build(),
                );
                let execution = context.verify_tx(&tx, 10_000_000);
                if expected == 7 {
                    assert!(execution.unwrap() > 0);
                } else {
                    let error = format!("{:?}", execution.unwrap_err());
                    assert!(error.contains("error code 5") || error.contains("error code: 5"), "{error}");
                }
            }
            Fixture::from_result(compiled);
        }
    }
}

fn store(word: u32, offset: i64) -> u32 {
    assert!((-2048..2048).contains(&offset));
    (word & !((31 << 7) | (127 << 25))) | ((offset as u32 & 31) << 7) | (((offset as u32 >> 5) & 127) << 25)
}

fn redirect(valid: &Fixture, pc: u64, target: &cellscript_artifact_checker::LoweringBlock) -> Fixture {
    let mut changed = valid.clone();
    let from = changed.record.blocks.iter().find(|block| block.range.contains(pc)).unwrap().id.clone();
    changed.record.edges.iter_mut().find(|edge| edge.from == from && edge.kind == EdgeKind::Call).unwrap().to = target.id.clone();
    let delta = target.range.start as i64 - (pc - 4) as i64;
    let upper = (delta + 0x800) >> 12;
    let lower = delta - (upper << 12);
    changed.replace_machine_words(&[
        (pc - 4, ((upper as u32) << 12) | (1 << 7) | 0x17),
        (pc, ((lower as u32 & 4095) << 20) | (1 << 15) | (1 << 7) | 0x67),
    ]);
    changed
}

#[test]
fn cache_forwarders_reject_rebound_parameter_result_frame_and_call_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(compile_forwarder(SOURCE, diagnostics));
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let entry = valid.record.entries.iter().find(|entry| entry.kind == cellscript_artifact_checker::EntryKind::Lock).unwrap();
        let owned = |pc| valid.record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(pc));
        let words = elf.instructions.iter().filter(|word| owned(word.address)).collect::<Vec<_>>();
        let cache = words.iter().find(|word| word.word & 0x000fffff == (2 << 15 | 27 << 7 | 0x13)).unwrap();
        let base = i64::from((cache.word as i32) >> 20);
        assert_eq!(base, 24);
        let call = elf
            .control_flow
            .iter()
            .find(|edge| owned(edge.address) && words.iter().any(|word| word.address == edge.address && word.word & 0x7f == 0x67))
            .unwrap();
        let parameter = words
            .iter()
            .find(|word| {
                word.address > cache.address
                    && word.address < call.address
                    && word.word & 0x01fff07f == (10 << 20 | 2 << 15 | 3 << 12 | 0x23)
            })
            .unwrap();
        let result = words
            .iter()
            .find(|word| word.address > call.address && word.word & 0x01fff07f == (10 << 20 | 2 << 15 | 3 << 12 | 0x23))
            .unwrap();
        for instruction in [parameter, result] {
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, store(instruction.word, base));
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        for load in
            words.iter().filter(|word| word.address > cache.address && word.word & 0x000fffff == (2 << 15 | 3 << 12 | 10 << 7 | 3))
        {
            let mut changed = valid.clone();
            changed.replace_machine_word(load.address, (load.word & 0xfffff) | ((base as u32) << 20));
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let mut changed = valid.clone();
        changed.replace_machine_word(cache.address, cache.word & 0xfffff);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        let other = valid.record.entries.iter().find(|entry| entry.name == "revision").unwrap();
        let other = valid.record.blocks.iter().find(|block| block.id == other.entry_block).unwrap();
        assert_code(&redirect(&valid, call.address, other), CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
        let incoming = elf.control_flow.iter().find(|edge| edge.target == start && !owned(edge.address)).unwrap();
        let body =
            valid.record.blocks.iter().find(|block| block.owner_entry == entry.id && block.lowering_block_id.is_some()).unwrap();
        // The global stack contract rejects this prologue bypass before the
        // forwarding body's independent partial-entry check is reached.
        assert_code(&redirect(&valid, incoming.address, body), CheckerRejectionCode::V2407AbiOrStackInvalid);
        let adjustments = elf.stack_adjustments.iter().filter(|adjustment| owned(adjustment.address)).collect::<Vec<_>>();
        assert_eq!(adjustments.len(), 2);
        let mut replacements = Vec::new();
        for adjustment in adjustments {
            let adjusted = adjustment.delta + if adjustment.delta < 0 { -16 } else { 16 };
            let upper = (adjusted + 0x800) >> 12;
            let lower = adjusted - (upper << 12);
            for address in [adjustment.address - 8, adjustment.address - 4] {
                assert_eq!(elf.instructions.iter().find(|word| word.address == address).unwrap().word >> 7 & 31, 31);
            }
            replacements.push((adjustment.address - 8, ((upper as u32) << 12) | (31 << 7) | 0x37));
            replacements.push((adjustment.address - 4, ((lower as u32 & 4095) << 20) | (31 << 15) | (31 << 7) | 0x13));
        }
        let mut changed = valid.clone();
        changed.record.entries.iter_mut().find(|candidate| candidate.id == entry.id).unwrap().frame_size_bytes += 16;
        for block in &mut changed.record.blocks {
            if block.owner_entry == entry.id {
                block.frame_size_bytes += 16;
            }
        }
        changed.replace_machine_words(&replacements);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}
