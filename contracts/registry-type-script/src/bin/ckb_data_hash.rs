use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let mut args = env::args_os();
    let _program = args.next();
    let Some(path) = args.next() else {
        eprintln!("usage: cellscript-registry-type-script-hash <artifact>");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("expected exactly one artifact path");
        return ExitCode::from(2);
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("failed to read {}: {error}", path.to_string_lossy());
            return ExitCode::FAILURE;
        }
    };
    for byte in ckb_hash::blake2b_256(bytes) {
        print!("{byte:02x}");
    }
    println!();
    ExitCode::SUCCESS
}
