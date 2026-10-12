//! Explicit opt-in host pilot. Fresh processes isolate Rayon and RSS high-water
//! observations. All secrets and setup randomness here are public test fixtures.
use anyhow::{ensure, Context, Result};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use cellscript_zk_private_counter::{self as counter, package, wire, CounterCircuit, Witness};
use ckb_types::{core::TransactionView, packed, prelude::*};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command, time::Instant};

fn bytes(v: &Value) -> Vec<u8> {
    hex::decode(v.as_str().unwrap()).unwrap()
}
fn write(path: impl AsRef<Path>, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn circuit(fixture: &Value) -> Result<(TransactionView, CounterCircuit)> {
    let tx = packed::Transaction::from_slice(&bytes(&fixture["transaction"]))?.into_view();
    let point = tx.inputs().get(0).context("counter input")?.previous_output();
    let input = fixture["cells"].as_array().unwrap().iter().find(|v| bytes(&v["out_point"]) == point.as_slice()).unwrap();
    let output = packed::CellOutput::from_slice(&bytes(&input["output"]))?;
    let script = output.type_().to_opt().context("counter Script")?;
    let old = bytes(&input["data"]);
    let new = tx.outputs_data().get(0).unwrap().raw_data();
    let (owner, old_counter) = counter::parse_state(&old)?;
    let (_, new_counter) = counter::parse_state(&new)?;
    let secret = [0x23; 32];
    ensure!(owner == counter::owner(&secret), "public fixture owner differs");
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
    let circuit = CounterCircuit { statement, witness: Witness { secret, old_counter, new_counter } };
    circuit.validate()?;
    Ok((tx, circuit))
}

#[test]
#[ignore = "internal worker; requires an explicit public-fixture job file"]
fn proving_cost_worker() -> Result<()> {
    let job: Value = serde_json::from_slice(&fs::read(std::env::var_os("CELLSCRIPT_ZK_COST_WORKER").context("worker job")?)?)?;
    let directory = Path::new(job["directory"].as_str().unwrap());
    let fixture: Value = serde_json::from_slice(&fs::read(directory.join(job["fixture"].as_str().unwrap()))?)?;
    let (_, circuit) = circuit(&fixture)?;
    let package_dir = directory.join("setup");
    let reuse = job["reuse_key"].as_bool().unwrap();
    let startup = Instant::now();
    let cached = if reuse { Some(package::load_package(&package_dir)?.1) } else { None };
    let startup_key_ns = startup.elapsed().as_nanos() as u64;
    let mut rows = Vec::new();
    for repetition in 0..=3 {
        let request_start = Instant::now();
        let load_start = Instant::now();
        let fresh = if reuse { None } else { Some(package::load_package(&package_dir)?.1) };
        let key = cached.as_ref().or(fresh.as_ref()).unwrap();
        let load_ns = load_start.elapsed().as_nanos() as u64;
        let proof_start = Instant::now();
        let proof = counter::prove(key, circuit.clone(), &mut StdRng::seed_from_u64(0x434f535400 + repetition))?;
        let proof_ns = proof_start.elapsed().as_nanos() as u64;
        let request_ns = request_start.elapsed().as_nanos() as u64;
        // counter::prove includes local cryptographic verification. Repeat the
        // public API check outside timing to catch serialization/key drift.
        counter::verify(&counter::serialize(&key.vk)?, &circuit.statement, &proof)?;
        let name = format!("{}-{repetition}.proof", job["id"].as_str().unwrap());
        fs::write(directory.join(&name), proof)?;
        rows.push(
            json!({"repetition":repetition,"warmup":repetition == 0,"checked_key_load_ns":if reuse {Value::Null} else {json!(load_ns)},
            "prove_and_local_verify_ns":proof_ns,"request_ns":request_ns,"proof_file":name,"proof_sha256":package::sha256(&proof)}),
        );
    }
    write(
        directory.join(format!("{}.json", job["id"].as_str().unwrap())),
        &json!({
            "job":job,"startup_checked_key_ns":if reuse {json!(startup_key_ns)} else {Value::Null},
            "rayon_threads_environment":std::env::var("RAYON_NUM_THREADS").ok(),"rows":rows,
            "observed_threads_after_proving":fs::read_to_string("/proc/self/status")?.lines().find(|s|s.starts_with("Threads:")).unwrap_or("unavailable")
        }),
    )
}

#[test]
#[ignore = "explicit host pilot; consumes migration fixtures and GNU Time, never run as an ordinary test"]
fn application_proving_cost_pilot() -> Result<()> {
    run_host_measurement(false)
}

#[test]
#[ignore = "explicit acceptance against archived pilot-derived frozen ceilings"]
fn application_proving_cost_acceptance() -> Result<()> {
    run_host_measurement(true)
}

fn run_host_measurement(acceptance: bool) -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let phase = if acceptance { "acceptance" } else { "pilot" };
    let directory = root.join(format!("target/zk-cost-{phase}"));
    ensure!(!directory.exists(), "measurement output already exists; preserve previous observations");
    let limit_bytes = include_bytes!("../../../docs/reports/0.32/zk-costs/limits.json");
    let limits: Value = serde_json::from_slice(limit_bytes)?;
    fs::create_dir_all(&directory)?;
    let fixture_bytes = fs::read(root.join("target/migration-client-fixtures.json"))?;
    let fixture: Value = serde_json::from_slice(&fixture_bytes)?;
    ensure!(fixture["status"] == "passed", "complete migration fixtures required");
    let setup_start = Instant::now();
    let pk = counter::setup(&mut StdRng::seed_from_u64(0x4d494752415445))?.0;
    let setup_ns = setup_start.elapsed().as_nanos() as u64;
    let key = counter::serialize(&pk.vk)?;
    ensure!(
        package::sha256(&key) == fixture["setup_manifests"][0]["verification_key_sha256"],
        "pilot requires public deterministic key 0"
    );
    let setup = package::write_package(&directory.join("setup"), &pk, package::SetupKind::PublicTestSeed)?;
    drop(pk);
    let maximum: Vec<Value> = serde_json::from_slice(&fs::read(root.join("target/migration-resource-fixtures.json"))?)?;
    let cases = [("ordinary", fixture["transactions"][0].clone()), ("maximum", maximum[0].clone())];
    let mut statements = Vec::new();
    for (name, case) in &cases {
        write(directory.join(format!("{name}-fixture.json")), case)?;
        let (tx, circuit) = circuit(case)?;
        statements.push(json!({"case":name,"statement":hex::encode(circuit.statement.encode()),
            "public_inputs":hex::encode(circuit.statement.public_inputs()),"transaction_hash":hex::encode(tx.hash().as_slice()),
            "transaction_bytes":tx.data().as_slice().len(),"witness_bytes":tx.witnesses().as_slice().len()}));
    }
    let executable = std::env::current_exe()?;
    let mut observations = Vec::new();
    for (name, _) in &cases {
        for (label, threads, reuse) in
            [("reference", None, false), ("four-workers", Some("4"), false), ("reuse-key", None, true), ("combined", Some("4"), true)]
        {
            let id = format!("{name}-{label}");
            let job = json!({"id":id,"case":name,"candidate":label,"directory":directory,"fixture":format!("{name}-fixture.json"),"reuse_key":reuse});
            let job_path = directory.join(format!("{id}-job.json"));
            write(&job_path, &job)?;
            let rss_path = directory.join(format!("{id}-rss-kib.txt"));
            let mut command = Command::new("timeout");
            command
                .args(["60s", "/usr/bin/time", "-f", "%M", "-o"])
                .arg(&rss_path)
                .arg(&executable)
                .args(["--exact", "proving_cost_worker", "--ignored", "--nocapture"])
                .env("CELLSCRIPT_ZK_COST_WORKER", &job_path)
                .env_remove("RAYON_NUM_THREADS")
                .env_remove("RAYON_RS_NUM_CPUS");
            if let Some(threads) = threads {
                command.env("RAYON_NUM_THREADS", threads);
            }
            let started = Instant::now();
            let result = command.output()?;
            let elapsed_ns = started.elapsed().as_nanos() as u64;
            fs::write(directory.join(format!("{id}.log")), [&result.stdout[..], &result.stderr[..]].concat())?;
            ensure!(result.status.success(), "pilot {id} failed: {:?}; inspect its retained log", result.status.code());
            let worker: Value = serde_json::from_slice(&fs::read(directory.join(format!("{id}.json")))?)?;
            let rss_kib: u64 = fs::read_to_string(rss_path)?.trim().parse()?;
            ensure!(rss_kib > 0, "GNU Time peak RSS unavailable");
            if acceptance {
                let h = &limits["host"];
                ensure!(elapsed_ns <= h["max_process_elapsed_ns"].as_u64().unwrap(), "process elapsed ceiling: {id}");
                ensure!(rss_kib <= h["max_process_peak_rss_kib"].as_u64().unwrap(), "process RSS ceiling: {id}");
                if let Some(startup) = worker["startup_checked_key_ns"].as_u64() {
                    ensure!(startup <= h["max_startup_key_ns"].as_u64().unwrap(), "startup key ceiling: {id}");
                }
                for row in worker["rows"].as_array().unwrap() {
                    ensure!(row["request_ns"].as_u64().unwrap() <= h["max_request_ns"].as_u64().unwrap(), "request ceiling: {id}");
                    ensure!(
                        row["prove_and_local_verify_ns"].as_u64().unwrap() <= h["max_prove_and_local_verify_ns"].as_u64().unwrap(),
                        "proving ceiling: {id}"
                    );
                }
            }
            observations.push(json!({"worker":worker,"process_elapsed_ns":elapsed_ns,"process_peak_rss_kib":rss_kib}));
            eprintln!("measured {id}: {:.3}s, {rss_kib} KiB", elapsed_ns as f64 / 1e9);
        }
    }
    // Equal RNG seeds and inputs must retain byte-identical proofs across the
    // four execution policies; scheduling cannot change the relation or codec.
    for (name, _) in &cases {
        for rep in 1..=3 {
            let baseline = fs::read(directory.join(format!("{name}-reference-{rep}.proof")))?;
            for label in ["four-workers", "reuse-key", "combined"] {
                ensure!(baseline == fs::read(directory.join(format!("{name}-{label}-{rep}.proof")))?, "candidate proof drift");
            }
        }
    }
    let command_output = |program: &str, args: &[&str]| -> Result<String> {
        let result = Command::new(program).args(args).output()?;
        ensure!(result.status.success(), "environment observation failed");
        Ok(String::from_utf8(result.stdout)?.trim().into())
    };
    let cpu = fs::read_to_string("/proc/cpuinfo")?;
    let memory = fs::read_to_string("/proc/meminfo")?;
    let status = fs::read_to_string("/proc/self/status")?;
    if acceptance {
        ensure!(setup_ns <= limits["host"]["max_setup_generation_ns"].as_u64().unwrap(), "setup generation ceiling");
        ensure!(
            fs::metadata(directory.join("setup/proving-key.bin"))?.len() <= limits["host"]["max_proving_key_bytes"].as_u64().unwrap(),
            "PK size ceiling"
        );
    }
    write(
        directory.join(format!("{phase}.json")),
        &json!({
            "schema":"cellscript-zk-cost-observations-v1","status":"passed","phase":phase,
            "frozen_limits_sha256":if acceptance {Some(package::sha256(limit_bytes))} else {None},
            "setup":setup,"setup_generation_ns":setup_ns,"proving_key_bytes":fs::metadata(directory.join("setup/proving-key.bin"))?.len(),
            "verification_key_bytes":key.len(),"proof_bytes":wire::PROOF_BYTES,"statement_bytes":wire::STATEMENT_BYTES,
            "public_input_bytes":wire::PUBLIC_INPUT_BYTES,"request_bytes":wire::REQUEST_BYTES,
            "source_commit":command_output("git", &["rev-parse","HEAD"])?,"source_dirty":!command_output("git", &["status","--porcelain"])?.is_empty(),
            "source_sha256":package::sha256(include_bytes!("proving_costs.rs")),
            "circuit_source_sha256":package::sha256(include_bytes!("../src/lib.rs")),
            "package_source_sha256":package::sha256(include_bytes!("../src/package.rs")),
            "prover_test_binary_sha256":package::sha256(&fs::read(executable)?),
            "fixture_sha256":package::sha256(&fixture_bytes),"warmups_per_process":1,"measured_repetitions":3,
            "cache":"warm filesystem; no dropped caches; fresh process per case/candidate; serial jobs",
            "memory_scope":"GNU Time ru_maxrss in KiB for whole worker including warmup and checked key loading; not phase-local heap",
            "phase_peak_heap_bytes":null,"phase_peak_heap_unavailable_reason":"no allocator instrumentation",
            "environment":{"kernel":command_output("uname", &["-srvmo"])?,"rust":command_output("rustc", &["--version"] )?,
                "time":command_output("/usr/bin/time", &["--version"] )?,"parallelism":std::thread::available_parallelism()?.get(),
                "cpu":cpu.lines().find(|s|s.starts_with("model name")),"memory":memory.lines().find(|s|s.starts_with("MemTotal:")),
                "affinity":status.lines().find(|s|s.starts_with("Cpus_allowed_list:")),"exclusive_host":false},
            "statements":statements,"observations":observations,"candidate_proof_bytes_equal":true,
            "production_setup_admitted":false,"ci_or_gates_run":false
        }),
    )
}
