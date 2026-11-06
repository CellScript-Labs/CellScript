use super::*;

fn options(diagnostics: cellscript::CompilerDiagnostics) -> CompileOptions {
    CompileOptions {
        edition: CURRENT_EDITION,
        diagnostics,
        target: Some("riscv64-elf".into()),
        target_profile: Some("ckb".into()),
        ..Default::default()
    }
}

fn source(helper: bool, memory: bool) -> String {
    let direct = include_str!("../fixtures/document_config_script_hash.cell");
    if memory {
        let source = direct
            .replace("let descriptor = script::new(code, 4, script::args(issuer))", "let digest = ckb::hash_data_packed(code)")
            .replace("script::hash(descriptor) == expected", "digest == issuer");
        return if helper {
            source
                .replace("action verify", "fn digest(code: Hash) -> Hash {\nreturn ckb::hash_data_packed(code)\n}\naction verify")
                .replace("let digest = ckb::hash_data_packed(code)", "let digest = digest(code)")
        } else {
            source
        };
    }
    if helper {
        return direct
            .replace("action verify", "fn descriptor_hash(code: Hash, issuer: Hash) -> ScriptHash {\nreturn script::hash(script::new(code, 4, script::args(issuer)))\n}\naction verify")
            .replace("let descriptor = script::new(code, 4, script::args(issuer))", "let complete = descriptor_hash(code, issuer)")
            .replace("script::hash(descriptor) == expected", "complete == expected");
    }
    direct.to_string()
}

#[test]
fn typed_document_composes_with_complete_script_and_memory_hashes() {
    use cellscript::EntryWitnessArg as A;
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (helper, memory) in [(false, false), (true, false), (false, true), (true, true)] {
            let compiled = cellscript::compile_with_executable_surface_policy(
                &source(helper, memory),
                options(diagnostics),
                cellscript::ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("helper={helper} memory={memory} {diagnostics}: {error:?}"));
            let code = [0x31; 32];
            let issuer = if memory { cellscript_artifact_checker::ckb_blake2b256(&code) } else { [0x41; 32] };
            let script =
                packed::Script::new_builder().code_hash(code.pack()).hash_type(4u8).args(Bytes::from(issuer.to_vec()).pack()).build();
            let expected: [u8; 32] = script.calc_script_hash().unpack();
            for valid in [true, false] {
                let mut expected = expected;
                let mut supplied_issuer = issuer;
                if !valid {
                    if memory {
                        supplied_issuer[0] ^= 1;
                    } else {
                        expected[0] ^= 1;
                    }
                }
                let payload = compiled.metadata.actions[0]
                    .entry_witness_args(&[A::Hash(code), A::Hash(supplied_issuer), A::Hash(expected)])
                    .unwrap();
                let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                transaction.current_type_script_input_indices = vec![0];
                transaction.inputs[0].data = 7u64.to_le_bytes().to_vec().into();
                transaction.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                let execution = ckb_script_runner::execute_cellscript_script(
                    cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
                    &transaction,
                );
                assert_eq!(execution.exit_code, if valid { 7 } else { 5 }, "helper={helper} memory={memory} {diagnostics}");
            }
            Fixture::from_result(compiled);
        }
    }
}

#[test]
fn memory_hash_empty_and_maximum_script_args_use_the_exact_read_extent() {
    use cellscript::EntryWitnessArg as A;
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (memory, width) in [(false, 0), (false, 459), (true, 1), (true, 459)] {
            let args = "x".repeat(width);
            let source = if memory {
                source(false, true).replace("ckb::hash_data_packed(code)", &format!("ckb::hash_data_packed(b\"{args}\")"))
            } else {
                let args_expression =
                    if width == 0 { "script::args_empty()".to_string() } else { format!("script::args(b\"{args}\")") };
                source(false, false).replace("script::args(issuer)", &args_expression)
            };
            let compiled = compile(&source, options(diagnostics)).unwrap();
            let code = [0x31; 32];
            let script = packed::Script::new_builder()
                .code_hash(code.pack())
                .hash_type(4u8)
                .args(Bytes::from(args.clone().into_bytes()).pack())
                .build();
            let expected: [u8; 32] = script.calc_script_hash().unpack();
            let issuer = cellscript_artifact_checker::ckb_blake2b256(args.as_bytes());
            let payload =
                compiled.metadata.actions[0].entry_witness_args(&[A::Hash(code), A::Hash(issuer), A::Hash(expected)]).unwrap();
            let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            transaction.current_type_script_input_indices = vec![0];
            transaction.inputs[0].data = 7u64.to_le_bytes().to_vec().into();
            transaction.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            let valid = Fixture::from_result(compiled);
            assert_eq!(
                ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code,
                7,
                "width={width} memory={memory} {diagnostics}"
            );
        }
    }
}

fn call(valid: &Fixture, name: &str) -> (cellscript_artifact_checker::ParsedElf, u64, String) {
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let callee = valid.record.entries.iter().find(|entry| entry.name == name).unwrap();
    let start = valid.record.blocks.iter().find(|block| block.id == callee.entry_block).unwrap().range.start;
    let edge = elf
        .control_flow
        .iter()
        .find(|edge| {
            edge.target == start
                && valid.record.blocks.iter().any(|block| {
                    block.range.contains(edge.address)
                        && valid.record.typed_semantics.entries.iter().any(|entry| entry.id == block.owner_entry)
                })
        })
        .unwrap();
    let owner = valid.record.blocks.iter().find(|block| block.range.contains(edge.address)).unwrap().owner_entry.clone();
    let pc = edge.address;
    (elf, pc, owner)
}

fn definition(elf: &cellscript_artifact_checker::ParsedElf, before: u64, register: u32) -> u64 {
    elf.instructions
        .iter()
        .rev()
        .find(|instruction| {
            instruction.address < before
                && instruction.word >> 7 & 31 == register
                && matches!(instruction.word & 0x7f, 3 | 0x13 | 0x17 | 0x1b | 0x33 | 0x37 | 0x3b)
        })
        .unwrap()
        .address
}

fn addi(rd: u32, rs: u32, immediate: i64) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 4095) << 20) | (rs << 15) | (rd << 7) | 0x13
}

#[test]
fn memory_hash_callers_reject_rebound_input_lengths_pointers_and_result_extents() {
    for memory in [false, true] {
        let valid =
            Fixture::from_result(compile(&source(false, memory), options(cellscript::CompilerDiagnostics::default())).unwrap());
        let (elf, pc, owner) = call(&valid, if memory { "__ckb_hash_blake2b_var" } else { "__ckb_script_hash" });
        let output = if memory { 12 } else { 14 };
        let frame = valid.record.entries.iter().find(|entry| entry.id == owner).unwrap().frame_size_bytes;
        let mutations = if memory {
            vec![(10, 0, 0), (11, 0, -1), (11, 0, 0), (11, 0, 33), (output, 2, i64::from(frame) - 16)]
        } else {
            vec![(10, 0, 0), (12, 0, 0), (13, 0, 0), (13, 0, 33), (13, 0, 460), (output, 2, i64::from(frame) - 16)]
        };
        for (register, base, immediate) in mutations {
            let mut changed = valid.clone();
            changed.replace_machine_word(definition(&elf, pc, register), addi(register, base, immediate));
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
    let valid = Fixture::from_result(compile(&source(true, true), options(cellscript::CompilerDiagnostics::default())).unwrap());
    let (elf, pc, _) = call(&valid, "digest");
    for (register, immediate) in [(10, 0), (11, 33)] {
        let mut changed = valid.clone();
        changed.replace_machine_word(definition(&elf, pc, register), addi(register, 0, immediate));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn memory_hash_outputs_preserve_live_nested_cell_storage() {
    for memory in [false, true] {
        let source = source(false, memory)
            .replace("revision: u64,", "revision: u64, messages: NestedValues<Vec<u8>, 2, 8, 32>,")
            .replace(
                "let revision = ckb::input<Document>(0).data.revision",
                "let data = ckb::input<Document>(0).data\nlet revision = data.revision\nlet messages = data.messages",
            )
            .replace("return revision", "require messages.len() == 1\nreturn revision")
            .replace("ckb::hash_data_packed(code)", "ckb::hash_data_packed(b\"ckb!\")");
        let valid = Fixture::from_result(compile(&source, options(cellscript::CompilerDiagnostics::default())).unwrap());
        let (elf, pc, owner) = call(&valid, if memory { "__ckb_hash_blake2b_var" } else { "__ckb_script_hash" });
        let output = if memory { 12 } else { 14 };
        let protected = valid
            .record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.id == owner)
            .unwrap()
            .locals
            .iter()
            .find(|local| local.ty.starts_with("NestedValues<"))
            .unwrap()
            .source_id as i64
            * 8;
        let mut changed = valid.clone();
        changed.replace_machine_word(definition(&elf, pc, output), addi(output, 2, protected));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn memory_hash_complete_dependency_bodies_reject_rebound_writes_and_call_targets() {
    let valid = Fixture::from_result(compile(&source(false, false), options(cellscript::CompilerDiagnostics::default())).unwrap());
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    for name in ["__ckb_hash_blake2b_var", "__cellscript_blake2b_compress"] {
        let owner = &valid.record.entries.iter().find(|entry| entry.name == name).unwrap().id;
        let store = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.word & 0x7f == 0x23
                    && valid.record.blocks.iter().any(|block| block.owner_entry == *owner && block.range.contains(instruction.address))
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(store.address, store.word ^ (1 << 7));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
    let (_, pc, _) = call(&valid, "__ckb_script_hash");
    let target = valid.record.entries.iter().find(|entry| entry.name == "__ckb_hash_blake2b_var").unwrap();
    let target_block = valid.record.blocks.iter().find(|block| block.id == target.entry_block).unwrap();
    let displacement = target_block.range.start as i64 - (pc - 4) as i64;
    let upper = (displacement + 0x800) >> 12;
    let lower = displacement - (upper << 12);
    let mut changed = valid.clone();
    let from = changed.record.blocks.iter().find(|block| block.range.contains(pc)).unwrap().id.clone();
    changed.record.edges.iter_mut().find(|edge| edge.from == from && edge.kind == EdgeKind::Call).unwrap().to =
        target_block.id.clone();
    changed.replace_machine_words(&[
        (pc - 4, ((upper as u32) << 12) | (1 << 7) | 0x17),
        (pc, ((lower as u32 & 4095) << 20) | (1 << 15) | (1 << 7) | 0x67),
    ]);
    // Rebind sidecar edges and digests as well as instructions. The actual
    // callee's input/output ABI must still reject the redirected call.
    assert!(changed.check().is_err());

    let compression = valid.record.entries.iter().find(|entry| entry.name == "__cellscript_blake2b_compress").unwrap();
    let compression_start = valid.record.blocks.iter().find(|block| block.id == compression.entry_block).unwrap().range.start;
    let pc = elf
        .control_flow
        .iter()
        .find(|edge| {
            edge.target == compression_start
                && valid.record.blocks.iter().any(|block| block.range.contains(edge.address) && block.owner_entry == target.id)
        })
        .unwrap()
        .address;
    let displacement = target_block.range.start as i64 - (pc - 4) as i64;
    let upper = (displacement + 0x800) >> 12;
    let lower = displacement - (upper << 12);
    let mut changed = valid.clone();
    let from = changed.record.blocks.iter().find(|block| block.range.contains(pc)).unwrap().id.clone();
    changed.record.edges.iter_mut().find(|edge| edge.from == from && edge.kind == EdgeKind::Call).unwrap().to =
        target_block.id.clone();
    changed.replace_machine_words(&[
        (pc - 4, ((upper as u32) << 12) | (1 << 7) | 0x17),
        (pc, ((lower as u32 & 4095) << 20) | (1 << 15) | (1 << 7) | 0x67),
    ]);
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}
