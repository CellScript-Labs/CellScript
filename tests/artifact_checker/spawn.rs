use super::*;

fn fixture(diagnostics: cellscript::CompilerDiagnostics, explicit: bool) -> Fixture {
    let invocation = if explicit {
        "verifier::btc::bip340::require_signature_from_cell_dep(3, message, pubkey, signature)"
    } else {
        "verifier::btc::bip340::require_signature(message, pubkey, signature)"
    };
    let source = format!("module spawn_binding\npublic action verify(witness verifier_data_hash: Hash, witness message: Hash, witness pubkey: [u8; 32], witness signature: [u8; 64]) -> bool {{ verification let verifier_dep = source::cell_dep(3) ckb::require_cell_data_hash(verifier_dep, verifier_data_hash) {invocation} true }}");
    Fixture::from_result(
        compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
    )
}

fn rejects_spawn(changed: &Fixture) {
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error:?}");
    assert!(error.message.contains("SPAWN machine contract"), "{error:?}");
}

#[test]
fn spawn_private_buffers_child_id_and_status_reject_rebound_machine_changes() {
    for (diagnostics, explicit) in
        [(cellscript::CompilerDiagnostics::default(), false), (cellscript::CompilerDiagnostics::without_optimizations(), true)]
    {
        let valid = fixture(diagnostics, explicit);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let entry = valid.record.entries.iter().find(|entry| entry.id == "runtime:__ckb_spawn_with_fd1").unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
        let owned = elf
            .instructions
            .iter()
            .filter(|instruction| {
                valid.record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
            })
            .collect::<Vec<_>>();
        let unique = |mask, expected| {
            let matches = owned.iter().filter(|instruction| instruction.word & mask == expected).collect::<Vec<_>>();
            assert_eq!(matches.len(), 1, "ambiguous SPAWN mutation {expected:#x}/{mask:#x}");
            matches[0].address
        };
        let word = |address| owned.iter().find(|instruction| instruction.address == address).unwrap().word;
        let store = |register: u32, offset: u32| {
            ((offset >> 5) << 25) | (register << 20) | (2 << 15) | (3 << 12) | ((offset & 31) << 7) | 0x23
        };
        let terminator = unique(u32::MAX, store(0, 16));
        let pid_pointer = unique(u32::MAX, (24 << 20) | (2 << 15) | (5 << 7) | 0x13);
        let inherited_pointer = unique(u32::MAX, (8 << 20) | (2 << 15) | (5 << 7) | 0x13);
        let pid = unique(u32::MAX, (24 << 20) | (2 << 15) | (3 << 12) | (11 << 7) | 3);
        let status = unique(u32::MAX, (10 << 7) | 0x13);
        let branch = unique(0x1fff07f, (10 << 15) | (1 << 12) | 0x63);
        for (address, replacement) in [
            (terminator, store(0, 24)),
            (terminator, word(terminator) & !(3 << 12)),
            (pid_pointer, (word(pid_pointer) & 0x000fffff) | (72 << 20)),
            (inherited_pointer, (word(inherited_pointer) & 0x000fffff) | (72 << 20)),
            (pid, (word(pid) & 0x000fffff) | (16 << 20)),
            (pid, (word(pid) & !(7 << 12)) | (4 << 12)),
            (status, word(status) | (1 << 20)),
            (branch, word(branch) ^ (1 << 12)),
        ] {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, replacement);
            rejects_spawn(&changed);
        }
        let calls = elf.control_flow.iter().filter(|edge| edge.target == start).collect::<Vec<_>>();
        assert_eq!(calls.len(), 1);
        let guard = calls[0].address + 4;
        let guard_word = elf.instructions.iter().find(|instruction| instruction.address == guard).unwrap().word;
        for replacement in [guard_word & !(31 << 15), guard_word ^ (1 << 12)] {
            let mut changed = valid.clone();
            changed.replace_machine_word(guard, replacement);
            rejects_spawn(&changed);
        }
        let mut wrong_success = valid.clone();
        let abort = valid.record.blocks.iter().find(|block| block.owner_entry == "runtime:__cellscript_abort").unwrap().range.start;
        wrong_success.redirect_conditional_branch(guard, abort);
        rejects_spawn(&wrong_success);
    }
}

#[test]
fn every_spawn_call_requires_its_own_guard() {
    let invocation = "verifier::btc::bip340::require_signature_from_cell_dep(3, message, pubkey, signature)";
    let source = format!("module repeated_spawn\npublic action verify(witness verifier_data_hash: Hash, witness message: Hash, witness pubkey: [u8; 32], witness signature: [u8; 64]) -> bool {{ verification let verifier_dep = source::cell_dep(3) ckb::require_cell_data_hash(verifier_dep, verifier_data_hash) {invocation} {invocation} true }}");
    let valid =
        Fixture::from_result(compile(&source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap());
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let entry = valid.record.entries.iter().find(|entry| entry.id == "runtime:__ckb_spawn_with_fd1").unwrap();
    let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
    let calls = elf.control_flow.iter().filter(|edge| edge.target == start).collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    for call in calls {
        let address = call.address + 4;
        let original = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
        let mut changed = valid.clone();
        changed.replace_machine_word(address, original & !(31 << 15));
        rejects_spawn(&changed);
    }
}

#[test]
fn exact_zk_spawn_calls_bind_each_profile_and_reject_guard_bypasses() {
    let hash = |byte: u8| format!("Hash::from_bytes(b\"{}\")", format!("\\x{byte:02x}").repeat(32));
    let source = format!(
        r#"module zk_spawn_binding
action update(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle) -> u64 {{
    verification
    zk::require_valid("transition", proof, ckb::cell_dep(0), verifier, {}, {}, {}, {})
    return 0
}}
"#,
        hash(1),
        hash(2).replace("Hash::", "VerificationKeyCommitment::"),
        hash(3),
        hash(4)
    );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &source,
                CompileOptions {
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    target_profile: Some("ckb".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let entry = valid.record.entries.iter().find(|entry| entry.id == "runtime:__ckb_spawn_with_fd1").unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
        let calls = elf.control_flow.iter().filter(|edge| edge.target == start).collect::<Vec<_>>();
        assert_eq!(calls.len(), 1);
        let guard = calls[0].address + 4;
        let original = elf.instructions.iter().find(|instruction| instruction.address == guard).unwrap().word;
        for replacement in [original & !(31 << 15), original ^ (1 << 12)] {
            let mut changed = valid.clone();
            changed.replace_machine_word(guard, replacement);
            rejects_spawn(&changed);
        }
        let success = elf.control_flow.iter().find(|edge| edge.address == guard).unwrap().target;
        let later = valid.record.blocks.iter().map(|block| block.range.start).filter(|address| *address > success).min().unwrap();
        let mut changed = valid.clone();
        changed.redirect_conditional_branch(guard, later);
        rejects_spawn(&changed);
    }
}

#[test]
fn mixed_bip340_and_zk_spawn_calls_keep_distinct_failure_guards() {
    let hash = |byte: u8| format!("Hash::from_bytes(b\"{}\")", format!("\\x{byte:02x}").repeat(32));
    let bip = "let verifier_dep = source::cell_dep(3) ckb::require_cell_data_hash(verifier_dep, verifier_data_hash) verifier::btc::bip340::require_signature_from_cell_dep(3, message, pubkey, signature)";
    let zk = format!(
        "zk::require_valid(\"transition\", proof, ckb::cell_dep(0), verifier, {}, {}, {}, {})",
        hash(1),
        hash(2).replace("Hash::", "VerificationKeyCommitment::"),
        hash(3),
        hash(4)
    );
    for bip_first in [true, false] {
        let body = if bip_first { format!("{bip}\n{zk}") } else { format!("{zk}\n{bip}") };
        let source = format!(
            "module mixed_spawn_binding\naction update(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle, witness verifier_data_hash: Hash, witness message: Hash, witness pubkey: [u8; 32], witness signature: [u8; 64]) -> u64 {{ verification\n{body}\nreturn 0 }}"
        );
        let valid = Fixture::from_result(
            compile(
                &source,
                CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let entry = valid.record.entries.iter().find(|entry| entry.id == "runtime:__ckb_spawn_with_fd1").unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
        let calls = elf.control_flow.iter().filter(|edge| edge.target == start).collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        for call in calls {
            let guard = call.address + 4;
            let original = elf.instructions.iter().find(|instruction| instruction.address == guard).unwrap().word;
            let mut changed = valid.clone();
            changed.replace_machine_word(guard, original & !(31 << 15));
            rejects_spawn(&changed);
        }
    }
}
