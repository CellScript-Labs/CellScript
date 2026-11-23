#[test]
fn typed_semantics_requires_reference_coercion_for_reference_calls() {
    let valid = Fixture::from_source(
        r#"
module checker::reference_call;

struct Wallet {
    amount: u64,
}

fn inspect(wallet: &Wallet) -> u64 {
    return wallet.amount;
}

action main(wallet: Wallet) -> u64 {

        return inspect(&wallet);
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
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn typed_semantics_requires_exact_guard_for_unsigned_narrowing() {
    let valid = Fixture::from_source(
        r#"
module checker::narrowing;

action narrow(value: u64) -> u8 {

        return value as u8;
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
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn typed_semantics_rejects_field_enum_layout_and_instantiation_mutations() {
    let valid = Fixture::from_source(include_str!("../syntax_combo/seeds/generic-value.cell"));

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
    changed.rebind_layered_identities();
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
    changed.rebind_layered_identities();
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
    changed.rebind_layered_identities();
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
    let valid = Fixture::from_source(include_str!("../syntax_combo/seeds/explicit-borrow.cell"));

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].borrows[0].start_operation = u32::MAX;
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].ownership[0].final_state = "available".to_string();
    changed.rebind_layered_identities();
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
    changed.rebind_layered_identities();
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
    changed.rebind_layered_identities();
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
module nested_policy_transport;
resource Token has store, consume { amount: u64, }
action first(input token: Token, witness a: u64, witness b: u64, witness c: u64, witness d: u64, witness e: u64, witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness tail: Hash) {

    require values.len() == 1;
    require token.amount == a;
    consume token;
}
action second(input token: Token, witness values: NestedValues<Vec<u8>, 3, 5, 64>, witness other: NestedValues<Vec<u8>, 2, 3, 32>, witness nonce: u16) {

    require values.len() == 2;
    require other.len() == 1;
    consume token;
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
module nested_read_contract;
struct Row { a: u8, b: u16, c: u32, d: u64, big: u128, hash: Hash, pad: [u8; 17], payload: Vec<u8>, }
action inspect(witness values: NestedValues<Row, 3, 8, 512>, witness at: u64, witness big: u128, witness hash: Hash, witness pad: [u8; 17]) -> u64 {

    let row = values[at];
    let alias = row;
    require row.a == 1;
    require alias.b == 2;
    require row.c == 3;
    require row.d == 4;
    require alias.big == big;
    require row.hash == hash;
    require alias.pad == pad;
    let bytes = alias.payload;
    require !bytes.is_empty();
    require bytes[at] == 171;
    return values.len();
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
    let source = format!("module wide_nested_read;\nstruct Row {{ {fields}, payload: Vec<u8> }}\naction inspect(witness values: NestedValues<Row, 1, 3, 4096>, witness expected: [u8; 256]) -> u64 {{\nlet row = values[0];\nrequire row.f7 == expected;\nlet payload = row.payload;\nreturn payload.len(); }}");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        for index in [305419896u64, u64::MAX] {
            let source = format!("module high_nested_index;\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 {{ \nlet item = values[{index}];\nreturn item.len(); }}");
            Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
        }
    }
}

#[test]
fn nested_independent_views_cannot_share_a_coherently_rewritten_length_slot() {
    let source = "module nested_view_slots;\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 { \nlet first = values[0];\nlet second = values[1];\nreturn first.len() + second.len(); }";
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
