use cellscript::{ast::Module, CellScriptEdition, CompileOptions};

const DECLARATION: &str = "module owned\ncollections Work {\nlimits { bytes: 2048, allocations: 16 }\nvalues: Vec<u64> { items: 8 }\ntotals: OrderedMap<u64, u64> { items: 8 }\nseen: OrderedSet<u64> { items: 8 }\n}\n";

fn typed(source: &str) -> cellscript::error::Result<Module> {
    let ast = cellscript::frontend::parse(source, CellScriptEdition::Edition2027)?;
    cellscript::types::check(&ast)?;
    Ok(ast)
}

fn action(body: &str) -> String {
    format!("{DECLARATION}\naction inspect() -> u64 {{\n{body}\nreturn 0\n}}")
}

#[test]
fn owned_collections_resolve_record_fields_even_in_unused_declarations() {
    let options = CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() };
    for (record, member) in [
        ("struct Value { amount: u64 }", "Vec<Value>"),
        ("struct Value { bytes: [u8; 64] }", "Vec<Value>"),
        ("struct Value { bytes: [u8; 32] }", "OrderedMap<u64, Value>"),
        ("struct Pair<T: fixed_value> { left: T, right: T }", "Vec<Pair<u128>>"),
        ("struct Pair<T: fixed_value> { left: T, right: T }\nstruct Value { pair: Pair<u64> }", "Vec<Value>"),
    ] {
        let source = format!("module records\ncollections Work {{ limits {{ bytes: 2048, allocations: 8 }}\nvalues: {member} {{ items: 4 }} }}\n{record}\naction inspect() -> u64 {{ let work = Work::new()\nreturn 0 }}");
        cellscript::compile(&source, options.clone()).unwrap_or_else(|error| panic!("{error}\n{source}"));
    }
    for (record, member) in [
        ("struct Value { bytes: [u8; 65] }", "Vec<Value>"),
        ("struct Value { bytes: [u8; 33] }", "OrderedMap<u64, Value>"),
        ("struct Value { bytes: [u8; 0] }", "Vec<Value>"),
        ("struct Value {}", "Vec<Value>"),
        ("enum Value { One, Two }", "Vec<Value>"),
        ("struct Value { amount: u64 }", "OrderedSet<Value>"),
        ("struct Value { amount: u64 }", "OrderedMap<Value, u64>"),
        ("resource Token has store { amount: u64 }\nstruct Value { token: Token }", "Vec<Value>"),
        ("struct Value { next: Value }", "Vec<Value>"),
        ("struct Value { next: Other }\nstruct Other { next: Value }", "Vec<Value>"),
        ("struct Value { values: Vec<u64> }", "Vec<Value>"),
        ("struct Value { value: &u64 }", "Vec<Value>"),
        ("struct Value { owner: Work }", "Vec<Value>"),
        ("struct Pair<T: fixed_value> { left: T, right: T }", "Vec<Pair<[u8; 33]>>"),
        (
            "struct Pair<T: fixed_value> { left: T, right: T }\nfn identity<T: fixed_value>(value: T) -> T { return value }",
            "Vec<Pair<T>>",
        ),
        ("", "Vec<Unknown>"),
    ] {
        let source = format!("module records\n{record}\ncollections Work {{ limits {{ bytes: 2048, allocations: 8 }}\nvalues: {member} {{ items: 4 }} }}\naction inspect() -> u64 {{ return 0 }}");
        assert!(cellscript::compile(&source, options.clone()).is_err(), "accepted unused {source}");
    }
    let source = "module budget\nstruct Value { bytes: [u8; 64] }\ncollections Work { limits { bytes: 64, allocations: 1 }\nvalues: Vec<Value> { items: 1 } }\naction inspect() -> u64 { return 0 }";
    let error = cellscript::compile(source, options).unwrap_err();
    assert!(error.to_string().contains("arena byte budget"), "{error}");
}

#[test]
fn owned_collections_accept_moves_scoped_reads_and_local_value_programs() {
    for source in [
        action("let mut work = Work::new()\nwork.values.reserve(8)\nwork.values.push(7)\nlet copied = work.values[0]\nlet moved = work\nrequire moved.values.get(0) == copied"),
        action("let mut work = Work::new()\nwork.values.insert(0, 7)\nwork.values.set(0, 8)\nlet removed = work.values.remove(0)\nwork.values.push(removed)\nlet popped = work.values.pop()\nrequire popped == 8"),
        action("let mut work = Work::new()\nlet fresh = work.totals.insert(1, 7)\nrequire fresh\nrequire work.totals.get_or(2, 9) == 9\nlet removed = work.totals.remove(1)\nrequire removed\nwork.drop()"),
        action("let work = Work::new()\nborrow work as view { require view.values.is_empty()\nborrow view as nested { require nested.totals.len() == 0 } }"),
        action("for i in 0..4 { let mut work = Work::new()\nwork.values.push(i) }"),
        action("let mut work = Work::new()\nif true { work.drop()\nreturn 0 }\nwork.values.push(1)"),
        action("let mut work = Work::new()\nif false { let moved = work\nreturn 0 }\nwork.values.push(1)"),
        action("let mut work = Work::new()\nborrow work as view { if true { return 0 } }\nwork.values.push(1)"),
        include_str!("syntax_combo/seeds/collections-aggregation.cell").into(),
        include_str!("syntax_combo/seeds/collections-deduplication.cell").into(),
    ] {
        let ast = typed(&source).unwrap_or_else(|error| panic!("{error}\n{source}"));
        let errors = cellscript::types::diagnostics(&ast);
        assert!(errors.is_empty(), "{errors:?}\n{source}");
        let result = cellscript::compile(&source, CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() });
        let compiled = result.unwrap_or_else(|error| panic!("{error}\n{source}"));
        assert!(compiled.metadata.runtime.fail_closed_runtime_features.iter().any(|feature| feature == "allocated-values-artifact-proof"));
    }
}

#[test]
fn owned_collections_reject_stale_owners_escaping_handles_and_mutation_during_borrow() {
    for (body, message) in [
        ("let work = Work::new()\nlet other = work\nwork.values.len()", "moved or dropped"),
        ("let work = Work::new()\nwork.drop()\nwork.drop()", "moved or dropped"),
        ("let work = Work::new()\nwork.values.push(1)", "read-only"),
        ("let mut work = Work::new()\nborrow work as view { work.values.push(1) }", "borrow"),
        ("let mut work = Work::new()\nborrow work as view { let other = work }", "borrow"),
        ("let mut work = Work::new()\nborrow work as view { work.drop() }", "borrow"),
        ("let mut work = Work::new()\nborrow work as view { view.values.push(1) }", "read-only"),
        ("let mut work = Work::new()\nfor item in work.values { work.values.push(item) }", "borrow"),
        ("let mut work = Work::new()\nfor item in work.values { let other = work }", "borrow"),
        ("let work = Work::new()\nlet escaped = work.values", "members must stay"),
        ("let work = Work::new()\nlet escaped = &work", "wrapped or escape"),
        ("let work = Work::new()\nlet escaped = [work]", "wrapped or escape"),
        ("let work = Work::new()\nlet escaped = (work, 1)", "wrapped or escape"),
        ("let work = Work::new()\nrequire work == work", "pointer identity"),
        ("let work = Work::forged", "originate"),
        ("let mut work = Work::new()\nrequire work.seen.insert(1)", "pure constraint"),
        ("let mut work = Work::new()\nwork.values.push({ work.drop()\n1 })", "moved or dropped"),
        ("let mut work = Work::new()\nlet read = work.values[{ work.drop()\n0 }]", "moved or dropped"),
        ("let work = Work::new()\nfor i in 0..4 { let moved = work }", "loop"),
        ("let work = Work::new()\nif true { let moved = work }", "inconsistent ownership"),
    ] {
        let source = action(body);
        let error = typed(&source).unwrap_err();
        assert!(error.to_string().contains(message), "expected {message}, got {error}\n{source}");
        let ast = cellscript::frontend::parse(&source, CellScriptEdition::Edition2027).unwrap();
        assert!(!cellscript::types::diagnostics(&ast).is_empty(), "diagnostics admitted {source}");
    }
}

#[test]
fn owned_collections_reject_storage_external_parameters_and_reference_wrappers() {
    for extra in [
        "struct Wrapped { value: Work }",
        "resource Stored { value: Work }",
        "struct Wrapped { value: Vec<[Work; 2]> }",
        "struct Wrapped { value: Vec<(Work, u64)> }",
        "enum Wrapped { Value(Work) }",
        "const VALUE: Work = Work::new()",
        "fn escaped() -> Work { return Work::new() }",
        "fn accepts(value: Work) -> u64 { return 0 }",
        "fn accepts(value: (&Work, u64)) -> u64 { return 0 }",
        "fn accepts(value: &mut Work) -> u64 { return 0 }",
        "action witness_owner(witness value: Work) -> u64 { return 0 }",
    ] {
        let source = format!("{DECLARATION}\n{extra}");
        assert!(typed(&source).is_err(), "accepted {extra}");
    }
    let source = format!("{DECLARATION}\nresource Token has store {{ amount: u64 }}\naction unchanged(input token: Token) -> u64 {{ let work = Work::new()\nreturn 0 }}");
    assert!(typed(&source).unwrap_err().to_string().contains("was not consumed"));
}

#[test]
fn owned_collections_helpers_borrow_without_transfer_and_forward_without_escape() {
    let helpers = r#"
fn count(view: &Work) -> u64 { return view.values.len() }
fn copied(value: u64) -> u64 { return value }
#[effect(ReadOnly)]
fn summarize(view: &Work, other: &Work, extra: u64) -> u64 {
    let mut sum = 0
    for item in view.values { sum = sum + item }
    borrow view as nested { let same = count(nested) == count(view) }
    let copied_count = copied(view.values.len())
    return sum + count(other) + extra
}
"#;
    for body in [
        "let mut work = Work::new()\nwork.values.push(7)\nlet copied = summarize(&work, &work, count(&work))\nrequire copied == 9\nwork.values.push(8)\nwork.drop()",
        "let work = Work::new()\nborrow work as view { require summarize(view, view, 0) == 0 }\nwork.drop()",
        "let work = Work::new()\nborrow work as view { require summarize(view, view, count(view)) == 0 }\nwork.drop()",
        "let work = Work::new()\nlet moved = work\nrequire owned::count(&moved) == 0",
    ] {
        let source = format!("{}\n{helpers}", action(body));
        let ast = typed(&source).unwrap_or_else(|error| panic!("{error}\n{source}"));
        assert!(cellscript::types::diagnostics(&ast).is_empty());
        let compiled = cellscript::compile(&source, CompileOptions { edition: CellScriptEdition::Edition2027, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap_or_else(|error| panic!("{error}\n{source}"));
        assert!(compiled.metadata.runtime.fail_closed_runtime_features.iter().any(|feature| feature == "allocated-values-artifact-proof"));
    }
}

#[test]
fn owned_collections_helpers_reject_escaping_and_invalidated_arguments() {
    let helpers = "fn count(view: &Work, extra: u64) -> u64 { return view.values.len() + extra }";
    for body in [
        "let work = Work::new()\ncount(work, 0)",
        "let work = Work::new()\nwork.drop()\ncount(&work, 0)",
        "let work = Work::new()\ncount(&work, { work.drop()\n0 })",
        "let mut work = Work::new()\ncount(&work, { work.values.push(1)\n0 })",
        "let work = Work::new()\ncount(&work, { let moved = work\n0 })",
        "let work = Work::new()\nborrow work as view { count(&view, 0) }",
        "let work = Work::new()\ncount(&work, true)",
        "let work = Work::new()\ncount(&work)",
        "let work = Work::new()\ncount<u64>(&work, 0)",
    ] {
        let source = format!("{}\n{helpers}", action(body));
        assert!(typed(&source).is_err(), "accepted {source}");
        let ast = cellscript::frontend::parse(&source, CellScriptEdition::Edition2027).unwrap();
        assert!(!cellscript::types::diagnostics(&ast).is_empty(), "diagnostics admitted {source}");
    }
    for helper in [
        "fn bad(view: &Work) -> u64 { view.values.push(1)\nreturn 0 }",
        "fn bad(view: &Work) -> u64 { view.drop()\nreturn 0 }",
        "fn bad(view: &Work) -> u64 { let alias = view\nreturn 0 }",
        "fn bad(view: &Work) -> Work { return *view }",
        "fn bad(view: &Work) -> &Work { return view }",
        "#[effect(Mutating)]\nfn bad(view: &Work) -> u64 { return view.values.len() }",
        "action bad(view: &Work) -> u64 { return 0 }",
    ] {
        assert!(typed(&format!("{DECLARATION}\n{helper}")).is_err(), "accepted {helper}");
    }
}

#[test]
fn owned_collections_are_not_cell_lifecycle_resources() {
    for operation in ["consume work", "destroy work", "settle work", "claim work"] {
        let source = action(&format!("let work = Work::new()\n{operation}"));
        let error = typed(&source).unwrap_err();
        assert!(error.to_string().contains("cell-backed linear value"), "{error}\n{source}");
    }
}

#[test]
fn owned_collections_validate_operation_types_arity_and_names() {
    for operation in [
        "work.values.push(true)",
        "work.values.push(1, 2)",
        "work.values.get(true)",
        "work.values.reserve(true)",
        "work.values.unknown()",
        "work.unknown.len()",
        "work.totals.insert(1, false)",
        "work.totals.contains_key(true)",
        "work.seen.insert(false)",
        "work.seen[0]",
        "work.values.push<u64>(1)",
    ] {
        assert!(typed(&action(&format!("let mut work = Work::new()\n{operation}"))).is_err(), "accepted {operation}");
    }
}

#[test]
fn owned_collections_sorted_batches_reject_invalid_inputs_and_borrow_mutation() {
    for body in [
        "let mut work = Work::new()\nwork.values.load_sorted([1, 2])",
        "let mut work = Work::new()\nwork.totals.load_sorted([1, 2])",
        "let mut work = Work::new()\nwork.seen.load_sorted([(1, 2)])",
        "let mut work = Work::new()\nwork.seen.load_sorted(1)",
        "let mut work = Work::new()\nwork.seen.load_sorted()",
        "let work = Work::new()\nwork.seen.load_sorted([1, 2])",
        "let mut work = Work::new()\nborrow work as view { work.seen.load_sorted([1, 2]) }",
        "let mut work = Work::new()\nborrow work as view { view.seen.load_sorted([1, 2]) }",
        "let mut work = Work::new()\nwork.seen.load_sorted([{ work.drop()\n1 }])",
        "let mut work = Work::new()\nwork.seen.load_sorted<u64>([1, 2])",
    ] {
        assert!(typed(&action(body)).is_err(), "accepted {body}");
    }
    let records = "module sorted_records\nstruct Value { amount: u64 }\ncollections Work { limits { bytes: 128, allocations: 2 }\ntotals: OrderedMap<u64, Value> { items: 4 } }\naction inspect() -> u64 { let mut work = Work::new()\nwork.totals.load_sorted([(1, Value { amount: 7 })])\nreturn 0 }";
    let error = typed(records).unwrap_err();
    assert!(error.to_string().contains("record-valued sorted batches are deferred"), "{error}");
    let entries = vec!["1"; 1025].join(",");
    assert!(typed(&action(&format!("let mut work = Work::new()\nwork.seen.load_sorted([{entries}])"))).is_err());
}
