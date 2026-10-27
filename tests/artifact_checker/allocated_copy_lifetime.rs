use super::*;
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[test]
fn collection_copies_reject_intervening_buffer_and_pointer_writes() {
    for (width, bytes) in [(1, 512), (8, 512), (9, 512), (16, 512), (32, 512), (64, 512), (9, 65536), (64, 65536)] {
        let literal = format!("b\"{}\"", "a".repeat(width));
        for borrowed in [false, true] {
            let declarations = format!(
                "module collection_copy_lifetime\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 4 }}\n\
                 values: Vec<[u8; {width}]> {{ items: 2 }} }}\n\
                 fn noise(seed: u8) -> u64 {{ if seed == 0 {{ return 9 }}\nreturn 4 }}\n"
            );
            let setup = format!("let mut work = Work::new()\nwork.values.push({literal})\n");
            let source = if borrowed {
                format!(
                    "{declarations}\nfn matches(view: &Work, seed: u8) -> bool {{\n\
                     let copied = view.values.get(0)\nlet value = noise(seed)\n\
                     if value == 9 {{ return false }}\nreturn copied == {literal} }}\n\
                     action inspect(witness seed: u8) -> u64 {{ {setup}\nrequire matches(&work, seed)\nreturn 0 }}"
                )
            } else {
                format!(
                    "{declarations}\naction inspect(witness seed: u8) -> u64 {{ {setup}\n\
                     let copied = work.values.get(0)\nlet value = noise(seed)\n\
                     require value != 9\nrequire copied == {literal}\nreturn 0 }}"
                )
            };
            for opt_level in [0, 3] {
                let compiled = compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap();
                let payload = compiled.metadata.actions[0].entry_witness_args(&[cellscript::EntryWitnessArg::U8(1)]).unwrap();
                let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                transaction.current_type_script_input_indices = vec![0];
                transaction.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                let valid = Fixture::from_result(compiled);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let helper = valid.record.entries.iter().find(|entry| entry.name == "noise").unwrap();
                let start = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
                let call = elf.control_flow.iter().find(|edge| edge.target == start).unwrap();
                let store = elf.instructions.iter().find(|instruction| instruction.address == call.address + 4).unwrap();
                assert_eq!(store.word & 0x707f, 0x3023);
                assert_eq!(store.word >> 15 & 31, 2);
                let owner = if borrowed { "helper:matches" } else { "action:inspect" };
                let (pointer, buffer) = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| block.owner_entry == owner)
                    .flat_map(|block| &block.allocated_lifecycle_labels)
                    .find_map(|label| {
                        let fields = label.strip_prefix(".Lallocated_result_")?.split('_').collect::<Vec<_>>();
                        (fields.len() == 6 && fields[5] == "end")
                            .then(|| (fields[0].parse::<u32>().unwrap() * 8, fields[3].parse::<u32>().unwrap()))
                    })
                    .unwrap();
                for offset in [buffer, buffer + width as u32 - 1, pointer] {
                    assert!(offset < 2048);
                    let mut changed = valid.clone();
                    let word = (store.word & 0x01fff07f) | ((offset >> 5) << 25) | ((offset & 31) << 7);
                    changed.replace_machine_word(store.address, word);
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if offset == buffer {
                        assert_ne!(ckb_script_runner::execute_cellscript_script(&changed.artifact, &transaction).exit_code, 0);
                    }
                }
            }
        }
    }
}

#[test]
fn collection_copy_protection_preserves_mutable_records_and_owner_drop() {
    let source = r#"module mutable_collection_copy
struct Pair { left: u64, right: u64 }
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<Pair> { items: 2 }
}
fn copied(view: &Work) -> Pair { return view.values.get(0) }
action inspect() -> u64 {
    let mut work = Work::new()
    work.values.push(Pair { left: 1, right: 7 })
    let mut current = work.values.get(0)
    let original = current
    current.left = 5
    require original.left == 1
    require current.left == 5
    let returned = copied(&work)
    require work.values.get(0).left == 1
    work.drop()
    require returned.left == 1
    require returned.right == 7
    require original.right == 7
    return 0
}"#;
    for opt_level in [0, 3] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
    }
}
