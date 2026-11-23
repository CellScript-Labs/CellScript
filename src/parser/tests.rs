use super::*;
use crate::lexer::lex;

#[test]
fn generic_lookahead_does_not_cross_comparisons_or_function_boundaries() {
    let source = r#"
module comparison_boundary;
fn before(index: u64, end: u64, start: u64) -> bool {
    while (index < 3) { break; }
    if (end < start) || (end - start < 4) { return false; }
    return true;
}
fn after() -> (u64, u64) { return (0, 0); }
"#;
    assert_eq!(parse(&lex(source).unwrap()).unwrap().items.len(), 2);
    for source in [
        "module generic_call;\nfn f() -> u64 { return choose::<u64>(1); }",
        "module generic_struct;\nfn f() -> u64 { let x = new Box<[u8; 32]> { value: 0, }; return 0; }",
        "module generic_nested;\nfn f() -> u64 { return choose::<Box<u64>>(1); }",
    ] {
        parse(&lex(source).unwrap()).unwrap();
    }
}

#[test]
fn fixed_value_profile_expands_to_the_canonical_closed_ability_set() {
    let module = parse(
            &lex("module profile;\nstruct Pair<T: fixed_value> { left: T, right: T, }\nfn id<T: non_linear + fixed + copy + serializable + store + drop>(value: T) -> T { value }")
                .unwrap(),
        )
        .unwrap();
    let Item::Struct(pair) = &module.items[0] else { panic!("expected struct") };
    let Item::Function(id) = &module.items[1] else { panic!("expected function") };
    assert_eq!(pair.type_params[0].constraints, ValueAbility::FIXED_VALUE_PROFILE);
    assert_eq!(id.type_params[0].constraints, ValueAbility::FIXED_VALUE_PROFILE);

    let error = parse(&lex("module duplicate;\nfn id<T: fixed_value + copy>(value: T) -> T { value }").unwrap()).unwrap_err();
    assert!(error.message.contains("duplicate 'copy' constraint"), "unexpected error: {error}");

    for source in [
        "module cells;\nresource Vault<T: fixed_value> { value: u64 }",
        "module actions;\naction verify<T: fixed_value>() { verification }",
        "module locks;\nlock verify<T: fixed_value>() -> bool { true }",
    ] {
        assert!(parse(&lex(source).unwrap()).is_err(), "generic Cell-backed declarations and entries must remain rejected: {source}");
    }
}

#[test]
fn parses_labeled_loop_control() {
    let source = r#"
module loop_control;

fn count() -> u64 {
    label outer: for i in 0..4 {
        while i < 3 {
            continue;
        }
        break outer;
    }
    return 0;
}
"#;
    let module = parse(&lex(source).unwrap()).unwrap();
    let Item::Function(function) = &module.items[0] else {
        panic!("expected function");
    };
    let Stmt::For(outer) = &function.body[0] else {
        panic!("expected for loop");
    };
    assert_eq!(outer.label.as_deref(), Some("outer"));
    assert!(matches!(outer.body[0], Stmt::While(_)));
    assert!(matches!(&outer.body[1], Stmt::Break(control) if control.label.as_deref() == Some("outer")));
}

#[test]
fn test_parse_resource() {
    let input = r#"
module test;

resource Token has store, replace, relock, consume, burn {
    amount: u64,
    symbol: [u8; 8],
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    assert_eq!(module.name, "test");
    assert_eq!(module.items.len(), 1);
}

#[test]
fn parse_diagnostics_collects_multiple_statement_errors() {
    let input = r#"
module test;

action bad() -> bool {

        let first: u64 true
        let second: bool 1
        return true;
}
"#;
    let tokens = lex(input).unwrap();
    let diagnostics = parse_diagnostics(&tokens).expect_err("recovery should report parse diagnostics");
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().any(|error| error.message.contains("expected '=', found 'true'")));
    assert!(diagnostics.iter().any(|error| error.message.contains("expected '=', found integer 1")));
}

#[test]
fn parse_diagnostics_collects_multiple_top_level_errors() {
    let input = r#"
module test;

resource BrokenOne {
    amount:
}

resource BrokenTwo {
    amount:
}
"#;
    let tokens = lex(input).unwrap();
    let diagnostics = parse_diagnostics(&tokens).expect_err("recovery should report parse diagnostics");
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().all(|error| error.message.contains("expected type")));
    assert_eq!(diagnostics[0].span.line, 6);
    assert_eq!(diagnostics[1].span.line, 10);
}

#[test]
fn test_parse_sole_inline_capability_declaration() {
    let input = r#"
module test;

resource Token has store, replace, relock, destroy {
    amount: u64,
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let resource = match &module.items[0] {
        Item::Resource(resource) => resource,
        other => panic!("expected resource item, found {:?}", other),
    };

    assert!(resource.capabilities.contains(&Capability::Store));
    assert!(resource.capabilities.contains(&Capability::Replace));
    assert!(resource.capabilities.contains(&Capability::Relock));
    assert!(resource.capabilities.contains(&Capability::Destroy));
}

#[test]
fn test_parse_invariant() {
    let input = r#"
module test;

invariant token_conservation {
    trigger: type_group;
    scope: group;
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount;
    assert_invariant(true, "token amount is conserved");
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let invariant = match &module.items[0] {
        Item::Invariant(invariant) => invariant,
        other => panic!("expected invariant item, found {:?}", other),
    };

    assert_eq!(invariant.name, "token_conservation");
    assert_eq!(invariant.trigger.as_deref(), Some("type_group"));
    assert_eq!(invariant.scope.as_deref(), Some("group"));
    assert_eq!(
        invariant.reads.iter().map(ToString::to_string).collect::<Vec<_>>(),
        vec!["group_inputs<Token>.amount", "group_outputs<Token>.amount"]
    );
    assert_eq!(invariant.reads[0].source, SourceView::GroupInput);
    assert_eq!(invariant.reads[0].type_and_field(), Some(("Token", "amount")));
    assert_eq!(invariant.asserts.len(), 1);
}

#[test]
fn test_parse_aggregate_invariant_primitives() {
    let input = r#"
module test;

invariant token_conservation {
    trigger: type_group;
    scope: group;
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount;
    assert_conserved(Token.amount, scope = group);
    assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount);
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let invariant = match &module.items[0] {
        Item::Invariant(invariant) => invariant,
        other => panic!("expected invariant item, found {:?}", other),
    };

    assert_eq!(invariant.aggregates.len(), 2);
    assert_eq!(invariant.aggregates[0].kind, AggregateInvariantKind::Conserved);
    assert_eq!(invariant.aggregates[0].target.to_string(), "Token.amount");
    assert_eq!(invariant.aggregates[0].target.source, SourceView::SelectedCells);
    assert_eq!(invariant.aggregates[0].target.type_and_field(), Some(("Token", "amount")));
    assert_eq!(invariant.aggregates[0].scope, "group");
    assert_eq!(invariant.aggregates[1].kind, AggregateInvariantKind::Sum);
    assert_eq!(invariant.aggregates[1].relation, Some(AggregateRelation::Le));
    assert_eq!(invariant.aggregates[1].target.to_string(), "group_outputs<Token>.amount");
    assert_eq!(invariant.aggregates[1].target.source, SourceView::GroupOutput);
    assert_eq!(invariant.aggregates[1].rhs.as_ref().map(ToString::to_string).as_deref(), Some("group_inputs<Token>.amount"));
}

#[test]
fn test_parse_rejects_unknown_generic_invariant_source_view() {
    let input = r#"
module test;

invariant invalid_source {
    trigger: type_group;
    scope: group;
    reads: mystery<Token>.amount
    assert_conserved(Token.amount, scope = group)
}

resource Token {
    amount: u64
}
"#;
    let tokens = lex(input).unwrap();
    let error = parse(&tokens).unwrap_err();
    assert!(error.message.contains("unknown invariant source view 'mystery'"), "{}", error.message);
}

#[test]
fn test_parse_invariant_assert_statement() {
    let input = r#"
module test;

invariant token_conservation {
    trigger: type_group;
    scope: group;
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount;
    assert_invariant(true, "token conserved");
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let invariant = match &module.items[0] {
        Item::Invariant(invariant) => invariant,
        other => panic!("expected invariant item, found {:?}", other),
    };

    assert_eq!(invariant.asserts.len(), 1);
    assert!(matches!(invariant.asserts[0], Expr::Assert(_)));
}

#[test]
fn test_parse_type_id_attribute() {
    let input = r#"
module test;

#[type_id("cellscript::token::Token:v1")]
resource Token has store {
    amount: u64,
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let resource = match &module.items[0] {
        Item::Resource(resource) => resource,
        other => panic!("expected resource item, found {:?}", other),
    };

    assert_eq!(resource.type_id.as_ref().map(|type_id| type_id.value.as_str()), Some("cellscript::token::Token:v1"));
}

#[test]
fn test_rejects_type_id_on_action() {
    let input = r#"
module test;

#[type_id("cellscript::action:v1")]
action run() -> u64 {
    verification
    return 0
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();

    assert!(err.message.contains("#[type_id] can only be applied"), "unexpected error: {}", err.message);
}

#[test]
fn test_rejects_generic_resource_definition() {
    let input = r#"
module test;

resource Vault<T> has store {
    content: T
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();

    assert!(err.message.contains("Cell-backed type 'Vault' cannot declare generic parameters"), "unexpected error: {}", err.message);
}

#[test]
fn test_parse_action() {
    let input = r#"
module test;

action mint(amount: u64) -> token: Token {

    create token = Token { amount: amount, symbol: b"TEST", }
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    assert_eq!(module.items.len(), 1);
}

#[test]
fn rejects_retired_effect_attributes() {
    for effect in ["Pure", "ReadOnly", "Creating", "Destroying", "Mutating"] {
        let source = format!("module test;\n#[effect({effect})]\nfn inspect(value: u64) -> u64 {{ return value }}");
        let error = parse(&lex(&source).unwrap()).unwrap_err();
        assert!(error.message.contains("effect declarations have been removed"));
    }
}

#[test]
fn test_parse_flow_and_action_transition_clause() {
    let input = r#"
module test;

flow OfferFlow for Offer.state {
    initial Created;
    terminal Filled;

    Created -> Live by publish;
    Live -> Filled by accept;
}

	action accept(input: Offer) -> output: Offer {
	    transition input.state: Live -> output.state: Filled;

            require output.state == OfferState::Filled;
    }
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    assert!(matches!(&module.items[0], Item::Flow(machine) if machine.name.as_deref() == Some("OfferFlow")));
    let Item::Flow(machine) = &module.items[0] else {
        panic!("expected flow");
    };
    assert_eq!(machine.initial_states, vec!["Created"]);
    assert_eq!(machine.terminal_states, vec!["Filled"]);
    let Item::Action(action) = &module.items[1] else {
        panic!("expected action");
    };
    assert_eq!(action.params[0].source, ParamSource::Default);
    assert_eq!(action.outputs.len(), 1);
    assert_eq!(action.outputs[0].name, "output");
    assert_eq!(action.state_edges.len(), 1);
    assert_eq!(action.state_edges[0].path.base, "input");
    assert_eq!(action.state_edges[0].to_path.base, "output");
    assert_eq!(action.state_edges[0].from, "Live");
    assert_eq!(action.state_edges[0].to, "Filled");
}

#[test]
fn test_parse_action_continuation_transitions() {
    let input = r#"
module test;

action settle(input: Offer, receipt: Receipt) -> (output: Offer, next_receipt: Receipt) {
    transition input -> output;
    transition receipt -> next_receipt;

        require output.state == OfferState::Filled;
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let Item::Action(action) = &module.items[0] else {
        panic!("expected action");
    };
    assert_eq!(action.state_edges.len(), 2);
    assert_eq!(action.state_edges[0].path.base, "input");
    assert_eq!(action.state_edges[1].path.base, "receipt");
    assert_eq!(action.state_edges[1].to_path.base, "next_receipt");
    assert!(action.state_edges[0].path.field.is_empty());
}

#[test]
fn test_rejects_transition_block() {
    let input = r#"
module test;

action accept(input: Offer) -> output: Offer {
    transition {
    }
    verification
        require output.state == OfferState::Filled
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("transition block syntax is not part"), "unexpected error: {}", err.message);
}

#[test]
fn test_rejects_legacy_move_as_ordinary_unexpected_token() {
    let input = r#"
module test;

action accept(input: Offer) -> output: Offer {
    move; input.state;: Live -> output.state: Filled
    verification
        require output.state == OfferState::Filled
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("unexpected token"), "unexpected error: {}", err.message);
}

#[test]
fn test_rejects_transition_clause_without_state_colons() {
    let input = r#"
module test;

action accept(input: Offer) -> output: Offer {
    transition input.state Live -> output.state Filled
    verification
        require output.state == OfferState::Filled
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("state transition must put ':' before the source state"), "unexpected error: {}", err.message);
}

#[test]
fn test_accepts_action_brace_body_without_verification() {
    let input = r#"
module test;

action accept(input: Offer) -> output: Offer {
    require input.state == output.state;
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    assert!(matches!(module.items.last(), Some(Item::Action(_))));
}

#[test]
fn test_rejects_where_action_proof_block_without_compatibility() {
    let input = r#"
module test;

action accept(input: Offer) -> output: Offer {
    where
        require input.state == output.state
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("not supported"), "unexpected error: {}", err.message);
    assert!(err.message.contains("require"), "unexpected error: {}", err.message);
}

#[test]
fn test_rejects_read_ref_as_type_qualifier() {
    let input = r#"
module test;

action grant(config: read_ref Config) -> grant: Grant {
    verification
    create grant = Grant { admin: config.admin }
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("`read_ref` is not a type qualifier"), "unexpected error: {}", err.message);
}

#[test]
fn test_rejects_output_parameter_source_prefix() {
    let input = r#"
module test;

action accept(input: Offer, output output: Offer) {
    verification
    require output.state == 1
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("only valid on the action return side"), "unexpected error: {}", err.message);
}

#[test]
fn test_parse_prefix_source_and_create_target() {
    let input = r#"
module test;

action grant(read config: Config, token: Token) -> grant: Grant {

    consume token;
    create grant = Grant { admin: config.admin, }
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let Item::Action(action) = &module.items[0] else {
        panic!("expected action");
    };
    assert!(action.params[0].is_read_ref);
    assert_eq!(action.params[0].name, "config");
    assert_eq!(action.outputs[0].name, "grant");
    let Stmt::Expr(Expr::Create(create)) = &action.body[1] else {
        panic!("expected targeted create");
    };
    assert_eq!(create.target.as_deref(), Some("grant"));
}

#[test]
fn test_parse_prefix_source_before_keyword_like_name() {
    let input = r#"
module test;

lock receipt_owner(protected receipt: ReceiptCell, witness claimed_owner: Address) -> bool {

            require receipt.owner == claimed_owner;
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let Item::Lock(lock) = &module.items[0] else {
        panic!("expected lock");
    };
    assert_eq!(lock.params[0].name, "receipt");
    assert_eq!(lock.params[0].source, ParamSource::Protected);
    assert_eq!(lock.params[1].source, ParamSource::Witness);
}

#[test]
fn test_parse_create_field_shorthand() {
    let input = r#"
module test;

action mint(amount: u64, symbol: [u8; 8]) -> Token {

    create Token { amount, symbol, }
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    assert_eq!(module.items.len(), 1);
}

#[test]
fn test_parse_expression() {
    let input = r#"
module test;

action test(x: u64, y: u64) -> u64 {

    let z = x + y * 2;
    return z;
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    assert_eq!(module.items.len(), 1);
}

#[test]
fn test_if_expr_with_all_caps_constant_before_block() {
    let input = r#"
module test;

const SOFT_CAP_PER_DEPOSIT: u128 = 10000000000000;

action discount(raw: u128) -> u128 {

    let oversize = if raw > SOFT_CAP_PER_DEPOSIT { raw - SOFT_CAP_PER_DEPOSIT } else { 0 };
    return raw - oversize;
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    assert_eq!(module.items.len(), 2);
}

#[test]
fn test_parse_grouped_use_imports() {
    let input = r#"
module test;

use cellscript::fungible_token::{Token, MintAuthority};
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();

    let use_stmt = match &module.items[0] {
        Item::Use(use_stmt) => use_stmt,
        other => panic!("expected use item, found {:?}", other),
    };

    assert_eq!(use_stmt.module_path, vec!["cellscript".to_string(), "fungible_token".to_string()]);
    assert_eq!(use_stmt.imports.len(), 2);
    assert_eq!(use_stmt.imports[0].name, "Token");
    assert_eq!(use_stmt.imports[1].name, "MintAuthority");
}

#[test]
fn test_launch_expression_is_reserved_until_lowering_exists() {
    let input = r#"
module test;

action bad() -> u64 {

    return launch(Token)
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("launch is reserved for a post-v1 transaction builder"), "unexpected error: {}", err.message);
}

#[test]
fn test_postfix_does_not_cross_statement_newline() {
    let input = r#"
module test;

action test() -> (u64, u64) {

    let value = foo(
        1,
        2
    );

    (value, 3)
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let action = match &module.items[0] {
        Item::Action(action) => action,
        other => panic!("expected action, found {:?}", other),
    };

    match &action.body[0] {
        Stmt::Let(let_stmt) => {
            assert!(matches!(let_stmt.value, Expr::Call(_)));
        }
        other => panic!("expected let statement, found {:?}", other),
    }

    match &action.body[1] {
        Stmt::Expr(Expr::Tuple(items)) | Stmt::Discard(Expr::Tuple(items)) => {
            assert_eq!(items.len(), 2);
        }
        other => panic!("expected tuple return expr, found {:?}", other),
    }
}

#[test]
fn test_parses_expression_match_arms() {
    let input = r#"
module test;

enum Flag {
    On,
    Off,
}

action test(flag: Flag) -> u64 {

    match flag {
        On => 1,
        Off => 0,
    }
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let Item::Action(action) = &module.items[1] else { panic!("expected action") };
    let Stmt::Expr(Expr::Match(match_expr)) = &action.body[0] else { panic!("expected match expression") };
    assert_eq!(match_expr.arms.len(), 2);
    assert!(matches!(match_expr.arms[0].pattern, MatchPattern::Variant { ref path, .. } if path == "On"));
}

// === 0.13.1 preserve sugar and anonymous require block tests ===

#[test]
fn test_parse_preserve_block() {
    let input = r#"
module test;

resource Offer has store {
    seller: [u8; 20],
    price: u64,
    payment_symbol: [u8; 8],
}

action fill(input: Offer) -> (output: Offer) {
    transition input -> output;

    preserve output from input {
        seller,
        price,
        payment_symbol,
    }
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let action = match &module.items[1] {
        Item::Action(action) => action,
        other => panic!("expected action, found {:?}", other),
    };
    // Find the preserve statement in the action body
    let found_preserve = action.body.iter().any(|stmt| matches!(stmt, Stmt::Discard(Expr::Preserve(_))));
    assert!(found_preserve, "expected preserve expression in action body");
}

#[test]
fn test_parse_require_block() {
    let input = r#"
module test;

action test(x: u64, y: u64) -> u64 {

    require {
        x > 0,
        y > 0,
    };
    return x + y;
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let action = match &module.items[0] {
        Item::Action(action) => action,
        other => panic!("expected action, found {:?}", other),
    };
    let found_require_block = action.body.iter().any(|stmt| matches!(stmt, Stmt::Discard(Expr::RequireBlock(_))));
    assert!(found_require_block, "expected require block expression in action body");
}

#[test]
fn test_parse_preserve_single_field() {
    let input = r#"
module test;

resource Token has store {
    amount: u64,
}

action test(input: Token) -> (output: Token) {

    preserve output from input {
        amount,
    }
}
"#;
    let tokens = lex(input).unwrap();
    let module = parse(&tokens).unwrap();
    let action = match &module.items[1] {
        Item::Action(action) => action,
        other => panic!("expected action, found {:?}", other),
    };
    let preserve = action
        .body
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::Expr(Expr::Preserve(p)) | Stmt::Discard(Expr::Preserve(p)) => Some(p.clone()),
            _ => None,
        })
        .expect("expected preserve expression");
    assert_eq!(preserve.output_name, "output");
    assert_eq!(preserve.input_name, "input");
    assert_eq!(preserve.fields, vec!["amount".to_string()]);
}

#[test]
fn test_reject_empty_preserve_block() {
    let input = r#"
module test;

action test(input: u64) -> (output: u64) {

    preserve output from input {
    }
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("preserve block is empty"), "unexpected error: {}", err.message);
    assert_eq!(err.code.as_deref(), Some("E1001"));
}

#[test]
fn test_reject_empty_require_block() {
    let input = r#"
module test;

action test() -> u64 {

    require {
    }
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("require block is empty"), "unexpected error: {}", err.message);
    assert_eq!(err.code.as_deref(), Some("E1001"));
}

#[test]
fn test_reject_preserve_wildcard() {
    let input = r#"
module test;

action test(input: u64) -> (output: u64) {

    preserve output from input {
        *
    }
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("wildcard"), "unexpected error: {}", err.message);
    assert_eq!(err.code.as_deref(), Some("E1007"));
}

#[test]
fn test_reject_preserve_except() {
    let input = r#"
module test;

action test(input: u64) -> (output: u64) {

    preserve output from input {
        except
    }
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("except"), "unexpected error: {}", err.message);
    assert_eq!(err.code.as_deref(), Some("E1008"));
}

#[test]
fn test_reject_bare_preserve() {
    let input = r#"
module test;

action test(input: u64) -> (output: u64) {

    preserve output from input
    return 0
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("bare"), "unexpected error: {}", err.message);
    assert_eq!(err.code.as_deref(), Some("E1009"));
}

#[test]
fn test_reject_require_block_with_consume() {
    let input = r#"
module test;

resource Token has store {
    amount: u64
}

action test(input: Token) -> u64 {

    require {
        consume input
    }
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("statement"), "unexpected error: {}", err.message);
    assert_eq!(err.code.as_deref(), Some("E1005"));
}

#[test]
fn test_reject_require_block_with_control_flow() {
    let input = r#"
module test;

action test(x: u64) -> u64 {

    require {
        if x > 0 {
            true
        } else {
            false
        }
    }
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("control flow"), "unexpected error: {}", err.message);
    assert_eq!(err.code.as_deref(), Some("E1006"));
}

#[test]
fn bounded_quantifier_rejects_unbounded_forall_syntax() {
    let input = r#"
module test;

resource Token {
    amount: u64
}

invariant bad {
    trigger: type_group;
    scope: group;
    reads: group_outputs<Token>.amount;
    forall token: Token {
        require token.amount > 0
    }
}
"#;
    let tokens = lex(input).unwrap();
    let err = parse(&tokens).unwrap_err();
    assert!(err.message.contains("unbounded forall"), "unexpected error: {}", err.message);
}

#[test]
fn rejects_deeply_nested_expressions_without_stack_overflow() {
    let expression = format!("{}1{}", "(".repeat(MAX_PARSE_RECURSION_DEPTH + 1), ")".repeat(MAX_PARSE_RECURSION_DEPTH + 1));
    let input = format!("module test;\naction deep() -> u64 {{\n\nreturn {expression};\n}}");
    let tokens = lex(&input).unwrap();
    let error = parse(&tokens).expect_err("deep expression must hit the parser recursion budget");
    assert!(error.message.contains("expression nesting exceeds"), "unexpected error: {}", error.message);
}

#[test]
fn rejects_deeply_nested_unary_expressions_without_stack_overflow() {
    let expression = format!("{}1", "-".repeat(MAX_PARSE_RECURSION_DEPTH + 1));
    let input = format!("module test;\naction deep() -> i32 {{\n\nreturn {expression};\n}}");
    let tokens = lex(&input).unwrap();
    let error = parse(&tokens).expect_err("deep unary expression must hit the parser recursion budget");
    assert!(error.message.contains("unary expression nesting exceeds"), "unexpected error: {}", error.message);
}

#[test]
fn rejects_deeply_nested_types_without_stack_overflow() {
    let ty = format!("{}u8{}", "[".repeat(MAX_PARSE_RECURSION_DEPTH + 1), "; 1]".repeat(MAX_PARSE_RECURSION_DEPTH + 1));
    let input = format!("module test;\nstruct Deep {{\nvalue: {ty},\n}}");
    let tokens = lex(&input).unwrap();
    let error = parse(&tokens).expect_err("deep type must hit the parser recursion budget");
    assert!(error.message.contains("type nesting exceeds"), "unexpected error: {}", error.message);
}

#[test]
fn rejects_deeply_nested_statements_without_stack_overflow() {
    let nested =
        format!("{}return 1;\n{}", "if true {\n".repeat(MAX_PARSE_RECURSION_DEPTH + 1), "}\n".repeat(MAX_PARSE_RECURSION_DEPTH + 1));
    let input = format!("module test;\naction deep() -> u64 {{\n\n{nested}}}");
    let tokens = lex(&input).unwrap();
    let error = parse(&tokens).expect_err("deep statements must hit the parser recursion budget");
    assert!(error.message.contains("statement nesting exceeds"), "unexpected error: {}", error.message);
}

#[test]
fn rejects_deep_else_if_chains_without_stack_overflow() {
    let chain = format!("{}{{ return 1; }}", "if true { return 1; } else ".repeat(MAX_PARSE_RECURSION_DEPTH + 1));
    let input = format!("module test;\naction deep() -> u64 {{\n\n{chain}\n}}");
    let tokens = lex(&input).unwrap();
    let error = parse(&tokens).expect_err("deep else-if chain must hit the parser recursion budget");
    assert!(error.message.contains("if nesting exceeds"), "unexpected error: {}", error.message);
}
