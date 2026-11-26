const READ_ONLY_EFFECT_PROGRAM: &str = r#"
module test;

shared Config {
    threshold: u64,
}

action inspect() -> u64 {

        let cfg = read_ref::<Config>();
        return cfg.threshold;
}
"#;

const CREATION_EFFECT_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action issue(witness amount: u64) -> Token {

        let out = create Token {
            amount: amount,
        };
        return out;
}
"#;

const IMPURE_FN_PROGRAM: &str = r#"
module test;

shared Config {
    threshold: u64,
}

fn helper() -> u64 {
    let cfg = read_ref::<Config>();
    return cfg.threshold;
}
"#;

const INDIRECT_IMPURE_FN_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action issue(witness amount: u64) -> u64 {

        let out = create Token {
            amount: amount,
        };
        destroy out;
        return amount;

}
fn helper(amount: u64) -> u64 {
    return issue(amount);
}
"#;

const FN_ENV_RUNTIME_PROGRAM: &str = r#"
module test;

fn helper() -> u64 {
    return env::current_timepoint();
}
"#;

const FN_CKB_HEADER_RUNTIME_PROGRAM: &str = r#"
module test;

fn helper() -> u64 {
    return ckb::header_epoch_number();
}
"#;

const FN_TYPE_HASH_RUNTIME_PROGRAM: &str = r#"
module test;

struct Plain {
    value: u64,
}

fn helper(plain: Plain) -> Hash {
    return plain.type_hash();
}
"#;

const ACTION_CALLS_FN_PROGRAM: &str = r#"
module test;

fn add_one(x: u64) -> u64 {
    return x + 1;
}

action run(witness x: u64) -> u64 {

        return add_one(x);
}
"#;

const CALL_MISSING_ARGUMENT_PROGRAM: &str = r#"
module test;

fn add(a: u64, b: u64) -> u64 {
    return a + b;
}

action run(witness x: u64) -> u64 {

        return add(x);
}
"#;

const CALL_TYPE_MISMATCH_PROGRAM: &str = r#"
module test;

fn add(a: u64, b: u64) -> u64 {
    return a + b;
}

action run(witness x: u64) -> u64 {

        return add(x, false);
}
"#;

const QUALIFIED_ACTION_CALLS_FN_PROGRAM: &str = r#"
module test;

fn add_one(x: u64) -> u64 {
    return x + 1;
}

action run(witness x: u64) -> u64 {

        return test::add_one(x);
}
"#;

const QUALIFIED_CALL_EXTRA_ARGUMENT_PROGRAM: &str = r#"
module test;

fn add_one(x: u64) -> u64 {
    return x + 1;
}

action run(witness x: u64) -> u64 {

        return test::add_one(x, 2);
}
"#;

const BOOL_FN_CALL_PROGRAM: &str = r#"
module test;

fn ready() -> bool {
    return true;
}

action run() -> bool {

        return test::ready();
}
"#;

const DUPLICATE_ACTION_PARAM_PROGRAM: &str = r#"
module test;

action bad(witness x: u64, witness x: u64) -> u64 {

        return x;
}
"#;

const WILDCARD_ACTION_PARAM_PROGRAM: &str = r#"
module test;

action bad(witness _: u64) -> u64 {

        return 1;
}
"#;

const DUPLICATE_FN_PARAM_PROGRAM: &str = r#"
module test;

fn bad(x: u64, x: u64) -> u64 {
    return x;
}

action run() -> u64 {

        return 1;
}
"#;

const WILDCARD_LOCK_PARAM_PROGRAM: &str = r#"
module test;

lock owned(witness _: Address) -> bool {

        return true;
}
"#;

const LOCAL_BINDING_REUSE_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let x = 1;
        let x = 2;
        return x;
}
"#;

const TUPLE_BINDING_REUSE_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let (x, x) = (1, 2);
        return x;
}
"#;

const BLOCK_BINDING_SHADOW_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let x = 1;
        let y = {
            let x = 2;
            x
        };
        return x + y;
}
"#;

const UNIT_FN_CALL_PROGRAM: &str = r#"
module test;

fn note(x: u64) {
    let y = x + 1;
}

action run(witness x: u64) -> u64 {

        note(x);
        return x;
}
"#;

const BIND_UNIT_FN_CALL_PROGRAM: &str = r#"
module test;

fn note(x: u64) {
    let y = x + 1;
}

action bad(witness x: u64) -> u64 {

        let y = note(x);
        return x;
}
"#;

const RETURN_UNIT_FN_CALL_PROGRAM: &str = r#"
module test;

fn note(x: u64) {
    let y = x + 1;
}

action bad(witness x: u64) -> u64 {

        return note(x);
}
"#;

const RETURN_VALUE_FROM_UNIT_ACTION_PROGRAM: &str = r#"
module test;

action bad() {

        return 1;
}
"#;

const BARE_RETURN_FROM_VALUE_ACTION_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        return;
}
"#;

const MISSING_ACTION_RETURN_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let x = 1;
}
"#;

const MISSING_FUNCTION_RETURN_PROGRAM: &str = r#"
module test;

fn bad() -> u64 {
    let x = 1;
}

action run() -> u64 {

        return 1;
}
"#;

const TAIL_EXPR_ACTION_RETURN_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        1
}
"#;

const BRANCH_COMPLETE_RETURN_PROGRAM: &str = r#"
module test;

action choose(witness flag: bool) -> u64 {

        if flag {
            return 1;
        } else {
            return 2;
        }
}
"#;
