use super::*;

#[test]
fn unadmitted_section_names_are_rejected_before_any_string_copy() {
    let valid = Fixture::new();
    let mut bad = valid.clone();
    let section_table = u64::from_le_bytes(bad.artifact[40..48].try_into().unwrap()) as usize;
    let section_count = u16::from_le_bytes(bad.artifact[60..62].try_into().unwrap()) as usize;
    let entry_size = 64usize;
    let strtab_header = (1..section_count)
        .map(|index| section_table + index * entry_size)
        .find(|&offset| u32::from_le_bytes(bad.artifact[offset + 4..offset + 8].try_into().unwrap()) == 3)
        .expect("fixture has a .shstrtab section");
    let strtab_offset = u64::from_le_bytes(bad.artifact[strtab_header + 24..strtab_header + 32].try_into().unwrap()) as usize;
    let dot_text =
        bad.artifact[strtab_offset..].windows(6).position(|window| window == b".text\0").expect("string table spells .text");
    // Retarget the first real section header at "ext\0": a name outside the
    // admitted set must be rejected by the table-bytes peek, never by an
    // allocated copy of a possibly huge repeated entry.
    bad.artifact[section_table + entry_size..section_table + entry_size + 4]
        .copy_from_slice(&((strtab_offset + dot_text + 1) as u32).to_le_bytes());
    bad.bind_artifact_identity();
    assert_eq!(bad.check(), Err(CheckerRejectionCode::V2412ElfSectionInvalid));
}

#[test]
fn ret_cannot_consume_an_unpaired_auipc_defined_ra() {
    let valid = Fixture::new();
    assert_eq!(valid.check(), Ok(()));
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    // Find a ret in user text and poison the instruction right before it
    // (skipping canonical auipc/jalr pairs) with `auipc ra, 0`: the return
    // would jump to an attacker-chosen PC-relative address.
    let words = &elf.instructions;
    let position = words
        .iter()
        .rposition(|instruction| instruction.word == 0x0000_8067 && instruction.address >= elf.entry + 20)
        .expect("fixture returns from user text");
    let previous = &words[position - 1];
    assert!(previous.word & 0x7f != 0x17, "instruction before ret must not already be an auipc");
    let mut bad = valid.clone();
    bad.replace_machine_word(previous.address, 0x0000_0097);
    assert_eq!(bad.check(), Err(CheckerRejectionCode::V2413InstructionInvalid));
}

#[test]
fn ld_ra_requires_a_paired_unsaved_frame_slot() {
    let valid = Fixture::new();
    assert_eq!(valid.check(), Ok(()));
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    // Neutralise a real prologue `sd ra, off(sp)` with a no-op: the frame's
    // own `ld ra, off(sp)` now restores a slot nobody saved, which is how a
    // poisoned stack value reaches `ra` and then `ret` outside verified text.
    let save = elf
        .instructions
        .iter()
        .find(|instruction| {
            let word = instruction.word;
            word & 0x7f == 0x23
                && (word >> 12) & 0x7 == 3
                && (word >> 15) & 0x1f == 2
                && (word >> 20) & 0x1f == 1
                && instruction.address >= elf.entry + 20
        })
        .expect("fixture saves ra in a user frame");
    let mut bad = valid.clone();
    bad.replace_machine_word(save.address, 0x0000_0013);
    assert_eq!(bad.check(), Err(CheckerRejectionCode::V2414ControlFlowInvalid));
}

#[test]
fn clobbered_ra_slot_rejects_every_later_restore() {
    // The plain (non-bounded) fixture keeps the typed machine-binding layer
    // out of the picture so this exercises the slot-pairing contract itself.
    let valid = Fixture::new();
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    assert_eq!(valid.check(), Ok(()));
    // Find a real `sd ra, off(sp)` prologue save and the next `ld ra, off(sp)`
    // restore of the same slot; overwrite an instruction in between with
    // a store overlapping off(sp) so the saved return address is clobbered and the
    // restore would load an attacker-chosen stack value into `ra`.
    let decode_save = |word: u32| {
        (word & 0x7f == 0x23 && (word >> 12) & 0x7 == 3 && (word >> 15) & 0x1f == 2 && (word >> 20) & 0x1f == 1)
            .then_some((((word >> 25) & 0x7f) << 5) | ((word >> 7) & 0x1f))
    };
    let decode_load = |word: u32| {
        (word & 0x7f == 0x03 && (word >> 12) & 0x7 == 3 && (word >> 15) & 0x1f == 2 && (word >> 7) & 0x1f == 1)
            .then_some((word >> 20) & 0xfff)
    };
    let mut clobber_site = None;
    for (index, instruction) in elf.instructions.iter().enumerate() {
        let Some(offset) = decode_save(instruction.word) else {
            continue;
        };
        let Some((_, restore)) =
            elf.instructions.iter().skip(index + 1).find_map(|candidate| {
                decode_load(candidate.word).filter(|candidate| *candidate == offset).map(|_| (index, candidate))
            })
        else {
            continue;
        };
        // Pick an instruction strictly between save and restore that is not
        // itself an auipc/jalr pair element.
        let restore_index = elf.instructions.iter().position(|x| x.address == restore.address).unwrap();
        for middle in &elf.instructions[index + 1..restore_index] {
            if middle.word & 0x7f != 0x17 && middle.word & 0x7f != 0x67 {
                clobber_site = Some((middle.address, offset));
                break;
            }
        }
        if clobber_site.is_some() {
            break;
        }
    }
    let (address, offset) = clobber_site.expect("fixture has a save/restore pair to poison");
    for funct3 in 0..=3 {
        let width = 1u32 << funct3;
        for delta in [0, 1, 7, 1 - width as i32] {
            for source in [1, 5] {
                // Include writes beginning before the slot and narrow stores
                // of ra itself; only an exact sd ra is a legitimate resave.
                if funct3 == 3 && delta == 0 && source == 1 {
                    continue;
                }
                let shifted = offset.wrapping_add_signed(delta) & 0xfff;
                let poison = ((shifted >> 5) << 25) | (source << 20) | (2 << 15) | (funct3 << 12) | ((shifted & 0x1f) << 7) | 0x23;
                let mut bad = valid.clone();
                bad.replace_machine_word(address, poison);
                assert_eq!(
                    bad.check(),
                    Err(CheckerRejectionCode::V2414ControlFlowInvalid),
                    "width={width}, delta={delta}, source={source}"
                );
            }
        }
    }
}
