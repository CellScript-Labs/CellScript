use super::*;

#[test]
fn rebound_bundles_reject_store_immediates_that_disguise_unpaired_auipc_ra() {
    let valid = Fixture::new();
    assert_eq!(valid.check(), Ok(()));
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let position = elf.instructions.iter().rposition(|instruction| instruction.word == 0x00008067).unwrap();
    assert!(position >= 2);
    for function in 0..=3 {
        let mut changed = valid.clone();
        changed.replace_machine_words(&[
            (elf.instructions[position - 2].address, 0x00000097),
            (elf.instructions[position - 1].address, (2 << 15) | (function << 12) | (1 << 7) | 0x23),
        ]);
        // Hashes are rebound by replace_machine_word; this assertion names the
        // actual early ELF rejection, not complete-bundle exploitability.
        assert_code(&changed, CheckerRejectionCode::V2413InstructionInvalid);
    }
}

#[test]
fn rebound_bundles_keep_instruction_length_reserved_encodings_and_partial_call_entries_closed() {
    let valid = Fixture::new();
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let site = elf.instructions.iter().rfind(|instruction| instruction.word == 0x00008067).unwrap().address;
    for word in [0x80824501, 0x0205151b, 0x0000002f, 0x00101073] {
        let mut changed = valid.clone();
        changed.replace_machine_word(site, word);
        assert_code(&changed, CheckerRejectionCode::V2413InstructionInvalid);
    }
    let second =
        elf.instructions.iter().find(|instruction| instruction.word & 0x707f == 0x67 && instruction.word != 0x00008067).unwrap();
    for target in [site + 2, second.address] {
        let mut changed = valid.clone();
        changed.replace_machine_word(site, encode_jal((target as i64 - site as i64) as i32));
        assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);
    }
}

#[test]
fn commitment_bundles_preserve_word_constant_semantics_and_reject_legal_op32_argument_changes() {
    let valid = Fixture::from_source(COMMITTED_STATE_SOURCE);
    valid.check().unwrap();
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let opening = valid
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == "action:inspect"
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lcommitment_opening_header_ready_"))
        })
        .unwrap();
    let memcmp = valid.record.entries.iter().find(|entry| entry.id == "runtime:__cellscript_memcmp_fixed").unwrap();
    let target = valid.record.blocks.iter().find(|block| block.id == memcmp.entry_block).unwrap().range.start;
    let compare = elf.control_flow.iter().find(|flow| flow.address > opening.range.start && flow.target == target).unwrap().address;
    let length = elf
        .instructions
        .iter()
        .rev()
        .find(|instruction| {
            opening.range.start <= instruction.address && instruction.address < compare && instruction.word == 0x02000613
        })
        .unwrap();
    let mut equivalent = valid.clone();
    // ADDIW from x0 still establishes the exact 32-byte length. This is a
    // complete-bundle compatibility check, not a new compiler emission rule.
    equivalent.replace_machine_word(length.address, 0x0200061b);
    equivalent.check().unwrap();
    for word in [0x0000063b, 0x0005063b, 0x4000063b, 0x0200563b] {
        assert!(cellscript_artifact_checker::machine::decode(word).is_some());
        let mut changed = valid.clone();
        changed.replace_machine_word(length.address, word);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}
