const INDEXED_TUPLE_PROGRAM: &str = r#"
module test;

action second(witness entries: [(Address, u64); 2]) -> u64 {

        return entries[0].1;
}
"#;

const FOREACH_ARRAY_PROGRAM: &str = r#"
module test;

action sum(witness items: [u64; 3]) -> u64 {

        let mut total: u64 = 0;
        for item in items {
            total += item;
        }
        return total;
}
"#;

const LOCAL_FOREACH_ARRAY_PROGRAM: &str = r#"
module test;

action sum() -> u64 {

        let items = [1, 2, 3];
        let mut total: u64 = 0;
        for item in items {
            total += item;
        }
        return total;
}
"#;

const LOCAL_FOREACH_ARRAY_OF_TUPLES_PROGRAM: &str = r#"
module test;

action sum() -> u64 {

        let entries = [(Address::zero, 2), (Address::zero, 5)];
        let mut total: u64 = 0;
        for (_, amount) in entries {
            total += amount;
        }
        return total;
}
"#;

const LEN_METHOD_PROGRAM: &str = r#"
module test;

action count(witness items: [u64; 3]) -> u64 {

        return items.len();
}
"#;

const FORBIDDEN_UNWRAP_CALL_PROGRAM: &str = r#"
module test;

action bad(witness value: u64) -> u64 {

        return unwrap(value);
}
"#;

const FORBIDDEN_EXPECT_METHOD_PROGRAM: &str = r#"
module test;

action bad(witness value: u64) -> u64 {

        return value.expect("checked");
}
"#;

const FORBIDDEN_NAMESPACED_UNWRAP_OR_PROGRAM: &str = r#"
module test;

action bad(witness value: u64) -> u64 {

        return Option::unwrap_or(value, 0);
}
"#;

const LOCAL_ARRAY_LEN_PROGRAM: &str = r#"
module test;

action count() -> u64 {

        let items = [1, 2, 3];
        return items.len();
}
"#;

const TYPED_EMPTY_ARRAY_PROGRAM: &str = r#"
module test;

action count() -> u64 {

        let items: [u8; 0] = [];
        return items.len();
}
"#;

const UNTYPED_EMPTY_ARRAY_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let items = [];
        return 0;
}
"#;

const WRONG_LENGTH_EMPTY_ARRAY_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let items: [u8; 1] = [];
        return 0;
}
"#;

const EMPTY_LITERAL_NON_VEC_CONTEXT_PROGRAM: &str = r#"
module test;

struct Snapshot {
    values: [u8; 1],
}

action bad() -> u64 {

        let snapshot = new Snapshot {
            values: [],
        };
        return 0;
}
"#;

const LOCAL_ARRAY_STATIC_INDEX_PROGRAM: &str = r#"
module test;

action tweak() -> u64 {

        let mut items = [1, 2, 3];
        items[1] += 5;
        items[0] = 7;
        return items[0] + items[1] + items[2];
}
"#;

const IMMUTABLE_ARRAY_ASSIGN_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let items = [1, 2];
        items[0] = 3;
        return items[0];
}
"#;

const HETEROGENEOUS_ARRAY_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let items = [1, false];
        return 1;
}
"#;

const LOCAL_ARRAY_OOB_READ_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let items = [1, 2];
        return items[2];
}
"#;

const LOCAL_ARRAY_OOB_WRITE_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let mut items = [1, 2];
        items[2] = 3;
        return items[0];
}
"#;

const LOCAL_TUPLE_STATIC_FIELD_PROGRAM: &str = r#"
module test;

action tweak() -> u64 {

        let mut pair = (1, 2);
        pair.1 += 5;
        pair.0 = 7;
        return pair.0 + pair.1;
}
"#;

const ARRAY_OF_TUPLES_STATIC_INDEX_PROGRAM: &str = r#"
module test;

action pick() -> u64 {

        let entries = [(Address::zero, 2), (Address::zero, 5)];
        return entries[1].1;
}
"#;

const IMMUTABLE_TUPLE_ASSIGN_PROGRAM: &str = r#"
module test;

action bad() -> u64 {

        let pair = (1, 2);
        pair.1 = 3;
        return pair.1;
}
"#;

const LOCAL_TUPLE_DESTRUCTURE_PROGRAM: &str = r#"
module test;

action split() -> u64 {

        let pair = (1, 2);
        let (a, b) = pair;
        return a + b;
}
"#;

const MATCH_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

action select(witness flag: Flag) -> u64 {

        return match flag {
            Flag::On => { 1 }
            _ => { 2 }
        };
}
"#;

const EXHAUSTIVE_MATCH_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

action select(witness flag: Flag) -> u64 {

        return match flag {
            Flag::On => { 1 }
            Flag::Off => { 2 }
        };
}
"#;

const LINEAR_MATCH_EXPR_LET_MOVE_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

resource Token {
    amount: u64,
}

action choose(input token: Token, witness flag: Flag) -> Token {

        let moved = match flag {
            Flag::On => { token }
            Flag::Off => { token }
        };
        return moved;
}
"#;

const LINEAR_MATCH_EXPR_INCONSISTENT_LET_MOVE_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

resource Token {
    amount: u64,
}

action bad(input left: Token, input right: Token, witness flag: Flag) -> Token {

        let moved = match flag {
            Flag::On => { left }
            Flag::Off => { right }
        };
        return moved;
}
"#;

const LINEAR_MATCH_EXPR_STATEFUL_ARMS_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

resource Token has destroy {
    amount: u64,
}

action burn(input token: Token, witness flag: Flag) {

        match flag {
            Flag::On => { destroy token; }
            Flag::Off => { destroy token; }
        }
}
"#;

const LINEAR_MATCH_EXPR_INCONSISTENT_STATEFUL_ARMS_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

resource Token has destroy {
    amount: u64,
}

action bad(input token: Token, witness flag: Flag) {

        match flag {
            Flag::On => { destroy token; }
            Flag::Off => { (); }
        }
}
"#;

const UNKNOWN_MATCH_VARIANT_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

action select(witness flag: Flag) -> u64 {

        return match flag {
            Flag::Maybe => { 1 }
            _ => { 2 }
        };
}
"#;

const DUPLICATE_MATCH_VARIANT_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

action select(witness flag: Flag) -> u64 {

        return match flag {
            Flag::On => { 1 }
            On => { 2 }
            _ => { 3 }
        };
}
"#;

const NON_EXHAUSTIVE_MATCH_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

action select(witness flag: Flag) -> u64 {

        return match flag {
            Flag::On => { 1 }
        };
}
"#;

const ENUM_PAYLOAD_VARIANT_PROGRAM: &str = r#"
module test;

enum MaybeAmount {
    Some(u64),
    None,
}

action select(witness value: MaybeAmount) -> u64 {

        return match value {
            MaybeAmount::Some => { 1 }
            _ => { 0 }
        };
}
"#;

const ENUM_PAYLOAD_VALUE_PROGRAM: &str = r#"
module test;

enum AssetType {
    Native,
    Token(u64),
}

fn bad() -> AssetType {
    return AssetType::Token;
}
"#;

const UNKNOWN_ENUM_VALUE_PROGRAM: &str = r#"
module test;

enum Flag {
    On,
    Off,
}

action bad() -> Flag {

        return Flag::Maybe;
}
"#;

const UNKNOWN_NAMED_TYPE_PROGRAM: &str = r#"
module test;

resource Bad {
    missing: MissingType,
}

action run(input value: Bad) -> u64 {

        destroy value;
        return 0;
}
"#;

const RESERVED_OPTION_TYPE_PROGRAM: &str = r#"
module test;

action bad(witness value: Option<u64>) -> u64 {

        return 0;
}
"#;

const USER_GENERIC_TYPE_PROGRAM: &str = r#"
module test;

resource Token has store {
    amount: u64,
}

resource Vault has store {
    amount: u64,
}

action bad(input: Vault<Token>) -> u64 {

        return 0;
}
"#;

const DUPLICATE_TOP_LEVEL_SYMBOL_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action Token() -> u64 {

        return 0;
}
"#;
