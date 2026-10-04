use anyhow::{bail, ensure, Result};
use ark_std::rand::{rngs::OsRng, RngCore};
use cellscript_zk_private_counter::{self as counter, package, wire, CounterCircuit, Witness};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
fn usage() -> anyhow::Error {
    anyhow::anyhow!("usage: counter identity | setup-local DIR | inspect DIR | new-secret FILE | state SECRET_FILE COUNTER OUTPUT | prove PACKAGE STATEMENT_BIN WITNESS_JSON PROOF_OUTPUT | check-repro FIRST_DIR SECOND_DIR OUTPUT | bundle PACKAGE SCHEDULER NODE REPRO OUTPUT | admit PACKAGE EVIDENCE --trust-local-setup")
}
fn read(path: &str, max: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    ensure!(file.metadata()?.len() <= max as u64, "input exceeds size bound");
    let mut bytes = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= max, "input exceeds size bound");
    Ok(bytes)
}
fn write_new(path: &str, bytes: &[u8], secret: bool) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if secret { 0o600 } else { 0o644 });
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["identity"] => println!("{}", serde_json::to_string_pretty(&counter::circuit_identity()?)?),
        ["setup-local", out] => {
            let (pk, _) = counter::setup(&mut OsRng)?;
            let manifest = package::write_package(Path::new(out), &pk, package::SetupKind::LocalSingleParty)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        ["inspect", dir] => {
            let (manifest, _) = package::load_package(Path::new(dir))?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        ["new-secret", out] => {
            let mut secret = [0; 32];
            OsRng.fill_bytes(&mut secret);
            write_new(out, &secret, true)?;
            println!("secret saved; {} bytes", secret.len());
        }
        ["state", secret, count, out] => {
            let secret: [u8; 32] = read(secret, 32)?.try_into().map_err(|_| anyhow::anyhow!("secret must be exactly 32 bytes"))?;
            let data = counter::state(&counter::owner(&secret), count.parse()?);
            write_new(out, &data, false)?;
        }
        ["prove", dir, statement, witness, out] => {
            let (_, pk) = package::load_package(Path::new(dir))?;
            let statement = wire::Statement::decode(&read(statement, wire::STATEMENT_BYTES)?)
                .map_err(|_| anyhow::anyhow!("invalid exact statement"))?;
            let witness: Witness = serde_json::from_slice(&read(witness, 4096)?)?;
            let proof = counter::prove(&pk, CounterCircuit { statement, witness }, &mut OsRng)?;
            write_new(out, &proof, false)?;
        }
        ["check-repro", first, second, out] => {
            let report = package::reproducibility(Path::new(first), Path::new(second))?;
            write_new(out, &serde_json::to_vec_pretty(&report)?, false)?;
        }
        ["bundle", dir, scheduler, node, repro, out] => {
            let report = package::evidence_bundle(Path::new(dir), Path::new(scheduler), Path::new(node), Path::new(repro))?;
            write_new(out, &serde_json::to_vec_pretty(&report)?, false)?;
        }
        ["admit", dir, evidence, "--trust-local-setup"] => {
            let result = package::verify_admission(Path::new(dir), Path::new(evidence), true)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        _ => bail!(usage()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
