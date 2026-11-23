const CREATE_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action mint(owner: Address) -> Token {

        let token = create Token {
            amount: 42,
        } with_lock(owner);
        return token;
}
"#;

const CREATE_VERIFY_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action issue() -> Token {

        let token = create Token {
            amount: 42,
        };
        return token;
}
"#;

const CREATE_UNSUPPORTED_OUTPUT_EXPR_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action issue(amount: u64) -> Token {

        let token = create Token {
            amount: amount * 2,
        };
        return token;
}
"#;

const CREATE_DUPLICATE_FIELD_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action issue() -> Token {

        return create Token {
            amount: 1,
            amount: 2,
        };
}
"#;

const CREATE_MISSING_FIELD_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
    owner: Address,
}

action issue(owner: Address) -> Token {

        return create Token {
            amount: 1,
        };
}
"#;

const CREATE_UNKNOWN_FIELD_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action issue() -> Token {

        return create Token {
            amount: 1,
            owner: Address::zero(),
        };
}
"#;

const CREATE_FIELD_TYPE_MISMATCH_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action issue() -> Token {

        return create Token {
            amount: false,
        };
}
"#;

const CREATE_STRUCT_TARGET_PROGRAM: &str = r#"
module test;

struct Snapshot {
    amount: u64,
}

action bad() -> Snapshot {

        return create Snapshot {
            amount: 1,
        };
}
"#;

const CONSUME_NON_CELL_PROGRAM: &str = r#"
module test;

struct Snapshot {
    amount: u64,
}

action bad(snapshot: Snapshot) -> u64 {

        consume snapshot;
        return 0;
}
"#;

const CONSUME_NON_NAMED_CELL_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(token: Token) -> u64 {

        consume if true { token } else { token };
        return 0;
}
"#;

const IF_BOTH_BRANCHES_CONSUME_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action burn(token: Token, flag: bool) {

        if flag {
            consume token;
        } else {
            consume token;
        }
}
"#;

const IF_PARTIAL_BRANCH_CONSUME_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action burn(token: Token, flag: bool) {

        if flag {
            consume token;
        }
        consume token;
}
"#;

const IF_MISMATCHED_BRANCH_CONSUME_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action burn(token: Token, flag: bool) {

        if flag {
            consume token;
        } else {
            let amount = token.amount;
        }
        consume token;
}
"#;

const CONSUME_DESTROY_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action burn(a: Token, b: Token) {

        consume a;
        destroy b;
}
"#;

const PARAM_FIELD_PROGRAM: &str = r#"
module test;

struct Snapshot {
    amount: u64,
}

action inspect(snapshot: Snapshot) -> u64 {

        return snapshot.amount;
}
"#;

const FIXED_BYTE_FIELD_COMPARISON_PROGRAM: &str = r#"
module test;

resource Token {
    symbol: [u8; 8],
}

action same_symbol(left: Token, right: Token) -> bool {

        let same = left.symbol == right.symbol;
        consume left;
        consume right;
        return same;
}
"#;

const PACKED_SCALAR_FIELD_PROGRAM: &str = r#"
module test;

shared Flags {
    enabled: bool,
    nonce: u32,
}

action inspect() -> u32 {

        let flags = read_ref::<Flags>();
        let enabled = flags.enabled;
        return flags.nonce;
}
"#;

const CREATE_SCALAR_VERIFY_PROGRAM: &str = r#"
module test;

resource Flags {
    enabled: bool,
    nonce: u32,
}

action issue(nonce: u32) -> Flags {

        let enabled = true;
        let out = create Flags {
            enabled: enabled,
            nonce: nonce,
        };
        return out;
}
"#;

const CONSUME_CREATE_SCALAR_ALIAS_PROGRAM: &str = r#"
module test;

resource Flags {
    enabled: bool,
    nonce: u32,
}

action pass(flags: Flags) -> Flags {

        let enabled = flags.enabled;
        let nonce = flags.nonce;
        consume flags;
        let out = create Flags {
            enabled: enabled,
            nonce: nonce,
        };
        return out;
}
"#;

const READ_REF_FIELD_PROGRAM: &str = r#"
module test;

shared Config {
    threshold: u64,
}

action inspect() -> u64 {

        let cfg = read_ref::<Config>();
        return cfg.threshold;
}
"#;

const READ_REF_STRUCT_TARGET_PROGRAM: &str = r#"
module test;

struct Snapshot {
    amount: u64,
}

action bad() -> u64 {

        let snapshot = read_ref::<Snapshot>();
        return snapshot.amount;
}
"#;
