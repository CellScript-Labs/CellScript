//! Actual CKB syscalls and every required Script group, independent of the
//! fault-injection syscall fixture in the parent module.
use super::*;
use ckb_testtool::{
    ckb_types::{
        bytes::Bytes,
        core::{ScriptHashType, TransactionBuilder},
        packed,
        prelude::*,
    },
    context::Context,
};

mod migration;
mod settlement;

fn code_identity(tag: u8) -> packed::Script {
    packed::Script::new_builder()
        .code_hash([0x77; 32].pack())
        .hash_type(ScriptHashType::Type)
        .args(Bytes::from(vec![tag; 32]).pack())
        .build()
}

fn script(identity: &packed::Script, tag: u8) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(identity.calc_script_hash())
        .hash_type(ScriptHashType::Type)
        .args(Bytes::from(vec![tag]).pack())
        .build()
}

fn hash(script: &packed::Script) -> [u8; 32] {
    script.calc_script_hash().unpack()
}

fn deploy(context: &mut Context, identity: packed::Script, elf: Vec<u8>) {
    context.create_cell(
        packed::CellOutput::new_builder().capacity(100_000_000_000u64).type_(Some(identity).pack()).build(),
        Bytes::from(elf),
    );
}

fn exit_program(status: u8) -> Vec<u8> {
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    for line in [
        ".section .text".to_owned(),
        ".global entry".into(),
        "entry:".into(),
        format!("li a0, {status}"),
        "li a7, 93".into(),
        "ecall".into(),
    ] {
        generator.emit(line);
    }
    assemble_generated_elf(&generator.assembly).unwrap()
}

#[derive(Clone, Copy, Debug)]
enum Change {
    None,
    Custody,
    Layout,
    Duplicate,
    Pair,
    Missing,
    ExtraFee,
    PeerReject,
    NestedPayload,
}

fn source(contract: &Contract) -> String {
    let mut source = String::from("module exact_roles::source\n");
    for (index, role) in contract.roles.iter().enumerate() {
        source.push_str(&format!("resource Role{index} has store {{\n"));
        let mut prefix = role.key_offset;
        let mut field = 0;
        while prefix > 0 {
            let width = prefix.min(256);
            source.push_str(&format!("prefix{field}: [u8; {width}],\n"));
            prefix -= width;
            field += 1;
        }
        source.push_str(&format!("id: [u8; {}],\n", role.key_width));
        let suffix = role.width - role.key_offset - role.key_width;
        if suffix > 0 {
            source.push_str(&format!("suffix: [u8; {suffix}],\n"));
        }
        source.push_str("}\n");
    }
    source.push_str("action verify() -> u64 {\nverification\nlet input_count = ckb::cell_count(ckb::input<Role0>(0))\n");
    let verification_start = source.len();
    source.push_str(&format!(
        "ckb::verify_cell_roles({}, {}, {}, {},\n",
        contract.input_scan, contract.output_scan, contract.selected_per_side, contract.aggregate_bytes
    ));
    for (index, role) in contract.roles.iter().enumerate() {
        source.push_str(&format!(
            "ckb::exact_cell_role<Role{index}>(\"0x{}\", \"0x{}\", \"id\", \"{}\", \"preserve\", {}, {}, {}, {}){}\n",
            hex::encode(role.type_hash),
            hex::encode(role.lock_hash),
            if role.access == Access::LocalType { "local-type" } else { "observe" },
            role.inputs.min,
            role.inputs.max,
            role.outputs.min,
            role.outputs.max,
            if index + 1 == contract.roles.len() { "" } else { "," }
        ));
    }
    source.push_str(")\n");
    // Reuse the reserved frame and keep an ordinary local alive across both
    // scans; the source operation must neither retain stale records nor clobber it.
    let second_verification = source[verification_start..].to_string();
    source.push_str(&second_verification);
    source.push_str("require input_count == ckb::cell_count(ckb::input<Role0>(0))\nreturn 0\n}\n");
    source
}

#[derive(Clone, Copy, Debug)]
enum SelectionReads {
    Valid,
    End,
    High,
}

#[derive(Clone, Copy, Debug)]
enum CatalogForm {
    Positional,
    Named,
    Reordered,
    Renamed,
    NestedPrimitive,
    NestedSource,
    NestedPositional,
}

fn named_source(contract: &Contract) -> String {
    let primitive = source(contract);
    let mut source = primitive.split_once("action verify").unwrap().0.to_string();
    source.push_str(&format!(
        "cell_roles Settlement {{\nlimits {{ input_scan: {}, output_scan: {}, selected_per_side: {}, aggregate_bytes: {} }}\n",
        contract.input_scan, contract.output_scan, contract.selected_per_side, contract.aggregate_bytes
    ));
    for (index, role) in contract.roles.iter().enumerate() {
        source.push_str(&format!("role_{index}: Role{index} {{\ntype_hash: \"0x{}\", lock_hash: \"0x{}\", identity: id, authority: {}, correspondence: preserve, inputs: {}..={}, outputs: {}..={}\n}}\n",
            hex::encode(role.type_hash), hex::encode(role.lock_hash), if role.access == Access::LocalType { "local_type" } else { "observe" },
            role.inputs.min, role.inputs.max, role.outputs.min, role.outputs.max));
    }
    source.push_str("}\naction verify() -> u64 {\nverification\nlet input_count = ckb::cell_count(ckb::input<Role0>(0))\nckb::verify_cell_roles<Settlement>()\nckb::verify_cell_roles<Settlement>()\nrequire input_count == ckb::cell_count(ckb::input<Role0>(0))\nreturn 0\n}\n");
    source
}

fn source_program(
    contract: &Contract,
    opt_level: u8,
    nested: bool,
    reads: Option<SelectionReads>,
    form: CatalogForm,
) -> crate::CompileResult {
    let named = !matches!(form, CatalogForm::Positional | CatalogForm::NestedPositional);
    let nested_roles = matches!(form, CatalogForm::NestedSource | CatalogForm::NestedPositional);
    let mut source = if named { named_source(contract) } else { source(contract) };
    if nested_roles {
        let start = source.find("resource Role0 has store {").unwrap();
        let end = start + source[start..].find('}').unwrap() + 1;
        source.replace_range(start..end, &format!(
            "struct Entry {{ tag: u8, payload: Vec<u8> }}\nresource Role0 has store {{ messages: NestedValues<Vec<u8>, 32, 256, 4096>, id: [u8; {}], entries: NestedValues<Entry, 32, 256, 4096> }}",
            contract.roles[0].key_width));
        if named {
            source = source.replace("messages: NestedValues<Vec<u8>, 32, 256, 4096>", "messages: Messages")
                .replace("entries: NestedValues<Entry, 32, 256, 4096>", "entries: Entries")
                .replace("struct Entry", "data_shape Messages { element: Vec<u8>, limits { items: 32, payload_bytes: 256, encoded_bytes: 4096 } }\ndata_shape Entries { element: Entry, limits { items: 32, payload_bytes: 256, encoded_bytes: 4096 } }\nstruct Entry");
        }
    }
    if let Some(reads) = reads {
        source = if named {
            source.replacen("ckb::verify_cell_roles<Settlement>()", "let selected = ckb::select_cell_roles<Settlement>()", 1)
        } else {
            source.replacen("ckb::verify_cell_roles(", "let selected = ckb::select_cell_roles(", 1)
        };
        let role = &contract.roles[0];
        let mut checks = format!("let other = ckb::select_cell_roles({}, {}, {}, {}, ckb::exact_cell_role<Role0>(\"0x{}\", \"0x{}\", \"id\", \"local-type\", \"preserve\", 0, 32, 0, 32))\nlet alias = selected\n", contract.input_scan, contract.output_scan, contract.selected_per_side, contract.aggregate_bytes, hex::encode(role.type_hash), hex::encode(role.lock_hash));
        let per_role = if contract.roles.len() == 8 { 4 } else { 1 };
        if named {
            checks = format!("let other = ckb::select_cell_roles<Settlement>()\nlet alias = selected\nrequire other.role_0.input_count() == {per_role}\n");
        } else {
            checks.push_str(&format!("require ckb::selected_input_count<Role0>(other, 0) == {per_role}\n"));
        }
        for (index, role) in contract.roles.iter().enumerate() {
            checks.push_str("{\n");
            let ordinal = match reads {
                SelectionReads::Valid => "count - 1",
                SelectionReads::End => "count",
                SelectionReads::High => "18446744073709551615",
            };
            if named {
                checks.push_str(&format!("let role = alias.role_{index}\nlet role_alias = role\nlet count = role_alias.input_count()\nrequire count == {per_role}\nrequire selected.role_{index}.output_count() == count\nlet input = role_alias.input({ordinal})\nlet output = role.output(0)\nlet partner = selected.role_{index}.successor(count - 1)\nrequire input.capacity == partner.capacity\nrequire output.type_hash == input.type_hash\nrequire partner.output_index == output.output_index\nrequire ckb::cell_data_u64_le(input, {}) == ckb::cell_data_u64_le(partner, {})\n", role.key_offset, role.key_offset));
            } else {
                checks.push_str(&format!("let count = ckb::selected_input_count<Role{index}>(alias, {index})\nrequire count == {per_role}\nrequire ckb::selected_output_count<Role{index}>(selected, {index}) == count\nlet input = ckb::selected_input<Role{index}>(selected, {index}, {ordinal})\nlet output = ckb::selected_output<Role{index}>(alias, {index}, 0)\nlet partner = ckb::selected_successor<Role{index}>(selected, {index}, count - 1)\nrequire input.capacity == partner.capacity\nrequire output.type_hash == input.type_hash\nrequire partner.output_index == output.output_index\nrequire ckb::cell_data_u64_le(input, {}) == ckb::cell_data_u64_le(partner, {})\n", role.key_offset, role.key_offset));
            }
            if nested_roles && index == 0 {
                // Identity moved after a variable-length field. Only the
                // checked role table resolves its offset; raw reads cannot.
                checks = checks.replace(
                    &format!(
                        "require ckb::cell_data_u64_le(input, {}) == ckb::cell_data_u64_le(partner, {})\n",
                        role.key_offset, role.key_offset
                    ),
                    "",
                );
                checks.push_str("let payload = input.data.messages[0]\nrequire input.data.id == partner.data.id\nrequire partner.data.entries[0].tag == 7\nrequire payload.len() <= 256\n");
            }
            // The fixture interleaves fee Cells and reverses outputs. Assert
            // coordinates independently of the role table's declaration order.
            // Otherwise equal role counts could mask a consistent retargeting.
            let last_global_input = 2 * (index + (per_role - 1) * contract.roles.len()) + 1;
            let last_global_output = 2 * per_role * contract.roles.len() - 1 - last_global_input;
            checks.push_str(&format!("require partner.output_index == {last_global_output}\n"));
            if nested && !(nested_roles && index == 0) {
                checks.push_str("require input.data.id == partner.data.id\n");
                // The maximum catalog crosses the real hot-cache threshold;
                // ordinary catalogs retain the cold four-way path.
                for byte in 0..3 {
                    let offset = role.key_offset + byte;
                    checks.push_str(&format!("require ckb::cell_data_u8(input, {offset}) == ckb::cell_data_u8(partner, {offset})\n"));
                }
                if index == 0 {
                    checks.push_str(&format!(
                        "require read_role_zero(partner.output_index) == ckb::cell_data_u64_le(partner, {})\n",
                        role.key_offset
                    ));
                }
            }
            checks.push_str("}\n");
        }
        source = source.replace("return 0", &format!("{checks}return 0"));
        if nested && !nested_roles {
            source = source.replace("action verify", &format!("fn read_role_zero(index: u64) -> u64 {{ let view = ckb::output<Role0>(index)\nreturn ckb::cell_data_u64_le(view, {}) }}\naction verify", contract.roles[0].key_offset));
        }
    }
    if nested {
        source = source
            .replace("verify()", "verify(witness items: NestedValues<Vec<u8>, 2, 8, 32>)")
            .replacen("verification\n", "verification\nlet first = items[0]\nlet before = first.len()\n", 1)
            .replace("return 0", "require items.len() == 1\nrequire first.len() == before\nrequire first[0] == 170\nreturn 0");
    }
    let edition = if named { crate::CellScriptEdition::Edition2027 } else { crate::CellScriptEdition::Edition2026 };
    if matches!(form, CatalogForm::Renamed) {
        source = source.replace("Settlement", "BusinessRules");
        for index in 0..contract.roles.len() {
            source = source.replace(&format!("role_{index}"), &format!("business_{index}"));
        }
    }
    let mut ast = crate::frontend::parse(&source, edition).unwrap();
    if matches!(form, CatalogForm::Reordered) {
        for item in &mut ast.items {
            if let crate::ast::Item::CellRoles(def) = item {
                def.roles.reverse();
            }
        }
        // Move declarations after their uses as well as reversing roles.
        // Business expressions retain their original names and field offsets.
        ast.items.reverse();
        source = crate::fmt::format_default(&ast).unwrap();
    }
    let formatted = crate::fmt::format_default(&ast).unwrap();
    assert_eq!(crate::fmt::format_default(&crate::frontend::parse(&formatted, edition).unwrap()).unwrap(), formatted);
    let options = crate::CompileOptions { edition, opt_level, target: Some("riscv64-elf".into()), ..Default::default() };
    let compiled = crate::compile(&source, options.clone()).unwrap_or_else(|error| panic!("{error}\n{source}"));
    assert!(compiled
        .metadata
        .runtime
        .fail_closed_runtime_features
        .iter()
        .any(|feature| feature == "exact-role-selection-artifact-proof"));
    let rejected =
        crate::compile_with_executable_surface_policy(&source, options, crate::ExecutableSurfacePolicy::DenyFailClosed).unwrap_err();
    assert!(rejected.message.contains("exact-role-selection-artifact-proof"), "{rejected}");
    compiled
}

fn execute(maximum: bool, change: Change, source_opt: Option<u8>) -> std::result::Result<u64, String> {
    execute_case(maximum, change, source_opt, None)
}

#[derive(Clone, Copy, Debug)]
enum NestedWitness {
    Valid,
    WrongValue,
    Malformed,
}

fn execute_case(
    maximum: bool,
    change: Change,
    source_opt: Option<u8>,
    nested: Option<NestedWitness>,
) -> std::result::Result<u64, String> {
    execute_with_reads(maximum, change, source_opt, nested, None)
}

fn execute_with_reads(
    maximum: bool,
    change: Change,
    source_opt: Option<u8>,
    nested: Option<NestedWitness>,
    reads: Option<SelectionReads>,
) -> std::result::Result<u64, String> {
    execute_program(maximum, change, source_opt, nested, reads, CatalogForm::Positional)
}

fn execute_program(
    maximum: bool,
    change: Change,
    source_opt: Option<u8>,
    nested: Option<NestedWitness>,
    reads: Option<SelectionReads>,
    form: CatalogForm,
) -> std::result::Result<u64, String> {
    let mut context = Context::new_with_deterministic_rng();
    let main_identity = code_identity(1);
    let peer_identity = code_identity(2);
    let lock_identity = code_identity(3);
    let main = script(&main_identity, 0);
    let lock = script(&lock_identity, 0);
    let mut contract = contract();
    let role_count = if maximum { 8 } else { 2 };
    let scripts: Vec<_> = (0..role_count).map(|i| if i == 0 { main.clone() } else { script(&peer_identity, i as u8) }).collect();
    contract.roles = scripts
        .iter()
        .enumerate()
        .map(|(i, script)| Role {
            type_hash: script.calc_script_hash().unpack(),
            lock_hash: lock.calc_script_hash().unpack(),
            schema: [i as u8 + 21; 32],
            width: if maximum { 512 } else { 16 + i * 8 },
            key_offset: if maximum { 480 } else { i * 8 },
            key_width: if maximum { 32 } else { 8 },
            inputs: Count { min: 0, max: 32 },
            outputs: Count { min: 0, max: 32 },
            access: if i == 0 { Access::LocalType } else { Access::Observe },
            correspondence: reference::Correspondence::Preserve,
        })
        .collect();
    let count = if maximum { 32 } else { 2 };
    let fee = Cell { type_hash: None, lock_hash: lock.calc_script_hash().unpack(), data: vec![] };
    let mut inputs: Vec<_> = (0..count).flat_map(|i| [fee.clone(), cell(&contract, i % role_count, i as u8)]).collect();
    let mut outputs = inputs.clone();
    outputs.reverse();
    match change {
        Change::None | Change::PeerReject | Change::NestedPayload => {}
        Change::Custody => outputs[0].lock_hash = script(&lock_identity, 1).calc_script_hash().unpack(),
        Change::Layout => {
            outputs[0].data.pop();
        }
        Change::Duplicate => outputs[0] = cell(&contract, 0, 0),
        Change::Pair => {
            let last = outputs[0].data.len() - 1;
            outputs[0].data[last] ^= 1;
            let role = &contract.roles[(count - 1) % role_count];
            outputs[0].data[role.key_offset] ^= 1;
        }
        Change::Missing => {
            outputs.remove(0);
        }
        Change::ExtraFee => outputs.push(fee),
    }
    let expected = reference::select(
        &contract,
        CurrentScript { role: ScriptRole::Type, hash: main.calc_script_hash().unpack() },
        &inputs,
        &outputs,
    );
    let compiled = source_opt.map(|opt| source_program(&contract, opt, nested.is_some(), reads, form));
    let mut internal = derived(&contract);
    if matches!(form, CatalogForm::NestedPrimitive | CatalogForm::NestedSource | CatalogForm::NestedPositional) {
        if matches!(form, CatalogForm::NestedPrimitive) {
            assert!(source_opt.is_none() && nested.is_none() && reads.is_none());
        }
        super::nested::configure_role(&mut internal.roles[0]);
        let role = &contract.roles[0];
        for (index, cell) in inputs.iter_mut().chain(&mut outputs).enumerate() {
            if cell.type_hash == Some(role.type_hash) {
                super::nested::encode_role_cell(cell, role.key_offset, role.key_width, &vec![index as u8; index % 257], &[7; 9]);
            }
        }
        if matches!(change, Change::NestedPayload) {
            let cell = outputs.iter_mut().find(|cell| cell.type_hash == Some(role.type_hash)).unwrap();
            let last = cell.data.len() - 10; // final record's Bytes length prefix
            cell.data[last] ^= 1;
        }
    }
    let elf = compiled.as_ref().map_or_else(
        || program(&internal, expected.as_ref().ok()),
        |compiled| crate::strip_vm_abi_trailer(&compiled.artifact_bytes).to_vec(),
    );
    deploy(&mut context, main_identity, elf);
    deploy(&mut context, peer_identity, exit_program(if matches!(change, Change::PeerReject) { 71 } else { 0 }));
    deploy(&mut context, lock_identity, exit_program(0));
    let make_output = |cell: &Cell| {
        packed::CellOutput::new_builder()
            .capacity(100_000_000_000u64)
            .lock(if cell.lock_hash == hash(&lock) { lock.clone() } else { script(&code_identity(3), 1) })
            .type_(cell.type_hash.map(|identity| scripts.iter().find(|script| hash(script) == identity).unwrap().clone()).pack())
            .build()
    };
    let mut transaction = TransactionBuilder::default();
    for cell in &inputs {
        let out_point = context.create_cell(make_output(cell), Bytes::from(cell.data.clone()));
        transaction = transaction.input(packed::CellInput::new_builder().previous_output(out_point).build());
    }
    for cell in &outputs {
        transaction = transaction.output(make_output(cell)).output_data(Bytes::from(cell.data.clone()).pack());
    }
    if let Some(nested) = nested {
        // Independently frozen DynVec<Bytes>: one two-byte item. Use the
        // public entry encoder, then mutate the transaction-carried payload.
        let wire = hex::decode("0e0000000800000002000000aabb").unwrap();
        let mut payload =
            compiled.as_ref().unwrap().metadata.actions[0].entry_witness_args(&[crate::EntryWitnessArg::Bytes(wire)]).unwrap();
        match nested {
            NestedWitness::Valid => {}
            NestedWitness::WrongValue => {
                let index = payload.len() - 2;
                payload[index] ^= 1;
            }
            NestedWitness::Malformed => payload[12] ^= 1, // Incorrect DynVec total byte size.
        }
        let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
        // Input 0 is an unrelated fee Cell. The current Type group's first
        // input is global index 1, independently of output permutation.
        transaction = transaction.witness(Bytes::new().pack()).witness(witness.as_bytes().pack());
    }
    let transaction = context.complete_tx(transaction.build());
    context.verify_tx(&transaction, 10_000_000).map_err(|error| format!("{error:?}"))
}

#[test]
fn nested_role_source_real_ckb_checks_codecs_coordinates_and_live_witnesses() {
    for opt in 0..=3 {
        for form in [CatalogForm::NestedSource, CatalogForm::NestedPositional] {
            execute_program(false, Change::None, Some(opt), None, Some(SelectionReads::Valid), form).unwrap();
        }
    }
    for maximum in [false, true] {
        let cycles = execute_program(
            maximum,
            Change::None,
            Some(3),
            Some(NestedWitness::Valid),
            Some(SelectionReads::Valid),
            CatalogForm::NestedSource,
        )
        .unwrap();
        eprintln!("[nested-role source transaction] maximum={maximum} cycles={cycles}");
        for (change, code) in [(Change::NestedPayload, 16), (Change::Custody, 41), (Change::Pair, 7), (Change::PeerReject, 71)] {
            let error =
                execute_program(maximum, change, Some(3), None, Some(SelectionReads::Valid), CatalogForm::NestedSource).unwrap_err();
            assert!(error.contains(&format!("error code {code}")), "{change:?}: {error}");
        }
    }
}

#[test]
fn nested_role_machine_real_ckb_checks_variable_tables_and_every_peer_group() {
    for maximum in [false, true] {
        execute_program(maximum, Change::None, None, None, None, CatalogForm::NestedPrimitive).unwrap();
        for (change, code) in [(Change::NestedPayload, 16), (Change::Custody, 41), (Change::Pair, 7), (Change::PeerReject, 71)] {
            let error = execute_program(maximum, change, None, None, None, CatalogForm::NestedPrimitive).unwrap_err();
            assert!(error.contains(&format!("error code {code}")), "{change:?}: {error}");
        }
    }
}

#[test]
fn exact_role_machine_real_ckb_runs_all_groups_and_rejects_substitutions() {
    for maximum in [false, true] {
        let cycles = execute(maximum, Change::None, None).unwrap();
        eprintln!("[exact-role all-group transaction] maximum={maximum} cycles={cycles}");
        for (change, code) in [
            (Change::Custody, 41),
            (Change::Layout, 4),
            (Change::Duplicate, 21),
            (Change::Pair, 7),
            (Change::Missing, 7),
            (Change::PeerReject, 71),
        ] {
            let error = execute(maximum, change, None).unwrap_err();
            assert!(
                error.contains(&format!("ValidationFailure: see error code {code} ")),
                "maximum={maximum} change={change:?}: {error}"
            );
        }
        if maximum {
            assert!(execute(maximum, Change::ExtraFee, None).unwrap_err().contains("ValidationFailure: see error code 21 "));
        } else {
            execute(maximum, Change::ExtraFee, None).unwrap();
        }
    }
}

#[test]
fn named_cell_roles_read_aliases_in_full_transactions() {
    for opt in 0..=3 {
        for maximum in [false, true] {
            let cycles =
                execute_program(maximum, Change::None, Some(opt), None, Some(SelectionReads::Valid), CatalogForm::Named).unwrap();
            eprintln!("[named role selection] O{opt} maximum={maximum} cycles={cycles}");
            for reads in [SelectionReads::End, SelectionReads::High] {
                let error = execute_program(maximum, Change::None, Some(opt), None, Some(reads), CatalogForm::Named).unwrap_err();
                assert!(error.contains("ValidationFailure: see error code 21 "), "{error}");
            }
            let error = execute_program(maximum, Change::PeerReject, Some(opt), None, Some(SelectionReads::Valid), CatalogForm::Named)
                .unwrap_err();
            assert!(error.contains("ValidationFailure: see error code 71 "), "{error}");
        }
    }
}

#[test]
fn named_cell_roles_reorder_and_rename_preserve_full_transaction_role_bindings() {
    for opt in 0..=3 {
        for maximum in [false, true] {
            for form in [CatalogForm::Reordered, CatalogForm::Renamed] {
                execute_program(maximum, Change::None, Some(opt), None, Some(SelectionReads::Valid), form)
                    .unwrap_or_else(|error| panic!("{form:?}, O{opt}, maximum={maximum}: {error}"));
                for (change, code) in [(Change::Pair, 7), (Change::Custody, 41), (Change::PeerReject, 71)] {
                    let error = execute_program(maximum, change, Some(opt), None, Some(SelectionReads::Valid), form).unwrap_err();
                    assert!(error.contains(&format!("ValidationFailure: see error code {code} ")), "{form:?}: {error}");
                }
            }
        }
    }
}

#[test]
fn named_cell_roles_execute_identical_full_transaction_checks() {
    for opt in [0, 3] {
        for maximum in [false, true] {
            let named = execute_program(maximum, Change::None, Some(opt), None, None, CatalogForm::Named).unwrap();
            let primitive = execute(maximum, Change::None, Some(opt)).unwrap();
            assert_eq!(named, primitive, "the declaration must not add VM work: O{opt}, maximum={maximum}");
            for (change, code) in [(Change::Layout, 4), (Change::Custody, 41), (Change::PeerReject, 71)] {
                let error = execute_program(maximum, change, Some(opt), None, None, CatalogForm::Named).unwrap_err();
                assert!(error.contains(&format!("ValidationFailure: see error code {code} ")), "{error}");
            }
        }
    }
}

#[test]
fn exact_role_source_executes_typed_catalogs_and_retains_the_proof_admission_gate() {
    for opt in 0..=3 {
        for maximum in [false, true] {
            let cycles = execute(maximum, Change::None, Some(opt)).unwrap();
            eprintln!("[exact-role source transaction, two verifications] opt={opt} maximum={maximum} cycles={cycles}");
            for (change, code) in [(Change::Layout, 4), (Change::Custody, 41), (Change::PeerReject, 71)] {
                let error = execute(maximum, change, Some(opt)).unwrap_err();
                assert!(error.contains(&format!("ValidationFailure: see error code {code} ")), "{error}");
            }
        }
    }
}

#[test]
fn exact_role_source_keeps_nested_witness_views_live_across_repeated_scans() {
    for opt in 0..=3 {
        for maximum in [false, true] {
            let cycles = execute_case(maximum, Change::None, Some(opt), Some(NestedWitness::Valid)).unwrap();
            eprintln!("[exact-role source with live nested views] opt={opt} maximum={maximum} cycles={cycles}");
            for nested in [NestedWitness::WrongValue, NestedWitness::Malformed] {
                let error = execute_case(maximum, Change::None, Some(opt), Some(nested)).unwrap_err();
                let code = if matches!(nested, NestedWitness::WrongValue) {
                    CellScriptRuntimeError::AssertionFailed as u8
                } else {
                    CellScriptRuntimeError::DynamicFieldBoundsInvalid as u8
                };
                assert!(error.contains(&format!("ValidationFailure: see error code {code} ")), "{nested:?}: {error}");
            }
            let peer = execute_case(maximum, Change::PeerReject, Some(opt), Some(NestedWitness::Valid)).unwrap_err();
            assert!(peer.contains("ValidationFailure: see error code 71 "), "{peer}");
        }
    }
}

#[test]
fn exact_role_selected_views_keep_catalogs_coordinates_and_partners_separate() {
    for opt in 0..=3 {
        for maximum in [false, true] {
            let cycles = execute_with_reads(maximum, Change::None, Some(opt), None, Some(SelectionReads::Valid)).unwrap();
            eprintln!("[selected typed views] opt={opt} maximum={maximum} cycles={cycles}");
            for read in [SelectionReads::End, SelectionReads::High] {
                let error = execute_with_reads(maximum, Change::None, Some(opt), None, Some(read)).unwrap_err();
                assert!(error.contains("ValidationFailure: see error code 21 "), "{read:?}: {error}");
            }
            let error = execute_with_reads(maximum, Change::PeerReject, Some(opt), None, Some(SelectionReads::Valid)).unwrap_err();
            assert!(error.contains("ValidationFailure: see error code 71 "), "{error}");
        }
    }
}

#[test]
fn exact_role_selected_views_compose_with_live_nested_witness_reads() {
    for opt in 0..=3 {
        for maximum in [false, true] {
            for nested in [NestedWitness::Valid, NestedWitness::WrongValue, NestedWitness::Malformed] {
                let result = execute_with_reads(maximum, Change::None, Some(opt), Some(nested), Some(SelectionReads::Valid));
                match nested {
                    NestedWitness::Valid => assert!(result.is_ok(), "{result:?}"),
                    NestedWitness::WrongValue | NestedWitness::Malformed => {
                        let code = if matches!(nested, NestedWitness::WrongValue) { 5 } else { 16 };
                        let error = result.unwrap_err();
                        assert!(error.contains(&format!("ValidationFailure: see error code {code} ")), "{error}");
                    }
                }
            }
        }
    }
}
