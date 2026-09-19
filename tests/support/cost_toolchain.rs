//! Cost evidence must use the selected Rust toolchain and must never skip.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const RUST_CKB_TARGET: &str = "riscv64imac-unknown-none-elf";

fn rustc_output(args: &[&str]) -> Result<String, String> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(rustc).args(args).output().map_err(|error| format!("cost toolchain: rustc {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!("cost toolchain: rustc {args:?} failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8(output.stdout).map(|text| text.trim().to_owned()).map_err(|error| error.to_string())
}

pub fn resolve_strip(sysroot: &Path, host: &str, target_libdir: &Path) -> Result<PathBuf, String> {
    let has_core = fs::read_dir(target_libdir).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            entry.path().is_file() && name.starts_with("libcore-") && name.ends_with(".rlib")
        })
    });
    if !has_core {
        return Err(format!("cost toolchain: missing {RUST_CKB_TARGET} libcore; run rustup target add {RUST_CKB_TARGET}"));
    }
    let strip = sysroot.join("lib/rustlib").join(host).join("bin").join(format!("llvm-strip{}", env::consts::EXE_SUFFIX));
    let output = Command::new(&strip)
        .arg("--version")
        .output()
        .map_err(|error| format!("cost toolchain: {}: {error}; run rustup component add llvm-tools-preview", strip.display()))?;
    if !output.status.success() {
        return Err(format!("cost toolchain: {} --version failed", strip.display()));
    }
    Ok(strip)
}

pub fn require_strip() -> PathBuf {
    let sysroot = rustc_output(&["--print", "sysroot"]).expect("cost evidence requires rustc");
    let version = rustc_output(&["-vV"]).expect("cost evidence requires rustc host identity");
    let host = version.lines().find_map(|line| line.strip_prefix("host: ")).expect("rustc host field");
    let libdir =
        rustc_output(&["--print", "target-libdir", "--target", RUST_CKB_TARGET]).expect("cost evidence requires target identity");
    resolve_strip(Path::new(&sysroot), host, Path::new(&libdir))
        .unwrap_or_else(|error| panic!("{error}; cost evidence was NOT executed"))
}
