pub fn source(ty: &str) -> String {
    format!(
        r#"module scalar_definition_binding
collections Work {{ limits {{ bytes: 4096, allocations: 16 }}
    values: Vec<{ty}> {{ items: 32 }}
    flags: Vec<bool> {{ items: 4 }}
}}
fn read(work: &Work, a: {ty}, b: {ty}) -> {ty} {{
    let value = a + b
    return work.values.get(0) + value
}}
action inspect(witness a: {ty}, witness b: {ty}, witness flag: bool, witness shift: u64, witness expected: [{ty}; 13]) -> u64 {{
    let mut work = Work::new()
    let mut current = a
    let saved = current
    current = b
    work.values.push(saved)
    work.values.push(current)
    work.values.push(a + b)
    work.values.push(a - b)
    work.values.push(a * b)
    work.values.push(a / b)
    work.values.push(a % b)
    work.values.push(a & b)
    work.values.push(a | b)
    work.values.push(a ^ b)
    work.values.push(a << shift)
    work.values.push(a >> shift)
    work.values.push(-a)
    work.flags.push(!flag)
    work.flags.push(flag && !flag)
    work.flags.push(flag || !flag)
    if flag {{ current = 7 }} else {{ current = 9 }}
    work.values.push(current)
    require read(&work, a, b) == saved + (a + b)
    for index in 0..13 {{ if work.values.get(index) != expected[index] {{ return 70 + index }} }}
    require work.flags.get(0) == !flag
    require !work.flags.get(1)
    require work.flags.get(2)
    return 0
}}"#
    )
}

pub fn comparison_source(ty: &str) -> String {
    let operators: &[&str] = if ty == "bool" { &["==", "!="] } else { &["==", "!=", "<", "<=", ">", ">="] };
    let mut checks = String::new();
    for (index, operator) in operators.iter().enumerate() {
        let bit = 1u64 << index;
        checks.push_str(&format!(
            "work.flags.push(a {operator} b)\nif a {operator} b {{ actual = actual + {bit} }}\n\
             if work.flags.get({index}) != ((expected & {bit}) != 0) {{ return {} }}\n",
            70 + index
        ));
    }
    format!(
        r#"module scalar_comparison_binding
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    flags: Vec<bool> {{ items: 8 }}
}}
action inspect(witness a: {ty}, witness b: {ty}, witness expected: u64) -> u64 {{
    let mut work = Work::new()
    work.flags.reserve(8)
    let mut actual: u64 = 0
    {checks}
    if actual != expected {{ return 80 }}
    return 0
}}"#
    )
}
