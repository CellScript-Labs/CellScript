//! Shared source for independently checked caller-relative collection reads.
pub fn source(padding: usize, bytes: usize) -> String {
    let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
    let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
    let arguments = "0, ".repeat(padding);
    format!(
        r#"module borrowed_collection_reads
struct Pair {{ left: u64, right: u64 }}
collections Work {{ limits {{ bytes: {bytes}, allocations: 8 }}
    values: Vec<Pair> {{ items: 4 }}
    totals: OrderedMap<u64, u64> {{ items: 4 }}
    seen: OrderedSet<[u8; 3]> {{ items: 4 }}
}}
fn read({params}view: &Work, other: &Work, index: u64, key: u64) -> u64 {{
    if view.values.len() != 1 {{ return 0 }}
    if view.values.capacity() < 1 {{ return 0 }}
    if view.values.is_empty() {{ return 0 }}
    if other.values.len() != 1 {{ return 0 }}
    let pair = view.values.get(index)
    if pair.right != 9 {{ return 0 }}
    if !view.totals.contains_key(4) {{ return 0 }}
    for (stored_key, stored_value) in view.totals {{
        if stored_key != 4 {{ return 0 }}
        if stored_value != 11 {{ return 0 }}
    }}
    if !view.seen.contains(b"key") {{ return 0 }}
    let found = view.totals.get(key)
    let fallback = view.totals.get_or(99, 13)
    return pair.left + found + fallback + other.values.len()
}}
fn forward({params}view: &Work, other: &Work, index: u64, key: u64) -> u64 {{
    borrow view as alias {{ borrow alias as nested {{
        return read({forwarded}nested, other, index, key)
    }} }}
}}
action inspect(witness index: u64, witness key: u64) -> u64 {{
    let mut work = Work::new()
    work.values.push(Pair {{ left: 7, right: 9 }})
    work.totals.insert(4, 11)
    work.seen.insert(b"key")
    let mut other = Work::new()
    other.values.push(Pair {{ left: 3, right: 5 }})
    require forward({arguments}&work, &other, index, key) == 32
    other.drop()
    work.drop()
    return 0
}}"#
    )
}
