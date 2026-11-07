use super::*;

#[test]
fn all_op32_writes_kill_a_preexisting_expected_pointer_fact() {
    for (high, functions) in [(0, &[0, 1, 5][..]), (1, &[0, 4, 5, 6, 7][..]), (0x20, &[0, 5][..])] {
        for &function in functions {
            let word = high << 25 | 10 << 15 | function << 12 | 11 << 7 | 0x3b;
            let mut values = [CommitmentMachineValue::Unknown; 32];
            values[0] = CommitmentMachineValue::Constant(0);
            values[10] = CommitmentMachineValue::StackAddress(512);
            values[11] = CommitmentMachineValue::StackLoadedAddress { stack_slot: 16, displacement: 0 };
            update_commitment_registers(&mut values, word);
            assert_eq!(values[11], CommitmentMachineValue::Unknown, "{word:#010x}");
            assert_eq!(values[10], CommitmentMachineValue::StackAddress(512));
        }
    }
}

#[test]
fn syscall_and_opaque_call_effects_do_not_preserve_stale_argument_facts_or_reset_sp() {
    let mut values = [CommitmentMachineValue::Constant(17); 32];
    values[0] = CommitmentMachineValue::Constant(0);
    values[2] = CommitmentMachineValue::StackAddress(-32);
    update_commitment_registers(&mut values, 0x00000073);
    assert_eq!(values[10], CommitmentMachineValue::Unknown);
    for word in [0x000080e7, 0x000000ef] {
        let mut called = values;
        update_commitment_registers(&mut called, word);
        for (register, value) in called.iter().enumerate() {
            if crate::machine::caller_saved(register as u32) {
                assert_eq!(*value, CommitmentMachineValue::Unknown);
            }
        }
        assert_eq!(called[2], CommitmentMachineValue::StackAddress(-32));
        assert_eq!(called[8], CommitmentMachineValue::Constant(17));
    }
}

fn local_flow(words: &[u32], edges: &[(u64, u64)]) -> ParsedElf {
    // Synthetic decoded words test this local transfer, not ELF/bundle admission.
    let section = crate::elf::ElfSection {
        name: ".text".into(),
        section_type: 1,
        flags: 6,
        address: 0,
        offset: 0,
        size: words.len() as u64 * 4,
    };
    ParsedElf {
        entry: 0,
        sections: vec![section.clone()],
        segments: vec![],
        text: section.clone(),
        rodata: crate::elf::ElfSection { name: ".rodata".into(), flags: 2, size: 0, ..section },
        instruction_count: words.len() as u64,
        instructions: words
            .iter()
            .enumerate()
            .map(|(index, &word)| crate::elf::DecodedInstruction { address: index as u64 * 4, word })
            .collect(),
        stack_adjustments: vec![],
        syscall_addresses: vec![],
        control_flow: edges
            .iter()
            .map(|&(address, target)| crate::elf::DecodedControlFlow {
                address,
                target,
                kind: crate::elf::DecodedControlFlowKind::ConditionalBranch,
            })
            .collect(),
    }
}

#[test]
fn commitment_facts_are_intersections_of_actual_paths_not_physical_instruction_order() {
    // a1=32; conditional branch to use; fallthrough overwrites a1=31.
    let elf = local_flow(&[0x02000593, 0x00050463, 0x01f00593], &[(4, 12)]);
    assert_eq!(commitment_registers_before(&elf, 0, 12)[11], CommitmentMachineValue::Unknown);
    let same = local_flow(&[0x02000593, 0x00050463, 0x02000593], &[(4, 12)]);
    assert_eq!(commitment_registers_before(&same, 0, 12)[11], CommitmentMachineValue::Constant(32));
    // Branch bypasses the only constant definition.
    let bypass = local_flow(&[0x00050463, 0x02000593], &[(0, 8)]);
    assert_eq!(commitment_registers_before(&bypass, 0, 8)[11], CommitmentMachineValue::Unknown);
    // An independent edge into the middle must not inherit the normal prefix.
    let mut external = local_flow(&[0x02000593, 0x00000013], &[(16, 4)]);
    external.control_flow[0].kind = crate::elf::DecodedControlFlowKind::DirectJump;
    assert_eq!(commitment_registers_before(&external, 0, 8)[11], CommitmentMachineValue::Unknown);
}
