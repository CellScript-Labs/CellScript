#[test]
fn terminal_verifier_failures_reject_hash_rebound_machine_and_record_mutations() {
    let source = "module fatal_checker;\naction main(witness value: u64) {  require value > 0; }";
    {
        let edition = CURRENT_EDITION;
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
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

        assert_eq!(valid.metadata["typed_semantics"]["failure_semantics"], "current-vm-process-exit-v1");
        let mut malformed = valid.metadata["typed_semantics"].clone();
        malformed.as_object_mut().unwrap().remove("failure_semantics");
        assert!(serde_json::from_value::<cellscript_artifact_checker::TypedSemanticRecord>(malformed).is_err());
    }
}

#[test]
fn terminal_verifier_operations_cannot_be_inserted_before_continuation_after_rebinding() {
    let source = "module fatal_operation_checker;\naction main(witness value: u64) {  require value > 0; }";
    {
        let edition = CURRENT_EDITION;
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
            changed.rebind_layered_identities();
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        }
    }
}

#[test]
fn implicit_verifier_exit_inventory_cannot_be_hidden_by_renaming_the_sink() {
    let valid = Fixture::from_source("module implicit_failure;\nfn divide(value: u64) -> u64 { return 7 / value; }\naction main(witness value: u64) -> u64 {  return divide(value); }");
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
module common_calls;
resource Token has consume { amount: u64, }
fn checked(value: u64) -> u64 { return 7 / value; }
action common() {  require checked(7) == 1; }
action burn(input token: Token) {  consume token; }
"#;
    {
        let edition = cellscript::CURRENT_EDITION;
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
                    context: ArtifactContext::TypeGroup {},
                    bindings: cellscript::artifact::EntryCellBindings::GroupRelative,
                    coverage: cellscript::artifact::EntryCoverage::FixedGroup,
                    layout: Some("Token".into()),
                    dispatch: ArtifactDispatch::PolicyWitnessV1,
                    entry: None,
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
            changed.rebind_layered_identities();
            let error =
                cellscript_artifact_checker::validate_policy_metadata(&changed.metadata, &changed.record.typed_semantics).unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{edition:?}: {mutation}: {error}");
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        }
    }
}

const REFERENCE_ENTRY_SOURCE: &str = r#"
module artifact_checker_reference_fixture;

resource Token has consume {
    amount: u64,
}

fn inspect(token: &Token) -> u64 {
    return token.amount;
}

action main() -> u64 {

        return 0;
}
"#;

const NATIVE_EDITION_2027_SOURCE: &str = r#"// test-artifact: {"entry":"transfer","layout":"Token"}

module artifact_checker_native_2027;

resource Token has store, replace, relock {
    owner: Address,
    amount: u64,
}

action transfer(
        input token: Token,
        witness recipient: Address,
        output next: Token,
    ) {
        require token.amount > 0;

        replace token -> next {
            data {
                owner = same,
                amount = same,
            };
            type_script = same;
            lock = exact(recipient);
            capacity = same;
        }
    }
"#;

const NATIVE_FRESH_AUDIT_EDITION_2027_SOURCE: &str = r#"// test-artifact: {"entry":"mint","layout":"Token"}

module artifact_checker_native_fresh_2027;
#[type_id("artifact-checker::Token:v1")]
resource Token has store, create, burn identity(ckb_type_id) { amount: u64, }
action mint(
        witness amount: u64,
        witness recipient: Address,
        output next: Token,
    ) {
        require amount > 0;
        audit issuance_policy {
            expected_evidence = external_policy(recipient);
        }
        fresh next {
            data { amount = amount, };
            identity = ckb_type_id;
            lock = exact(recipient);
        }
    }
"#;

const NATIVE_RETIRE_EDITION_2027_SOURCE: &str = r#"// test-artifact: {"entry":"retire_note","layout":"Note"}

module artifact_checker_native_retire_2027;
resource Note has store, consume, burn identity(field(note_id)) { note_id: u64, amount: u64, }
action retire_note(input note: Note) {
        require note.amount == 0;
        retire note {
            absence = field(note_id);
        }
    }
"#;

const NATIVE_POOL_EDITION_2027_SOURCE: &str = r#"// test-artifact: {"entry":"merge","layout":"Token"}

module artifact_checker_native_pool_2027;
resource Token has store, create, consume { owner: Address, amount: u64, }
action merge(
        input left: Token,
        input right: Token,
        witness recipient: Address,
        output merged: Token,
    ) {
        require left.amount > 0;
        pool value_flow {
            inputs { left, right };
            outputs { merged };
            data {
                owner { merged = recipient, },
                amount = conserve,
            };
            lock { merged = exact(recipient), };
        }
    }
"#;

const ORDINARY_LOCK_SOURCE: &str = r#"
module artifact_checker_lock_2027;
resource Vault has store { owner: Address, }
lock unlock(protected vault: Vault, lock_args owner: Address, witness claimed_owner: Address) -> bool {

        require vault.owner == owner;
        require claimed_owner == owner;
        true
}
"#;

const NATIVE_LOCK_EDITION_2027_SOURCE: &str = r#"
module artifact_checker_lock_2027;
resource Vault has store { owner: Address, }
lock unlock(
        protected vault: Vault,
        lock_args owner: Address,
        witness claimed_owner: Address,
    ) {
        require vault.owner == owner;
        require claimed_owner == owner;
        true
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
    let source = "module signed_literal;\naction inspect() -> u64 { let value: i32 = 7;\nlet negative = -value;\nrequire negative < value;\nreturn 0; }";
    let valid = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                edition: CURRENT_EDITION,
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
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn allocated_record_contracts_retain_unused_layouts_and_reject_forged_types() {
    let source = r#"
module record_proof;
struct Inner { amount: u64, }
struct Value { inner: Inner, }
struct Unused { bytes: [u8; 32], }
collections Work { limits { bytes: 512, allocations: 4 },
values: Vec<Value> { items: 4 },
unused: OrderedMap<u64, Unused> { items: 4 }
}
fn read(view: &Work) -> Value { return view.values.get(0); }
action inspect(witness value: Value) -> u64 {
    let mut work = Work::new();
    work.values.push(value);
    let copied = read(&work);
    work.drop();
    require (copied.inner.amount == value.inner.amount);
    return 0;
}
"#;
    let valid = Fixture::from_result(
        compile(source, CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".into()), ..Default::default() })
            .unwrap(),
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
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}
