const LINEAR_BRANCH_RETURN_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action choose(input token: Token, witness flag: bool) -> Token {

        if flag {
            return token;
        } else {
            return token;
        }
}
"#;

const LINEAR_BRANCH_INCONSISTENT_RETURN_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(input token: Token, witness flag: bool) -> u64 {

        if flag {
            return 1;
        } else {
            consume token;
            return 2;
        }
}
"#;

const LINEAR_TAIL_IF_RETURN_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action choose(input token: Token, witness flag: bool) -> Token {

        if flag { token } else { token }
}
"#;

const LINEAR_TAIL_IF_INCONSISTENT_RETURN_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(input left: Token, input right: Token, witness flag: bool) -> Token {

        if flag { left } else { right }
}
"#;

const LINEAR_IF_EXPR_LET_MOVE_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action choose(input token: Token, witness flag: bool) -> Token {

        let moved = if flag { token } else { token };
        return moved;
}
"#;

const LINEAR_IF_EXPR_INCONSISTENT_LET_MOVE_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(input left: Token, input right: Token, witness flag: bool) -> Token {

        let moved = if flag { left } else { right };
        return moved;
}
"#;

const LINEAR_IF_EXPR_STATEFUL_BRANCHES_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action burn(input token: Token, witness flag: bool) -> u64 {

        if flag { destroy token; } else { destroy token; }
        return 0;
}
"#;

const LINEAR_TUPLE_BINDING_DROPPED_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(input left: Token, input right: Token) {

        let pair = (left, right);
}
"#;

const LINEAR_ARRAY_BINDING_DROPPED_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(input left: Token, input right: Token) {

        let items = [left, right];
}
"#;

const LINEAR_TUPLE_DESTRUCTURE_HANDLES_ITEMS_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action choose(input left: Token, input right: Token) -> Token {

        let (kept, burned) = (left, right);
        destroy burned;
        return kept;
}
"#;

const LINEAR_WILDCARD_DISCARD_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(input token: Token) {

        let _ = token;
}
"#;

const LINEAR_TUPLE_WILDCARD_DISCARD_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(input left: Token, input right: Token) {

        let (_, kept) = (left, right);
        destroy kept;
}
"#;

const LINEAR_TUPLE_FIELD_PROJECTION_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(input left: Token, input right: Token) -> (Token, Token) {

        let pair = (left, right);
        let first = pair.0;
        destroy first;
        pair
}
"#;

const LINEAR_ARRAY_INDEX_PROJECTION_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(input left: Token, input right: Token) {

        let items = [left, right];
        let first = items[0];
        destroy first;
}
"#;

const LINEAR_BLOCK_EXPR_LET_MOVE_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action choose(input token: Token) -> Token {

        let moved = { token };
        return moved;
}
"#;

const LINEAR_BLOCK_EXPR_PREFIX_MOVE_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action choose(input token: Token) -> Token {

        let moved = {
            let inner = token;
            inner
        };
        return moved;
}
"#;

const LINEAR_BLOCK_EXPR_STATEFUL_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action burn(input token: Token) -> u64 {

        { destroy token; }
        return 0;
}
"#;

const LINEAR_BLOCK_EXPR_DROPPED_LOCAL_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad() -> u64 {

        {
            let out = create Token {
                amount: 1,
            };
            1
        }
}
"#;

const BLOCK_TAIL_IF_VALUE_PROGRAM: &str = r#"
module test;

action choose(witness flag: bool) -> u64 {

        let value = {
            if flag {
                1
            } else {
                2
            }
        };
        return value;
}
"#;

const LINEAR_BLOCK_TAIL_IF_MOVE_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action choose(input token: Token, witness flag: bool) -> Token {

        let moved = {
            let inner = token;
            if flag { inner } else { inner }
        };
        return moved;
}
"#;

const LINEAR_BLOCK_TAIL_IF_INCONSISTENT_MOVE_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(input left: Token, input right: Token, witness flag: bool) -> Token {

        let moved = {
            if flag { left } else { right }
        };
        return moved;
}
"#;

const TAIL_IF_ACTION_RETURN_PROGRAM: &str = r#"
module test;

action choose(witness flag: bool) -> u64 {

        if flag { 1 } else { 2 }
}
"#;

const BRANCH_INCOMPLETE_RETURN_PROGRAM: &str = r#"
module test;

action bad(witness flag: bool) -> u64 {

        if flag {
            return 1;
        }
        let x = 2;
}
"#;

const UNREACHABLE_AFTER_RETURN_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        return 1;
        let x = 2;
}
"#;

const UNREACHABLE_AFTER_BRANCH_RETURN_PROGRAM: &str = r#"
module test;

action bad(witness flag: bool) -> u64 {

        if flag {
            return 1;
        } else {
            return 2;
        }
        let x = 3;
}
"#;

const CKB_HEADER_EPOCH_PROGRAM: &str = r#"
module test;

action epoch() -> u64 {

        let number = ckb::header_epoch_number();
        let start = ckb::header_epoch_start_block_number();
        let length = ckb::header_epoch_length();
        let since = ckb::input_since();
        return number + start + length + since;
}
"#;

const BUILTIN_WRONG_ARITY_PROGRAM: &str = r#"
module test;

action now(witness x: u64) -> u64 {

        return env::current_timepoint(x);
}
"#;

const NUMERIC_BUILTIN_TYPE_MISMATCH_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        return min(1, false);
}
"#;

const UNKNOWN_NAMESPACED_CONSTRUCTOR_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let value = Missing::new();
        return 0;
}
"#;

const METHOD_WRONG_ARITY_PROGRAM: &str = r#"
module test;

action count(witness items: [u64; 3]) -> u64 {

        return items.len(1);
}
"#;

const LOCK_CALLS_FN_PROGRAM: &str = r#"
module test;

fn yes() -> bool {
    return true;
}

lock guard() -> bool {

        return yes();
}
"#;

const FN_CALLS_LOCK_PROGRAM: &str = r#"
module test;

lock guard() -> bool {

        return true;
}

fn bad() -> bool {
    return guard();
}
"#;

const INDIRECT_CREATION_EFFECT_PROGRAM: &str = r#"
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
action wrapper(witness amount: u64) -> Token {

        return issue(amount);
}
"#;

const QUALIFIED_CREATION_EFFECT_PROGRAM: &str = r#"
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
action wrapper(witness amount: u64) -> Token {

        return test::issue(amount);
}
"#;

const CONSUME_FIELD_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action inspect(input token: Token) -> u64 {

        let amount = token.amount;
        consume token;
        return amount;
}
"#;

const CONSUME_CREATE_CONSERVATION_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action pass(input token: Token) -> Token {

        let amount = token.amount;
        consume token;
        let out = create Token {
            amount: amount,
        };
        return out;
}
"#;

const CONSUME_CREATE_MERGE_CONSERVATION_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action merge(input left: Token, input right: Token) -> Token {

        let left_amount = left.amount;
        let right_amount = right.amount;
        let total = left_amount + right_amount;
        consume left;
        consume right;
        let out = create Token {
            amount: total,
        };
        return out;
}
"#;

const CONSUME_CREATE_SPLIT_CONSERVATION_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action split(input token: Token, witness fee: u64) -> (Token, Token) {

        let amount = token.amount;
        let remaining = amount - fee;
        consume token;
        let change = create Token {
            amount: remaining,
        };
        let paid_fee = create Token {
            amount: fee,
        };
        return (change, paid_fee);
}
"#;

const CONSUME_CREATE_IDENTITY_FIELD_MERGE_CONSERVATION_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
    symbol: [u8; 8],
}

action merge(input left: Token, input right: Token) -> Token {

        require left.symbol == right.symbol, "symbol mismatch";
        let left_amount = left.amount;
        let right_amount = right.amount;
        let total = left_amount + right_amount;
        let symbol = left.symbol;
        consume left;
        consume right;
        let out = create Token {
            amount: total,
            symbol: symbol,
        };
        return out;
}
"#;

const DUPLICATE_READ_REF_PROGRAM: &str = r#"
module test;

shared Config {
    threshold: u64,
}

action inspect() -> u64 {

        let left = read_ref::<Config>();
        let right = read_ref::<Config>();
        return left.threshold + right.threshold;
}
"#;
