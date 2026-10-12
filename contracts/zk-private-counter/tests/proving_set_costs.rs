//! Explicit complete-set host measurements: every leaf is proved and verified.
use anyhow::{ensure, Context, Result};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use cellscript_zk_private_counter::{self as counter, package, wire, CounterCircuit, Witness};
use ckb_types::{packed, prelude::*};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command, time::Instant};
fn bytes(v: &Value) -> Vec<u8> {
    hex::decode(v.as_str().unwrap()).unwrap()
}
fn write(path: impl AsRef<Path>, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn circuits(fixture: &Value) -> Result<Vec<CounterCircuit>> {
    let count = fixture["count"].as_u64().context("protected count")? as usize;
    ensure!([1, 2, 4].contains(&count), "unsupported measured set");
    let tx = packed::Transaction::from_slice(&bytes(&fixture["transaction"]))?.into_view();
    (0..count)
        .map(|i| {
            let point = tx.inputs().get(i).context("protected input")?.previous_output();
            let input = fixture["cells"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| bytes(&c["out_point"]) == point.as_slice())
                .context("snapshot")?;
            let output = packed::CellOutput::from_slice(&bytes(&input["output"]))?;
            let script = output.type_().to_opt().context("protected Type")?;
            ensure!(tx.outputs().get(i).unwrap().type_().to_opt().as_ref() == Some(&script), "successor correspondence");
            let old = bytes(&input["data"]);
            let new = tx.outputs_data().get(i).unwrap().raw_data();
            let (owner, old_counter) = counter::parse_state(&old)?;
            let (_, new_counter) = counter::parse_state(&new)?;
            let secret = [0x21 + i as u8; 32];
            ensure!(owner == counter::owner(&secret), "public fixture owner changed");
            let statement = wire::Statement {
                domain: counter::domain(),
                action: counter::action(),
                script_hash: script.calc_script_hash().as_slice().try_into()?,
                old_data_hash: counter::hash(&old),
                new_data_hash: counter::hash(&new),
                input_transaction_hash: point.tx_hash().as_slice().try_into()?,
                input_output_index: point.index().unpack(),
                transaction_hash: tx.hash().as_slice().try_into()?,
            };
            ensure!(hex::encode(statement.public_inputs()) == fixture["public_inputs"][i], "actual public input drift");
            let circuit = CounterCircuit { statement, witness: Witness { secret, old_counter, new_counter } };
            circuit.validate()?;
            Ok(circuit)
        })
        .collect()
}

#[test]
#[ignore = "internal complete-set measurement worker"]
fn set_cost_worker() -> Result<()> {
    let job: Value = serde_json::from_slice(&fs::read(std::env::var_os("CELLSCRIPT_SET_COST_JOB").context("job")?)?)?;
    let directory = Path::new(job["directory"].as_str().unwrap());
    let fixture: Value = serde_json::from_slice(&fs::read(directory.join(format!("set-{}.json", job["count"])))?)?;
    let circuits = circuits(&fixture)?;
    let reuse = job["shared_request_key"].as_bool().unwrap();
    let mut rows = Vec::new();
    for repetition in 0..=3 {
        let started = Instant::now();
        let loading = Instant::now();
        let shared = if reuse { Some(package::load_package(&directory.join("setup"))?.1) } else { None };
        let shared_load_ns = loading.elapsed().as_nanos() as u64;
        let mut leaves = Vec::new();
        let mut proofs = Vec::new();
        for (i, circuit) in circuits.iter().enumerate() {
            let loading = Instant::now();
            let fresh = if reuse { None } else { Some(package::load_package(&directory.join("setup"))?.1) };
            let key = shared.as_ref().or(fresh.as_ref()).unwrap();
            let load_ns = loading.elapsed().as_nanos() as u64;
            let proving = Instant::now();
            let proof =
                counter::prove(key, circuit.clone(), &mut StdRng::seed_from_u64(0x534554434f5354 + repetition * 4 + i as u64))?;
            let prove_ns = proving.elapsed().as_nanos() as u64;
            leaves.push(json!({"leaf":i,"checked_key_load_ns":if reuse {Value::Null}else{json!(load_ns)},"prove_and_verify_ns":prove_ns,"proof_sha256":package::sha256(&proof)}));
            proofs.extend(proof);
        }
        // Timing includes all request key loads, proof/local verification and
        // temporary per-leaf key disposal. No retained cross-request cache.
        let request_ns = started.elapsed().as_nanos() as u64;
        fs::write(directory.join(format!("{}-{repetition}.proofs", job["id"].as_str().unwrap())), &proofs)?;
        rows.push(json!({"repetition":repetition,"warmup":repetition==0,"request_ns":request_ns,"shared_checked_key_load_ns":if reuse{Some(shared_load_ns)}else{None},"proof_bytes":proofs.len(),"leaves":leaves}));
    }
    write(
        directory.join(format!("{}.json", job["id"].as_str().unwrap())),
        &json!({"job":job,"rows":rows,
        "threads_after_proving":fs::read_to_string("/proc/self/status")?.lines().find(|s|s.starts_with("Threads:"))}),
    )
}

#[test]
#[ignore = "explicit full protected-set host pilot; fresh workers and GNU Time"]
fn set_cost_pilot() -> Result<()> {
    measure(false)
}
#[test]
#[ignore = "explicit full protected-set acceptance under separately frozen limits"]
fn set_cost_acceptance() -> Result<()> {
    measure(true)
}

fn measure(acceptance: bool) -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()?;
    let phase = if acceptance { "acceptance" } else { "pilot" };
    let directory = root.join(format!("target/counter-set-cost-{phase}"));
    ensure!(!directory.exists(), "preserve prior measurements; output exists");
    let limit_bytes = if acceptance { Some(fs::read(root.join("docs/reports/0.32/zk-costs/set-limits.json"))?) } else { None };
    let limits: Option<Value> = limit_bytes.as_ref().map(|b| serde_json::from_slice(b)).transpose()?;
    fs::create_dir_all(&directory)?;
    let fixture_bytes = fs::read(root.join("target/counter-set/fixtures.json"))?;
    let fixtures: Vec<Value> = serde_json::from_slice(&fixture_bytes)?;
    ensure!(fixtures.len() == 3, "missing set cases");
    let started = Instant::now();
    let pk = counter::setup(&mut StdRng::seed_from_u64(0x53455401))?.0;
    let setup_ns = started.elapsed().as_nanos() as u64;
    let vk = counter::serialize(&pk.vk)?;
    ensure!(vk == fs::read(root.join("target/counter-set/verification-key.bin"))?, "set key mismatch");
    let setup = package::write_package(&directory.join("setup"), &pk, package::SetupKind::PublicTestSeed)?;
    drop(pk);
    let executable = std::env::current_exe()?;
    let mut observations = Vec::new();
    for fixture in fixtures {
        let count = circuits(&fixture)?.len();
        write(directory.join(format!("set-{count}.json")), &fixture)?;
        for (candidate, reuse) in [("per-leaf-key", false), ("shared-request-key", true)] {
            let id = format!("set-{count}-{candidate}");
            let job_path = directory.join(format!("{id}-job.json"));
            write(&job_path, &json!({"id":id,"count":count,"directory":directory,"candidate":candidate,"shared_request_key":reuse}))?;
            let rss_path = directory.join(format!("{id}-rss-kib.txt"));
            let started = Instant::now();
            let result = Command::new("timeout")
                .args(["180s", "/usr/bin/time", "-f", "%M", "-o"])
                .arg(&rss_path)
                .arg(&executable)
                .args(["--exact", "set_cost_worker", "--ignored", "--nocapture"])
                .env("CELLSCRIPT_SET_COST_JOB", &job_path)
                .env_remove("RAYON_NUM_THREADS")
                .env_remove("RAYON_RS_NUM_CPUS")
                .output()?;
            let elapsed_ns = started.elapsed().as_nanos() as u64;
            fs::write(directory.join(format!("{id}.log")), [&result.stdout[..], &result.stderr[..]].concat())?;
            ensure!(result.status.success(), "worker {id} failed; see retained log");
            let worker: Value = serde_json::from_slice(&fs::read(directory.join(format!("{id}.json")))?)?;
            let rss_kib: u64 = fs::read_to_string(&rss_path)?.trim().parse()?;
            ensure!(rss_kib > 0, "RSS unavailable");
            if let Some(limits) = &limits {
                ensure!(rss_kib <= limits["max_worker_rss_kib"].as_u64().unwrap(), "RSS ceiling {id}");
                ensure!(elapsed_ns <= limits["max_worker_elapsed_ns"].as_u64().unwrap(), "worker duration ceiling {id}");
                for row in worker["rows"].as_array().unwrap() {
                    ensure!(
                        row["request_ns"].as_u64().unwrap() <= limits["max_complete_set_request_ns"].as_u64().unwrap(),
                        "complete-set request ceiling {id}"
                    );
                    ensure!(row["leaves"].as_array().unwrap().len() == count, "partial set measured");
                    for leaf in row["leaves"].as_array().unwrap() {
                        ensure!(
                            leaf["prove_and_verify_ns"].as_u64().unwrap() <= limits["max_leaf_prove_and_verify_ns"].as_u64().unwrap(),
                            "leaf proving ceiling {id}"
                        );
                    }
                }
            }
            eprintln!("measured {id}: {:.2}s {rss_kib} KiB", elapsed_ns as f64 / 1e9);
            observations.push(json!({"worker":worker,"process_elapsed_ns":elapsed_ns,"process_peak_rss_kib":rss_kib}));
        }
        for repetition in 0..=3 {
            ensure!(
                fs::read(directory.join(format!("set-{count}-per-leaf-key-{repetition}.proofs")))?
                    == fs::read(directory.join(format!("set-{count}-shared-request-key-{repetition}.proofs")))?,
                "proof-byte drift"
            );
        }
    }
    let cpu = fs::read_to_string("/proc/cpuinfo")?;
    let memory = fs::read_to_string("/proc/meminfo")?;
    write(
        directory.join(format!("{phase}.json")),
        &json!({"schema":"cellscript-zk-set-costs-v1","status":"passed","phase":phase,
        "frozen_limits_sha256":limit_bytes.as_ref().map(|b|package::sha256(b)),"fixture_sha256":package::sha256(&fixture_bytes),
        "source_sha256":package::sha256(include_bytes!("proving_set_costs.rs")),"binary_sha256":package::sha256(&fs::read(executable)?),
        "circuit_source_sha256":package::sha256(include_bytes!("../src/lib.rs")),"setup":setup,"setup_generation_ns":setup_ns,
        "proving_key_bytes":fs::metadata(directory.join("setup/proving-key.bin"))?.len(),"verification_key_bytes":vk.len(),
        "warmups":1,"measured_repetitions":3,"candidate_proof_bytes_equal":true,"observations":observations,
        "environment":{"cpu":cpu.lines().find(|s|s.starts_with("model name")),"memory":memory.lines().find(|s|s.starts_with("MemTotal:")),"parallelism":std::thread::available_parallelism()?.get(),"exclusive_host":false},
        "memory_scope":"GNU Time whole-process ru_maxrss KiB, including warmup and key loading; no per-phase heap measurement",
        "cache":"fresh processes, warm filesystem, sequential requests and leaves; automatic Rayon; no retained cross-request key",
        "setup_peak_rss_kib":null,"production_setup_admitted":false,"ci_or_gates_run":false}),
    )
}
