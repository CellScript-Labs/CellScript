//! Shared source fixture for coordinate calls after guarded collection writes.
pub fn source(padding: usize, bytes: usize) -> String {
    let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
    let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
    let arguments = "0, ".repeat(padding);
    format!(
        r#"module collection_coordinate_helpers
resource Token has store {{ amount: u64 }}
collections Work {{
    limits {{ bytes: {bytes}, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
fn read({params}work: &Work, before: InputView<Token>) -> u64 {{
    return work.values.get(0) + before.capacity
}}
fn forward({params}work: &Work, before: InputView<Token>) -> u64 {{
    borrow work as alias {{ return read({forwarded}alias, before) }}
}}
action inspect(witness value: u64) -> u64 {{
    verification
    let before = ckb::input<Token>(0)
    let mut work = Work::new()
    work.values.push(value)
    let copied = forward({arguments}&work, before)
    require copied == value + before.capacity
    work.drop()
    return 0
}}"#
    )
}
