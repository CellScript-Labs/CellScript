use super::*;

fn rejecting_entry() -> Fixture {
    Fixture::from_source("module loaded_image\naction main() -> u64 { verification return 7 }\n")
}

fn execute(fixture: &Fixture) -> i64 {
    let tx = ckb_script_runner::build_simple_fixture(ckb_types::bytes::Bytes::new(), 1, 1);
    ckb_script_runner::execute_cellscript_script(&fixture.artifact, &tx).exit_code
}

#[test]
fn entry_trampoline_cannot_replace_a_rejecting_action_with_success() {
    let valid = rejecting_entry();
    assert_eq!(execute(&valid), 7);
    let mut bad = valid.clone();
    let entry = parse_elf(&bad.artifact, CheckerBudgets::default().instructions).unwrap().entry;
    bad.replace_machine_words(&[(entry, 0x00000513), (entry + 4, 0x05d00893), (entry + 8, 0x00000073)]);
    assert_eq!(execute(&bad), 0, "attack must bypass the rejecting action in the VM");
    assert_eq!(bad.check(), Err(CheckerRejectionCode::V2414ControlFlowInvalid));
}

#[test]
fn section_shadow_cannot_hide_the_actual_loaded_entry() {
    let valid = rejecting_entry();
    assert_eq!(execute(&valid), 7);
    let mut bad = valid.clone();
    let elf = parse_elf(&bad.artifact, CheckerBudgets::default().instructions).unwrap();
    let original = bad.artifact[elf.text.offset as usize..(elf.text.offset + elf.text.size) as usize].to_vec();
    let shadow_offset = bad.artifact.len() as u64;
    bad.artifact.extend_from_slice(&original);
    let section_table = u64::from_le_bytes(bad.artifact[40..48].try_into().unwrap()) as usize;
    let section_count = u16::from_le_bytes(bad.artifact[60..62].try_into().unwrap()) as usize;
    let text_header = (1..section_count)
        .map(|index| section_table + index * 64)
        .find(|&offset| u64::from_le_bytes(bad.artifact[offset + 8..offset + 16].try_into().unwrap()) == 6)
        .unwrap();
    bad.artifact[text_header + 24..text_header + 32].copy_from_slice(&shadow_offset.to_le_bytes());
    for (index, word) in [0x00000513u32, 0x05d00893, 0x00000073].into_iter().enumerate() {
        let offset = elf.text.offset as usize + index * 4;
        bad.artifact[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    bad.bind_artifact_identity();
    assert_eq!(execute(&bad), 0, "VM must execute PT_LOAD bytes, not the shadow section");
    assert_eq!(bad.check(), Err(CheckerRejectionCode::V2412ElfSectionInvalid));
}

#[test]
fn loaded_image_rejects_unbacked_sections_overlaps_and_entry_skips() {
    let valid = rejecting_entry();
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();

    let mut unbacked = valid.clone();
    let program_table = u64::from_le_bytes(unbacked.artifact[32..40].try_into().unwrap()) as usize;
    let file_size = elf.text.offset + elf.text.size - 4;
    unbacked.artifact[program_table + 32..program_table + 40].copy_from_slice(&file_size.to_le_bytes());
    unbacked.bind_artifact_identity();
    assert_eq!(unbacked.check(), Err(CheckerRejectionCode::V2412ElfSectionInvalid));

    let mut overlapping = valid.clone();
    let header = overlapping.artifact[program_table..program_table + 56].to_vec();
    let offset = overlapping.artifact.len() as u64;
    overlapping.artifact.extend_from_slice(&header);
    overlapping.artifact.extend_from_slice(&header);
    overlapping.artifact[32..40].copy_from_slice(&offset.to_le_bytes());
    overlapping.artifact[56..58].copy_from_slice(&2u16.to_le_bytes());
    overlapping.bind_artifact_identity();
    assert_eq!(overlapping.check(), Err(CheckerRejectionCode::V2411ElfFormatInvalid));

    let mut skipped = valid.clone();
    skipped.artifact[24..32].copy_from_slice(&(elf.entry + 8).to_le_bytes());
    skipped.bind_artifact_identity();
    assert_eq!(skipped.check(), Err(CheckerRejectionCode::V2414ControlFlowInvalid));

    let mut exit_status = valid.clone();
    exit_status.replace_machine_word(elf.entry + 12, 0x05e88893);
    assert_eq!(exit_status.check(), Err(CheckerRejectionCode::V2414ControlFlowInvalid));
}

#[test]
fn startup_cannot_call_a_different_checked_entry() {
    let valid = Fixture::new();
    let mut bad = valid.clone();
    let elf = parse_elf(&bad.artifact, CheckerBudgets::default().instructions).unwrap();
    let helper = bad.record.entries.iter().find(|entry| entry.name == "increment").unwrap();
    let target = bad.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
    let displacement = target as i64 - elf.entry as i64;
    let high = (displacement + 0x800) >> 12;
    let low = displacement - (high << 12);
    bad.replace_machine_words(&[(elf.entry, ((high as u32) << 12) | 0x97), (elf.entry + 4, ((low as u32 & 4095) << 20) | 0x80e7)]);
    assert_eq!(bad.check(), Err(CheckerRejectionCode::V2414ControlFlowInvalid));
}

#[test]
fn declared_fallthrough_must_follow_machine_instruction_order() {
    let valid = Fixture::new();
    let mut accepted = Vec::new();
    let mut attempted = 0;
    for index in 0..valid.record.edges.len() {
        let edge = &valid.record.edges[index];
        if !matches!(edge.kind, EdgeKind::Fallthrough | EdgeKind::ConditionalFallthrough) {
            continue;
        }
        attempted += 1;
        let mut bad = valid.clone();
        bad.record.edges[index].to = edge.from.clone();
        let mut reachable = std::collections::BTreeSet::new();
        let mut pending = bad.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if reachable.insert(id.clone()) {
                pending.extend(bad.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
            }
        }
        for block in &mut bad.record.blocks {
            block.reachable = reachable.contains(&block.id);
        }
        bad.record.canonicalize();
        bad.rebind_sidecars();
        if bad.check().is_ok() {
            accepted.push(edge.from.clone());
        }
    }
    assert!(attempted > 0);
    assert!(accepted.is_empty(), "accepted forged fallthroughs: {accepted:?}");
}
