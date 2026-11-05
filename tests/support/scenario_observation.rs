//! Actual maintainer observations; tracked frozen fixtures stay read-only.

pub fn write(name: &str, value: &serde_json::Value) {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fixture-observations");
    std::fs::create_dir_all(&directory).expect("create scenario observation directory");
    std::fs::write(directory.join(name), serde_json::to_vec_pretty(value).expect("serialize actual scenario observation"))
        .expect("write actual scenario observation");
}
