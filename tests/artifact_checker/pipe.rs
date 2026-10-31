use super::*;
const SOURCE: &str = "module pipe_binding\npublic action verify() -> u64 { verification let ends = pipe() require ends.0 != ends.1 require close(ends.0) == 0 require close(ends.1) == 0 return 0 }";
fn fixture(source: &str, diagnostics: cellscript::CompilerDiagnostics) -> Fixture {
    Fixture::from_result(
        compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
    )
}
fn word(elf: &cellscript_artifact_checker::ParsedElf, at: u64) -> u32 {
    elf.instructions.iter().find(|word| word.address == at).unwrap().word
}
fn pipe_call(valid: &Fixture, elf: &cellscript_artifact_checker::ParsedElf) -> (u64, u64) {
    let entry = valid.record.entries.iter().find(|entry| entry.id == "runtime:__ckb_pipe").unwrap();
    let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
    let address = elf.control_flow.iter().find(|edge| edge.target == start).unwrap().address;
    (address, start)
}
fn rejects_pipe(changed: &Fixture) {
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error:?}");
    assert!(error.message.contains("PIPE machine contract"), "{error:?}");
}
fn redirect_jump(changed: &mut Fixture, address: u64, target: u64) {
    let from = changed.record.blocks.iter().find(|block| block.range.contains(address)).unwrap().id.clone();
    let to = changed.record.blocks.iter().find(|block| block.range.start == target).unwrap().id.clone();
    changed.record.edges.iter_mut().find(|edge| edge.from == from && edge.kind == EdgeKind::Jump).unwrap().to = to;
    let immediate = (target as i64 - address as i64) as u32;
    let instruction = ((immediate >> 20 & 1) << 31)
        | ((immediate >> 1 & 0x3ff) << 21)
        | ((immediate >> 11 & 1) << 20)
        | (immediate & 0xff000)
        | 0x6f;
    changed.replace_machine_word(address, instruction);
}
#[test]
fn pipe_status_mapping_and_helper_slots_reject_rebound_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = fixture(SOURCE, diagnostics);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let (call, helper) = pipe_call(&valid, &elf);
        // Keep the branch's shape and decoded edge but stop testing status.
        let mut no_guard = valid.clone();
        no_guard.replace_machine_word(call + 4, word(&elf, call + 4) & !(31 << 15));
        rejects_pipe(&no_guard);
        for address in [call + 12, call + 16] {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, word(&elf, address) & !(31 << 15));
            rejects_pipe(&changed);
        }
        let mut success_to_abort = valid.clone();
        let abort = valid.record.blocks.iter().find(|block| block.owner_entry == "runtime:__cellscript_abort").unwrap().range.start;
        success_to_abort.redirect_conditional_branch(call + 4, abort);
        rejects_pipe(&success_to_abort);
        let mut ignored_abort = valid.clone();
        // A valid fixed-code error block remains fatal, but cannot replace the
        // helper's returned error status on this generic call boundary.
        let target = valid.record.runtime_error_exits.iter().find(|exit| exit.code == 5).unwrap().address;
        redirect_jump(&mut ignored_abort, call + 8, target);
        rejects_pipe(&ignored_abort);
        let mut displaced_result = valid.clone();
        let entry = displaced_result.record.typed_semantics.entries.iter_mut().find(|entry| entry.id == "action:verify").unwrap();
        let id = entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|op| op.call.as_ref().is_some_and(|call| call.target == "__ckb_pipe"))
            .unwrap()
            .destinations[0];
        let displaced = entry.locals.iter().map(|local| local.source_id).max().unwrap() + 1;
        entry.locals.iter_mut().find(|local| local.id == id).unwrap().source_id = displaced;
        displaced_result.rebind_typed_semantics();
        rejects_pipe(&displaced_result);
        let mut wrong_type = valid.clone();
        let entry = wrong_type.record.typed_semantics.entries.iter_mut().find(|entry| entry.id == "action:verify").unwrap();
        entry.locals.iter_mut().find(|local| local.id == id).unwrap().ty = "(u64, u64, u64)".into();
        wrong_type.rebind_typed_semantics();
        assert!(wrong_type.check().is_err(), "a forged non-pair result must not be admitted");
        let read_load = elf
            .instructions
            .iter()
            .find(|instruction| instruction.address > helper && instruction.word & 0x000fffff == 0x00013583)
            .unwrap();
        let mut wrong_slot = valid.clone();
        wrong_slot.replace_machine_word(read_load.address, (read_load.word & 0x000fffff) | (16 << 20));
        rejects_pipe(&wrong_slot);
    }
}
#[test]
fn pipe_tuple_buffer_cannot_alias_an_earlier_live_wide_value() {
    let source = "module pipe_isolation\npublic action verify() -> u64 { verification let keep: u128 = (process_id() as u128) + 100000000000000000000 let ends = pipe() require close(ends.0) == 0 require close(ends.1) == 0 require keep == 100000000000000000000 return 0 }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = fixture(source, diagnostics);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let (call, _) = pipe_call(&valid, &elf);
        let start = call + 24; // first byte store after guard, remap and mv t0,a0
        let first = word(&elf, start);
        assert_eq!(first & 0x01fff07f, 0x00510023);
        let original = (((first >> 25) << 5) | ((first >> 7) & 31)) as i32;
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.id == "action:verify").unwrap();
        let fixed_start = ((entry.locals.iter().map(|local| local.source_id).max().unwrap() + 1) * 8) as i32;
        assert!(fixed_start >= 0 && fixed_start + 16 <= original && fixed_start % 8 == 0);
        let stores: Vec<_> = elf
            .instructions
            .iter()
            .filter(|word| start <= word.address && word.address < start + 128 && word.word & 0x01fff07f == 0x00510023)
            .take(16)
            .collect();
        assert_eq!(stores.len(), 16);
        let input = ckb_script_runner::build_simple_fixture(ckb_testtool::bytes::Bytes::new(), 1, 1);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &input).exit_code, 0);
        // Runtime-derived u128 arithmetic owns a real 16-byte frame value.
        // Probe the bounded, aligned fixed-value prefix and require a real VM
        // observation of that live value being overwritten, not just rejection
        // of a mutation that happens to hit an inactive temporary bank.
        let mut observed_corruption = false;
        for earlier in (fixed_start..=original - 16).step_by(8) {
            let mut replacements = Vec::new();
            for (index, store) in stores.iter().enumerate() {
                let offset = (earlier + index as i32) as u32;
                replacements.push((store.address, (store.word & 0x01fff07f) | ((offset & 0xfe0) << 20) | ((offset & 31) << 7)));
            }
            // Rebind the published tuple pointer too: this is a consistent
            // overlapping layout, not a dangling pointer-only mutation.
            let pointer = elf
                .instructions
                .iter()
                .find(|word| word.address > replacements.last().unwrap().0 && word.word & 0x000fffff == 0x00010293)
                .unwrap();
            replacements.push((pointer.address, (pointer.word & 0x000fffff) | ((earlier as u32) << 20)));
            let mut changed = valid.clone();
            changed.replace_machine_words(&replacements);
            rejects_pipe(&changed);
            if ckb_script_runner::execute_cellscript_script(&changed.artifact, &input).exit_code == 5 {
                observed_corruption = true;
                break;
            }
        }
        assert!(observed_corruption, "an overlapping PIPE output must corrupt the still-live value observed after cleanup");
    }
}

#[test]
fn bip340_internal_pipe_call_keeps_its_raw_status_and_endpoint_abi() {
    for invocation in [
        "verifier::btc::bip340::require_signature(message, pubkey, signature)",
        "verifier::btc::bip340::require_signature_from_cell_dep(3, message, pubkey, signature)",
    ] {
        let source = format!("module bip340_pipe_abi\npublic action verify(witness verifier_data_hash: Hash, witness message: Hash, witness pubkey: [u8; 32], witness signature: [u8; 64]) -> bool {{ verification let verifier_dep = source::cell_dep(3) ckb::require_cell_data_hash(verifier_dep, verifier_data_hash) {invocation} true }}");
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            fixture(&source, diagnostics);
        }
    }
}
