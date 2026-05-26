use cellscript::{
    ast::{BinaryOp, Expr, Item, Stmt},
    compile, lexer, parser, CompileOptions,
};

fn parse_source(source: &str) -> cellscript::ast::Module {
    let tokens = lexer::lex(source).unwrap_or_else(|err| panic!("lexing should succeed: {}", err.message));
    parser::parse(&tokens).unwrap_or_else(|err| panic!("parsing should succeed: {}", err.message))
}

fn first_action(module: &cellscript::ast::Module) -> &cellscript::ast::ActionDef {
    module
        .items
        .iter()
        .find_map(|item| match item {
            Item::Action(action) => Some(action),
            _ => None,
        })
        .expect("test module should contain an action")
}

#[test]
fn adversarial_0_13_rejects_unsupported_generic_collection_surfaces() {
    let cases = [
        (
            "hashmap",
            r#"
module bad_hashmap

action main() -> u64
where
    let orders: HashMap<Hash, u64> = HashMap::new()
    return orders.len()
"#,
            "HashMap",
        ),
        (
            "cell_vec",
            r#"
module bad_cell_vec

resource Token has store, create, consume, replace, burn, relock, read_ref {
    amount: u64,
}

action main() -> u64
where
    let cells: Vec<Cell<Token>> = Vec::new()
    return cells.len()
"#,
            "Cell",
        ),
        (
            "option_reserved",
            r#"
module bad_option

action main() -> Option<u64>
where
    return Option::some(1)
"#,
            "Option",
        ),
    ];

    for (name, source, expected) in cases {
        let err = compile(source, CompileOptions::default()).expect_err(name);
        assert!(err.message.contains(expected), "{name} should mention {expected}; got {}", err.message);
    }
}

#[test]
fn adversarial_0_13_rejects_invalid_hash_type_dsl() {
    let err = compile(
        r#"
module bad_hash_type

resource Token
with_default_hash_type(Legacy)
{
    amount: u64,
}

action main(amount: u64) -> Token
where
    create Token { amount: amount }
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .expect_err("invalid hash_type should be rejected");

    assert!(err.message.contains("unsupported CKB hash_type 'Legacy'"), "unexpected error: {}", err.message);
}

#[test]
fn adversarial_parser_preserves_operator_precedence_in_ambiguous_sequences() {
    let module = parse_source(
        r#"
module adversarial_precedence

action main() -> u64
where
    let value: bool = 1 + 2 * 3 == 7 || false && true
    return 0
"#,
    );
    let action = first_action(&module);
    let Stmt::Let(stmt) = &action.body[0] else {
        panic!("first action statement should be a let binding");
    };
    let Expr::Binary(or_expr) = &stmt.value else {
        panic!("top-level expression should be logical OR: {:?}", stmt.value);
    };
    assert_eq!(or_expr.op, BinaryOp::Or);

    let Expr::Binary(eq_expr) = or_expr.left.as_ref() else {
        panic!("OR left side should be equality");
    };
    assert_eq!(eq_expr.op, BinaryOp::Eq);

    let Expr::Binary(add_expr) = eq_expr.left.as_ref() else {
        panic!("equality left side should be addition");
    };
    assert_eq!(add_expr.op, BinaryOp::Add);

    let Expr::Binary(mul_expr) = add_expr.right.as_ref() else {
        panic!("addition right side should be multiplication");
    };
    assert_eq!(mul_expr.op, BinaryOp::Mul);

    let Expr::Binary(and_expr) = or_expr.right.as_ref() else {
        panic!("OR right side should be logical AND");
    };
    assert_eq!(and_expr.op, BinaryOp::And);
}

#[test]
fn adversarial_parser_binds_else_to_nearest_if() {
    let module = parse_source(
        r#"
module adversarial_dangling_else

action main(flag: bool) -> u64
where
    if flag {
        if false {
            return 1
        } else {
            return 2
        }
    }
    return 3
"#,
    );
    let action = first_action(&module);
    let Stmt::If(outer_if) = &action.body[0] else {
        panic!("first action statement should be an if");
    };
    assert!(outer_if.else_branch.is_none(), "outer if must not steal the inner else branch");
    let Stmt::If(inner_if) = &outer_if.then_branch[0] else {
        panic!("outer then branch should contain the nested if");
    };
    assert!(inner_if.else_branch.is_some(), "else branch should bind to the nearest if");
}

#[test]
fn adversarial_parser_rejects_deep_unary_expression_without_panicking() {
    let source = format!(
        r#"
module adversarial_deep_unary

action main() -> u64
where
    return {}1
"#,
        "-".repeat(160)
    );
    let err = compile(&source, CompileOptions::default()).expect_err("deep unary nesting should be rejected");
    assert!(err.message.contains("parser recursion limit exceeded"), "unexpected error: {}", err.message);
}

#[test]
fn adversarial_parser_rejects_deep_nested_control_flow_without_panicking() {
    let mut source = String::from(
        r#"
module adversarial_deep_if

action main() -> u64
where
"#,
    );
    for _ in 0..150 {
        source.push_str("    if true {\n");
    }
    source.push_str("    return 1\n");
    for _ in 0..150 {
        source.push_str("    }\n");
    }
    let err = compile(&source, CompileOptions::default()).expect_err("deep nested if statements should be rejected");
    assert!(err.message.contains("parser recursion limit exceeded"), "unexpected error: {}", err.message);
}

#[test]
fn adversarial_integer_literals_fail_closed_on_lexical_and_contextual_overflow() {
    let lexical_err = compile(
        r#"
module adversarial_integer_lex

action main() -> u64
where
    return 18446744073709551616
"#,
        CompileOptions::default(),
    )
    .expect_err("u64::MAX + 1 should be rejected by the lexer");
    assert!(lexical_err.message.contains("invalid integer literal"), "unexpected error: {}", lexical_err.message);

    let contextual_err = compile(
        r#"
module adversarial_integer_context

action main() -> u8
where
    return 256
"#,
        CompileOptions::default(),
    )
    .expect_err("u8 overflow should be rejected by type checking");
    assert!(contextual_err.message.contains("out of range for u8"), "unexpected error: {}", contextual_err.message);
}
