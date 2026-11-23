use std::{env, fs, path::Path};

use anyhow::{bail, Context, Result};
use cellscript_zk_private_counter::{self as counter, CounterCircuit};

mod artifacts;
mod counter_fixture;
mod counter_proof;
mod r1cs;

fn main() -> Result<()> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command, work] if command == "export" => {
            let work = Path::new(work);
            fs::create_dir_all(work)?;
            r1cs::write_r1cs(&work.join("counter.r1cs"), &r1cs::synthesize(CounterCircuit::template())?)?;
            fs::write(work.join("circuit.json"), serde_json::to_vec_pretty(&counter::circuit_identity()?)?)?;
        }
        [command, work, cellc] if command == "prepare" => counter_fixture::prepare(Path::new(work), Path::new(cellc))?,
        [command, work] if command == "verify" => {
            let report = counter_fixture::verify(Path::new(work))?;
            println!("{}", serde_json::to_string_pretty(&report).context("serialize counter report")?);
        }
        _ => bail!("usage: zk-counter-experiment export WORK | prepare WORK CELLC | verify WORK"),
    }
    Ok(())
}
