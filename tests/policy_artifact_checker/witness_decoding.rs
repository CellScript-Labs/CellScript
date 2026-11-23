#[test]
fn fixed_external_codec_checks_complete_available_public_scalar_profile() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for ty in ["u8", "u16", "u32", "u64", "i32"] {
            let source = if ty == "i32" {
                EXTERNAL_SOURCE
                    .replace("witness value: u64", "witness value: i32, witness zero: i32")
                    .replace("value > 0", "value > zero")
            } else {
                EXTERNAL_SOURCE.replace("witness value: u64", &format!("witness value: {ty}"))
            };
            let fixture = external_fixture(&source, opt);
            let checked = fixed_external_codec(&fixture).unwrap();
            let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            assert_eq!(record["callables"][0]["availability"], "external-policy-entry");
            assert_eq!(record["public_layouts"][0]["width"], 8);
            assert!(checked.fields().is_some());
            let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
            let entry = fixture.record.entries.iter().find(|entry| entry.name == "burn").unwrap();
            let address = fixture.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start + 24;
            let original = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
            assert_eq!((original >> 20) & 31, 12);
            for (mask, bits) in [(31 << 20, 17 << 20), (0x1f << 7, (original.wrapping_add(16 << 7)) & (0x1f << 7))] {
                let mut changed = fixture.clone();
                changed.replace_machine_word(address, (original & !mask) | bits);
                changed.check().unwrap();
                fixed_cell_fields(&changed).unwrap();
                assert_eq!(fixed_external_codec(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

fn single_entry_fixture(lock: bool, witness: bool, diagnostics: cellscript::CompilerDiagnostics) -> Fixture {
    let param = if witness { "witness value: u64" } else { "" };
    let predicate = if witness { "value > 7" } else { "true" };
    let body = if lock {
        format!("public lock verify({param}) -> bool {{  {predicate} }}")
    } else {
        format!("public action verify({param}) {{  require {predicate}; }}")
    };
    let result = cellscript::compile(
        &format!("module single_codec;\n{body}\n"),
        CompileOptions {
            source_contracts: true,
            target_profile: Some("ckb".into()),
            edition: CellScriptEdition::Edition2027,
            diagnostics,
            target: Some("riscv64-elf".into()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    Fixture {
        artifact: result.artifact_bytes,
        metadata: serde_json::to_value(result.metadata).unwrap(),
        record: result.verified_lowering_record.unwrap(),
        source_map: result.source_artifact_map.unwrap(),
    }
}

#[test]
fn single_entry_codec_proves_lock_and_action_witness_decoders() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for lock in [false, true] {
            for witness in [false, true] {
                let fixture = single_entry_fixture(lock, witness, opt);
                fixed_external_codec(&fixture).unwrap_or_else(|e| panic!("{opt} lock={lock} witness={witness}: {e}"));
            }
        }
    }
}

#[test]
fn single_entry_codec_rejects_rebound_envelope_and_argument_instructions() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = single_entry_fixture(true, true, opt);
        fixed_external_codec(&fixture).unwrap();
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        for (prefix, offset) in [
            (".Lentry_witness_buffer_ok_", 4),
            (".Lentry_witness_size_ok_", 4),
            (".Lentry_witness_exact_size_ok_", 4),
            (".Lentry_witness_v2_field_done_", 4),
        ] {
            let address = fixture
                .record
                .blocks
                .iter()
                .find(|b| b.machine_label.as_deref().is_some_and(|l| l.starts_with(prefix)))
                .unwrap()
                .range
                .start
                + offset;
            let mut changed = fixture.clone();
            let word = elf.instructions.iter().find(|i| i.address == address).unwrap().word;
            changed.replace_machine_word(address, word ^ (1 << 20));
            assert!(fixed_external_codec(&changed).is_err(), "{opt} mutation {prefix} admitted");
        }
    }
}
#[test]
fn fixed_external_codec_does_not_grant_pruned_helpers_outputs_or_unsupported_layouts() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for extra in [
            "public fn extra() { }",
            "public struct Extra { value: bool, }",
            "public struct Extra { values: [u64; 2], }",
            "public enum Extra { A, B, }",
        ] {
            let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\n{extra}"), opt);
            fixture.check().unwrap();
            assert_eq!(fixed_external_codec(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        for source in [
            "module output_external;\nresource Token has store, consume { amount: u64, }\naction burn(witness recipient: Address) {  create Token { amount: 7, } with_lock(recipient) }",
            "module multiple_external;\nresource Token has store, consume { amount: u64, }\naction burn(input left: Token, input right: Token) {  require left.amount > 0; require right.amount > 0; consume left; consume right; }",
        ] {
            let fixture = external_fixture(source, opt);
            assert_eq!(fixed_external_codec(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}"), opt);
        let checked = fixed_external_codec(&fixture).unwrap();
        let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert!(record["callables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|callable| callable["availability"] == "declaration-only" && callable["entry"].is_null()));
    }
}
#[test]
fn fixed_external_codec_generic_instances_are_not_declaration_only() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let declaration = "public struct Pair<T: fixed_value> { value: T, }";
        let absent = external_fixture(&format!("{EXTERNAL_SOURCE}\n{declaration}"), opt);
        let checked = fixed_external_codec(&absent).unwrap();
        let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert!(record["public_layouts"].as_array().unwrap().iter().any(|layout| layout["availability"] == "declaration-only"));
        let instantiated = EXTERNAL_SOURCE
            .replace("require value > 0", "let pair: Pair<u64> = new Pair<u64> { value: value, }; require pair.value > 0");
        let fixture = external_fixture(&format!("{instantiated}\n{declaration}"), opt);
        assert!(fixture
            .record
            .typed_semantics
            .instantiations
            .iter()
            .any(|instance| instance.kind == "struct" && instance.template == "Pair"));
        let error = fixed_external_codec(&fixture).unwrap_err();
        assert!(error.message.contains("generic concrete layout"), "{error:?}");
        let instantiated = EXTERNAL_SOURCE.replace("require value > 0", "require first::<u64>(value) > 0");
        let fixture = external_fixture(&format!("{instantiated}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}"), opt);
        if fixture
            .record
            .typed_semantics
            .instantiations
            .iter()
            .any(|instance| instance.kind == "function" && instance.template == "first")
        {
            let error = fixed_external_codec(&fixture).unwrap_err();
            assert!(error.message.contains("generic concrete instance"), "{error:?}");
        } else {
            // Inlining may erase the concrete helper and its instance record.
            // That leaves a symbolic declaration, never an external entry.
            assert!(fixture.record.typed_semantics.entries.iter().all(|entry| entry.kind != "function"));
            let checked = fixed_external_codec(&fixture).unwrap();
            let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            assert!(record["callables"].as_array().unwrap().iter().any(|callable| callable["declaration"] == "external_codec::first"
                && callable["availability"] == "declaration-only"
                && callable["entry"].is_null()));
        }
    }
}

#[test]
fn fixed_external_codec_binds_machine_bytes_without_claiming_predicate_equivalence() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let required = external_fixture(EXTERNAL_SOURCE, opt);
        let other = external_fixture(&EXTERNAL_SOURCE.replace("token.amount > 0", "token.amount == 7"), opt);
        let required_checked = fixed_external_codec(&required).unwrap();
        let other_checked = fixed_external_codec(&other).unwrap();
        required_checked
            .parameters()
            .module_projection()
            .check_required_contracts(other_checked.parameters().module_projection())
            .unwrap();
        assert_ne!(required_checked.identity(), other_checked.identity());
        let narrowed = external_fixture(&EXTERNAL_SOURCE.replace("amount: u64", "amount: u32"), opt);
        let narrowed_checked = fixed_external_codec(&narrowed).unwrap();
        assert!(required_checked
            .parameters()
            .module_projection()
            .check_required_contracts(narrowed_checked.parameters().module_projection())
            .is_err());
    }
}

fn receipt_bundle(fixture: &Fixture) -> [Vec<u8>; 4] {
    [
        fixture.artifact.clone(),
        serde_json::to_vec(&fixture.metadata).unwrap(),
        serde_json::to_vec(&fixture.record).unwrap(),
        serde_json::to_vec(&fixture.source_map).unwrap(),
    ]
}
fn receipt_deployment(fixture: &Fixture, args: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let type_script =
        packed::Script::new_builder().code_hash([8u8; 32].pack()).hash_type(1u8).args(Bytes::from(vec![4; 32]).pack()).build();
    let tx = TransactionBuilder::default()
        .input(
            packed::CellInput::new_builder()
                .previous_output(packed::OutPoint::new_builder().tx_hash([7u8; 32].pack()).index(3u32).build())
                .build(),
        )
        .output(
            packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock).type_(Some(type_script.clone()).pack()).build(),
        )
        .output_data(Bytes::from(fixture.artifact.clone()).pack())
        .build();
    let type_hash = fixture.metadata["target_profile"]["name"] == "ckb-type-hash";
    let selected = packed::Script::new_builder()
        .code_hash(if type_hash { type_script.calc_script_hash() } else { packed::CellOutput::calc_data_hash(&fixture.artifact) })
        .hash_type(if type_hash { 1u8 } else { 4u8 })
        .args(Bytes::from(args).pack())
        .build();
    (tx.data().raw().as_slice().to_vec(), selected.as_slice().to_vec())
}
fn fixed_policy_receipt(
    fixture: &Fixture,
    args: Vec<u8>,
) -> Result<cellscript_artifact_checker::fixed_policy_receipt::CheckedFixedPolicyReceipt, CheckerError> {
    let bytes = receipt_bundle(fixture);
    let (raw, script) = receipt_deployment(fixture, args);
    cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt(
        bytes.each_ref().map(Vec::as_slice),
        &raw,
        0,
        &script,
        &CheckerBudgets::default(),
    )
}
#[test]
fn fixed_policy_receipt_recomputes_complete_finite_fields_from_actual_inputs() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for target in ["ckb", "ckb-type-hash"] {
            let mut declaration = declaration();
            declaration.actions.retain(|action| action.action == "burn");
            declaration.common_checks.clear();
            let fixture = Fixture::new_source_with_target(EXTERNAL_SOURCE, CellScriptEdition::Edition2027, opt, declaration, target);
            let bytes = receipt_bundle(&fixture);
            let (raw, script) = receipt_deployment(&fixture, vec![1, 2, 3]);
            let checked = fixed_policy_receipt(&fixture, vec![1, 2, 3]).unwrap();
            checked.check_unchanged_inputs(bytes.each_ref().map(Vec::as_slice), &raw, 0, &script).unwrap();
            let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            assert_eq!(record["schema"], "cellscript-fixed-policy-interface-receipt-v3");
            assert_eq!(record["declared_interface"], fixture.metadata["public_interface"]);
            assert_eq!(
                record["entry_contract"],
                serde_json::to_value(&fixture.record.typed_semantics.foundation.entry_contract).unwrap()
            );
            assert_eq!(record["entry_contract"]["script_role"], "type");
            let origin = checked.target_origin().origin();
            assert_eq!(record["module_contract"], origin.codec().parameters().module_projection().identity());
            assert_eq!(record["external_codec"], origin.codec().identity());
            assert_eq!(record["code_origin"], origin.identity());
            assert_eq!(record["target_selection"], checked.target_origin().identity());
            for (index, input) in bytes.iter().enumerate() {
                assert_eq!(record["bundle_byte_lengths"][index], input.len());
                assert_eq!(record["bundle_byte_hashes"][index], code_origin_hex(&cellscript_artifact_checker::ckb_blake2b256(input)));
            }
            assert_eq!(record["artifact_report"], serde_json::to_value(checked.artifact_report()).unwrap());
            assert!(!checked.artifact_report().semantic_equivalence_claimed);
            assert_eq!(checked.artifact_report().chain_evidence, cellscript_artifact_checker::EvidenceState::NotProvided);
            assert_eq!(checked.artifact_report().ckb_vm_evidence, cellscript_artifact_checker::EvidenceState::NotExecuted);
            let mut material = b"cellscript-fixed-policy-interface-receipt-id-v3\0".to_vec();
            material.extend_from_slice(&checked.canonical_bytes().unwrap());
            assert_eq!(checked.identity(), code_origin_hex(&cellscript_artifact_checker::ckb_blake2b256(&material)));
            assert_eq!(checked.identity(), fixed_policy_receipt(&fixture, vec![1, 2, 3]).unwrap().identity());
            assert_ne!(checked.identity(), fixed_policy_receipt(&fixture, vec![1, 2, 4]).unwrap().identity());
        }
    }
}
#[test]
fn fixed_policy_receipts_compare_directionally_without_predicate_or_deployment_equivalence() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let required = fixed_policy_receipt(&external_fixture(EXTERNAL_SOURCE, opt), vec![1]).unwrap();
        let other =
            fixed_policy_receipt(&external_fixture(&EXTERNAL_SOURCE.replace("token.amount > 0", "token.amount == 7"), opt), vec![2])
                .unwrap();
        required.check_required_contracts(&other).unwrap();
        assert_ne!(required.identity(), other.identity());
        assert_ne!(required.target_origin().origin().artifact_hash(), other.target_origin().origin().artifact_hash());
        for source in [
            EXTERNAL_SOURCE.replace("amount: u64", "amount: u32"),
            EXTERNAL_SOURCE.replace("witness value: u64", "witness value: u32"),
        ] {
            let narrowed = fixed_policy_receipt(&external_fixture(&source, opt), vec![1]).unwrap();
            assert!(required.check_required_contracts(&narrowed).is_err());
        }
        let source = format!("{EXTERNAL_SOURCE}\naction audit(input token: Token, witness value: u64) {{ require token.amount > 0; require value > 0; consume token; }}");
        let mut declaration = declaration();
        declaration.actions.retain(|action| action.action == "burn");
        declaration.actions.push(ArtifactAction { tag: 55, action: "audit".into() });
        declaration.common_checks.clear();
        let extension =
            fixed_policy_receipt(&Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt, declaration), vec![3])
                .unwrap();
        required.check_required_contracts(&extension).unwrap();
        assert!(extension.check_required_contracts(&required).is_err());
    }
}
#[test]
fn fixed_policy_receipt_admits_proven_constants_and_rejects_unproven_ones() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        // A constant inside the closed grammar is independently re-evaluated
        // and admitted through the finite external codec.
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic const LIMIT: u64 = 3 * 7;"), opt);
        let checked = fixed_external_codec(&fixture).unwrap();
        let codec_record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(codec_record["constants"][0]["value"], "21");
        assert!(fixed_policy_receipt(&fixture, vec![1]).is_ok());
        // Constants outside the closed grammar keep no proven value and the
        // receipt fails closed exactly as before.
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic const LABEL: String = \"sealed\";"), opt);
        let error = fixed_policy_receipt(&fixture, vec![1]).unwrap_err();
        assert!(error.message.contains("constant"), "{error:?}");
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic struct Pair<T: fixed_value> {{ value: T }}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}"), opt);
        let checked = fixed_policy_receipt(&fixture, vec![1]).unwrap();
        let record: Value = serde_json::from_slice(&checked.target_origin().origin().codec().canonical_bytes().unwrap()).unwrap();
        assert!(record["callables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["availability"] == "declaration-only" && entry["entry"].is_null()));
        assert!(record["public_layouts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["availability"] == "declaration-only" && entry["width"].is_null()));
    }
}
#[test]
fn fixed_policy_receipt_detects_postcheck_substitution_of_every_exact_input() {
    use cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = external_fixture(EXTERNAL_SOURCE, opt);
        let checked = fixed_policy_receipt(&fixture, vec![1]).unwrap();
        let original = receipt_bundle(&fixture);
        let (raw, script) = receipt_deployment(&fixture, vec![1]);
        for index in 0..4 {
            let mut changed = original.clone();
            changed[index].push(b'\n');
            assert!(checked
                .check_unchanged_inputs(changed.each_ref().map(Vec::as_slice), &raw, 0, &script)
                .unwrap_err()
                .message
                .contains("changed"));
        }
        let mut altered_raw = raw.clone();
        *altered_raw.last_mut().unwrap() ^= 1;
        assert!(checked.check_unchanged_inputs(original.each_ref().map(Vec::as_slice), &altered_raw, 0, &script).is_err());
        assert!(checked.check_unchanged_inputs(original.each_ref().map(Vec::as_slice), &raw, 1, &script).is_err());
        let (_, other_script) = receipt_deployment(&fixture, vec![2]);
        assert!(checked.check_unchanged_inputs(original.each_ref().map(Vec::as_slice), &raw, 0, &other_script).is_err());
        // A semantically identical valid metadata encoding is a distinct frozen tuple.
        let mut pretty = original.clone();
        pretty[1] = serde_json::to_vec_pretty(&fixture.metadata).unwrap();
        let other =
            check_fixed_policy_receipt(pretty.each_ref().map(Vec::as_slice), &raw, 0, &script, &CheckerBudgets::default()).unwrap();
        checked.check_required_contracts(&other).unwrap();
        assert_ne!(checked.identity(), other.identity());
    }
}
#[test]
fn fixed_policy_receipt_bounds_creation_and_substitution_checks_before_any_parser_or_hash() {
    use cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt;
    let fixture = external_fixture(EXTERNAL_SOURCE, cellscript::CompilerDiagnostics::without_optimizations());
    let checked = fixed_policy_receipt(&fixture, vec![1]).unwrap();
    let original = receipt_bundle(&fixture);
    let (raw, script) = receipt_deployment(&fixture, vec![1]);
    for field in ["artifact", "record", "source-map"] {
        let mut budgets = CheckerBudgets::default();
        match field {
            "artifact" => budgets.artifact_bytes = 0,
            "record" => budgets.record_bytes = 0,
            _ => budgets.source_map_bytes = 0,
        }
        assert_eq!(
            check_fixed_policy_receipt(original.each_ref().map(Vec::as_slice), &raw, 0, &script, &budgets).unwrap_err().code,
            CheckerRejectionCode::V2400BudgetExceeded
        );
    }
    let oversized = vec![0; 4 * 1024 * 1024 + 1];
    assert_eq!(
        check_fixed_policy_receipt([&oversized, &[], &[], &[]], &[], 0, &[], &CheckerBudgets::default()).unwrap_err().code,
        CheckerRejectionCode::V2400BudgetExceeded
    );
    assert_eq!(
        checked.check_unchanged_inputs([&oversized, &[], &[], &[]], &[], 0, &[]).unwrap_err().code,
        CheckerRejectionCode::V2400BudgetExceeded
    );
    let limit = vec![0; 4 * 1024 * 1024];
    for constructor in [true, false] {
        let bundle = [limit.as_slice(); 4];
        let error = if constructor {
            check_fixed_policy_receipt(bundle, &[0], 0, &[], &CheckerBudgets::default()).unwrap_err()
        } else {
            checked.check_unchanged_inputs(bundle, &[0], 0, &[]).unwrap_err()
        };
        assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
    }
}

fn code_origin_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn code_origin(
    fixture: &Fixture,
    raw: &[u8],
    index: u32,
    script: &[u8],
) -> Result<cellscript_artifact_checker::code_origin::CheckedCodeCellOrigin, CheckerError> {
    cellscript_artifact_checker::code_origin::check_code_cell_origin(
        [
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
        ],
        raw,
        index,
        script,
        &CheckerBudgets::default(),
    )
}
#[test]
fn code_cell_origin_recomputes_sdk_transaction_script_and_code_identities() {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = external_fixture(EXTERNAL_SOURCE, opt);
        let lock =
            packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::from(vec![1, 2, 3]).pack()).build();
        let type_script =
            packed::Script::new_builder().code_hash([8u8; 32].pack()).hash_type(1u8).args(Bytes::from(vec![4; 32]).pack()).build();
        let input = packed::CellInput::new_builder()
            .previous_output(packed::OutPoint::new_builder().tx_hash([7u8; 32].pack()).index(3u32).build())
            .build();
        let code = packed::CellOutput::new_builder()
            .capacity(1000000000000u64)
            .lock(lock.clone())
            .type_(Some(type_script.clone()).pack())
            .build();
        let other = packed::CellOutput::new_builder().capacity(100000000000u64).lock(lock.clone()).build();
        let tx = TransactionBuilder::default()
            .input(input.clone())
            .output(code.clone())
            .output_data(Bytes::from(fixture.artifact.clone()).pack())
            .output(other.clone())
            .output_data(Bytes::from(vec![1, 2, 3]).pack())
            .build();
        let raw = tx.data().raw();
        for hash_type in [0u8, 1, 2, 4] {
            let code_hash =
                if hash_type == 1 { type_script.calc_script_hash() } else { packed::CellOutput::calc_data_hash(&fixture.artifact) };
            let selected = packed::Script::new_builder()
                .code_hash(code_hash.clone())
                .hash_type(hash_type)
                .args(Bytes::from(vec![31, 32, 33]).pack())
                .build();
            let checked = code_origin(&fixture, raw.as_slice(), 0, selected.as_slice()).unwrap();
            assert_eq!(checked.transaction_hash(), code_origin_hex(tx.hash().as_slice()));
            assert_eq!(checked.artifact_hash(), code_origin_hex(packed::CellOutput::calc_data_hash(&fixture.artifact).as_slice()));
            assert_eq!(checked.type_script_hash(), Some(code_origin_hex(type_script.calc_script_hash().as_slice()).as_str()));
            assert_eq!(checked.selected_script_hash(), code_origin_hex(selected.calc_script_hash().as_slice()));
            assert_eq!(checked.output_index(), 0);
            assert_eq!(checked.selected_hash_type(), hash_type);
            let other_args = selected.clone().as_builder().args(Bytes::from(vec![31, 32, 34]).pack()).build();
            let changed = code_origin(&fixture, raw.as_slice(), 0, other_args.as_slice()).unwrap();
            assert_eq!(checked.transaction_hash(), changed.transaction_hash());
            assert_ne!(checked.selected_script_hash(), changed.selected_script_hash());
            assert_ne!(checked.identity(), changed.identity());
            let wrong = selected.clone().as_builder().code_hash([0u8; 32].pack()).build();
            assert!(code_origin(&fixture, raw.as_slice(), 0, wrong.as_slice()).is_err());
            assert!(code_origin(&fixture, raw.as_slice(), 1, selected.as_slice()).is_err());
            assert!(code_origin(&fixture, raw.as_slice(), 2, selected.as_slice()).is_err());
            let mut altered = fixture.artifact.clone();
            let last = altered.len() - 1;
            altered[last] ^= 1;
            let bad_code = TransactionBuilder::default()
                .input(input.clone())
                .output(code.clone())
                .output_data(Bytes::from(altered).pack())
                .build();
            assert!(code_origin(&fixture, bad_code.data().raw().as_slice(), 0, selected.as_slice()).is_err());
        }
    }
}
#[test]
fn code_cell_target_requires_the_actual_checked_profile_even_for_valid_byte_origins() {
    use cellscript_artifact_checker::code_origin::check_code_cell_target;
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (target, accepted) in [("ckb", 4u8), ("ckb-type-hash", 1u8)] {
            let mut declaration = declaration();
            declaration.actions.retain(|action| action.action == "burn");
            declaration.common_checks.clear();
            let fixture = Fixture::new_source_with_target(EXTERNAL_SOURCE, CellScriptEdition::Edition2027, opt, declaration, target);
            let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
            let type_script =
                packed::Script::new_builder().code_hash([8u8; 32].pack()).hash_type(1u8).args(Bytes::from(vec![4; 32]).pack()).build();
            let tx = TransactionBuilder::default()
                .output(
                    packed::CellOutput::new_builder()
                        .capacity(1000000000000u64)
                        .lock(lock)
                        .type_(Some(type_script.clone()).pack())
                        .build(),
                )
                .output_data(Bytes::from(fixture.artifact.clone()).pack())
                .build();
            for hash_type in [0u8, 1, 2, 4] {
                let code_hash = if hash_type == 1 {
                    type_script.calc_script_hash()
                } else {
                    packed::CellOutput::calc_data_hash(&fixture.artifact)
                };
                let selected = packed::Script::new_builder()
                    .code_hash(code_hash)
                    .hash_type(hash_type)
                    .args(Bytes::from(vec![31, 32, 33]).pack())
                    .build();
                // All four are byte-bound to this exact SDK-built code Cell.
                let origin = code_origin(&fixture, tx.data().raw().as_slice(), 0, selected.as_slice()).unwrap();
                let origin_id = origin.identity().to_owned();
                let result = check_code_cell_target(origin);
                if hash_type == accepted {
                    let checked = result.unwrap();
                    assert_eq!(checked.origin().identity(), origin_id);
                    assert_eq!(checked.runtime_contract().target_profile, target);
                    let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
                    assert_eq!(record["code_origin"], origin_id);
                    assert_eq!(record["runtime"], fixture.metadata["public_interface"]["runtime_contract"]);
                    assert_eq!(record["deployment_hash_type"], if accepted == 4 { "data2" } else { "type" });
                    let mut material = b"cellscript-code-cell-target-id-v1\0".to_vec();
                    material.extend_from_slice(&checked.canonical_bytes().unwrap());
                    assert_eq!(checked.identity(), code_origin_hex(&cellscript_artifact_checker::ckb_blake2b256(&material)));
                    // Exact args remain bound after profile checking.
                    let changed = selected.as_builder().args(Bytes::from(vec![31, 32, 34]).pack()).build();
                    let other =
                        check_code_cell_target(code_origin(&fixture, tx.data().raw().as_slice(), 0, changed.as_slice()).unwrap())
                            .unwrap();
                    assert_ne!(checked.identity(), other.identity());
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    assert!(error.message.contains("selected Script hash type differs"), "{error:?}");
                }
            }
        }
    }
}

#[test]
fn code_cell_origin_rejects_noncanonical_and_unselected_transaction_structures() {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let fixture = external_fixture(EXTERNAL_SOURCE, cellscript::CompilerDiagnostics::default());
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let code = packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock.clone()).build();
    let tx = TransactionBuilder::default()
        .output(code.clone())
        .output_data(Bytes::from(fixture.artifact.clone()).pack())
        .output(code.clone())
        .output_data(Bytes::new().pack())
        .build();
    let raw = tx.data().raw();
    let selected = lock.clone().as_builder().code_hash(packed::CellOutput::calc_data_hash(&fixture.artifact)).build();
    code_origin(&fixture, raw.as_slice(), 0, selected.as_slice()).unwrap();
    for index in [0usize, 4, 8, 12, 16, 20, 24] {
        let mut malformed = raw.as_slice().to_vec();
        malformed[index] ^= 1;
        assert!(code_origin(&fixture, &malformed, 0, selected.as_slice()).is_err());
    }
    for length in [0, 1, 3, 4, 7, 27, raw.as_slice().len() - 1] {
        assert!(code_origin(&fixture, &raw.as_slice()[..length], 0, selected.as_slice()).is_err());
    }
    let mut trailing = raw.as_slice().to_vec();
    trailing.push(0);
    assert!(code_origin(&fixture, &trailing, 0, selected.as_slice()).is_err());
    let broken_lock = lock.as_builder().hash_type(3u8).build();
    let unselected = code.as_builder().lock(broken_lock).build();
    let bad = TransactionBuilder::default()
        .output(tx.outputs().get(0).unwrap())
        .output_data(Bytes::from(fixture.artifact.clone()).pack())
        .output(unselected)
        .output_data(Bytes::new().pack())
        .build();
    assert!(code_origin(&fixture, bad.data().raw().as_slice(), 0, selected.as_slice()).is_err());
    let wrong_cardinality = TransactionBuilder::default().output(tx.outputs().get(0).unwrap()).build();
    assert!(code_origin(&fixture, wrong_cardinality.data().raw().as_slice(), 0, selected.as_slice()).is_err());
}
