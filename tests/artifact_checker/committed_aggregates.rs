use super::*;

#[test]
fn commitment_helper_results_require_owned_copy_extent_source_and_domain() {
    let source = "module owned_commitment;\nfn build(value: u64) -> Commitment<u64> { return commitment::commit(value); }\naction verify(witness opening: Opening<u64>) -> u64 {  let expected = build(7); let value = commitment::open(expected, opening); require value == 7; return 0; }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        valid.check().unwrap();
        for mutation in 0..4 {
            let mut changed = valid.clone();
            let entry = changed.record.entries.iter_mut().find(|entry| entry.name == "build").unwrap();
            match mutation {
                0 => entry.fixed_result_abi = None,
                1 => entry.fixed_result_abi.as_mut().unwrap().width_bytes = 31,
                2 => entry.fixed_result_abi.as_mut().unwrap().copy_sources.clear(),
                _ => entry.fixed_result_abi.as_mut().unwrap().type_name = "Commitment<u32>".into(),
            }
            changed.rebind_sidecars();
            assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);
        }
        let abi = valid.record.entries.iter().find(|entry| entry.name == "build").unwrap().fixed_result_abi.as_ref().unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let extent = elf
            .instructions
            .iter()
            .find(|instruction| {
                abi.copy_ranges.iter().any(|range| range.contains(instruction.address)) && instruction.word == 0x0200_0613
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(extent.address, 0x01f0_0613);
        assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);
    }
}

#[test]
fn packed_hash_case_is_independently_bound_after_all_sidecar_hashes_are_rebound() {
    fn replace(value: &mut Value, from: &str, to: &str) {
        match value {
            Value::String(text) if text == from => *text = to.into(),
            Value::Array(values) => values.iter_mut().for_each(|value| replace(value, from, to)),
            Value::Object(values) => values.values_mut().for_each(|value| replace(value, from, to)),
            _ => {}
        }
    }
    for ty in ["Hash", "Address", "[Hash; 1]", "(Hash, ())"] {
        let source = format!("module packed_case_binding;\naction verify(witness expected: Commitment<{ty}>, witness opening: Opening<{ty}>) -> u64 {{  let value = commitment::open(expected, opening); let actual = commitment::commit(value); require actual == expected; return 0; }}");
        let valid = Fixture::from_source(&source);
        valid.check().unwrap();
        let mut changed = valid.clone();
        let mut typed = serde_json::to_value(&changed.record.typed_semantics).unwrap();
        for wrapper in ["Commitment", "Opening"] {
            replace(&mut typed, &format!("{wrapper}<{ty}>"), &format!("{wrapper}<{}>", ty.to_lowercase()));
        }
        changed.record.typed_semantics = serde_json::from_value(typed).unwrap();
        changed.rebind_layered_identities();
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{ty}: {error:?}");
    }
}

#[test]
fn committed_aggregates_bind_width_domain_authentication_and_materialized_source() {
    for (ty, width, predicate) in [("[u8; 16]", 16u32, "value[0] == 7"), ("(u64, [u8; 2])", 10u32, "value.0 == 7")] {
        let source = format!("module checked_committed_aggregate;\naction verify(witness expected: Commitment<{ty}>, witness opening: Opening<{ty}>) -> u64 {{  let value = commitment::open(expected, opening); require {predicate}; return 0; }}");
        let valid = Fixture::from_source(&source);
        assert!(valid.check().is_ok());
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let marker = |prefix: &str| {
            valid
                .record
                .blocks
                .iter()
                .find(|block| {
                    block.owner_entry == "action:verify"
                        && block.machine_label.as_deref().is_some_and(|label| label.starts_with(prefix))
                })
                .unwrap()
        };
        let opening = marker(".Lcommitment_opening_header_ready_");
        let verified = marker(".Lcommitment_opening_verified_");
        let guard = elf
            .instructions
            .iter()
            .find(|instruction| {
                opening.range.start <= instruction.address
                    && instruction.address < verified.range.start
                    && instruction.word & 0x000f_ffff == 0x0000_0593
                    && instruction.word >> 20 == width
            })
            .expect("fixed-width size guard");
        let mut changed = valid.clone();
        changed.replace_machine_word(guard.address, (guard.word & 0x000f_ffff) | ((width + 1) << 20));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let domain = elf
            .instructions
            .iter()
            .find(|instruction| instruction.address < opening.range.start && instruction.word == 0x0430_0293)
            .expect("first packed domain byte");
        let mut changed = valid.clone();
        changed.replace_machine_word(domain.address, domain.word ^ (1 << 20));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let compare = elf
            .control_flow
            .iter()
            .find(|flow| {
                opening.range.start <= flow.address && flow.address < verified.range.start && flow.target == verified.range.start
            })
            .unwrap();
        let branch = elf.instructions.iter().find(|instruction| instruction.address == compare.address).unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(branch.address, branch.word ^ (1 << 12));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let source_pointer = elf
            .instructions
            .iter()
            .find(|instruction| {
                verified.range.start <= instruction.address
                    && instruction.address < verified.range.end
                    && instruction.word & 0x000f_ffff == if ty == "u64" { 0x0001_3e83 } else { 0x0001_3503 }
            })
            .expect("post-authentication opening pointer load");
        let mut changed = valid.clone();
        changed.replace_machine_word(source_pointer.address, source_pointer.word & !(31 << 15));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        if ty == "u64" {
            let byte = elf
                .instructions
                .iter()
                .find(|instruction| {
                    verified.range.start <= instruction.address
                        && instruction.address < verified.range.end
                        && instruction.word == 0x000e_c303
                })
                .expect("scalar opening first byte");
            let mut changed = valid.clone();
            changed.replace_machine_word(byte.address, byte.word | (1 << 20));
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        } else {
            let extent = elf
                .instructions
                .iter()
                .find(|instruction| {
                    verified.range.start <= instruction.address
                        && instruction.address < verified.range.end
                        && instruction.word & 0x000f_ffff == 0x0000_0613
                        && instruction.word >> 20 == width
                })
                .expect("authenticated result copy width");
            let mut changed = valid.clone();
            changed.replace_machine_word(extent.address, (extent.word & 0x000f_ffff) | ((width + 1) << 20));
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn authenticated_array_copy_cannot_substitute_digest_bytes_for_the_opening() {
    use ckb_testtool::{
        ckb_hash::blake2b_256,
        ckb_types::{bytes::Bytes, packed, prelude::*},
    };
    let source = "module committed_payload_binding;\naction verify(witness expected: Commitment<[u8; 16]>, witness opening: Opening<[u8; 16]>) -> u64 {  let value = commitment::open(expected, opening); require value[0] == 7; return 0; }";
    let valid = Fixture::from_source(source);
    let opening = hex::decode("08000000000000004800000000000000").unwrap();
    let mut preimage = b"CellScriptPackedHashV0\0[u8; 16]\0".to_vec();
    preimage.extend_from_slice(&16u32.to_le_bytes());
    preimage.extend_from_slice(&opening);
    let expected = blake2b_256(&preimage);
    assert_eq!(opening[0], 8);
    assert_eq!(expected[0], 7);
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend_from_slice(&expected);
    payload.extend_from_slice(&16u32.to_le_bytes());
    payload.extend_from_slice(&opening);
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    assert!(valid.check().is_ok());
    assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &fixture).exit_code, 5);
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let verified = valid
        .record
        .blocks
        .iter()
        .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lcommitment_opening_verified_")))
        .unwrap();
    let pointer = elf
        .instructions
        .iter()
        .find(|instruction| verified.range.contains(instruction.address) && instruction.word & 0x000f_ffff == 0x0001_3503)
        .unwrap();
    let mut changed = valid.clone();
    // Both pointers are valid. Redirect the materialization load to the first
    // parameter's digest, while authenticating the original opening unchanged.
    changed.replace_machine_word(pointer.address, pointer.word & 0x000f_ffff);
    assert_eq!(ckb_script_runner::execute_cellscript_script(&changed.artifact, &fixture).exit_code, 0);
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    if let Some(output) = std::env::var_os("CELLSCRIPT_MATERIALIZATION_POC_OUTPUT") {
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("main.cell"), source).unwrap();
        std::fs::write(output.join("main.elf"), &changed.artifact).unwrap();
        std::fs::write(output.join("main.elf.meta.json"), canonical_bytes(&changed.metadata).unwrap()).unwrap();
        std::fs::write(output.join("main.elf.lowering.json"), canonical_bytes(&changed.record).unwrap()).unwrap();
        std::fs::write(output.join("main.elf.sourcemap.json"), canonical_bytes(&changed.source_map).unwrap()).unwrap();
    }
}
