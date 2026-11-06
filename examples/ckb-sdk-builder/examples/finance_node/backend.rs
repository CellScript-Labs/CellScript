use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use cellscript::{
    package::frozen_interface::{compile_module, EntrySelection, FrozenPackageModule},
    CompileMetadata, CompileOptions, EntryWitnessArg,
};
use cellscript_ckb_adapter::{build_action_transaction, CellOutputWithData, ResolvedActionTx};
use cellscript_tools::ckb_devnet::{always_success_dep, CkbDevnet};
use ckb_sdk::{
    traits::SecpCkbRawKeySigner,
    types::ScriptGroup,
    unlock::{ScriptSigner, SecpSighashScriptSigner},
    CkbRpcClient, SECP256K1,
};
use ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
use serde_json::{json, Value};
use std::{
    cell::{Cell as Counter, RefCell},
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub const CELL_CAPACITY: u64 = 100_000_000_000;
pub const FEE: u64 = 100_000_000;
pub type Cell = (packed::CellOutput, Bytes);

pub fn prefixed(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}
pub fn point(value: &Value) -> Result<packed::OutPoint> {
    let hash = hex::decode(value["tx_hash"].as_str().context("OutPoint hash missing")?.trim_start_matches("0x"))?;
    Ok(packed::OutPoint::new_builder()
        .tx_hash(packed::Byte32::from_slice(&hash)?)
        .index(value["index"].as_u64().context("OutPoint index missing")? as u32)
        .build())
}
pub fn cell(lock: packed::Script, type_script: Option<packed::Script>, data: Vec<u8>) -> Cell {
    (packed::CellOutput::new_builder().capacity(CELL_CAPACITY).lock(lock).type_(type_script.pack()).build(), Bytes::from(data))
}

pub fn copy_package(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            bail!("package snapshot must not follow symlinks");
        }
        if entry.file_name() == "Cell.lock" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            copy_package(&entry.path(), &destination.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub struct Program {
    pub name: String,
    pub module: FrozenPackageModule,
    pub metadata: CompileMetadata,
    pub action: bool,
    pub encoding_nanoseconds: Counter<u64>,
    pub encoding_calls: Counter<u64>,
}

impl Program {
    pub fn compile(root: &Path, name: &str, action: bool, cellc: &Path, chain: &str, genesis: &str, artifacts: &Path) -> Result<Self> {
        let manifest = root.join("Cell.toml");
        let mut text = fs::read_to_string(&manifest)?;
        let environment = format!("\n[environments.dev]\nchain_id = {chain:?}\ngenesis_hash = {genesis:?}\n");
        if text.contains("\n[environments.dev]") {
            if !text.ends_with(&environment) {
                bail!("snapshot has a different development environment");
            }
        } else {
            text.push_str(&environment);
        }
        fs::write(manifest, text)?;
        let locked = Command::new(cellc).current_dir(root).args(["lock", "--json"]).output()?;
        if !locked.status.success() {
            bail!("{name} lock: {}", String::from_utf8_lossy(&locked.stderr));
        }
        let lock = fs::read(root.join("Cell.lock"))?;
        let module = compile_module(
            Utf8Path::from_path(root).context("non-UTF8 package path")?,
            "dev",
            CompileOptions::default(),
            if action { EntrySelection::Action("verify".into()) } else { EntrySelection::Lock("verify".into()) },
        )?;
        if fs::read(root.join("Cell.lock"))? != lock {
            bail!("compilation mutated the package lock");
        }
        let metadata = serde_json::from_slice(module.bundle()[1])?;
        fs::create_dir_all(artifacts)?;
        for (suffix, bytes) in [
            ("elf", module.bundle()[0]),
            ("elf.meta.json", module.bundle()[1]),
            ("elf.lowering.json", module.bundle()[2]),
            ("elf.sourcemap.json", module.bundle()[3]),
        ] {
            fs::write(artifacts.join(format!("{name}.{suffix}")), bytes)?;
        }
        fs::write(artifacts.join(format!("{name}.context.json")), module.context_bytes()?)?;
        let generated = Command::new(cellc)
            .args(["gen-builder", "--metadata"])
            .arg(artifacts.join(format!("{name}.elf.meta.json")))
            .args(["--target", "typescript", "--output"])
            .arg(artifacts.join(format!("{name}-sdk")))
            .output()?;
        if !generated.status.success() {
            bail!("{name} generated builder: {}", String::from_utf8_lossy(&generated.stderr));
        }
        Ok(Self {
            name: name.into(),
            module,
            metadata,
            action,
            encoding_nanoseconds: Counter::new(0),
            encoding_calls: Counter::new(0),
        })
    }
    pub fn code(&self) -> &[u8] {
        self.module.bundle()[0]
    }
    pub fn script(&self, args: &[u8]) -> packed::Script {
        packed::Script::new_builder()
            .code_hash(packed::CellOutput::calc_data_hash(self.code()))
            .hash_type(4u8)
            .args(Bytes::copy_from_slice(args).pack())
            .build()
    }
    pub fn witness(&self, values: &[EntryWitnessArg]) -> Result<packed::WitnessArgs> {
        let started = Instant::now();
        let payload = if self.action {
            self.metadata.actions.iter().find(|entry| entry.name == "verify").context("missing action")?.entry_witness_args(values)?
        } else {
            self.metadata.locks.iter().find(|entry| entry.name == "verify").context("missing lock")?.entry_witness_args(values)?
        };
        let result = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
        self.record_encoding(started)?;
        Ok(result)
    }
    pub fn data(&self, name: &str, fields: &[(&str, Vec<u8>)]) -> Result<Vec<u8>> {
        let started = Instant::now();
        let fields = fields.iter().map(|(name, bytes)| ((*name).to_owned(), bytes.as_slice())).collect();
        let result = cellscript::fixed_cell_builder::encode_fixed_cell_data(&self.metadata, name, &fields)?;
        self.record_encoding(started)?;
        Ok(result)
    }
    fn record_encoding(&self, started: Instant) -> Result<()> {
        let elapsed = u64::try_from(started.elapsed().as_nanos()).context("encoding duration exceeds u64")?;
        self.encoding_nanoseconds.set(self.encoding_nanoseconds.get().checked_add(elapsed).context("encoding time overflow")?);
        self.encoding_calls.set(self.encoding_calls.get().checked_add(1).context("encoding count overflow")?);
        Ok(())
    }
}

pub struct Backend {
    pub node: CkbDevnet,
    pub rpc: CkbRpcClient,
    pub chain: String,
    pub genesis: String,
    pub network_domain: [u8; 32],
    pub live: HashMap<packed::OutPoint, Cell>,
    pub deps: Vec<packed::CellDep>,
    pub records: Vec<Value>,
    pub secp_code: packed::Byte32,
    pub run_dir: PathBuf,
    pub code_points: HashMap<String, packed::OutPoint>,
    pub key_labels: Vec<u8>,
    run_clock: Instant,
    pub quote_events: RefCell<Vec<super::resource_limits::QuoteEvent>>,
    pub packing_build_events: RefCell<Vec<super::resource_limits::BuildEvent>>,
    pub full_operation_samples: Vec<super::operations::Sample>,
    pub c4: bool,
}

impl Backend {
    pub fn start(repo: PathBuf, binary: PathBuf, run_dir: PathBuf, key_labels: Vec<u8>) -> Result<Self> {
        let run_clock = Instant::now();
        let mut node = CkbDevnet::new(repo, binary, run_dir.clone())?;
        node.start()?;
        let genesis = node.get_block_by_number(0)?;
        let genesis_hash = genesis["header"]["hash"].as_str().context("genesis header hash missing")?.to_owned();
        let network_domain =
            hex::decode(genesis_hash.trim_start_matches("0x"))?.try_into().map_err(|_| anyhow::anyhow!("genesis hash width"))?;
        let cellbase_hash = genesis["transactions"][0]["hash"].as_str().context("genesis cellbase hash missing")?;
        let secp_bytes = hex::decode(
            genesis["transactions"][0]["outputs_data"][1].as_str().context("genesis secp code missing")?.trim_start_matches("0x"),
        )?;
        let secp_code = packed::CellOutput::calc_data_hash(&secp_bytes);
        let mut deps = vec![];
        for index in [1u64, 3] {
            deps.push(packed::CellDep::new_builder().out_point(point(&json!({"tx_hash":cellbase_hash,"index":index}))?).build());
        }
        deps.push(serde_json::from_value::<ckb_jsonrpc_types::CellDep>(always_success_dep(cellbase_hash))?.into());
        let consensus = node.rpc("get_consensus", vec![])?;
        let chain = consensus["id"].as_str().context("node chain identity missing")?.to_owned();
        if consensus["genesis_hash"] != genesis_hash {
            bail!("consensus genesis differs from actual block.header.hash");
        }
        let rpc = CkbRpcClient::new(&node.rpc_url);
        Ok(Self {
            node,
            rpc,
            chain,
            genesis: genesis_hash,
            network_domain,
            live: HashMap::new(),
            deps,
            records: vec![],
            secp_code,
            run_dir,
            code_points: HashMap::new(),
            key_labels,
            run_clock,
            quote_events: RefCell::new(vec![]),
            packing_build_events: RefCell::new(vec![]),
            full_operation_samples: vec![],
            c4: false,
        })
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.run_clock.elapsed().as_secs_f64() * 1000.0
    }

    pub fn measure_quote<T, E>(
        &self,
        operation: &str,
        quote: impl FnOnce() -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let started = Instant::now();
        let result = quote();
        let elapsed_ns = u64::try_from(started.elapsed().as_nanos()).expect("bounded pure quote completes within u64 nanoseconds");
        self.quote_events.borrow_mut().push(super::resource_limits::QuoteEvent {
            operation: operation.into(),
            elapsed_ns,
            success: result.is_ok(),
        });
        result
    }

    pub fn contention_window(&self, start_elapsed_ms: f64, pattern: &str) -> Value {
        let end_elapsed_ms = self.elapsed_ms();
        json!({"pattern":pattern,"start_elapsed_ms":start_elapsed_ms,"end_elapsed_ms":end_elapsed_ms,"duration_ms":end_elapsed_ms-start_elapsed_ms,
            "offered_candidates":2,"actual_submission_attempts":2,"committed":1,"submission_conflicts":1,"new_economic_retries":0,"terminal":false,"serialized_contenders":true,
            "scope":"one-local-serialized-two-candidate-pattern; stale-original-send-after-commit; excludes-financial-invalid-probes; no-load-percentile"})
    }

    pub fn require_stale_input(error: &anyhow::Error) -> Result<Value> {
        let failure = error
            .downcast_ref::<cellscript_tools::ckb_devnet::RpcFailure>()
            .context("unavailable RPC is not a stale-input conflict")?;
        Self::require_stale_rpc_error(&failure.error)?;
        Ok(failure.error.clone())
    }

    fn require_stale_rpc_error(error: &Value) -> Result<()> {
        let data = error["data"].as_str().unwrap_or("");
        if error["code"].as_i64() != Some(-301)
            || !(data.contains("Resolve(Unknown(OutPoint(") || data.contains("Resolve(Dead(OutPoint("))
        {
            bail!("competing candidate failed for a different RPC reason: {error}");
        }
        Ok(())
    }

    /// Deliberately bypass the optional preflight only in this private-node
    /// experiment, reusing exactly the already-signed original loser after
    /// consensus consumed its predecessors. This is not an economic retry.
    pub fn submit_stale_original(&self, transaction: &TransactionView) -> Result<Value> {
        let started = Instant::now();
        let error = match cellscript_ckb_adapter::CkbSdkAcceptance::new(&self.rpc).send_transaction(transaction) {
            Ok(hash) => bail!("node unexpectedly admitted stale signed original {hash:?}"),
            Err(ckb_sdk::RpcError::Rpc(error)) => serde_json::to_value(error)?,
            Err(error) => bail!("stale submission unavailable or failed at a different stage: {error}"),
        };
        Self::require_stale_rpc_error(&error)?;
        Ok(
            json!({"method":"send_transaction","status":"stale-input-rpc-rejected","rpc_error":error,"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,
            "raw_transaction_hash":prefixed(transaction.hash().as_slice()),"signed_serialized_transaction_hash":prefixed(&ckb_hash::blake2b_256(transaction.data().as_slice())),
            "same_previously_signed_original":true,"rebuilt":false,"resigned":false,"new_economic_retry":false,
            "scope":"private-node deliberate preflight bypass after consensus predecessor consumption"}),
        )
    }

    pub fn owner(&self, key: u8) -> packed::Script {
        let secret = secp256k1::SecretKey::from_slice(&[key; 32]).expect("disposable local key");
        let public = secp256k1::PublicKey::from_secret_key(&SECP256K1, &secret);
        let hash = ckb_hash::blake2b_256(public.serialize());
        packed::Script::new_builder()
            .code_hash(self.secp_code.clone())
            .hash_type(4u8)
            .args(Bytes::copy_from_slice(&hash[..20]).pack())
            .build()
    }

    pub fn refresh(&mut self, point: &packed::OutPoint) -> Result<Cell> {
        let response = self.node.rpc(
            "get_live_cell",
            vec![
                json!({"tx_hash":prefixed(point.tx_hash().as_slice()),"index":format!("0x{:x}",u32::from(point.index()))}),
                json!(true),
            ],
        )?;
        if response["status"] != "live" {
            bail!("input is not live: {response}");
        }
        let output: packed::CellOutput =
            serde_json::from_value::<ckb_jsonrpc_types::CellOutput>(response["cell"]["output"].clone())?.into();
        let data = Bytes::from(hex::decode(
            response["cell"]["data"]["content"].as_str().context("live cell data missing")?.trim_start_matches("0x"),
        )?);
        self.live.insert(point.clone(), (output.clone(), data.clone()));
        Ok((output, data))
    }

    pub fn build(
        &self,
        inputs: &[packed::OutPoint],
        mut outputs: Vec<Cell>,
        change: usize,
        mut witnesses: Vec<packed::WitnessArgs>,
    ) -> Result<TransactionView> {
        let started = Instant::now();
        let total = inputs.iter().try_fold(0u64, |sum, point| {
            sum.checked_add(u64::from(self.live.get(point).context("unresolved input")?.0.capacity()))
                .context("input capacity overflow")
        })?;
        let fixed = outputs
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != change)
            .try_fold(0u64, |sum, (_, cell)| sum.checked_add(u64::from(cell.0.capacity())).context("output capacity overflow"))?;
        outputs[change].0 = outputs[change]
            .0
            .clone()
            .as_builder()
            .capacity(total.checked_sub(fixed + FEE).context("insufficient funding capacity")?)
            .build();
        witnesses.resize(witnesses.len().max(inputs.len()).max(outputs.len()), packed::WitnessArgs::default());
        let resolved = ResolvedActionTx {
            metadata_hash: "ordinary-finance-package-metadata-encoders".into(),
            artifact_hash: None,
            action_selector: "application-composed-transaction".into(),
            inputs: inputs.iter().map(|point| packed::CellInput::new_builder().previous_output(point.clone()).build()).collect(),
            outputs: outputs.into_iter().map(|(output, data)| CellOutputWithData { output, data }).collect(),
            witnesses,
            cell_deps: self.deps.clone(),
            header_deps: vec![],
            lineage: vec![],
            fee_shannons: FEE,
        };
        let transaction = build_action_transaction(&resolved)?.0;
        let elapsed_ns = u64::try_from(started.elapsed().as_nanos()).context("packing duration exceeds u64 nanoseconds")?;
        self.packing_build_events
            .borrow_mut()
            .push(super::resource_limits::BuildEvent { raw_transaction_hash: prefixed(transaction.hash().as_slice()), elapsed_ns });
        Ok(transaction)
    }

    pub fn sign(&self, transaction: &TransactionView) -> Result<TransactionView> {
        let mut signed = transaction.clone();
        for key in &self.key_labels {
            let owner = self.owner(*key);
            let input_indices = transaction
                .inputs()
                .into_iter()
                .enumerate()
                .filter_map(|(index, input)| (self.live[&input.previous_output()].0.lock() == owner).then_some(index))
                .collect::<Vec<_>>();
            if input_indices.is_empty() {
                continue;
            }
            let signer = SecpSighashScriptSigner::new(Box::new(SecpCkbRawKeySigner::new_with_secret_keys(vec![
                secp256k1::SecretKey::from_slice(&[*key; 32])?,
            ])));
            let mut group = ScriptGroup::from_lock_script(&owner);
            group.input_indices = input_indices;
            signed = signer.sign_tx(&signed, &group)?;
        }
        Ok(signed)
    }

    pub fn mine_until(&self, hash: &str) -> Result<Value> {
        for _ in 0..80 {
            let observed = self.node.rpc("get_transaction", vec![json!(hash)])?;
            if observed["tx_status"]["status"] == "committed" {
                return Ok(observed["tx_status"].clone());
            }
            if observed["tx_status"]["status"] == "rejected" {
                bail!("node rejected original transaction: {observed}");
            }
            self.node.rpc("generate_block", vec![])?;
            std::thread::sleep(Duration::from_millis(50));
        }
        bail!("original transaction did not commit within the bounded mining window");
    }

    pub fn commit(&mut self, tx: &TransactionView, name: &str) -> Result<Vec<packed::OutPoint>> {
        self.node.wait_tx_pool_tip(name)?;
        let acceptance = cellscript_ckb_adapter::CkbSdkAcceptance::new(&self.rpc);
        let started = Instant::now();
        let estimate = acceptance.estimate_cycles(tx)?;
        let estimate_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let pool = acceptance.test_tx_pool_accept(tx)?;
        let preflight_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let hash = prefixed(acceptance.send_transaction(tx)?.as_bytes());
        if hash != prefixed(tx.hash().as_slice()) {
            bail!("node returned a different transaction identity");
        }
        let send_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.complete_submitted(
            tx,
            name,
            json!({"estimate_cycles":estimate,"tx_pool":pool,
            "timings_ms":{"estimate":estimate_ms,"preflight":preflight_ms,"send":send_ms}}),
        )
    }

    /// Execute an invalid signed candidate on the actual node without sending
    /// it. Match the executing verifier and stable predicate/role exit code, rather
    /// than counting stale inputs or invalid signatures as policy evidence.
    pub fn reject(&mut self, transaction: &TransactionView, program: &Program, name: &str, error_code: i64) -> Result<()> {
        self.reject_one_of(transaction, &[program], name, error_code)
    }

    pub fn reject_one_of(&mut self, transaction: &TransactionView, programs: &[&Program], name: &str, error_code: i64) -> Result<()> {
        self.node.wait_tx_pool_tip(name)?;
        let signed = self.sign(transaction)?;
        let actual = serde_json::to_value(cellscript_ckb_adapter::to_rpc_transaction(&signed))?;
        let code_hashes = programs.iter().map(|program| prefixed(&ckb_hash::blake2b_256(program.code()))).collect::<Vec<_>>();
        let started = Instant::now();
        let observation = self.node.dry_run_rejects(&actual, name, None, None, Some(error_code))?;
        let reason = observation["reason"].as_str().context("node rejection did not preserve its reason")?.to_ascii_lowercase();
        if !code_hashes.iter().any(|hash| reason.contains(hash.trim_start_matches("0x"))) {
            bail!("node rejected a different verifier: {observation}");
        }
        let inputs=signed.inputs().into_iter().map(|input|{let (output,data)=&self.live[&input.previous_output()];json!({"out_point":prefixed(input.previous_output().as_slice()),"output":prefixed(output.as_slice()),"data":prefixed(data)})}).collect::<Vec<_>>();
        self.records.push(json!({"case":name,"status":"node-dry-run-rejected","rejection_stage":"actual-node-script-execution","observation":observation,
            "expected_verifiers":programs.iter().map(|p|p.name.as_str()).collect::<Vec<_>>(),"expected_verifier_code_hashes":code_hashes,"transaction":prefixed(signed.data().as_slice()),"inputs":inputs,
            "raw_transaction_hash":prefixed(signed.hash().as_slice()),"serialized_transaction_hash":prefixed(&ckb_hash::blake2b_256(signed.data().as_slice())),
            "dry_run_ms":started.elapsed().as_secs_f64()*1000.0,"submitted":false,"transaction_cycles":null,"cycles_unavailable_reason":"rejected RPC does not return a measured cycle total"}));
        Ok(())
    }

    /// Finish observation of an already submitted original; never resubmit it.
    pub fn complete_submitted(&mut self, tx: &TransactionView, name: &str, observations: Value) -> Result<Vec<packed::OutPoint>> {
        let hash = prefixed(tx.hash().as_slice());
        let started = Instant::now();
        let status = self.mine_until(&hash)?;
        self.node.wait_tx_pool_tip(name)?;
        let confirm_ms = started.elapsed().as_secs_f64() * 1000.0;
        let original = serde_json::to_value(cellscript_ckb_adapter::to_rpc_transaction(tx))?;
        let observed = self.node.rpc("get_transaction", vec![json!(hash)])?;
        for field in ["version", "cell_deps", "header_deps", "inputs", "outputs", "outputs_data", "witnesses"] {
            if observed["transaction"][field] != original[field] {
                bail!("node committed different original transaction field {field}");
            }
        }
        let inputs = tx.inputs().into_iter().map(|input| {
            let (output,data) = &self.live[&input.previous_output()];
            json!({"out_point":prefixed(input.previous_output().as_slice()),"output":prefixed(output.as_slice()),"data":prefixed(data)})
        }).collect::<Vec<_>>();
        let points = tx
            .outputs()
            .into_iter()
            .zip(tx.outputs_data())
            .enumerate()
            .map(|(index, (output, data))| {
                let point = packed::OutPoint::new(tx.hash(), index as u32);
                self.live.insert(point.clone(), (output, data.raw_data()));
                point
            })
            .collect::<Vec<_>>();
        for input in tx.inputs() {
            self.live.remove(&input.previous_output());
        }
        for point in &points {
            self.node.wait_live_cell(&hash, u32::from(point.index()) as u64)?;
        }
        self.records.push(
            json!({"case":name,"status":"node-committed","transaction":prefixed(tx.data().as_slice()),"raw_transaction_hash":hash,
            "serialized_transaction_hash":prefixed(&ckb_hash::blake2b_256(tx.data().as_slice())),"inputs":inputs,
            "node_admission":observations,"commitment":status,"transaction_bytes":tx.data().as_slice().len(),
            "witness_bytes":tx.witnesses().into_iter().map(|witness|witness.raw_data().len()).sum::<usize>(),
            "commit_observation_ms":confirm_ms,
            "observed_vm_stack":null,"stack_unavailable_reason":"node RPC does not expose observed per-VM stack"}),
        );
        Ok(points)
    }
}
