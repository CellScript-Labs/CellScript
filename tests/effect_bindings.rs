use cellscript::{frontend, ir, types, CompileOptions, CompilerDiagnostics, CURRENT_EDITION};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn source_call(module: &str, name: &str, args: &str, parameters: &str) -> String {
    format!("module {module}\nfn {name}({parameters}) -> u64 {{ return 77 }}\naction run() -> u64 {{ verification return {module}::{name}({args}) }}")
}

fn assert_user_binding(source: &str, expected: &str) {
    let ast = frontend::parse(source, CURRENT_EDITION).unwrap();
    types::check(&ast).unwrap();
    let lowered = ir::generate(&ast).unwrap();
    let action = lowered
        .items
        .iter()
        .find_map(|item| match item {
            ir::IrItem::Action(action) if action.name == "run" => Some(action),
            _ => None,
        })
        .unwrap();
    let callees = action
        .body
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            ir::IrInstruction::Call { func, .. } => Some(func.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(callees, [expected], "lowering must preserve the resolved source callable");
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile(
            source,
            CompileOptions { diagnostics, source_contracts: true, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let record = compiled.verified_lowering_record.as_ref().unwrap();
        for entry in &record.typed_semantics.entries {
            for call in entry.blocks.iter().flat_map(|block| &block.operations).filter_map(|operation| operation.call.as_ref()) {
                assert!(
                    !matches!(call.target.as_str(), "__ckb_cell_capacity" | "__ckb_source_input" | "__ckb_commitment_open"),
                    "source binding was replaced during optimized artifact generation: {}",
                    call.target
                );
            }
        }
    }
}

#[test]
fn source_bindings_precede_collection_and_commitment_special_paths() {
    for (module, name, args, params) in
        [("Vec", "new", "", ""), ("Vec", "with_capacity", "5", "value: u64"), ("commitment", "open", "5, 6", "left: u64, right: u64")]
    {
        assert_user_binding(&source_call(module, name, args, params), name);
    }
}

#[test]
fn imported_source_binding_is_preserved_through_its_defining_owner() {
    let dependency = frontend::parse("module ckb\npublic fn cell_capacity(value: u64) -> u64 { return 77 }", CURRENT_EDITION).unwrap();
    let ast = frontend::parse(
        "module consumer\nuse ckb::{cell_capacity}\naction run() -> u64 { verification return ckb::cell_capacity(5) }",
        CURRENT_EDITION,
    )
    .unwrap();
    let mut resolver = cellscript::resolve::ModuleResolver::new();
    resolver.register_module(dependency).unwrap();
    resolver.register_module(ast.clone()).unwrap();
    types::check_with_resolver(&ast, &resolver, "consumer").unwrap();
    let lowered = ir::generate_with_resolver(&ast, &resolver, "consumer").unwrap();
    let target = lowered
        .items
        .iter()
        .find_map(|item| match item {
            ir::IrItem::Action(action) if action.name == "run" => {
                action.body.blocks.iter().flat_map(|block| &block.instructions).find_map(|instruction| match instruction {
                    ir::IrInstruction::Call { func, .. } => Some(func.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .unwrap();
    assert_ne!(target, "__ckb_cell_capacity");
    assert!(
        lowered.items.iter().any(|item| matches!(item, ir::IrItem::Function(function) if function.name == target)),
        "call must retain the imported source definition, not only its spelling"
    );
}

#[test]
fn imported_spawn_functions_and_aliases_keep_their_source_binding() {
    for (import, call) in [
        ("use foreign::spawn", "spawn(7)"),
        ("use foreign::spawn as relay", "relay(7)"),
        ("use foreign::relay as spawn", "spawn(7)"),
        ("", "foreign::spawn(7)"),
    ] {
        let dependency = frontend::parse(
            "module foreign\npublic fn spawn(value: u64) -> u64 { return value }\npublic fn relay(value: u64) -> u64 { return value }",
            CURRENT_EDITION,
        )
        .unwrap();
        let ast = frontend::parse(
            &format!("module consumer\n{import}\naction run() -> u64 {{ verification return {call} }}"),
            CURRENT_EDITION,
        )
        .unwrap();
        let mut resolver = cellscript::resolve::ModuleResolver::new();
        resolver.register_module(dependency).unwrap();
        resolver.register_module(ast.clone()).unwrap();
        types::check_with_resolver(&ast, &resolver, "consumer").unwrap();
        let lowered = ir::generate_with_resolver(&ast, &resolver, "consumer").unwrap();
        let target = lowered
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) => {
                    action.body.blocks.iter().flat_map(|block| &block.instructions).find_map(|instruction| match instruction {
                        ir::IrInstruction::Call { func, .. } => Some(func),
                        _ => None,
                    })
                }
                _ => None,
            })
            .unwrap();
        assert!(lowered.items.iter().any(|item| matches!(item, ir::IrItem::Function(function) if &function.name == target)));
    }
}

#[test]
fn distinct_definition_owners_keep_distinct_lowered_call_targets() {
    for (owners, symbols) in
        [(["a::b", "a__b"], ["value", "value"]), (["甲", "乙"], ["取", "取"]), (["producer", "producer"], ["甲", "乙"])]
    {
        let source = format!(
            "module owner_labels\naction run() -> u64 {{ verification require {}::{}() == 11 require {}::{}() == 22 return 0 }}",
            owners[0], symbols[0], owners[1], symbols[1]
        );
        let ast = frontend::parse(&source, CURRENT_EDITION).unwrap();
        let mut dependencies = std::collections::BTreeMap::<&str, String>::new();
        for (index, (owner, symbol)) in owners.into_iter().zip(symbols).enumerate() {
            dependencies
                .entry(owner)
                .or_insert_with(|| format!("module {owner}\n"))
                .push_str(&format!("public fn {symbol}() -> u64 {{ return {} }}\n", 11 * (index + 1)));
        }
        let mut resolver = cellscript::resolve::ModuleResolver::new();
        for source in dependencies.values() {
            resolver.register_module(frontend::parse(source, CURRENT_EDITION).unwrap()).unwrap();
        }
        resolver.register_module(ast.clone()).unwrap();
        types::check_with_resolver(&ast, &resolver, "owner_labels").unwrap();
        let lowered = ir::generate_with_resolver(&ast, &resolver, "owner_labels").unwrap();
        let action = lowered
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) => Some(action),
                _ => None,
            })
            .unwrap();
        let targets = action
            .body
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction {
                ir::IrInstruction::Call { func, .. } => Some(func),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(targets.len(), 2, "different resolved definitions must never collapse into the same assembly callable");
        for target in targets {
            assert!(lowered.items.iter().any(|item| matches!(item, ir::IrItem::Function(function) if &function.name == target)));
        }
    }
}

#[test]
fn owner_identity_and_transitive_aliases_reach_the_actual_vm() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("a")).unwrap();
    std::fs::write(
        root.path().join("a/b.cell"),
        "module a::b\npublic struct LeftAnchor { value: u64 }\npublic fn value() -> u64 { return 11 }",
    )
    .unwrap();
    std::fs::write(
        root.path().join("a__b.cell"),
        "module a__b\npublic struct RightAnchor { value: u64 }\npublic fn value() -> u64 { return 22 }",
    )
    .unwrap();
    std::fs::write(root.path().join("bridge.cell"),
        "module bridge\nuse a::b::LeftAnchor\npublic struct BridgeAnchor { value: u64 }\npublic fn relay() -> u64 { return a::b::value() }").unwrap();
    let path = root.path().join("main.cell");
    std::fs::write(&path,
        "module owner_labels\nuse a::b::LeftAnchor\nuse a__b::RightAnchor\nuse bridge::BridgeAnchor\nuse a::b::value as direct\npublic action run() -> u64 { verification require direct() == 11 require a__b::value() == 22 require bridge::relay() == 11 return 0 }").unwrap();
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_path_with_executable_surface_policy(
            path.to_str().unwrap(),
            CompileOptions { source_contracts: true, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            None,
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        cellscript_artifact_checker::interface::inspect_bundle(
            &compiled.artifact_bytes,
            &serde_json::to_vec(&compiled.metadata).unwrap(),
            &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
            &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
            &Default::default(),
        )
        .unwrap();
        let tx = ckb_script_runner::build_simple_fixture(ckb_testtool::ckb_types::bytes::Bytes::new(), 1, 1);
        let result = ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &tx);
        assert_eq!(result.exit_code, 0, "each owner and the transitive alias must execute its own definition");
    }
}

#[test]
fn a_source_callable_with_an_intrinsic_namespace_keeps_its_binding() {
    assert_user_binding(&source_call("ckb", "cell_capacity", "5", "value: u64"), "cell_capacity");
}

#[test]
fn an_explicit_source_binding_precedes_a_same_spelled_view_constructor() {
    assert_user_binding(&source_call("ckb", "input", "5", "value: u64"), "input");
}

#[test]
fn an_unshadowed_runtime_observation_still_lowers_to_the_registered_helper() {
    let source = "module builtin_observation\naction run() -> u64 { verification return ckb::cell_capacity(source::input(0)) }";
    let ast = frontend::parse(source, CURRENT_EDITION).unwrap();
    types::check(&ast).unwrap();
    let lowered = ir::generate(&ast).unwrap();
    assert!(lowered.items.iter().any(|item| match item {
        ir::IrItem::Action(action) => action
            .body
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction, ir::IrInstruction::Call { func, .. } if func == "__ckb_cell_capacity")),
        _ => false,
    }));
}
