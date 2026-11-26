const OPTIMIZER_PROGRAM: &str = r#"
module test;

action calc() -> u64 {

        return (2 + 3) * 4;
}
"#;

const IF_PROGRAM: &str = r#"
module test;

action increment_if(witness flag: bool, witness x: u64) -> u64 {

        if flag {
            let tmp = x + 1;
        }
        return x;
}
"#;

const CALL_PROGRAM: &str = r#"
module test;

action double(witness x: u64) -> u64 {

        return x + x;

}
action run(witness y: u64) -> u64 {

        let z = double(y);
        return z;
}
"#;

const IF_EXPR_PROGRAM: &str = r#"
module test;

action choose(witness flag: bool, witness x: u64) -> u64 {

        let y = if flag { x + 1 } else { x + 2 };
        return y;
}
"#;

const IF_EXPR_FIXED_BYTE_CONST_PROGRAM: &str = r#"
module test;

action choose_zero(witness flag: bool, witness target: Address) -> bool {

        let selected = if flag { Address::zero() } else { Address::zero() };
        return target == selected;
}
"#;

const WHILE_PROGRAM: &str = r#"
module test;

action spin(witness flag: bool, witness x: u64) -> u64 {

        while flag {
            let tmp = x + 1;
        }
        return x;
}
"#;

const FOR_RANGE_PROGRAM: &str = r#"
module test;

action visit(witness n: u64) -> u64 {

        for i in 0..n {
            let tmp = i + 1;
        }
        return n;
}
"#;

const ASSIGN_PROGRAM: &str = r#"
module test;

action countdown(witness n: u64) -> u64 {

        let mut x: u64 = n;
        while x > 0 {
            x = x - 1;
        }
        x += 1;
        return x;
}
"#;

const LOOP_LOCAL_LINEAR_COMPLETE_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action ok(witness n: u64) {

        for i in 0..n {
            let out = create Token {
                amount: i,
            };
            destroy out;
        }
}
"#;

const LOOP_DROPPED_LOCAL_LINEAR_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

action bad(witness n: u64) {

        for i in 0..n {
            let out = create Token {
                amount: i,
            };
        }
}
"#;

const FOR_LOOP_PARENT_LINEAR_CHANGE_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(input token: Token, witness n: u64) {

        for i in 0..n {
            destroy token;
        }
        destroy token;
}
"#;

const WHILE_LOOP_PARENT_LINEAR_CHANGE_PROGRAM: &str = r#"
module test;

resource Token has destroy {
    amount: u64,
}

action bad(input token: Token, witness flag: bool) {

        while flag {
            destroy token;
        }
        destroy token;
}
"#;

const STRUCT_FIELD_PROGRAM: &str = r#"
module test;

struct Point {
    x: u64,
    y: u64,
}

action tweak() -> u64 {

        let mut p = new Point { x: 1, y: 2, };
        let a = p.x;
        p.x = a + 4;
        p.x += 1;
        return p.x;
}
"#;

const TEMPORARY_FIELD_ASSIGN_PROGRAM: &str = r#"
module test;

struct Point {
    x: u64,
    y: u64,
}

fn point() -> Point {
    return new Point { x: 1, y: 2, };
}

action bad() -> u64 {

        point().x = 3;
        return 0;
}
"#;

const READ_REF_FIELD_ASSIGN_PROGRAM: &str = r#"
module test;

shared Config {
    threshold: u64,
}

action bad() -> u64 {

        read_ref::<Config>().threshold = 2;
        return 0;
}
"#;

const READ_REF_BINDING_FIELD_ASSIGN_PROGRAM: &str = r#"
module test;

shared Config {
    threshold: u64,
}

action bad() -> u64 {

        let mut cfg = read_ref::<Config>();
        cfg.threshold = 2;
        return cfg.threshold;
}
"#;

const READ_ONLY_REF_FIELD_ASSIGN_PROGRAM: &str = r#"
module test;

struct Point {
    x: u64,
}

action bad() -> u64 {

        let mut point = new Point { x: 1, };
        let mut view = &point;
        view.x = 2;
        return point.x;
}
"#;
