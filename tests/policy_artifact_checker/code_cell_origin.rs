#[test]
fn code_cell_origin_has_exact_script_count_and_preparse_byte_bounds() {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let fixture = external_fixture(EXTERNAL_SOURCE, cellscript::CompilerDiagnostics::default());
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let code = packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock.clone()).build();
    let code_hash = packed::CellOutput::calc_data_hash(&fixture.artifact);
    let script = lock.clone().as_builder().code_hash(code_hash.clone()).args(Bytes::from(vec![0xCC; 4043]).pack()).build();
    assert_eq!(script.as_slice().len(), 4096);
    for count in [256, 257] {
        let mut tx = TransactionBuilder::default().output(code.clone()).output_data(Bytes::from(fixture.artifact.clone()).pack());
        for _ in 1..count {
            tx = tx.output(code.clone()).output_data(Bytes::new().pack());
        }
        let tx = tx.build();
        let result = code_origin(&fixture, tx.data().raw().as_slice(), 0, script.as_slice());
        if count == 256 {
            result.unwrap();
        } else {
            assert!(result.unwrap_err().message.contains("dynvec count"));
        }
    }
    let tx = TransactionBuilder::default().output(code.clone()).output_data(Bytes::from(fixture.artifact.clone()).pack()).build();
    let too_long = script.as_builder().args(Bytes::from(vec![0xCC; 4044]).pack()).build();
    assert_eq!(too_long.as_slice().len(), 4097);
    assert!(code_origin(&fixture, tx.data().raw().as_slice(), 0, too_long.as_slice()).unwrap_err().message.contains("Script exceeds"));
    let selected = lock.as_builder().code_hash(code_hash).build();
    let budgets = CheckerBudgets { record_bytes: 1, ..CheckerBudgets::default() };
    let error = cellscript_artifact_checker::code_origin::check_code_cell_origin(
        [
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
        ],
        tx.data().raw().as_slice(),
        0,
        selected.as_slice(),
        &budgets,
    )
    .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
    for budgets in [
        CheckerBudgets { artifact_bytes: 0, ..CheckerBudgets::default() },
        CheckerBudgets { record_bytes: 0, ..CheckerBudgets::default() },
        CheckerBudgets { source_map_bytes: 0, ..CheckerBudgets::default() },
    ] {
        // Malformed raw bytes must never reach a parser with an over-budget
        // source file, even when that file would be parsed in a later phase.
        let error = cellscript_artifact_checker::code_origin::check_code_cell_origin(
            [
                &fixture.artifact,
                &serde_json::to_vec(&fixture.metadata).unwrap(),
                &serde_json::to_vec(&fixture.record).unwrap(),
                &serde_json::to_vec(&fixture.source_map).unwrap(),
            ],
            &[0],
            0,
            &[],
            &budgets,
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
    }
    let exactly = vec![b'?'; 4 * 1024 * 1024];
    let error = cellscript_artifact_checker::code_origin::check_code_cell_origin(
        [exactly.as_slice(); 4],
        &[1],
        0,
        &[],
        &CheckerBudgets::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
    let oversized = vec![b'?'; 4 * 1024 * 1024 + 1];
    let error = cellscript_artifact_checker::code_origin::check_code_cell_origin(
        [oversized.as_slice(), &[], &[], &[]],
        &[],
        0,
        &[],
        &CheckerBudgets::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
}

#[test]
fn code_cell_origin_rejects_duplicate_and_overbound_dependency_input_sets() {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let fixture = external_fixture(EXTERNAL_SOURCE, cellscript::CompilerDiagnostics::default());
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let code = packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock.clone()).build();
    let selected = lock.as_builder().code_hash(packed::CellOutput::calc_data_hash(&fixture.artifact)).build();
    let base = || TransactionBuilder::default().output(code.clone()).output_data(Bytes::from(fixture.artifact.clone()).pack());
    for (kind, maximum) in [("headers", 64usize), ("deps", 64), ("inputs", 256)] {
        for count in [maximum, maximum + 1] {
            let mut tx = base();
            for index in 0..count {
                let mut bytes = [7u8; 32];
                bytes[..4].copy_from_slice(&(index as u32).to_le_bytes());
                let point = packed::OutPoint::new_builder().tx_hash(bytes.pack()).index(3u32).build();
                tx = match kind {
                    "headers" => tx.header_dep(bytes.pack()),
                    "deps" => tx.cell_dep(packed::CellDep::new_builder().out_point(point).dep_type((index % 2) as u8).build()),
                    _ => tx.input(packed::CellInput::new_builder().previous_output(point).build()),
                };
            }
            let tx = tx.build();
            let result = code_origin(&fixture, tx.data().raw().as_slice(), 0, selected.as_slice());
            if count == maximum {
                result.unwrap();
            } else {
                assert!(result.unwrap_err().message.contains("fixvec count/width"));
            }
        }
    }
    let point = packed::OutPoint::new_builder().tx_hash([8u8; 32].pack()).index(3u32).build();
    let dep = packed::CellDep::new_builder().out_point(point.clone()).dep_type(0u8).build();
    let input = packed::CellInput::new_builder().previous_output(point.clone()).build();
    let cases = [
        (base().cell_dep(dep.clone()).cell_dep(dep).build(), "duplicate or unsupported raw CellDep"),
        (base().input(input.clone()).input(input).build(), "duplicate input OutPoint"),
        (base().header_dep([8u8; 32].pack()).header_dep([8u8; 32].pack()).build(), "duplicate header dependency"),
        (base().cell_dep(packed::CellDep::new_builder().out_point(point).dep_type(2u8).build()).build(), "unsupported raw CellDep"),
    ];
    for (tx, message) in cases {
        assert!(code_origin(&fixture, tx.data().raw().as_slice(), 0, selected.as_slice()).unwrap_err().message.contains(message));
    }
    let selected = selected.as_builder().hash_type(1u8).build();
    let tx = base().build();
    assert!(code_origin(&fixture, tx.data().raw().as_slice(), 0, selected.as_slice())
        .unwrap_err()
        .message
        .contains("no actual code Cell Type Script"));
}

fn fixed_cell_reads(fixture: &Fixture) -> Result<cellscript_artifact_checker::fixed_cell_reads::CheckedFixedCellReads, CheckerError> {
    cellscript_artifact_checker::fixed_cell_reads::check_fixed_cell_reads(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets::default(),
    )
}

fn fixed_cell_storage(
    fixture: &Fixture,
) -> Result<cellscript_artifact_checker::fixed_cell_storage::CheckedFixedCellParameterStorage, CheckerError> {
    cellscript_artifact_checker::fixed_cell_storage::check_fixed_cell_parameter_storage(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets::default(),
    )
}

fn fixed_cell_fields(
    fixture: &Fixture,
) -> Result<cellscript_artifact_checker::fixed_cell_fields::CheckedFixedCellScalarFields, CheckerError> {
    cellscript_artifact_checker::fixed_cell_fields::check_fixed_cell_scalar_fields(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets::default(),
    )
}

#[test]
fn fixed_cell_fields_decode_direct_unsigned_layouts_and_reject_rebound_bytes() {
    let mut coverage = Vec::new();
    let mut mutations = 0;
    for (members, predicate, widths) in [
        ("amount: u64", "require token.amount > 0;", vec![8]),
        (
            "a: u8, b: u16, c: u32, d: u64",
            "require token.a == 42; require token.b == 513; require token.c == 67305985; require token.d == 123456789012345;",
            vec![1, 2, 4, 8],
        ),
    ] {
        let source = SOURCE.replace("amount: u64, }", &format!("{members}, }}"));
        // The unselected mint is removed because its resource initializer belongs
        // to the original amount layout, independent of field checker profiles.
        let source = source.split("action mint(").next().unwrap().to_owned()
            + &format!("action burn(input token: Token) {{  {predicate} consume token; }}\n");
        let mut selected = declaration();
        selected.actions.retain(|action| action.action == "burn");
        selected.common_checks.clear();
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt, selected.clone());
            let checked = fixed_cell_fields(&fixture).unwrap();
            let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            let fields = value["fields"].as_array().unwrap();
            coverage.push(serde_json::json!({"diagnostics":opt.to_string(),"members":members,"sites":fields.len()}));
            let actual = fields.iter().map(|field| field["width"].as_u64().unwrap()).collect::<std::collections::BTreeSet<_>>();
            assert_eq!(actual, widths.iter().map(|width| *width as u64).collect());
            let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
            for field in fields {
                let start = field["pointer_load"].as_u64().unwrap();
                let first = elf.instructions.iter().find(|instruction| instruction.address == start + 4).unwrap().word;
                let source = if first & 0x7f == 0x03 { start + 4 } else { start + 8 };
                let load = elf.instructions.iter().find(|instruction| instruction.address == source).unwrap().word;
                for (mask, bits) in [(31 << 15, 28 << 15), (0xfff << 20, (((load >> 20) + 1) & 0xfff) << 20)] {
                    let mut changed = fixture.clone();
                    changed.replace_machine_word(source, (load & !mask) | bits);
                    changed.check().unwrap();
                    fixed_cell_storage(&changed).unwrap();
                    assert_eq!(fixed_cell_fields(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    mutations += 1;
                }
                if first & 0x7f != 0x03 {
                    let mut locations = vec![(start + 4, 0xfff << 20, 1 << 20), (start + 12, 31 << 20, 6 << 20)];
                    if field["width"].as_u64().unwrap() > 1 {
                        locations.push((start + 20, 63 << 20, 16 << 20));
                    }
                    for (address, mask, bits) in locations {
                        let original = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
                        let mut changed = fixture.clone();
                        changed.replace_machine_word(address, (original & !mask) | bits);
                        changed.check().unwrap();
                        fixed_cell_storage(&changed).unwrap();
                        assert_eq!(
                            fixed_cell_fields(&changed).unwrap_err().code,
                            CheckerRejectionCode::V2420TypedMachineBindingInvalid
                        );
                        mutations += 1;
                    }
                }
            }
        }
    }
    println!("{}", serde_json::json!({"field_coverage":coverage,"rebound_field_mutations":mutations}));
}

#[test]
fn fixed_cell_fields_reject_membership_helper_memory_and_register_substitutions() {
    let source = SOURCE.replace("consume token;", "require token.amount > 0; consume token;");
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt, selected.clone());
        fixed_cell_fields(&fixture).unwrap();
        let blocks = fixture
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == "runtime:__cellscript_require_cell_membership")
            .collect::<Vec<_>>();
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let syscall =
            elf.syscall_addresses.iter().copied().find(|address| blocks.iter().any(|block| block.range.contains(*address))).unwrap();
        let buffer = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < syscall
                    && instruction.word & 0x7f == 0x13
                    && ((instruction.word >> 7) & 31) == 10
                    && ((instruction.word >> 15) & 31) == 2
            })
            .unwrap();
        let store = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < buffer.address
                    && instruction.word & 0x7f == 0x23
                    && ((instruction.word >> 15) & 31) == 2
                    && (((instruction.word >> 25) << 5) | ((instruction.word >> 7) & 31)) == 0
            })
            .unwrap();
        let capacity = elf.instructions.iter().find(|instruction| instruction.address == store.address - 4).unwrap();
        for (address, word) in [
            (capacity.address, (capacity.word & !(0xfff << 20)) | (64 << 20)),
            (capacity.address, (capacity.word & !(31 << 7)) | (29 << 7)),
            (store.address, (store.word & !(31 << 15)) | (3 << 15)),
            (store.address, store.word & !(7 << 12)),
            (store.address, (store.word & !(31 << 7)) | (1 << 7)),
            (buffer.address, (buffer.word & !(31 << 15)) | (3 << 15)),
        ] {
            let mut changed = fixture.clone();
            changed.replace_machine_word(address, word);
            changed.check().unwrap();
            fixed_cell_storage(&changed).unwrap();
            assert_eq!(fixed_cell_fields(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn fixed_cell_fields_accept_rebound_native_u64_and_reject_unknown_field_profiles() {
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    let source = SOURCE.replace("consume token;", "require token.amount == 7; consume token;");
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let mut fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt, selected.clone());
        let bytes = fixed_cell_fields(&fixture).unwrap().canonical_bytes().unwrap();
        let record: Value = serde_json::from_slice(&bytes).unwrap();
        let field = &record["fields"][0];
        let load = field["pointer_load"].as_u64().unwrap() + 4;
        let end = field["decode_end"].as_u64().unwrap();
        let register = field["register"].as_u64().unwrap() as u32;
        // An alternate actual machine form, not a claim that this fixture's
        // compiler emitted LD. Rebind every outer identity after padding out
        // the original reconstruction, preserving addresses and its consumers.
        fixture.replace_machine_word(load, (29 << 15) | (3 << 12) | (register << 7) | 0x03);
        for address in (load + 4..end).step_by(4) {
            fixture.replace_machine_word(address, 0x00000013);
        }
        fixture.check().unwrap();
        let checked = fixed_cell_fields(&fixture).unwrap();
        let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(record["fields"][0]["native_load"], true);
        assert!(!checked.storage().reads().module_projection().artifact_report().semantic_equivalence_claimed);
        for replacement in
            [(8 << 20) | (29 << 15) | (3 << 12) | (register << 7) | 0x03, (29 << 15) | (4 << 12) | (register << 7) | 0x03]
        {
            let mut changed = fixture.clone();
            changed.replace_machine_word(load, replacement);
            changed.check().unwrap();
            fixed_cell_storage(&changed).unwrap();
            assert_eq!(fixed_cell_fields(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let signed = "module signed_cell_fields;\nresource Token has store, consume { signed: i32, }\naction burn(input token: Token, witness zero: i32) {  require token.signed < zero; consume token; }";
        let fixture = Fixture::new_source_with(signed, CellScriptEdition::Edition2027, opt, selected.clone());
        fixed_cell_storage(&fixture).unwrap();
        assert_eq!(fixed_cell_fields(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        // Fixed nested projections are executable, but the optional direct
        // unsigned-scalar field profile must still reject aggregate fields.
        let nested = "module nested_cell_fields;\nstruct Inner has copy, drop, store, fixed, serializable, non_linear { x: u64, }\nresource Token has store, consume { inner: Inner, }\naction burn(input token: Token) {  require token.inner.x > 0; consume token; }";
        let fixture = Fixture::new_source_with(nested, CellScriptEdition::Edition2027, opt, selected.clone());
        fixed_cell_storage(&fixture).unwrap();
        assert_eq!(fixed_cell_fields(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}
