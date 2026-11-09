use super::*;

#[cfg(feature = "cli")]
use super::common;

#[test]
#[cfg(feature = "cli")]
fn nested_call_aliases_require_independent_nominal_bindings_and_exact_bounds() {
    let root = tempfile::tempdir().unwrap();
    for owner in ["left", "right", "consumer"] {
        let package = root.path().join(owner);
        std::fs::create_dir_all(package.join("src")).unwrap();
        let dependencies =
            if owner == "consumer" { "[dependencies]\nleft = { path = \"../left\" }\nright = { path = \"../right\" }\n" } else { "" };
        std::fs::write(package.join("Cell.toml"), format!("[package]\nname = \"{owner}\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n{dependencies}")).unwrap();
        let source = if owner == "consumer" {
            "module consumer;\nuse left::Entry as First;\nuse left::Entry as Alias;\nuse right::Entry as Other;\nfn inspect(value: NestedRecordView<Alias, 8>) -> u64 { return value.key; }\naction verify(witness values: NestedValues<First, 2, 8, 128>) -> u64 {  require inspect(values[0]) == 42; return 0; }".into()
        } else {
            format!("module {owner};\npublic struct Entry {{ key: u64, payload: Vec<u8>, }}")
        };
        std::fs::write(package.join("src/main.cell"), source).unwrap();
    }
    let consumer = root.path().join("consumer");
    let locked = common::cellc_command().current_dir(&consumer).args(["lock", "--json"]).output().unwrap();
    assert!(locked.status.success(), "{} {}", String::from_utf8_lossy(&locked.stdout), String::from_utf8_lossy(&locked.stderr));
    let compiled = cellscript::compile_path_with_executable_surface_policy(
        camino::Utf8Path::from_path(&consumer).unwrap(),
        CompileOptions { source_contracts: false, target: Some("riscv64-elf".into()), ..Default::default() },
        None,
        cellscript::ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let valid = Fixture::from_result(compiled);
    let mut missing = valid.clone();
    missing.record.typed_semantics.nominal_declarations = None;
    missing.rebind_layered_identities();
    assert_code(&missing, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    let mut rebound = valid.clone();
    let binding = rebound
        .record
        .typed_semantics
        .nominal_declarations
        .as_mut()
        .unwrap()
        .layout_bindings
        .iter_mut()
        .find(|binding| binding.lowered_name == "Alias")
        .unwrap();
    assert_eq!(binding.owner_module, "left");
    binding.owner_module = "right".into();
    rebound.rebind_layered_identities();
    assert_code(&rebound, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    for wrong in ["NestedRecordView<Other, 8>", "NestedRecordView<Alias, 7>"] {
        let mut changed = valid.clone();
        let call = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .filter_map(|operation| operation.call.as_mut())
            .find(|call| call.target == "inspect")
            .unwrap();
        call.params[0] = wrong.into();
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn bounded_callers_reserve_only_the_declared_plain_helper_stack_arguments() {
    let source = r#"module mixed_helper_frames;
resource Token has store { amount: u64, }
fn plain(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, i: u64, j: u64) -> u64 {
    return i + j;
}
fn bounded(view: InputView<Token>, delta: u64) -> u64 { return view.data.amount + delta; }
action inspect(witness i: u64, witness j: u64) -> u64 {

    require bounded(ckb::input::<Token>(0), 4) == 7;
    require plain(0, 0, 0, 0, 0, 0, 0, 0, i, j) == 3;
    return 0;
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(
            source,
            CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let payload = compiled.metadata.actions[0]
            .entry_witness_args(&[cellscript::EntryWitnessArg::U64(1), cellscript::EntryWitnessArg::U64(2)])
            .unwrap();
        let valid = Fixture::from_result(compiled);
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.current_type_script_input_indices = vec![0];
        transaction.inputs[0].data = 3u64.to_le_bytes().to_vec().into();
        transaction.witnesses =
            vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let store = elf
            .instructions
            .iter()
            .find(|instruction| {
                instruction.word & 0x707f == 0x3023
                    && instruction.word >> 15 & 31 == 2
                    && ((instruction.word >> 7 & 31) | (instruction.word >> 25 << 5)) == 0xff0
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry.starts_with("action:") && block.range.contains(instruction.address))
            })
            .unwrap();
        // The ten scalar words need exactly 16 outgoing bytes. Moving the
        // first stack argument to -24 must not gain additional private storage.
        let mut outside = valid.clone();
        let offset = 0xfe8u32;
        let word = (store.word & !0xfe000f80) | ((offset >> 5) << 25) | ((offset & 31) << 7);
        outside.replace_machine_word(store.address, word);
        assert_code(&outside, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn bounded_scalar_calls_bind_forwarded_sources_and_preserve_prepared_arguments() {
    for (ty, argument, read) in
        [("InputView<Token>", "ckb::input::<Token>(0)", "view.data.amount"), ("Messages::Item", "messages[0]", "view.len()")]
    {
        for padding in [0, 8] {
            let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
            let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
            let args = "0, ".repeat(padding);
            let source = format!(
                r#"module bounded_scalar_calls;
resource Token has store {{ amount: u64, }}
data_shape Messages {{ element: Vec<u8>, limits {{ items: 2, payload_bytes: 8, encoded_bytes: 32 }} }}
fn identity(value: u64) -> u64 {{ return value; }}
fn read({params}a: u64, view: {ty}, bytes: Hash, b: u64) -> u64 {{ return a + b + {read}; }}
fn forward({params}a: u64, view: {ty}, bytes: Hash, b: u64) -> u64 {{
    return read({forwarded}identity(a), view, bytes, b);
}}
action inspect(witness a: u64, witness b: u64, witness skip: bool, witness bytes: Hash, witness messages: Messages) -> u64 {{

    if skip {{ return 0; }}
    require forward({args}a, {argument}, bytes, b) == a + b + 3;
    return 0;
}}"#
            );
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap_or_else(|error| panic!("{ty}/{padding}/{diagnostics}: {error}"));
                use cellscript::EntryWitnessArg as A;
                use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
                let messages = [15u32.to_le_bytes().as_slice(), &8u32.to_le_bytes(), &3u32.to_le_bytes(), &[1, 2, 3]].concat();
                let payload = compiled.metadata.actions[0]
                    .entry_witness_args(&[A::U64(1), A::U64(2), A::Bool(false), A::Hash([3; 32]), A::Bytes(messages)])
                    .unwrap();
                let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                transaction.current_type_script_input_indices = vec![0];
                transaction.inputs[0].data = 3u64.to_le_bytes().to_vec().into();
                transaction.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                let valid = Fixture::from_result(compiled);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
                let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.kind == "action").unwrap();
                let source_slot = |name| {
                    let id = typed.params.iter().find(|param| param.name == name).unwrap().binding_id;
                    typed.locals.iter().find(|local| local.id == id).unwrap().source_id as u32 * 8
                };
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let spans = allocated_calls::spans(&valid);
                assert_eq!(spans.len(), (padding + 2) * 2);
                let (_, _, start, end) = spans
                    .iter()
                    .find(|(owner, label, _, _)| owner == &typed.id && label.ends_with(&format!("_{padding}_begin")))
                    .unwrap();
                let load = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        (*start..*end).contains(&instruction.address)
                            && instruction.word & 0x707f == 0x3003
                            && instruction.word >> 15 & 31 == 2
                    })
                    .unwrap();
                assert_eq!(load.word >> 20, source_slot("a"));
                let mut substituted = valid.clone();
                substituted.replace_machine_word(load.address, (load.word & 0xfffff) | (source_slot("b") << 20));
                assert_code(&substituted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                assert_ne!(ckb_script_runner::execute_cellscript_script(&substituted.artifact, &transaction).exit_code, 0);

                let invoke = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| {
                        block.owner_entry == typed.id
                            && block
                                .allocated_lifecycle_labels
                                .iter()
                                .any(|label| label.starts_with(".Lallocated_call_") && label.ends_with("_invoke"))
                    })
                    .unwrap()
                    .range
                    .start;
                let branch = elf
                    .instructions
                    .iter()
                    .rev()
                    .find(|instruction| {
                        instruction.address < *start
                            && instruction.word & 0x7f == 0x63
                            && valid
                                .record
                                .blocks
                                .iter()
                                .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                    })
                    .unwrap();
                let prepared = spans
                    .iter()
                    .filter(|(owner, _, _, end)| owner == &typed.id && *end <= invoke)
                    .map(|(_, _, _, end)| *end)
                    .max()
                    .unwrap();
                let mut bypass = valid.clone();
                bypass.redirect_conditional_branch(branch.address, prepared);
                assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                if padding == 0 {
                    // The unused Hash extent follows a's checked load. Reuse
                    // that instruction to overwrite a0 without touching a's
                    // preparation span or the coordinate/pointer arguments.
                    let length = elf
                        .instructions
                        .iter()
                        .find(|instruction| {
                            (*end..invoke).contains(&instruction.address)
                                && instruction.word >> 7 & 31 == 14
                                && matches!(instruction.word & 0x7f, 3 | 0x13)
                        })
                        .unwrap();
                    let mut clobbered = valid.clone();
                    clobbered.replace_machine_word(length.address, (length.word & !(31 << 7)) | (10 << 7));
                    assert_code(&clobbered, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    assert_ne!(ckb_script_runner::execute_cellscript_script(&clobbered.artifact, &transaction).exit_code, 0);
                }
            }
        }
    }
}
