const MUT_CELL_PARAM_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(mut token: Token) {

        consume token;
}
"#;

const MUT_READ_REF_PARAM_PROGRAM: &str = r#"
module test;

shared Config {
    threshold: u64,
}

action bad(mut read cfg: Config) -> u64 {

        return cfg.threshold;
}
"#;

const REDUNDANT_MUT_REF_PARAM_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
}

action bad(mut pool: &mut Pool) {

        pool.reserve = pool.reserve + 1;
}
"#;

const FUNCTION_MUT_REF_PARAM_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
}

fn bad(pool: &mut Pool) -> u64 {
    pool.reserve = pool.reserve + 1;
    return pool.reserve;
}
"#;

const LOCK_MUT_REF_PARAM_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
}

lock bad(pool: &mut Pool) -> bool {

        pool.reserve = pool.reserve + 1;
        return true;
}
"#;

const MUT_REF_LOCAL_ALIAS_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
}

action bad(pool: &mut Pool) -> u64 {

        let alias = pool;
        alias.reserve = alias.reserve + 1;
        return alias.reserve;
}
"#;

const MUT_REF_TUPLE_ALIAS_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
}

action bad(pool: &mut Pool) -> u64 {

        let pair = (pool, 0);
        pair.0.reserve = pair.0.reserve + 1;
        return pair.0.reserve;
}
"#;

const MUT_REF_IF_ALIAS_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
}

action bad(pool: &mut Pool, flag: bool) -> u64 {

        let alias = if flag { pool } else { pool };
        alias.reserve = alias.reserve + 1;
        return alias.reserve;
}
"#;

const MUT_REF_ASSIGN_ALIAS_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
}

action bad(pool: &mut Pool) -> u64 {

        let mut alias = read_ref::<Pool>();
        alias = pool;
        return alias.reserve;
}
"#;

const OWNED_LINEAR_FIELD_ASSIGN_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad() {

        let mut token = create Token { amount: 1, };
        token.amount = 2;
        destroy token;
}
"#;

const LINEAR_LOCAL_REF_ALIAS_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(token: Token) -> u64 {

        let view = &token;
        destroy token;
        return view.amount;
}
"#;

const LINEAR_FIELD_LOCAL_REF_ALIAS_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(token: Token) -> u64 {

        let amount = &token.amount;
        destroy token;
        return 0;
}
"#;

const LINEAR_TUPLE_REF_ALIAS_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(token: Token) -> u64 {

        let pair = (&token, 0);
        destroy token;
        return pair.0.amount;
}
"#;

const LINEAR_ARRAY_REF_ALIAS_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(token: Token) -> u64 {

        let refs = [&token];
        destroy token;
        return refs[0].amount;
}
"#;

const LINEAR_IF_REF_ALIAS_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(token: Token, flag: bool) -> u64 {

        let view = if flag { &token } else { &token };
        destroy token;
        return view.amount;
}
"#;

const LINEAR_ASSIGN_REF_ALIAS_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(token: Token) -> u64 {

        let mut view = read_ref::<Token>();
        view = &token;
        destroy token;
        return view.amount;
}
"#;

const LINEAR_ASSIGN_TUPLE_REF_ALIAS_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(token: Token) -> u64 {

        let mut pair = (read_ref::<Token>(), 0);
        pair = (&token, 0);
        destroy token;
        return pair.0.amount;
}
"#;

const LINEAR_ASSIGN_TUPLE_FIELD_REF_ALIAS_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(token: Token) -> u64 {

        let mut pair = (read_ref::<Token>(), 0);
        pair.0 = &token;
        destroy token;
        return pair.0.amount;
}
"#;

const ACTION_RETURN_REF_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action leak(token: Token) -> &Token {

        return &token;
}
"#;

const FUNCTION_RETURN_REF_PROGRAM: &str = r#"
module test;

struct Point {
    x: u64,
}

fn leak(point: &Point) -> &Point {
    return point;
}
"#;

const FUNCTION_CELL_PARAM_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

fn bad(token: Token) -> u64 {
    return token.amount;
}
"#;

const FUNCTION_CELL_RETURN_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

    fn bad(amount: u64) -> Token {
        return amount;
    }
"#;

const LOCK_CELL_PARAM_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

    lock bad(token: Token) -> bool {

            return true;
    }
"#;

const LOCK_BARE_REF_PARAM_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

lock bad(token: &Token) -> bool {

        return (token.amount > 0);
}
"#;

const CALLABLE_REFERENCE_TO_CELL_AGGREGATE_PARAM_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

fn bad(pair: &(Token, u64)) -> u64 {
    return 0;
}
"#;

const CALLABLE_MUT_REFERENCE_TO_CELL_AGGREGATE_PARAM_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
}

action bad(pool: &mut (Pool, u64)) -> u64 {

        return 0;
}
"#;

const FUNCTION_VEC_CELL_PARAM_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

fn bad(tokens: Vec<Token>) -> u64 {
    return 0;
}
"#;

const STRUCT_VEC_REFERENCE_FIELD_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

struct Holder {
    views: Vec<&Token>,
}
"#;

const VEC_PUSH_REFERENCE_PROGRAM: &str = r#"
module test;

struct Point {
    x: u64,
}

action bad(point: Point) -> u64 {

        let points = Vec::new();
        points.push(&point);
        return 0;
}
"#;

const CALLABLE_TUPLE_REFERENCE_PARAM_PROGRAM: &str = r#"
module test;

struct Point {
    x: u64,
}

fn bad(pair: (&Point, u64)) -> u64 {
    return pair.1;
}
"#;

const CALLABLE_ARRAY_MUT_REFERENCE_PARAM_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
}

action bad(pools: [&mut Pool; 1]) {

}
"#;

const CALLABLE_NESTED_REFERENCE_PARAM_PROGRAM: &str = r#"
module test;

struct Point {
    x: u64,
}

fn bad(view: &read_ref Point) -> u64 {
    return view.x
}
"#;

const ACTION_REF_PARAM_MODIFIER_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(ref token: Token) {
    verification
        consume token
}
"#;

const FUNCTION_REF_PARAM_MODIFIER_PROGRAM: &str = r#"
module test;

struct Point {
    x: u64,
}

fn bad(ref point: Point) -> u64 {
    return point.x
}
"#;

const LOCK_REF_PARAM_MODIFIER_PROGRAM: &str = r#"
module test;

lock bad(ref owner: Address) -> bool {
    verification
        return true
}
"#;

const SCHEMA_REFERENCE_FIELD_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

struct Holder {
    view: &Token,
}
"#;

const ENUM_REFERENCE_PAYLOAD_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

enum MaybeToken {
    Some(&Token),
    None,
}
"#;

const IF_MISMATCH_PROGRAM: &str = r#"
module test;

action choose(flag: bool) -> u64 {

        let y = if flag { 1 } else { false };
        return 1;
}
"#;

const UNKNOWN_FIELD_PROGRAM: &str = r#"
module test;

struct Point {
    x: u64,
}

action read(p: Point) -> u64 {

        return p.y;
}
"#;

const DUPLICATE_RESOURCE_FIELD_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
    amount: u128,
}
"#;

const DUPLICATE_SHARED_FIELD_PROGRAM: &str = r#"
module test;

shared Pool {
    reserve: u64,
    reserve: u64,
}
"#;

const DUPLICATE_RECEIPT_FIELD_PROGRAM: &str = r#"
module test;

receipt Grant {
    amount: u64,
    amount: u64,
}
"#;

const WILDCARD_STRUCT_FIELD_PROGRAM: &str = r#"
module test;

struct Point {
    _: u64,
}
"#;

const UNKNOWN_FUNCTION_PROGRAM: &str = r#"
module test;

action run(x: u64) -> u64 {

        return missing(x);
}
"#;

const CONSTANT_PROGRAM: &str = r#"
module test;

const STEP: u64 = 3;

action bump(x: u64) -> u64 {

        return x + STEP;
}
"#;

const CAST_PROGRAM: &str = r#"
module test;

action widen(x: u16) -> u64 {

        return x as u64;
}
"#;

const CHECKED_NARROWING_CAST_PROGRAM: &str = r#"
module test;

action narrow(x: u64) -> u8 {

        return x as u8;
}
"#;

const U64_DIVISION_PROGRAM: &str = r#"
module test;

const U64_MAX: u64 = 18446744073709551615;

struct Token {
    amount: u64,
}

action checked(token_a: Token, token_b: Token) -> u64 {

        require token_a.amount <= U64_MAX / token_b.amount;
        return token_a.amount / token_b.amount;
}
"#;

const ASSERT_PROGRAM: &str = r#"
module test;

action checked(x: u64) -> u64 {

        require x > 0, "x must be positive";
        return x;
}
"#;

const LOCK_BOUNDARY_CLASSIFICATION_PROGRAM: &str = r#"
module test;

resource Token {
    owner: Address,
}

lock owner_guard(token: protected Token, script_owner: lock_args Address, claimed_owner: witness Address) -> bool {

        require script_owner == token.owner;
        require claimed_owner == token.owner;
    true
}
"#;

const ACTION_PROTECTED_PARAM_PROGRAM: &str = r#"
module test;

resource Token {
    owner: Address,
}

action bad(protected token: Token) -> u64 {

        return 0;
}
"#;

const LOCK_ARGS_PARAM_PROGRAM: &str = r#"
module test;

resource Token {
    owner: Address,
}

lock bad(protected token: Token, lock_args owner: Address) -> bool {

        require owner == token.owner;
    true
}
"#;

const LOCK_ARGS_DYNAMIC_PARAM_PROGRAM: &str = r#"
module test;

resource Token {
    owner: Address,
}

struct Owner {
    address: Address,
}

lock bad(token: protected Token, owner: lock_args Owner) -> bool {

        require owner.address == token.owner;
    true
}
"#;

const ACTION_REQUIRE_PROGRAM: &str = r#"
module test;

action bad(x: u64) -> bool {

        require x > 0;
        true
}
"#;

const ACTION_REQUIRE_MESSAGE_PROGRAM: &str = r#"
module test;

action checked(x: u64) -> bool {

        require x > 0, "x must be positive";
        true
}
"#;

const REQUIRE_DYNAMIC_MESSAGE_PROGRAM: &str = r#"
module test;

action bad(x: u64, msg: String) -> bool {

        require x > 0, msg;
}
"#;

const FUNCTION_REQUIRE_PROGRAM: &str = r#"
module test;

fn bad(x: u64) -> bool {
    require x > 0;
        true
}
"#;

const ASSERT_NON_BOOL_PROGRAM: &str = r#"
module test;

action bad(x: u64) -> u64 {

        require x, "x must be boolean";
        return x;
}
"#;

const ASSERT_DYNAMIC_MESSAGE_PROGRAM: &str = r#"
module test;

action bad(x: u64) -> u64 {

        assert_invariant(x > 0, x);
        return x;
}
"#;

const ASSERT_BINDING_PROGRAM: &str = r#"
module test;

action bad(x: u64) -> u64 {

        let ok = assert_invariant(x > 0, "x must be positive");
        return x;
}
"#;

const ASSERT_TAIL_RETURN_PROGRAM: &str = r#"
module test;

action bad(x: u64) -> bool {

        assert_invariant(x > 0, "x must be positive")
}
"#;

const STRING_VALUE_PROGRAM: &str = r#"
module test;

action bad() -> String {

        return "not a lowered runtime value";
}
"#;
