use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use camino::Utf8PathBuf;
use cellscript::{compile_file_with_entry_action, ArtifactFormat, CompileOptions};

const RUST_CKB_TARGET: &str = "riscv64imac-unknown-none-elf";
const RUST_REFERENCE_PACKAGE: &str = "rust-ckb-token-transfer";
const TOKEN_TRANSFER_MAX_CELLSCRIPT_BYTES: usize = 7 * 1024;
const TOKEN_TRANSFER_MAX_RUST_STRIPPED_BYTES: usize = 3 * 1024;

#[test]
fn token_transfer_cellscript_artifact_is_compared_against_equivalent_rust_ckb_contract() {
    if !rust_riscv_target_is_installed() {
        eprintln!("skipping Rust CKB size comparison because {RUST_CKB_TARGET} is not installed");
        return;
    }
    if !command_is_available("llvm-strip") {
        eprintln!("skipping Rust CKB size comparison because llvm-strip is not available");
        return;
    }

    let repo = repo_root();
    let cellscript_source = Utf8PathBuf::from_path_buf(repo.join("examples/token/src/main.cell")).expect("repo path should be UTF-8");
    let cellscript = compile_file_with_entry_action(
        &cellscript_source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
        "transfer_token",
    )
    .expect("CellScript token transfer entry should compile");
    assert_eq!(cellscript.artifact_format, ArtifactFormat::RiscvElf);
    let cellscript_bytes = cellscript.artifact_bytes.len();
    let cellscript_load_bytes = elf_load_file_bytes(&cellscript.artifact_bytes);

    let temp = tempfile::tempdir().expect("tempdir should be available");
    let rust_unstripped = build_rust_reference(&repo, temp.path());
    let rust_stripped = temp.path().join("rust-ckb-token-transfer.stripped");
    fs::copy(&rust_unstripped, &rust_stripped).expect("copy Rust reference artifact for stripping");
    let strip_status = Command::new("llvm-strip").arg(&rust_stripped).status().expect("run llvm-strip");
    assert!(strip_status.success(), "llvm-strip should succeed for {}", rust_stripped.display());
    let rust_stripped_bytes = fs::metadata(&rust_stripped).expect("stripped Rust artifact metadata").len() as usize;
    let rust_load_bytes = elf_load_file_bytes(&fs::read(&rust_stripped).expect("read stripped Rust artifact"));

    eprintln!(
        "token transfer size comparison: cellscript={} bytes load={} bytes; rust_unstripped={} bytes; rust_stripped={} bytes load={} bytes",
        cellscript_bytes,
        cellscript_load_bytes,
        fs::metadata(&rust_unstripped).expect("Rust artifact metadata").len(),
        rust_stripped_bytes,
        rust_load_bytes
    );

    assert!(
        cellscript_bytes <= TOKEN_TRANSFER_MAX_CELLSCRIPT_BYTES,
        "CellScript transfer_token ELF grew past budget: {} > {} bytes",
        cellscript_bytes,
        TOKEN_TRANSFER_MAX_CELLSCRIPT_BYTES
    );
    assert!(
        rust_stripped_bytes <= TOKEN_TRANSFER_MAX_RUST_STRIPPED_BYTES,
        "Rust CKB transfer reference ELF grew past budget: {} > {} bytes",
        rust_stripped_bytes,
        TOKEN_TRANSFER_MAX_RUST_STRIPPED_BYTES
    );
    assert!(
        rust_stripped_bytes < cellscript_bytes,
        "the hand-written Rust lower-bound should remain smaller than CellScript's metadata-backed entry artifact"
    );
    assert!(
        cellscript_bytes * 100 <= rust_stripped_bytes * 300,
        "CellScript transfer_token should stay within 3.0x the stripped same-function Rust CKB reference: {} vs {} bytes",
        cellscript_bytes,
        rust_stripped_bytes
    );
    assert!(
        cellscript_load_bytes * 100 <= rust_load_bytes * 350,
        "CellScript transfer_token executable LOAD bytes should stay within 3.5x the same-function Rust CKB reference: {} vs {} bytes",
        cellscript_load_bytes,
        rust_load_bytes
    );
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_riscv_target_is_installed() -> bool {
    let Ok(output) = Command::new("rustup").args(["target", "list", "--installed"]).output() else {
        return true;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).lines().any(|line| line.trim() == RUST_CKB_TARGET)
}

fn command_is_available(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn build_rust_reference(repo: &Path, temp_root: &Path) -> PathBuf {
    let manifest = repo.join("tests/fixtures/rust_ckb_token_transfer/Cargo.toml");
    let target_dir = temp_root.join("rust-target");
    let cargo = env::var_os("CARGO").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cargo"));
    let output = Command::new(cargo)
        .args([
            "build",
            "--locked",
            "--manifest-path",
            manifest.to_str().expect("fixture manifest path should be UTF-8"),
            "--release",
            "--target",
            RUST_CKB_TARGET,
        ])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("run cargo build for Rust CKB reference");
    assert!(
        output.status.success(),
        "Rust CKB reference build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    target_dir.join(RUST_CKB_TARGET).join("release").join(RUST_REFERENCE_PACKAGE)
}

fn elf_load_file_bytes(bytes: &[u8]) -> usize {
    assert!(bytes.len() >= 64, "ELF file is too small");
    assert_eq!(&bytes[0..4], b"\x7fELF", "artifact should be ELF");
    assert_eq!(bytes[4], 2, "artifact should be ELF64");
    assert_eq!(bytes[5], 1, "artifact should be little-endian ELF");

    let phoff = le_u64(bytes, 32) as usize;
    let phentsize = le_u16(bytes, 54) as usize;
    let phnum = le_u16(bytes, 56) as usize;
    let mut load_bytes = 0usize;
    for index in 0..phnum {
        let offset = phoff + index * phentsize;
        assert!(offset + phentsize <= bytes.len(), "ELF program header should fit in file");
        let p_type = le_u32(bytes, offset);
        if p_type == 1 {
            load_bytes += le_u64(bytes, offset + 32) as usize;
        }
    }
    load_bytes
}

fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("u16 bytes"))
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 bytes"))
}

fn le_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("u64 bytes"))
}
