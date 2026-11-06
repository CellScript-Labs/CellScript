//! Complete-transaction VM infrastructure for the ordinary finance packages.
//! Only signed, untyped funding Cells are seeded. This local live set is not a
//! node's consensus state, and its reports make no submission/finality claim.

use camino::Utf8Path;
use cellscript::package::frozen_interface::{compile_module, EntrySelection, FrozenPackageModule};
use cellscript::{CompileMetadata, CompileOptions, CompilerDiagnostics, EntryWitnessArg};
use ckb_sdk::{traits::SecpCkbRawKeySigner, types::ScriptGroup, unlock::ScriptSigner, unlock::SecpSighashScriptSigner, SECP256K1};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::Capacity, core::TransactionBuilder, core::TransactionView, packed, prelude::*},
    context::Context,
};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path};

#[path = "finance_archive_format.rs"]
mod archive_format;

pub const CELL_CAPACITY: u64 = 100_000_000_000;
pub const FEE: u64 = 100_000_000;
// An exploratory execution allowance, not an accepted financial budget.
const EXPLORATORY_CYCLES: u64 = 100_000_000;
pub type Cell = (packed::CellOutput, Bytes);

pub struct Program {
    pub module: FrozenPackageModule,
    pub metadata: CompileMetadata,
    pub name: String,
    source_snapshot: Value,
    action: bool,
}

impl Program {
    pub fn compile(root: &Path, name: &str, action: bool) -> Self {
        let manifest = root.join("Cell.toml");
        let mut text = std::fs::read_to_string(&manifest).unwrap();
        if !text.contains("[environments.dev]") {
            text.push_str(&format!(
                "\n[environments.dev]\nchain_id = \"finance-local-vm\"\ngenesis_hash = \"0x{}\"\n",
                "11".repeat(32)
            ));
            std::fs::write(&manifest, text).unwrap();
        }
        let locked =
            std::process::Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["lock", "--json"]).output().unwrap();
        assert!(
            locked.status.success(),
            "{name} lock: stdout={} stderr={}",
            String::from_utf8_lossy(&locked.stdout),
            String::from_utf8_lossy(&locked.stderr)
        );
        let lock = std::fs::read(root.join("Cell.lock")).unwrap();
        let module = compile_module(
            Utf8Path::from_path(root).unwrap(),
            "dev",
            CompileOptions { diagnostics: CompilerDiagnostics::default(), ..Default::default() },
            if action { EntrySelection::Action("verify".into()) } else { EntrySelection::Lock("verify".into()) },
        )
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(std::fs::read(root.join("Cell.lock")).unwrap(), lock, "compilation must not repin");
        let metadata: CompileMetadata = serde_json::from_slice(module.bundle()[1]).unwrap();
        let source_context: Value = serde_json::from_slice(&module.context_bytes().unwrap()).unwrap();
        let units = metadata
            .source_units
            .iter()
            .map(|unit| {
                let bytes = std::fs::read(&unit.path).expect("frozen compiled source must still exist at capture");
                assert_eq!(archive_format::hash(&bytes), unit.hash);
                assert_eq!(bytes.len(), unit.size_bytes);
                let modules = source_context["modules"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|(_, module)| {
                        module["source_hash"] == unit.hash
                            && module["source_bytes"] == unit.size_bytes
                            && unit.path.ends_with(module["relative_path"].as_str().unwrap())
                    })
                    .collect::<Vec<_>>();
                assert_eq!(modules.len(), 1, "frozen source unit must have one actual module owner");
                let (module, contract) = modules[0];
                json!({"module":module,"package":contract["package"],"relative_path":contract["relative_path"],
                "metadata_path":unit.path,"source_hash":unit.hash,"size_bytes":bytes.len(),"bytes":hex::encode(bytes)})
            })
            .collect::<Vec<_>>();
        let source_snapshot =
            json!({"schema":"cellscript-finance-archived-source-v1","context_id":module.context_identity(),"units":units});
        Self { module, metadata, name: name.into(), source_snapshot, action }
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

    pub fn witness(&self, args: &[EntryWitnessArg]) -> packed::Bytes {
        let bytes = if self.action {
            self.metadata.actions.iter().find(|action| action.name == "verify").unwrap().entry_witness_args(args)
        } else {
            self.metadata.locks.iter().find(|lock| lock.name == "verify").unwrap().entry_witness_args(args)
        }
        .unwrap_or_else(|error| panic!("{} generated entry encoding: {error}", self.name));
        packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(bytes)).pack()).build().as_bytes().pack()
    }

    pub fn data(&self, ty: &str, fields: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let borrowed = fields.iter().map(|(name, value)| ((*name).to_owned(), value.as_slice())).collect();
        cellscript::fixed_cell_builder::encode_fixed_cell_data(&self.metadata, ty, &borrowed).unwrap_or_else(|error| {
            panic!(
                "{} generated {ty} data: {error}; available types: {:?}; fixed contracts: {:?}",
                self.name,
                self.metadata.types.iter().map(|definition| (&definition.name, &definition.kind)).collect::<Vec<_>>(),
                cellscript::fixed_cell_builder::fixed_cell_contracts(&self.metadata)
            )
        })
    }
}

pub fn copy_package(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        assert!(!file_type.is_symlink(), "package snapshot must not follow symlinks");
        if entry.file_name() == "Cell.lock" {
            continue;
        }
        if file_type.is_dir() {
            copy_package(&entry.path(), &destination.join(entry.file_name()));
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

pub fn owner_lock(key: u8) -> packed::Script {
    let code = ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_blake160_sighash_all").unwrap();
    let secret = secp256k1::SecretKey::from_slice(&[key; 32]).unwrap();
    let public = secp256k1::PublicKey::from_secret_key(&SECP256K1, &secret);
    let hash = cellscript_artifact_checker::ckb_blake2b256(&public.serialize());
    packed::Script::new_builder()
        .code_hash(packed::CellOutput::calc_data_hash(&code))
        .hash_type(4u8)
        .args(Bytes::copy_from_slice(&hash[..20]).pack())
        .build()
}

pub fn cell(lock: packed::Script, type_script: Option<packed::Script>, data: Vec<u8>) -> Cell {
    (packed::CellOutput::new_builder().capacity(CELL_CAPACITY).lock(lock).type_(type_script.pack()).build(), Bytes::from(data))
}

pub struct Ledger {
    pub context: Context,
    pub live: HashMap<packed::OutPoint, Cell>,
    pub deps: Vec<packed::CellDep>,
    pub records: Vec<Value>,
    keys: Vec<u8>,
}

impl Ledger {
    pub fn new(programs: &[&Program], keys: &[u8]) -> Self {
        let mut context = Context::new_with_deterministic_rng();
        let mut points =
            programs.iter().map(|program| context.deploy_cell(Bytes::copy_from_slice(program.code()))).collect::<Vec<_>>();
        for name in ["specs/cells/secp256k1_blake160_sighash_all", "specs/cells/secp256k1_data"] {
            points.push(context.deploy_cell(Bytes::copy_from_slice(&ckb_system_scripts_v0_6_0::BUNDLED_CELL.get(name).unwrap())));
        }
        Self {
            context,
            live: HashMap::new(),
            deps: points.into_iter().map(|point| packed::CellDep::new_builder().out_point(point).build()).collect(),
            records: Vec::new(),
            keys: keys.to_vec(),
        }
    }

    pub fn seed_funding(&mut self, key: u8, index: u32) -> packed::OutPoint {
        let (output, data) = cell(owner_lock(key), None, vec![]);
        let output = output.as_builder().capacity(100 * CELL_CAPACITY).build();
        let point = packed::OutPoint::new(owner_lock(key).calc_script_hash(), index);
        assert!(!self.live.contains_key(&point));
        self.context.create_cell_with_out_point(point.clone(), output.clone(), data.clone());
        self.live.insert(point.clone(), (output, data));
        point
    }

    pub fn build(
        &self,
        inputs: &[packed::OutPoint],
        mut outputs: Vec<Cell>,
        change: usize,
        witnesses: Vec<packed::Bytes>,
    ) -> TransactionView {
        let input_capacity: u64 = inputs.iter().map(|point| u64::from(self.live[point].0.capacity())).sum();
        let fixed: u64 =
            outputs.iter().enumerate().filter(|(index, _)| *index != change).map(|(_, cell)| u64::from(cell.0.capacity())).sum();
        outputs[change].0 = outputs[change].0.clone().as_builder().capacity(input_capacity.checked_sub(fixed + FEE).unwrap()).build();
        let tx = TransactionBuilder::default()
            .cell_deps(self.deps.clone())
            .inputs(inputs.iter().map(|point| packed::CellInput::new_builder().previous_output(point.clone()).build()))
            .outputs(outputs.iter().map(|cell| cell.0.clone()))
            .outputs_data(outputs.iter().map(|cell| cell.1.pack()))
            .witnesses(witnesses)
            .build();
        self.authorize(&tx)
    }

    pub fn authorize(&self, tx: &TransactionView) -> TransactionView {
        let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
        let empty = packed::WitnessArgs::default().as_bytes().pack();
        witnesses.resize(witnesses.len().max(tx.inputs().len()).max(tx.outputs().len()), empty.clone());
        for witness in &mut witnesses {
            if witness.raw_data().is_empty() {
                *witness = empty.clone();
            }
        }
        let mut signed = tx.as_advanced_builder().set_witnesses(witnesses).build();
        for key in &self.keys {
            let script = owner_lock(*key);
            let indices = tx
                .inputs()
                .into_iter()
                .enumerate()
                .filter_map(|(index, input)| (self.live[&input.previous_output()].0.lock() == script).then_some(index))
                .collect::<Vec<_>>();
            if indices.is_empty() {
                continue;
            }
            let signer = SecpSighashScriptSigner::new(Box::new(SecpCkbRawKeySigner::new_with_secret_keys(vec![
                secp256k1::SecretKey::from_slice(&[*key; 32]).unwrap(),
            ])));
            let mut group = ScriptGroup::from_lock_script(&script);
            group.input_indices = indices;
            signed = signer.sign_tx(&signed, &group).unwrap();
        }
        signed
    }

    fn capacity(&self, tx: &TransactionView) -> (u64, u64) {
        let inputs: u64 = tx.inputs().into_iter().map(|input| u64::from(self.live[&input.previous_output()].0.capacity())).sum();
        let outputs: u64 = tx.outputs().into_iter().map(|output| u64::from(output.capacity())).sum();
        assert_eq!(inputs.checked_sub(outputs), Some(FEE));
        for (index, output) in tx.outputs().into_iter().enumerate() {
            let size = tx.outputs_data().get(index).unwrap().raw_data().len();
            assert!(u64::from(output.capacity()) >= output.occupied_capacity(Capacity::bytes(size).unwrap()).unwrap().as_u64());
        }
        (inputs, outputs)
    }

    fn record(&self, tx: &TransactionView, name: &str) -> Value {
        let hex = |value: &[u8]| hex::encode(value);
        json!({
            "case": name,
            "transaction": hex(tx.data().as_slice()),
            "transaction_hash": hex(tx.hash().as_slice()),
            "serialized_transaction_hash": hex::encode(cellscript_artifact_checker::ckb_blake2b256(tx.data().as_slice())),
            "transaction_bytes": tx.data().as_slice().len(),
            "witness_bytes": tx.witnesses().into_iter().map(|witness| witness.raw_data().len()).sum::<usize>(),
            "fee": FEE,
            "inputs": tx.inputs().into_iter().map(|input| {
                let (output, data) = &self.live[&input.previous_output()];
                archive_format::cell(&input.previous_output(),output,data)
            }).collect::<Vec<_>>(),
        })
    }

    /// Verify an exact candidate without consuming its local predecessors.
    /// This is full transaction VM evidence, not node admission or commitment.
    pub fn accept_candidate(&mut self, tx: &TransactionView, name: &str) -> u64 {
        self.capacity(tx);
        let cycles = self.context.verify_tx(tx, EXPLORATORY_CYCLES).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let mut record = self.record(tx, name);
        record["outcome"] = json!("accepted");
        record["transaction_cycles"] = json!(cycles);
        record["local_bookkeeping"] = json!("not-consumed-vm-preflight");
        record["node_acceptance"] = Value::Null;
        self.records.push(record);
        cycles
    }

    pub fn commit(&mut self, tx: &TransactionView, name: &str) -> Vec<packed::OutPoint> {
        self.capacity(tx);
        let cycles = self.context.verify_tx(tx, EXPLORATORY_CYCLES).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let mut record = self.record(tx, name);
        record["outcome"] = json!("accepted");
        record["transaction_cycles"] = json!(cycles);
        self.records.push(record);
        for input in tx.inputs() {
            self.live.remove(&input.previous_output()).expect("accepted input must still be locally live");
        }
        tx.outputs()
            .into_iter()
            .enumerate()
            .map(|(index, output)| {
                let point = packed::OutPoint::new(tx.hash(), index as u32);
                let data = tx.outputs_data().get(index).unwrap().raw_data();
                self.context.create_cell_with_out_point(point.clone(), output.clone(), data.clone());
                assert!(self.live.insert(point.clone(), (output, data)).is_none());
                point
            })
            .collect()
    }

    pub fn reject(&mut self, tx: &TransactionView, groups: &[&str], name: &str) {
        let tx = self.authorize(tx);
        self.capacity(&tx);
        let error = self.context.verify_tx(&tx, EXPLORATORY_CYCLES).expect_err("hostile transaction must reject");
        let error = format!("{error:?}");
        assert!(error.contains("ValidationFailure"), "{name}: a VM/tool failure is not the intended policy rejection: {error}");
        assert!(groups.iter().any(|group| error.contains(group)), "{name}: expected {groups:?}, observed {error}");
        let mut record = self.record(&tx, name);
        record["outcome"] = json!("rejected");
        record["expected_groups"] = json!(groups);
        record["error"] = json!(error);
        record["transaction_cycles"] = Value::Null;
        record["cycles_unavailable_reason"] = json!("Context::verify_tx does not return cycles on rejection");
        self.records.push(record);
    }

    pub fn archive(&self, directory: &Path, programs: &[&Program]) {
        std::fs::create_dir_all(directory).unwrap();
        let mut manifest = Vec::new();
        for program in programs {
            let mut files = serde_json::Map::new();
            for (suffix, bytes) in ["elf", "metadata.json", "lowering.json", "sourcemap.json"].into_iter().zip(program.module.bundle())
            {
                let file = format!("{}.{suffix}", program.name);
                std::fs::write(directory.join(&file), bytes).unwrap();
                files.insert(suffix.into(), json!({"file":file,"size_bytes":bytes.len(),"hash":archive_format::hash(bytes)}));
            }
            for (suffix, bytes) in [
                ("source-context.json", program.module.context_bytes().unwrap()),
                ("sources.json", serde_json::to_vec(&program.source_snapshot).unwrap()),
            ] {
                let file = format!("{}.{suffix}", program.name);
                std::fs::write(directory.join(&file), &bytes).unwrap();
                files.insert(suffix.into(), json!({"file":file,"size_bytes":bytes.len(),"hash":archive_format::hash(&bytes)}));
            }
            manifest.push(json!({"name":program.name,"context_id":program.module.context_identity(),
                "module_projection_id":program.module.projection().identity(),"source_content_hash":program.metadata.source_content_hash,
                "elf_hash":archive_format::hash(program.code()),"files":files}));
        }
        let dependencies = self
            .deps
            .iter()
            .map(|dep| {
                let (output, data) = self.context.get_cell(&dep.out_point()).unwrap();
                let mut cell = archive_format::cell(&dep.out_point(), &output, &data);
                cell["dep_type"] = json!(u8::from(dep.dep_type()));
                cell
            })
            .collect::<Vec<_>>();
        let (mut local_observations, mut cases): (Vec<_>, Vec<_>) =
            self.records.iter().cloned().partition(|record| record["rejection_stage"] == "local-live-set");
        for observation in &mut local_observations {
            assert_eq!(observation["outcome"], "rejected");
            assert!(observation["transaction_cycles"].is_null());
            let raw = hex::decode(observation["transaction"].as_str().unwrap()).unwrap();
            let tx = packed::Transaction::from_slice(&raw).unwrap().into_view();
            observation["transaction_hash"] = json!(hex::encode(tx.hash().as_slice()));
            observation["serialized_transaction_hash"] = json!(archive_format::hash(&raw));
            observation["observation_id"] = json!(archive_format::observation_identity(observation).unwrap());
        }
        for case in &mut cases {
            case["case_id"] = json!(archive_format::case_identity(case).unwrap());
        }
        let mut report = json!({
            "schema":archive_format::SCHEMA, "scope":"complete-transaction-local-ckb-vm",
            "compiler_version":env!("CARGO_PKG_VERSION"), "diagnostics":"standard",
            "node_acceptance":null, "external_reproduction":null, "frozen_financial_budgets":false,
            "dependencies":dependencies, "cases":cases,"programs":manifest,"local_observations":local_observations,
        });
        report["archive_id"] = json!(archive_format::archive_identity(&report).unwrap());
        std::fs::write(directory.join("report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
