#[test]
fn exact_role_nested_lifetime_handles_empty_branches_and_rejects_helper_escape() {
    let source = include_str!("../../examples/language/ckb/exact_role_verification.cell")
        .replace("verify()", "verify(witness items: NestedValues<Vec<u8>, 2, 8, 32>)");
    let start = source.find("    ckb::verify_cell_roles(").unwrap();
    let end = source.rfind("    return 0").unwrap();
    let selection = &source[start..end];
    let prefix = format!("{}let first = items[0];\nlet before = first.len();\n", &source[..start]);
    let suffix = format!("require first.len() == before;\nrequire items.len() == 1;\n{}", &source[end..]);
    let conditional = format!("{prefix}if before == 2 {{\n{selection}}}\n{suffix}");
    let helper = format!("{prefix}verify_roles();\n{suffix}")
        .replace("action verify(", &format!("fn verify_roles() {{\n{selection}}}\naction verify("));
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let options = CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() };
        let compiled = compile(&conditional, options.clone())
            .unwrap_or_else(|error| panic!("nested selection control flow {diagnostics}: {error}"));
        let valid = Fixture::from_result(compiled);
        let selection_end = valid
            .record
            .blocks
            .iter()
            .find(|block| block.exact_role_labels.iter().any(|label| label.ends_with("_done")))
            .unwrap()
            .range
            .start;
        let read = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.range.start >= selection_end)
            .filter(|block| {
                block.nested_read_labels.iter().any(|label| label.starts_with(".Lnested_use_length_") && !label.ends_with("_done"))
            })
            .min_by_key(|block| block.range.start)
            .unwrap();
        let empty = valid
            .record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.id == read.owner_entry)
            .unwrap()
            .blocks
            .iter()
            .find(|block| block.operations.is_empty())
            .unwrap();
        assert_ne!(read.lowering_block_id, Some(empty.id));
        let mut changed = valid.clone();
        let target = changed.record.blocks.iter_mut().find(|block| block.id == read.id).unwrap();
        target.lowering_block_id = Some(empty.id);
        target.typed_block_hash = Some(canonical_hash("cellscript-typed-block-v1", empty).unwrap());
        for binding in &mut changed.record.entries.iter_mut().find(|entry| entry.id == read.owner_entry).unwrap().typed_blocks {
            binding.machine_block_ids.retain(|id| id != &read.id);
            if binding.id == empty.id {
                binding.machine_block_ids.push(read.id.clone());
            }
        }
        changed.record.canonicalize();
        changed.rebind_sidecars();
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert!(error.message.contains("nested read crosses its owning typed block"), "{error}");
        let error = compile(&helper, options).unwrap_err();
        assert!(error.message.contains("exact-role verification requires an action or lock entry"), "{error}");
    }
}

fn large_policy_fixture(count: u32) -> CompileResult {
    policy_fixture_with_layouts(count, false)
}

fn policy_fixture_with_layouts(count: u32, distinct_layouts: bool) -> CompileResult {
    use cellscript::artifact::{compile_artifact, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    let mut source =
        "module policy_error_labels;\nresource Token has store, consume { amount: u64,\nexpected: [u8; 8], }\n".to_string();
    for index in 0..count {
        let extra = if distinct_layouts { format!(", witness extra: [u8; {}]", index + 1) } else { String::new() };
        source.push_str(&format!("action check_{index}(input before: Token, witness payload: [u8; 8]{extra}) {{\nrequire before.amount == {};\nrequire payload == before.expected;\nconsume before;\n}}\n",index+1));
    }
    compile_artifact(
        &source,
        CompileOptions {
            edition: CURRENT_EDITION,
            target: Some("riscv64-elf".into()),
            target_profile: Some("ckb".into()),
            diagnostics: cellscript::CompilerDiagnostics::default(),
            ..Default::default()
        },
        ArtifactDeclaration {
            name: "LabelOrdinalPolicy".into(),
            context: ArtifactContext::TypeGroup {},
            bindings: cellscript::artifact::EntryCellBindings::GroupRelative,
            coverage: cellscript::artifact::EntryCoverage::FixedGroup,
            layout: Some("Token".into()),
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            action: None,
            actions: (0..count).map(|index| ArtifactAction { tag: index, action: format!("check_{index}") }).collect(),
            common_checks: vec![],
        },
        cellscript::ExecutableSurfacePolicy::DenyFailClosed,
    )
    .expect("large policy must not interpret label ordinals as errors")
}

#[test]
fn policy_failure_label_ordinals_are_not_runtime_error_codes() {
    let mut reaches_large_ordinal = false;
    for count in [32, 64] {
        let compiled = policy_fixture_with_layouts(count, true);
        compiled.validate().expect("independent checker accepts the large policy");
        let record = compiled.verified_lowering_record.as_ref().unwrap();
        reaches_large_ordinal |= record.blocks.iter().any(|block| {
            block.machine_label.as_deref().is_some_and(|label| {
                label.rsplit_once("_fail_").and_then(|(_, suffix)| suffix.parse::<u32>().ok()).is_some_and(|ordinal| ordinal > 255)
            })
        });
        assert!(record.runtime_error_exits.iter().all(|exit| (1..=255).contains(&exit.code)));
        assert!(!record.verifier_failure_exits.is_empty(), "actual failure sites must remain bound");
    }
    assert!(reaches_large_ordinal, "fixtures reach ordinals beyond the runtime exit-code range");
}

#[test]
fn relaxed_policy_branches_reject_condition_skip_and_call_mutations() {
    let valid = Fixture::from_result(large_policy_fixture(64));
    assert_eq!(valid.check(), Ok(()));
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let pair = elf
        .instructions
        .windows(2)
        .find(|pair| {
            pair[0].word & 0x7f == 0x63
                && pair[1].word & 0xfff == 0x6f
                && valid
                    .record
                    .blocks
                    .iter()
                    .any(|block| block.range.contains(pair[0].address) && block.range.contains(pair[1].address))
                && elf.control_flow.iter().any(|flow| flow.address == pair[0].address && flow.target == pair[0].address + 8)
        })
        .expect("64 actions exercise a relaxed policy branch");
    for (address, word) in [
        (pair[0].address, pair[0].word ^ (1 << 12)), // Invert the accepted condition.
        (pair[0].address, pair[0].word ^ (1 << 9)),  // Skip twelve bytes instead of eight.
        (pair[1].address, pair[1].word | (1 << 7)),  // Turn the jump into a call.
    ] {
        let mut changed = valid.clone();
        changed.replace_machine_word(address, word);
        assert!(changed.check().is_err(), "hash-rebound relaxed-branch mutation at {address:#x} must reject");
    }
}

#[test]
fn compact_policy_adapter_rejects_frame_capacity_and_saved_return_address_mutations() {
    let valid = Fixture::from_result(large_policy_fixture(1));
    let adapter = valid.record.entries.iter().find(|entry| entry.name.starts_with(".Lpolicy_action_adapter_")).unwrap();
    assert_eq!(adapter.frame_size_bytes, 32, "eight-byte witness needs only a 32-byte private frame");
    let base = valid.record.blocks.iter().find(|block| block.id == adapter.entry_block).unwrap().range.start;
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let word = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
    // These change stack ownership or the bound before copying, even after
    // outer hashes and block digests are rebound by the mutation helper.
    for (address, replacement) in
        [(base, word(base) ^ (16 << 20)), (base + 4, word(base + 4) ^ (1 << 10)), (base + 8, word(base + 8) ^ (1 << 20))]
    {
        let mut changed = valid.clone();
        changed.replace_machine_word(address, replacement);
        assert!(changed.check().is_err(), "compact adapter mutation must be independently rejected");
    }
    let mut old = valid.clone();
    old.record.schema = "cellscript-verified-lowering-record-v8".into();
    old.record.version = 8;
    old.rebind_sidecars();
    assert!(old.check().is_err(), "v8 cannot certify the v9 private-frame contract");
}

#[test]
fn shared_policy_decoder_has_only_bound_callers_and_cannot_clobber_its_borrowed_frame() {
    let valid = Fixture::from_result(large_policy_fixture(4));
    assert_eq!(valid.check(), Ok(()));
    let decoders: Vec<_> = valid.record.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")).collect();
    assert_eq!(decoders.len(), 1, "four identical contracts use one decoder");
    let decoder = decoders[0];
    assert_eq!(decoder.frame_size_bytes, 0, "the direct stub owns the borrowed frame");
    let start = valid.record.blocks.iter().find(|block| block.id == decoder.entry_block).unwrap().range.start;
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let callers: Vec<_> = elf.control_flow.iter().filter(|flow| flow.target == start).collect();
    assert_eq!(callers.len(), 4);
    let word = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
    let copy = valid
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == decoder.id
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lpolicy_args_copy_"))
        })
        .unwrap();
    let done = valid
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == decoder.id
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_witness_done_"))
        })
        .unwrap();
    for (address, replacement) in [
        (callers[0].address, word(callers[0].address) & !(31 << 7)),
        (start, word(start) ^ (8 << 20)),
        (copy.range.start + 8, (word(copy.range.start + 8) & !(31 << 7)) | (1 << 7)),
        (done.range.start, 0x01010113), // addi sp, sp, 16 instead of ret.
    ] {
        let mut changed = valid.clone();
        changed.replace_machine_word(address, replacement);
        assert!(changed.check().is_err(), "shared decoder mutation at {address:#x} must reject after rebinding");
    }
    let distinct = policy_fixture_with_layouts(4, true);
    assert!(
        !distinct.verified_lowering_record.unwrap().entries.iter().any(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")),
        "different payload contracts retain dedicated decoders"
    );
}

const RUNTIME_PROVENANCE_SOURCE: &str = r#"
module artifact_checker_runtime_provenance;

resource Token has store { amount: u64, }

action inspect(witness source_index: u64, witness expected_data_hash: Hash) -> u64 {
    let input = ckb::input::<Token>(source_index);
    let dep = ckb::cell_dep(source_index);
    let transaction_hash = ckb::transaction_hash();
    require input.capacity > 0;
    require dep.data_hash == expected_data_hash;
    require transaction_hash != Hash::zero();
    return 0;
}
"#;

const BOUNDED_WITNESS_SOURCE: &str = r#"
module artifact_checker_bounded_witness;

action inspect() -> u64 {

        let witness_args = witness::args(0);
        let bytes = witness::bounded_lock(witness_args, 64);
        require bytes.size <= 64;
        require witness::byte(bytes, 0) >= 0;
        require witness::blake2b(bytes) != Hash::zero();
        return 0;
}
"#;

const SIGHASH_ZERO_LOCK_SOURCE: &str = r#"
module artifact_checker_sighash_zero_lock;

action inspect() -> u64 {

        let digest = env::sighash_all_zero_lock(4, 8, 4, 4096);
        return 0;
}
"#;

const HEADER_DEP_SOURCE: &str = r#"
module artifact_checker_header_dep;

action inspect(witness source_index: u64) -> u64 {
    let header = ckb::header_dep(source_index);
    return ckb::epoch_number_to_u64(header.epoch_number)
        + ckb::block_number_to_u64(header.epoch_start_block_number)
        + ckb::epoch_length_to_u64(header.epoch_length)
        + ckb::block_number_to_u64(header.block_number)
        + ckb::timestamp_millis_to_u64(header.timestamp);
}
"#;

const SCRIPT_HASH_SOURCE: &str = r#"
module artifact_checker_script_hash;

action inspect() -> u64 {
    let value = script::new(Hash::zero(), script::hash_type_data2(), script::args(b"ckb!"));
    let complete: ScriptHash = script::hash(value);
    require complete != ckb::script_hash(Hash::zero());
    return 0;
}
"#;

const COMMITTED_STATE_SOURCE: &str = r#"
module artifact_checker_committed_state;

struct State { counter: u64, }

action inspect(witness expected: Commitment<State>, witness opening: Opening<State>) -> u64 {

        let state = commitment::open(expected, opening);
        let successor = new State { counter: state.counter + 1, };
        let next = commitment::commit(successor);
        require next != expected;
        return 0;
}
"#;

const COMMITTED_STATE_FIELD_SOURCE: &str = r#"
module artifact_checker_committed_state_field;

struct State { counter: u64, }

struct Envelope {
    nonce: u64,
    state_commitment: Commitment<State>,
}

action inspect(witness envelope: Envelope, witness opening: Opening<State>) -> u64 {

        let state = commitment::open(envelope.state_commitment, opening);
        return state.counter;
}
"#;

const EXACT_HANDLE_SOURCE: &str = r#"
module artifact_checker_exact_handle;

resource Token has store { amount: u64, }

action inspect(witness handle: ExactScriptHandle) -> u64 {
    let dep = ckb::cell_dep(0);
    ckb::require_cell_dep_exact_verifier_handle(
        dep,
        handle,
        Hash::from_bytes(b"0123456789abcdef0123456789abcdef")
    );
    return 0;
}
"#;

const DEPLOYMENT_LINE_HANDLE_SOURCE: &str = r#"
module artifact_checker_deployment_line_handle;

resource Token has store { amount: u64, }

action inspect(witness line: DeploymentLineHandle) -> u64 {
    let admission = ckb::cell_dep(0);
    let code = ckb::cell_dep(1);
    ckb::require_cell_dep_deployment_line_verifier_handle(
        admission,
        code,
        line,
        Hash::from_bytes(b"0123456789abcdef0123456789abcdef")
    );
    return 0;
}
"#;
