// Ordinary Lock SDK generation owns entry-codec parity; these are not
// transaction execution, node acceptance or signing-authority claims.
#[test]
fn cellc_gen_builder_lock_only_typescript_executes_native_codec_parity() {
    use cellscript::EntryWitnessArg as A;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = repository.join("tests/fixtures/document_lock_builder.cell");
    let artifact = root.join("document.elf");
    let output = cellc_command().arg(source).args(["--target", "riscv64-elf", "-o"]).arg(&artifact).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let metadata_path = root.join("document.elf.meta.json");
    let metadata: cellscript::CompileMetadata = serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    assert!(metadata.actions.is_empty());
    let entry = metadata.locks.iter().find(|entry| entry.name == "authorize").unwrap();
    assert!(entry.params[0].lock_args_data_source);
    let native = entry
        .entry_witness_args(&[
            A::Bool(true),
            A::U8(u8::MAX),
            A::U16(u16::MAX),
            A::U32(u32::MAX),
            A::U64(u64::MAX),
            A::I32(i32::MIN),
            A::U128(u128::MAX),
            A::Hash([0xa5; 32]),
            A::Bytes(vec![0xa5; 3]),
            A::Unit,
        ])
        .unwrap();
    let sdk = root.join("sdk");
    let generated = cellc_command()
        .args(["gen-builder", "--metadata"])
        .arg(&metadata_path)
        .args(["--target", "typescript", "--output"])
        .arg(&sdk)
        .output()
        .unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let index = std::fs::read_to_string(sdk.join("src/index.ts")).unwrap();
    assert!(index.contains("encodeLockEntryWitness"));
    assert!(index.contains("readonly GeneratedActionSpec[] = []"));
    let document = cellscript::fixed_cell_builder::encode_fixed_cell_data(
        &metadata,
        "Document",
        &std::collections::BTreeMap::from([
            ("revision".to_owned(), 7u64.to_le_bytes().as_slice()),
            ("digest".to_owned(), [0xa5; 32].as_slice()),
        ]),
    )
    .unwrap();
    let oracle = serde_json::json!({"payload":hex::encode(native),"document":hex::encode(document)});
    std::fs::write(sdk.join("native.json"), serde_json::to_vec(&oracle).unwrap()).unwrap();
    std::fs::write(sdk.join("test/native-parity.test.mjs"), include_str!("lock_builder_parity.mjs")).unwrap();

    // Reuse the website's declared local tool, with no installation or network.
    // Set CELLSCRIPT_REQUIRE_TYPESCRIPT=1 to make an absent tool a hard failure.
    let tsc = repository.join("website/node_modules/typescript/bin/tsc");
    if !tsc.is_file() {
        assert!(std::env::var_os("CELLSCRIPT_REQUIRE_TYPESCRIPT").is_none(), "local TypeScript compiler is required");
        eprintln!("generated Lock SDK TypeScript/Node execution unavailable: local TypeScript dependency is absent");
        return;
    }
    let checked = Command::new("node").arg(tsc).args(["-p", "tsconfig.json"]).current_dir(&sdk).output().unwrap();
    assert!(
        checked.status.success(),
        "TypeScript: {}{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
    let tested = Command::new("node")
        .args(["--test", "test/builder.test.mjs", "test/native-parity.test.mjs"])
        .current_dir(&sdk)
        .output()
        .unwrap();
    assert!(
        tested.status.success(),
        "generated Node SDK: {}{}",
        String::from_utf8_lossy(&tested.stdout),
        String::from_utf8_lossy(&tested.stderr)
    );
    assert!(String::from_utf8_lossy(&tested.stdout).contains("# fail 0"));
    println!("ordinary Lock: strict TypeScript declarations and real generated Node/native-byte parity tests executed");
}
