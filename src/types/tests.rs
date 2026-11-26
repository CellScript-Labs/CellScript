use super::*;

#[test]
fn witness_view_type_admission_retains_exact_spelling_and_extent_limit() {
    use cellscript_artifact_checker::view_types::{parse_witness_bytes_view_type, WitnessViewSyntax};
    let vectors: serde_json::Value =
        serde_json::from_str(cellscript_artifact_checker::view_types::WITNESS_BYTES_VIEW_CONFORMANCE_VECTORS).unwrap();
    for vector in vectors["cases"].as_array().unwrap() {
        let wire = parse_witness_bytes_view_type(vector["input"].as_str().unwrap(), WitnessViewSyntax::ExactArguments);
        let ty = wire.map(|view| Type::witness_bytes_view(view.owner, view.maximum)).unwrap_or(Type::U64);
        let expected =
            vector["exact"].as_bool().unwrap() && vector["maximum"].as_u64().is_some_and(|maximum| maximum <= CKB_WITNESS_BYTES_MAX);
        assert_eq!(TypeChecker::witness_bytes_view_parts(&ty).is_some(), expected, "{ty:?}");
    }
    assert_eq!(TypeChecker::witness_bytes_view_parts(&Type::U64), None);
}

use crate::{lexer, parser, resolve::ModuleResolver};
use camino::Utf8PathBuf;

fn example_module(name: &str) -> Module {
    let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join(name);
    let source = std::fs::read_to_string(path).unwrap();
    let tokens = lexer::lex(&source).unwrap();
    parser::parse(&tokens).unwrap()
}

fn source_module(source: &str) -> Module {
    let tokens = lexer::lex(source).unwrap();
    parser::parse(&tokens).unwrap()
}

#[test]
fn diagnostics_collect_independent_callable_errors() {
    let module = source_module(
        r#"
module multi_errors;

action bad_one() -> u64 {

        return true;
}

action bad_two() -> bool {

        return 1;
}
"#,
    );
    let diagnostics = super::diagnostics(&module);
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().any(|error| error.message.contains("expected U64, found Bool")));
    assert!(diagnostics.iter().any(|error| error.message.contains("expected Bool, found U64")));
}

#[test]
fn checked_casts_reject_cell_identity_erasure() {
    let module = source_module(
        r#"
module checked_casts;

resource Token has store, create, consume {
    amount: u64,
}

action erase(input token: Token) -> u64 {

        let erased = token as u64;
        consume token;
        return erased;
}
"#,
    );
    let error = super::check(&module).expect_err("cell-backed cast must fail");
    assert!(error.message.contains("casts cannot erase Cell identity"), "unexpected error: {}", error.message);
}

#[test]
fn checked_casts_accept_unsigned_numeric_narrowing_for_runtime_guarding() {
    let module = source_module(
        r#"
module checked_casts;

fn narrow(value: u64) -> u8 {
    return value as u8;
}
"#,
    );
    super::check(&module).expect("numeric narrowing is checked during lowering");
}

#[test]
fn diagnostics_collect_independent_statement_errors_in_callable() {
    let module = source_module(
        r#"
module multi_errors;

action bad() -> bool {

        let first: u64 = true;
        let second: bool = 1;
        return true;
}
"#,
    );
    let diagnostics = super::diagnostics(&module);
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().any(|error| error.message.contains("expected U64, found Bool")));
    assert!(diagnostics.iter().any(|error| error.message.contains("expected Bool, found U64")));
}

#[test]
fn diagnostics_collect_nested_statement_errors() {
    let module = source_module(
        r#"
module multi_errors;

action bad() -> bool {

        if true {
            let first: u64 = true;
            let second: bool = 1;
        }
        return true;
}
"#,
    );
    let diagnostics = super::diagnostics(&module);
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().any(|error| error.message.contains("expected U64, found Bool")));
    assert!(diagnostics.iter().any(|error| error.message.contains("expected Bool, found U64")));
}

#[test]
fn imported_token_type_is_treated_as_linear() {
    let token = example_module("token.cell");
    let launch = example_module("launch.cell");

    let mut resolver = ModuleResolver::new();
    resolver.register_module(token).unwrap();
    resolver.register_module(launch.clone()).unwrap();

    let checker = TypeChecker::with_resolver(&resolver, launch.name.clone());
    assert!(checker.is_linear_type(&Type::Named("Token".to_string())));
}

#[test]
fn launch_module_type_checks_with_registered_imports() {
    let token = example_module("token.cell");
    let amm = example_module("amm_pool.cell");
    let launch = example_module("launch.cell");

    let mut resolver = ModuleResolver::new();
    resolver.register_module(token).unwrap();
    resolver.register_module(amm).unwrap();
    resolver.register_module(launch.clone()).unwrap();

    check_with_resolver(&launch, &resolver, &launch.name).unwrap();
}

#[test]
fn imported_type_ids_must_not_collide_in_visible_module_scope() {
    let left = source_module(
        r#"
module cellscript::left;

#[type_id("cellscript::asset::Token:v1")]
resource TokenA has store {
    amount: u64,
}
"#,
    );
    let right = source_module(
        r#"
module cellscript::right;

#[type_id("cellscript::asset::Token:v1")]
resource TokenB has store {
    amount: u64,
}
"#,
    );
    let app = source_module(
        r#"
module app;

use cellscript::left::TokenA;
use cellscript::right::TokenB;

action main(input a: TokenA) -> u64 {

        return a.amount;
}
"#,
    );

    let mut resolver = ModuleResolver::new();
    resolver.register_module(left).unwrap();
    resolver.register_module(right).unwrap();
    resolver.register_module(app.clone()).unwrap();

    let err = check_with_resolver(&app, &resolver, &app.name).unwrap_err();

    assert!(err.message.contains("duplicate type_id 'cellscript::asset::Token:v1'"), "unexpected error: {}", err.message);
}

#[test]
fn imported_linear_argument_is_marked_consumed_after_call() {
    let token = example_module("token.cell");
    let amm = example_module("amm_pool.cell");
    let launch = example_module("launch.cell");

    let mut resolver = ModuleResolver::new();
    resolver.register_module(token).unwrap();
    resolver.register_module(amm).unwrap();
    resolver.register_module(launch.clone()).unwrap();

    let action = launch
        .items
        .iter()
        .find_map(|item| match item {
            Item::Action(action) if action.name == "launch_token" => Some(action.clone()),
            _ => None,
        })
        .unwrap();

    let mut checker = TypeChecker::with_resolver(&resolver, launch.name.clone());
    checker.inferred_effects = crate::effects::infer_module(&launch, Some(&resolver)).unwrap();
    let mut env = checker.env.child();
    for param in &action.params {
        let is_linear = checker.is_linear_type(&param.ty);
        env.insert(param.name.clone(), param.ty.clone(), is_linear, param.is_mut);
    }
    checker.bind_action_outputs(&mut env, &action).unwrap();
    checker.current_callable = Some(CallableKind::Action);

    for stmt in &action.body {
        checker.check_stmt(&mut env, stmt).unwrap();
        if let Stmt::Let(let_stmt) = stmt
            && matches!(&let_stmt.pattern, BindingPattern::Tuple(_))
        {
            break;
        }
    }

    assert_eq!(env.linear_states.get("pool_paired_token"), Some(&LinearState::Consumed));
}

#[test]
fn preserve_rejects_mismatched_field_types() {
    let module = source_module(
        r#"
module test;

resource A has store, replace, relock, consume, burn {
    amount: u64,
}

resource B has store, replace, relock, consume, burn {
    amount: bool,
}

action bad_preserve(input a: A,  output b: B) {

        consume a;
        preserve b from a { amount, }
        create b = B { amount: true, }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("preserve field 'amount' type mismatch"), "unexpected error: {}", err.message);
}

#[test]
fn require_rejects_nested_cell_operation() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

action hidden_consume(input coin: Coin) {

        require (destroy coin) == 0;
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("require condition contains cell/runtime operation"), "unexpected error: {}", err.message);
}

#[test]
fn require_block_rejects_lifecycle_stdlib_call() {
    let module = source_module(
        r#"
module test;

receipt Voucher has destroy {
    amount: u64,
}

action hidden_claim(input voucher: Voucher) {

        require {
            std::receipt::claim(voucher, voucher, voucher.amount),
        };
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("require block contains verifier-boundary syntax"), "unexpected error: {}", err.message);
}

#[test]
fn require_block_rejects_assignment_expression() {
    let module = source_module(
        r#"
module test;

action hidden_mutation(witness flag: bool) {

        let mut ok = flag;
        require {
            ok = false,
        };
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("require block contains assignment"), "unexpected error: {}", err.message);
}

#[test]
fn numeric_equality_requires_exact_non_literal_types() {
    let module = source_module(
        r#"
module test;

action compare(witness left: u8, witness right: u128) -> bool {

        return left == right;
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("comparison requires matching types"), "unexpected error: {}", err.message);
}

#[test]
fn typed_integer_literals_keep_declared_numeric_widths() {
    let module = source_module(
        r#"
module test;

const SMALL: u8 = 2;

action literal_widths(witness flag: bool) -> u8 {

        let left: u8 = 1;
        let right: u8 = if flag { SMALL } else { 3 };
        return left + right;
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn explicit_return_integer_literals_use_declared_return_widths() {
    let module = source_module(
        r#"
module test;

fn as_u8() -> u8 {
    return 5;
}

fn as_u32() -> u32 {
    return 5;
}

fn as_i32() -> i32 {
    return 5;
}

fn as_u128() -> u128 {
    return 5;
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn i32_arithmetic_accepts_matching_i32_values() {
    let module = source_module(
        r#"
module test;

fn add(left: i32, right: i32) -> i32 {
    return left + right;
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn mixed_i32_u32_arithmetic_is_rejected() {
    let module = source_module(
        r#"
module test;

fn add(left: i32, right: u32) -> i32 {
    return left + right;
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("arithmetic operations require matching numeric types"), "unexpected error: {}", err.message);
}

#[test]
fn unsigned_arithmetic_widens_to_declared_result_type() {
    let module = source_module(
        r#"
module test;

fn add(left: u8, right: u16) -> u16 {
    return left + right;
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn vec_push_integer_literal_uses_item_width() {
    let module = source_module(
        r#"
module test;

action collect() -> u8 {

        let mut values: Vec<u8> = [];
        values.push(5);
        return values.first();
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn struct_field_integer_literal_uses_field_width() {
    let module = source_module(
        r#"
module test;

struct Narrow {
    amount: u8,
}

action make() -> Narrow {

        return new Narrow { amount: 5, };
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn unsigned_ordering_comparison_accepts_widening() {
    let module = source_module(
        r#"
module test;

action compare(witness left: u8, witness right: u16) -> bool {

        return left < right;
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn add_assign_integer_literal_uses_target_width() {
    let module = source_module(
        r#"
module test;

action increment() -> u8 {

        let mut value: u8 = 1;
        value += 2;
        return value;
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn typed_integer_literals_must_fit_expected_numeric_type() {
    let module = source_module(
        r#"
module test;

const TOO_BIG: u8 = 256;

action bad() -> u8 {

        return TOO_BIG;
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("integer literal 256 does not fit expected type u8"), "unexpected error: {}", err.message);
    assert_eq!(err.code, Some(crate::error::DiagnosticCode::E1030));
}

#[test]
fn ordering_comparison_retains_literal_range_identity_and_source_span() {
    for comparison in ["value < 256", "256 > value"] {
        let source = format!("module test;\naction bad(witness value: u8) -> bool {{\n    \n        return {comparison};\n}}\n");
        let err = check(&source_module(&source)).unwrap_err();
        assert_eq!(err.code, Some(crate::error::DiagnosticCode::E1030), "{err}");
        assert_eq!(err.span.line, 4);
        assert!(err.span.end > err.span.start);
    }
}

#[test]
fn byte_string_literal_infers_exact_fixed_array_length() {
    let module = source_module(
        r#"
module test;

action symbol() -> [u8; 4]
{

        return b"TEST";
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn match_wildcard_arm_must_be_last() {
    let module = source_module(
        r#"
module test;

enum Flag {
    Off,
    On,
}

action bad(witness flag: Flag) -> u64
{

        return match flag {
            _ => { 1 },
            Flag::Off => { 2 },
        };
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(
        err.message.contains("wildcard pattern '_' or another irrefutable pattern must be the last match arm"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn stdlib_claim_rejects_non_receipt_input() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

action bad_claim(input coin: Coin, witness to: Address,  output next_coin: Coin) {

        std::receipt::claim(coin, next_coin, to) { amount, }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("std::receipt::claim requires a receipt cell input"), "unexpected error: {}", err.message);
}

#[test]
fn stdlib_claim_rejects_declared_output_type_mismatch() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

resource Badge has store, replace, relock, consume, burn {
    amount: u64,
}

receipt Voucher -> Coin has destroy {
    amount: u64,
}

action bad_claim(input voucher: Voucher, witness to: Address,  output badge: Badge) {

        std::receipt::claim(voucher, badge, to) { amount, }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("std::receipt::claim output type mismatch"), "unexpected error: {}", err.message);
}

#[test]
fn stdlib_claim_rejects_extra_arguments() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

receipt Voucher has destroy {
    amount: u64,
}

action bad_claim(input voucher: Voucher,  output coin: Coin) {

        std::receipt::claim(voucher, coin, coin, coin)
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("std::receipt::claim expects 3 arguments"), "unexpected error: {}", err.message);
}

#[test]
fn stdlib_claim_output_requires_declared_claim_output_type() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

receipt Voucher has destroy {
    amount: u64,
    owner: Address,
}

action bad_claim(input voucher: Voucher,  output coin: Coin) {

        std::receipt::claim(voucher, coin, voucher.owner) { amount, }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(
        err.message.contains("std::receipt::claim with an output requires receipt 'Voucher' to declare a claim output type"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn stdlib_claim_requires_explicit_output_and_lock_arguments() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

receipt Voucher -> Coin has destroy {
    amount: u64,
}

action bad_claim(input voucher: Voucher,  output coin: Coin) {

        std::receipt::claim(voucher, coin) { amount, }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("std::receipt::claim expects 3 arguments"), "unexpected error: {}", err.message);
}

#[test]
fn stdlib_settle_requires_explicit_output_and_lock_arguments() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

action bad_settle(input coin: Coin,  output next_coin: Coin) {

        std::lifecycle::settle(coin, next_coin) { amount, }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("std::lifecycle::settle expects 3 arguments"), "unexpected error: {}", err.message);
}

#[test]
fn stdlib_transfer_output_requires_complete_field_coverage() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
    owner: Address,
}

action bad_transfer(input coin: Coin, witness to: Address,  output next_coin: Coin) {

        std::lifecycle::transfer(coin, next_coin, to) { amount, }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(
        err.message.contains("std::lifecycle::transfer output construction must cover every 'Coin' field; missing owner"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn stdlib_claim_output_requires_complete_field_coverage() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
    owner: Address,
}

receipt Voucher -> Coin has destroy {
    amount: u64,
    owner: Address,
}

action bad_claim(input voucher: Voucher,  output coin: Coin) {

        std::receipt::claim(voucher, coin, voucher.owner) { amount, }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(
        err.message.contains("std::receipt::claim output construction must cover every 'Coin' field; missing owner"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn stdlib_transfer_rejects_extra_arguments() {
    let module = source_module(
        r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

action bad_transfer(input coin: Coin, witness to: Address,  output next_coin: Coin) {

        std::lifecycle::transfer(coin, next_coin, to, to) { amount, }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("std::lifecycle::transfer expects 3 arguments"), "unexpected error: {}", err.message);
}

#[test]
fn typed_transaction_views_are_source_specific_read_only_handles() {
    let module = source_module(
        r#"
module test;

resource Token has store {
    amount: u64,
}

action inspect() -> u64 {

        let input = ckb::input::<Token>(0);
        let output = ckb::output::<Token>(0);
        let dep = ckb::cell_dep(0);
        let header = ckb::header_dep(0);
        let witness_args = witness::args(0);
        let out_point = input.out_point;
        let lock = input.lock;
        require lock.args_empty || (lock.hash_type <= 4);
        require (dep.data_hash == dep.data_hash);
        return input.capacity + input.occupied_capacity + input.unoccupied_capacity + ckb::since_to_raw(input.since) + output.output_index + dep.data_size + witness_args.size + out_point.index + ckb::epoch_number_to_u64(header.epoch_number) + ckb::block_number_to_u64(header.epoch_start_block_number) + ckb::epoch_length_to_u64(header.epoch_length);
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn script_view_hash_domains_are_not_interchangeable() {
    let module = source_module(
        r#"
module test;

resource Token has store { amount: u64, }

action inspect() -> bool {

        let input = ckb::input::<Token>(0);
        let script = input.lock;
        let complete: ScriptHash = script.hash;
        let code: Hash = script.code_hash;
        let args: [u8; 32] = script.args32;
        return (complete.0 == code.0) || (code.0 == args);
}
"#,
    );
    check(&module).unwrap();

    let confused = source_module(
        r#"
module test;

resource Token has store { amount: u64, }

action inspect() -> ScriptHash {

        return ckb::input::<Token>(0).lock.code_hash;
}
"#,
    );
    let err = check(&confused).unwrap_err();
    assert!(err.message.contains("return type mismatch") && err.message.contains("Hash"), "unexpected error: {}", err.message);
}

#[test]
fn ckb_temporal_domains_require_explicit_raw_conversion() {
    let valid = source_module(
        r#"
module test;

resource Token has store { amount: u64, }

action inspect() -> bool {

        let header = ckb::header_dep(0);
        let absolute = ckb::since_absolute_epoch(42, 3, 10);
        let another_absolute = ckb::since_absolute_epoch(43, 0, 10);
        let absolute_block = ckb::since_absolute_block(42);
        let another_absolute_block = ckb::since_absolute_block(43);
        let absolute_timestamp = ckb::since_absolute_timestamp(1700000000);
        let another_absolute_timestamp = ckb::since_absolute_timestamp(1700000001);
        let duration = ckb::epoch_duration(5);
        let epoch_after = ckb::epoch_add(header.epoch_number, duration);
        let epoch_before = ckb::epoch_sub(header.epoch_number, duration);
        let encoded = ckb::input::<Token>(0).since;
        let decoded = ckb::since_decode(encoded);
        require (absolute < another_absolute);
        require (absolute_block < another_absolute_block);
        require (absolute_timestamp < another_absolute_timestamp);
        return (ckb::epoch_number_to_u64(header.epoch_number) == 42)
            && (ckb::block_number_to_u64(header.epoch_start_block_number) == 97)
            && (ckb::epoch_length_to_u64(header.epoch_length) == 10)
            && (ckb::since_to_raw(encoded) >= 0)
            && (ckb::since_to_raw(decoded) >= 0)
            && (ckb::since_metric(decoded) <= 2)
            && (ckb::since_epoch_absolute(1, 0, 1) >= 0)
            && (ckb::input_since_at(source::input(0)) >= 0)
            && (ckb::epoch_number_to_u64(epoch_after) == 47)
            && (ckb::epoch_number_to_u64(epoch_before) == 37)
            && (ckb::epoch_duration_to_u64(duration) == 5)
            && (ckb::block_number_to_u64(header.block_number) >= 0)
            && (ckb::timestamp_millis_to_u64(header.timestamp) >= 0);
}
"#,
    );
    check(&valid).unwrap();

    for (expression, expected) in [
        ("ckb::since_absolute_epoch(1, 0, 1) == ckb::since_relative_epoch(1, 0, 1)", "comparison requires matching types"),
        ("ckb::since_absolute_block(1) == ckb::since_relative_block(1)", "comparison requires matching types"),
        ("ckb::since_absolute_block(1) == ckb::since_absolute_timestamp(1)", "comparison requires matching types"),
        ("ckb::since_decode(ckb::input::<Token>(0).since) == ckb::since_absolute_block(0)", "comparison requires matching types"),
        ("ckb::header_dep(0).epoch_number == ckb::header_dep(0).epoch_start_block_number", "comparison requires matching types"),
        ("ckb::input::<Token>(0).since == 0", "comparison requires matching types"),
        (
            "ckb::epoch_add(ckb::header_dep(0).epoch_number, 1) == ckb::header_dep(0).epoch_number",
            "expects (epoch: EpochNumber, duration: EpochDuration)",
        ),
        ("ckb::epoch_duration(1) == ckb::header_dep(0).epoch_number", "comparison requires matching types"),
        ("ckb::header_dep(0).timestamp == ckb::header_dep(0).block_number", "comparison requires matching types"),
    ] {
        let invalid = source_module(&format!(
            "module test;\nresource Token has store {{ amount: u64, }}\naction inspect() -> bool {{  return {expression}; }}"
        ));
        let err = check(&invalid).unwrap_err();
        assert!(err.message.contains(expected), "unexpected error for {expression}: {}", err.message);
    }
}

#[test]
fn typed_transaction_view_requires_a_cell_backed_type_argument() {
    let module = source_module(
        r#"
module test;

action inspect() -> u64 {

        let input = ckb::input::<u64>(0);
        return input.capacity;
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("type argument must be a named cell-backed type"), "unexpected error: {}", err.message);
}

#[test]
fn typed_transaction_view_rejects_source_mismatch() {
    let module = source_module(
        r#"
module test;

resource Token has store {
    amount: u64,
}

action inspect() -> u64 {

        return dao::accumulated_rate(ckb::input::<Token>(0));
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("HeaderDep"), "unexpected error: {}", err.message);
}

#[test]
fn typed_transaction_view_cannot_enter_linear_lifecycle_operations() {
    let module = source_module(
        r#"
module test;

resource Token has store {
    amount: u64,
}

action inspect() -> u64 {

        let input = ckb::input::<Token>(0);
        consume input;
        return 0;
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("consume requires a cell-backed linear value"), "unexpected error: {}", err.message);
}

#[test]
fn bounded_cell_dep_scan_requires_a_small_literal_bound() {
    for (bound, expected) in [("0", "must be in 1..=64"), ("65", "must be in 1..=64"), ("max_deps", "must be an integer literal")] {
        let module = source_module(&format!(
            r#"
module test;

action inspect(witness expected_hash: Hash, witness max_deps: u64) -> bool {{

        ckb::require_bounded_cell_dep_data_hash({bound}, expected_hash);
        true
}}
"#
        ));
        let err = check(&module).unwrap_err();
        assert!(err.message.contains(expected), "unexpected error for bound {bound}: {}", err.message);
    }
}

#[test]
fn bounded_sha256d_merkle_requires_a_small_literal_depth() {
    for (depth, expected) in [("17", "must be in 0..=16"), ("runtime_depth", "must be an integer literal")] {
        let module = source_module(&format!(
            r#"
module test;

action inspect(
    witness leaf: Hash,
    witness siblings: [Hash; 16],
    witness runtime_depth: u64,
    witness leaf_index: u64,
    witness expected_root: Hash,
) -> bool {{

        ckb::require_sha256d_merkle_root(leaf, siblings, {depth}, leaf_index, expected_root);
        true
}}
"#
        ));
        let err = check(&module).unwrap_err();
        assert!(err.message.contains(expected), "unexpected error for depth {depth}: {}", err.message);
    }
}

#[test]
fn bip340_explicit_cell_dep_requires_a_small_literal_index() {
    for (dep_index, expected) in [("64", "must be in 0..=63"), ("runtime_index", "must be an integer literal")] {
        let module = source_module(&format!(
            r#"
module test;

action inspect(
    witness runtime_index: u64,
    witness message_hash: Hash,
    witness xonly_pubkey: [u8; 32],
    witness signature: [u8; 64],
) -> bool {{

        verifier::btc::bip340::require_signature_from_cell_dep(
            {dep_index},
            message_hash,
            xonly_pubkey,
            signature,
        );
        true
}}
"#
        ));
        let err = check(&module).unwrap_err();
        assert!(err.message.contains(expected), "unexpected error for dep index {dep_index}: {}", err.message);
    }
}

#[test]
fn bounded_quantifiers_type_check_finite_cell_sources() {
    let module = source_module(
        r#"
module test;

resource Token {
    amount: u64,
}

invariant positive_outputs {
    trigger: type_group;
    scope: group;
    reads: group_outputs<Token>.amount;
    forall token in group_outputs<Token> {
        require token.amount > 0;
    }
}

invariant one_claim {
    trigger: explicit_entry;
    scope: transaction;
    reads: outputs<Token>.amount;
    count(outputs<Token> where amount == 7) == 1;
}
"#,
    );

    check(&module).unwrap();
}

#[test]
fn bounded_quantifier_requires_declared_read_coverage() {
    let module = source_module(
        r#"
module test;

resource Token {
    amount: u64,
}

invariant bad {
    trigger: type_group;
    scope: group;
    reads: group_inputs<Token>.amount;
    forall token in group_outputs<Token> {
        require token.amount > 0;
    }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("must be declared") && err.message.contains("reads metadata"), "unexpected error: {}", err.message);
}

#[test]
fn bounded_quantifier_predicate_rejects_lifecycle_operations() {
    let module = source_module(
        r#"
module test;

resource Token has consume {
    amount: u64,
}

invariant bad {
    trigger: type_group;
    scope: group;
    reads: group_outputs<Token>.amount;
    forall token in group_outputs<Token> {
        require (destroy token) > 0;
    }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(err.message.contains("contains cell/runtime operation"), "unexpected error: {}", err.message);
}

#[test]
fn bounded_quantifier_predicate_rejects_non_pure_helper_effect() {
    let module = source_module(
        r#"
module test;

resource Token {
    amount: u64,
}

fn positive(amount: u64) -> bool {
    return amount > ckb::cell_capacity(source::input(0));
}

invariant bad {
    trigger: type_group;
    scope: group;
    reads: group_outputs<Token>.amount;
    forall token in group_outputs<Token> {
        require positive(token.amount);
    }
}
"#,
    );

    let err = check(&module).unwrap_err();
    assert!(
        err.message.contains("only transitively free of external operations helpers are allowed"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn bounded_lifecycle_bodies_only_admit_outer_numeric_add_assign_accumulators() {
    let accepted = source_module(
        r#"
module test;

resource Token has store, consume { amount: u64, }

action batch(input inputs: BoundedCellSet<Token, 4>) -> u64 {

        let mut total: u64 = 0;
        consume_each token in inputs {
            require token.amount <= 100;
            total += token.amount;
        }
        require total <= 400;
        return 0;
}
"#,
    );
    check(&accepted).unwrap();

    let rejected = source_module(
        r#"
module test;

resource Token has store, consume { amount: u64, }

action batch(input inputs: BoundedCellSet<Token, 4>) -> u64 {

        let mut total: u64 = 0;
        consume_each token in inputs {
            total = token.amount;
        }
        return 0;
}
"#,
    );
    let err = check(&rejected).unwrap_err();
    assert!(
        err.message.contains("only permits '+=' updates to mutable outer numeric accumulators"),
        "unexpected error: {}",
        err.message
    );

    let impure_rhs = source_module(
        r#"
module test;

resource Token has store, consume { amount: u64, }

fn observed_amount(amount: u64) -> u64 {
    return amount + ckb::cell_capacity(source::input(0));
}

action batch(input inputs: BoundedCellSet<Token, 4>) -> u64 {

        let mut total: u64 = 0;
        consume_each token in inputs {
            total += observed_amount(token.amount);
        }
        return 0;
}
"#,
    );
    let err = check(&impure_rhs).unwrap_err();
    assert!(
        err.message.contains("only transitively free of external operations helpers are allowed"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn typed_opening_is_explicit_one_shot_and_type_matched() {
    let accepted = source_module(
        r#"
module committed;

struct State {
    counter: u64,
    owner: Address,
}

lock reveal(witness expected: Commitment<State>, witness opening: Opening<State>) -> bool {

        let state = commitment::open(expected, opening);
        return state.counter > 0;
}
"#,
    );
    check(&accepted).unwrap();

    let implicit = source_module(
        r#"
module committed;
struct State { counter: u64, }
lock reveal(witness expected: Commitment<State>, opening: Opening<State>) -> bool {

        let state = commitment::open(expected, opening);
        return state.counter > 0;
}
"#,
    );
    let err = check(&implicit).unwrap_err();
    assert!(err.message.contains("must be an explicit witness binding"), "unexpected error: {}", err.message);

    let escaped = source_module(
        r#"
module committed;
struct State { counter: u64, }
lock reveal(witness expected: Commitment<State>, witness opening: Opening<State>) -> bool {

        let alias = opening;
        let state = commitment::open(expected, alias);
        return state.counter > 0;
}
"#,
    );
    let err = check(&escaped).unwrap_err();
    assert!(err.message.contains("is opaque"), "unexpected error: {}", err.message);

    let mismatched = source_module(
        r#"
module committed;
struct State { counter: u64, }
struct Other { counter: u64, }
lock reveal(witness expected: Commitment<State>, witness opening: Opening<Other>) -> bool {

        let state = commitment::open(expected, opening);
        return state.counter > 0;
}
"#,
    );
    let err = check(&mismatched).unwrap_err();
    assert!(err.message.contains("type mismatch"), "unexpected error: {}", err.message);

    let unused = source_module(
        r#"
module committed;
struct State { counter: u64, }
lock reveal(witness expected: Commitment<State>, witness opening: Opening<State>) -> bool {

        return true;
}
"#,
    );
    let err = check(&unused).unwrap_err();
    assert!(err.message.contains("linear"), "unexpected error: {}", err.message);

    let reused = source_module(
        r#"
module committed;
struct State { counter: u64, }
lock reveal(witness expected: Commitment<State>, witness opening: Opening<State>) -> bool {

        let first = commitment::open(expected, opening);
        let second = commitment::open(expected, opening);
        return first.counter == second.counter;
}
"#,
    );
    let err = check(&reused).unwrap_err();
    assert!(err.message.contains("already Consumed"), "unexpected error: {}", err.message);

    let nested_commitment = source_module(
        r#"
module committed;
struct Inner { counter: u64, }
struct Outer { inner: Commitment<Inner>, }
lock reject(witness outer: Outer) -> bool {

        let nested = commitment::commit(outer);
        return nested == nested;
}
"#,
    );
    let err = check(&nested_commitment).unwrap_err();
    assert!(err.message.contains("non-wrapper"), "unexpected error: {}", err.message);

    let stored_opening = source_module(
        r#"
module committed;
struct State { counter: u64, }
resource Vault has store { opening: Opening<State>, }
lock reject() -> bool {

        return true;
}
"#,
    );
    let err = check(&stored_opening).unwrap_err();
    assert!(err.message.contains("cannot store Opening<T>"), "unexpected error: {}", err.message);
}
