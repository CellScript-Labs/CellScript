//! Offline replay of checksum-bound ordinary finance VM archives. Every load
//! creates its Context from archived Cells; no live Ledger or source compiler
//! participates. Checksums establish internal consistency, not publisher trust
//! or correspondence to a remote node's live Cell set.
use cellscript_artifact_checker::{CheckerBudgets, SourceArtifactMap, VerifiedLoweringRecord};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*},
    context::Context,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::Path,
};

#[path = "finance_archive_format.rs"]
pub mod format;

#[derive(Debug)]
pub struct ArchiveError {
    pub stage: &'static str,
    pub message: String,
}
impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.stage, self.message)
    }
}
impl Error for ArchiveError {}
type Result<T> = std::result::Result<T, ArchiveError>;
fn error(stage: &'static str, message: impl Into<String>) -> ArchiveError {
    ArchiveError { stage, message: message.into() }
}
fn require(value: bool, stage: &'static str, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(error(stage, message))
    }
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().ok_or_else(|| error("parse", format!("missing string {key}")))
}
fn number(value: &Value, key: &str) -> Result<u64> {
    value[key].as_u64().ok_or_else(|| error("parse", format!("missing integer {key}")))
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value[key].as_array().ok_or_else(|| error("parse", format!("missing array {key}")))
}
fn bytes(value: &Value, key: &str) -> Result<Vec<u8>> {
    hex::decode(text(value, key)?).map_err(|e| error("parse", format!("{key}: {e}")))
}
fn json(bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).map_err(|e| error("parse", e.to_string()))
}

#[derive(Clone)]
struct Cell {
    point: packed::OutPoint,
    output: packed::CellOutput,
    data: Bytes,
}
fn cell(value: &Value) -> Result<Cell> {
    let point = packed::OutPoint::from_slice(&bytes(value, "out_point")?).map_err(|e| error("parse", format!("out_point: {e}")))?;
    let output = packed::CellOutput::from_slice(&bytes(value, "output")?).map_err(|e| error("parse", format!("CellOutput: {e}")))?;
    let data = Bytes::from(bytes(value, "data")?);
    let expected = format::cell(&point, &output, &data);
    for key in ["output_hash", "data_hash", "content_hash"] {
        require(value[key] == expected[key], "integrity", &format!("resolved Cell {key} differs from its exact bytes"))?;
    }
    Ok(Cell { point, output, data })
}

pub struct ArchivedProgram {
    pub name: String,
    pub elf: Vec<u8>,
    pub metadata: Value,
    pub lowering: VerifiedLoweringRecord,
    pub source_map: SourceArtifactMap,
    pub source_context: Value,
    pub source_snapshot: Value,
    source_context_bytes: Vec<u8>,
}

pub struct Archive {
    report: Value,
    programs: Vec<ArchivedProgram>,
    dependencies: Vec<Cell>,
    cases: Vec<Value>,
    local_observations: Vec<Value>,
    transactions: Vec<TransactionView>,
    inputs: Vec<Vec<Cell>>,
}

impl Archive {
    pub fn identity(&self) -> &str {
        self.report["archive_id"].as_str().unwrap()
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn cases(&self) -> &[Value] {
        &self.cases
    }
    pub fn local_observations(&self) -> &[Value] {
        &self.local_observations
    }
    pub fn programs(&self) -> &[ArchivedProgram] {
        &self.programs
    }
    /// A new empty Context receives exactly this archive's dependencies and
    /// this case's resolved inputs. No auto-completion may add dependencies or
    /// rewrite the archived transaction, witnesses or outpoints.
    pub fn load_case(&self, index: usize) -> Result<(Context, TransactionView, Value)> {
        let case = self.cases.get(index).ok_or_else(|| error("resolution", "case index is outside archive"))?;
        let mut context = Context::new_with_deterministic_rng();
        for cell in self.dependencies.iter().chain(&self.inputs[index]) {
            context.create_cell_with_out_point(cell.point.clone(), cell.output.clone(), cell.data.clone());
        }
        require(context.cells.len() == self.dependencies.len() + self.inputs[index].len(), "resolution", "restored Cells alias")?;
        Ok((context, self.transactions[index].clone(), case.clone()))
    }

    pub fn replay_case(&self, index: usize, max_cycles: u64) -> Result<Value> {
        let (context, transaction, record) = self.load_case(index)?;
        let result = context.verify_tx(&transaction, max_cycles);
        let mut actual = json!({"index":index,"case":record["case"],"case_id":record["case_id"],
            "transaction_hash":hex::encode(transaction.hash().as_slice()),"fresh_context":true,
            "actual_vm_stack_bytes":null,"stack_observation":"not measured by this Context::verify_tx replay; use the separate cost tracer"});
        match (text(&record, "outcome")?, result) {
            ("accepted", Ok(cycles)) => {
                require(cycles > 0, "vm", "accepted archive case executed zero cycles")?;
                require(
                    number(&record, "transaction_cycles")? == cycles,
                    "expectation",
                    "fresh execution cycles differ from archived accepted cycles",
                )?;
                actual["outcome"] = json!("accepted");
                actual["transaction_cycles"] = json!(cycles);
            }
            ("rejected", Err(failure)) => {
                let failure = format!("{failure:?}");
                require(
                    failure.contains("ValidationFailure"),
                    "vm",
                    &format!("rejected case had a non-policy VM/tool failure: {failure}"),
                )?;
                let matched = array(&record, "expected_groups")?
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|group| failure.contains(group))
                    .collect::<Vec<_>>();
                require(
                    !matched.is_empty(),
                    "expectation",
                    &format!("rejected case failed outside its expected Script groups: {failure}"),
                )?;
                actual["outcome"] = json!("rejected");
                actual["transaction_cycles"] = Value::Null;
                actual["cycles_unavailable_reason"] = json!("Context::verify_tx returns no cycles on a rejected transaction");
                actual["matched_expected_groups"] = json!(matched);
                actual["error"] = json!(failure);
            }
            ("accepted", Err(failure)) => {
                return Err(error("vm", format!("accepted case {} failed fresh replay: {failure:?}", record["case"])))
            }
            ("rejected", Ok(_)) => return Err(error("expectation", "rejected archive case passed fresh execution")),
            _ => return Err(error("parse", "unknown archived outcome")),
        }
        Ok(actual)
    }

    pub fn replay_all(&self, max_cycles: u64) -> Result<Value> {
        let cases = (0..self.cases.len()).map(|index| self.replay_case(index, max_cycles)).collect::<Result<Vec<_>>>()?;
        Ok(json!({"schema":"cellscript-finance-archive-vm-replay-v1","archive_id":self.identity(),
            "scope":"fresh-context archived-byte CKB-VM replay only","node_acceptance":null,"external_reproduction":null,"cases":cases,
            "local_observations":self.local_observations,"local_observation_scope":"archived local bookkeeping only; not replayed as Script failures"}))
    }
}

fn read_file(directory: &Path, name: &str, limit: u64) -> Result<Vec<u8>> {
    let path = directory.join(name);
    let metadata = std::fs::symlink_metadata(&path).map_err(|e| error("integrity", format!("{}: {e}", path.display())))?;
    require(metadata.file_type().is_file() && metadata.len() <= limit, "integrity", "archive file is not bounded regular bytes")?;
    std::fs::read(&path).map_err(|e| error("integrity", e.to_string()))
}

fn sources(program: &ArchivedProgram, manifest: &Value) -> Result<()> {
    let context = &program.source_context;
    // FrozenModule hashes its exact typed-struct serialization. Re-encoding a
    // JSON Value would reorder fields and invent a different context identity.
    let context_id =
        cellscript_artifact_checker::domain_hash_bytes("cellscript-frozen-module-source-context-id-v1", &program.source_context_bytes);
    require(
        text(context, "schema")? == "cellscript-frozen-module-source-context-v1" && context_id == text(manifest, "context_id")?,
        "integrity",
        &format!("source context identity differs: {} actual={} expected={}", program.name, context_id, text(manifest, "context_id")?),
    )?;
    let snapshot = &program.source_snapshot;
    require(
        text(snapshot, "schema")? == "cellscript-finance-archived-source-v1" && text(snapshot, "context_id")? == context_id,
        "integrity",
        "source snapshot context differs",
    )?;
    let units = array(snapshot, "units")?;
    let metadata_units = array(&program.metadata, "source_units")?;
    let modules = context["modules"].as_object().ok_or_else(|| error("parse", "source context has no modules"))?;
    require(
        context["entry_module"] == program.metadata["module"]
            && context["compiler"] == program.metadata["compiler_version"]
            && modules.contains_key(text(context, "entry_module")?),
        "integrity",
        "source context entry/compiler differs from the checked module",
    )?;
    require(
        !units.is_empty() && units.len() == metadata_units.len() && units.len() == modules.len(),
        "integrity",
        "source closure is incomplete",
    )?;
    let mut seen = BTreeSet::new();
    for unit in units {
        let name = text(unit, "module")?;
        require(seen.insert(name), "integrity", "duplicate source module")?;
        let module = modules.get(name).ok_or_else(|| error("integrity", "source module has no frozen owner"))?;
        let matching = metadata_units.iter().filter(|candidate| candidate["path"] == unit["metadata_path"]).collect::<Vec<_>>();
        require(matching.len() == 1, "integrity", "source unit has no unique metadata binding")?;
        let data = bytes(unit, "bytes")?;
        require(
            format::hash(&data) == text(unit, "source_hash")?
                && number(unit, "size_bytes")? == data.len() as u64
                && module["source_hash"] == unit["source_hash"]
                && module["source_bytes"] == unit["size_bytes"]
                && matching[0]["hash"] == unit["source_hash"]
                && matching[0]["size_bytes"] == unit["size_bytes"]
                && module["package"] == unit["package"]
                && module["relative_path"] == unit["relative_path"],
            "integrity",
            "source bytes, metadata or frozen owner differ",
        )?;
    }
    let mut stable = metadata_units.iter().collect::<Vec<_>>();
    stable.sort_by(|left, right| {
        left["role"]
            .as_str()
            .cmp(&right["role"].as_str())
            .then(left["hash"].as_str().cmp(&right["hash"].as_str()))
            .then(left["size_bytes"].as_u64().cmp(&right["size_bytes"].as_u64()))
    });
    let mut material = Vec::new();
    for unit in stable {
        material.extend_from_slice(text(unit, "role")?.as_bytes());
        material.push(0);
        material.extend_from_slice(text(unit, "hash")?.as_bytes());
        material.push(0);
        material.extend_from_slice(&number(unit, "size_bytes")?.to_le_bytes());
        material.push(0);
    }
    let source_hash = format::hash(&material);
    require(
        program.metadata["source_content_hash"] == source_hash
            && manifest["source_content_hash"] == source_hash
            && program.source_map.source_digest == source_hash,
        "integrity",
        "source content hash differs from bundle",
    )?;
    let mut located = Vec::new();
    for unit in metadata_units {
        for field in ["role", "path", "hash"] {
            located.extend_from_slice(text(unit, field)?.as_bytes());
            located.push(0);
        }
        located.extend_from_slice(&number(unit, "size_bytes")?.to_le_bytes());
        located.push(0);
    }
    require(
        program.metadata["source_hash"] == format::hash(&located),
        "integrity",
        "source location hash differs from its archived unit bindings",
    )?;
    Ok(())
}

pub fn load_archive(directory: &Path) -> Result<Archive> {
    let report = json(&read_file(directory, "report.json", 64 * 1024 * 1024)?)?;
    require(text(&report, "schema")? == format::SCHEMA, "integrity", "strict replay requires complete v2 archives")?;
    require(
        text(&report, "archive_id")? == format::archive_identity(&report).map_err(|e| error("integrity", e))?,
        "integrity",
        "archive identity differs",
    )?;
    require(text(&report, "scope")? == "complete-transaction-local-ckb-vm", "integrity", "archive has a different execution scope")?;
    let mut programs = Vec::new();
    let mut names = BTreeSet::new();
    for manifest in array(&report, "programs")? {
        let name = text(manifest, "name")?;
        require(
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) && names.insert(name),
            "integrity",
            "program name is invalid or repeated",
        )?;
        let files = manifest["files"].as_object().ok_or_else(|| error("parse", "program manifest has no files"))?;
        require(files.len() == 6, "integrity", "program manifest is incomplete")?;
        let mut loaded = BTreeMap::new();
        for suffix in ["elf", "metadata.json", "lowering.json", "sourcemap.json", "source-context.json", "sources.json"] {
            let file = files.get(suffix).ok_or_else(|| error("integrity", "required program file is absent"))?;
            require(text(file, "file")? == format!("{name}.{suffix}"), "integrity", "program file escapes its declared owner")?;
            let bytes = read_file(directory, text(file, "file")?, 16 * 1024 * 1024)?;
            require(
                number(file, "size_bytes")? == bytes.len() as u64 && text(file, "hash")? == format::hash(&bytes),
                "integrity",
                "program file hash or size differs",
            )?;
            loaded.insert(suffix, bytes);
        }
        let projection = cellscript_artifact_checker::interface::project_bundle(
            &loaded["elf"],
            &loaded["metadata.json"],
            &loaded["lowering.json"],
            &loaded["sourcemap.json"],
            &CheckerBudgets::default(),
        )
        .map_err(|e| error("bundle", e.to_string()))?;
        require(
            projection.identity() == text(manifest, "module_projection_id")?
                && format::hash(&loaded["elf"]) == text(manifest, "elf_hash")?,
            "integrity",
            "program ELF or checked projection differs",
        )?;
        let program = ArchivedProgram {
            name: name.into(),
            elf: loaded["elf"].clone(),
            metadata: json(&loaded["metadata.json"])?,
            lowering: serde_json::from_slice(&loaded["lowering.json"]).map_err(|e| error("parse", e.to_string()))?,
            source_map: serde_json::from_slice(&loaded["sourcemap.json"]).map_err(|e| error("parse", e.to_string()))?,
            source_context: json(&loaded["source-context.json"])?,
            source_context_bytes: loaded["source-context.json"].clone(),
            source_snapshot: json(&loaded["sources.json"])?,
        };
        sources(&program, manifest)?;
        programs.push(program);
    }
    require(!programs.is_empty(), "integrity", "archive has no checked programs")?;
    let mut dependencies = Vec::new();
    let mut points = BTreeSet::new();
    for dependency in array(&report, "dependencies")? {
        require(number(dependency, "dep_type")? == 0, "resolution", "archive replay requires direct code dependencies")?;
        let parsed = cell(dependency)?;
        require(points.insert(parsed.point.as_slice().to_vec()), "resolution", "duplicate dependency outpoint")?;
        dependencies.push(parsed);
    }
    for program in &programs {
        require(
            dependencies.iter().any(|dep| dep.data.as_ref() == program.elf.as_slice()),
            "integrity",
            "checked ELF is absent from archived CellDeps",
        )?;
    }
    let cases = array(&report, "cases")?.clone();
    let local_observations = match report.get("local_observations") {
        None => Vec::new(),
        Some(observations) => observations.as_array().ok_or_else(|| error("parse", "local observations must be an array"))?.clone(),
    };
    for observation in &local_observations {
        require(
            text(observation, "rejection_stage")? == "local-live-set"
                && text(observation, "outcome")? == "rejected"
                && observation["transaction_cycles"].is_null()
                && !text(observation, "error")?.is_empty(),
            "parse",
            "local observation claims an invalid execution scope",
        )?;
        require(
            text(observation, "observation_id")? == format::observation_identity(observation).map_err(|e| error("integrity", e))?,
            "integrity",
            "local observation identity differs",
        )?;
        let raw = bytes(observation, "transaction")?;
        let tx = packed::Transaction::from_slice(&raw)
            .map_err(|e| error("parse", format!("local observation transaction: {e}")))?
            .into_view();
        require(
            text(observation, "transaction_hash")? == hex::encode(tx.hash().as_slice())
                && text(observation, "serialized_transaction_hash")? == format::hash(&raw),
            "integrity",
            "local observation transaction identity differs",
        )?;
    }
    require(!cases.is_empty(), "integrity", "archive has no cases")?;
    let mut transactions = Vec::new();
    let mut inputs = Vec::new();
    for case in &cases {
        require(
            case["rejection_stage"] != "local-live-set",
            "parse",
            "local bookkeeping rejection belongs in local_observations, not VM cases",
        )?;
        require(
            !text(case, "case")?.is_empty()
                && text(case, "case_id")? == format::case_identity(case).map_err(|e| error("integrity", e))?,
            "integrity",
            "case identity differs",
        )?;
        let raw = bytes(case, "transaction")?;
        require(raw.len() <= 16 * 1024 * 1024, "parse", "transaction bytes exceed replay profile")?;
        let tx = packed::Transaction::from_slice(&raw).map_err(|e| error("parse", format!("transaction Molecule: {e}")))?.into_view();
        require(
            number(case, "transaction_bytes")? == raw.len() as u64
                && text(case, "serialized_transaction_hash")? == format::hash(&raw)
                && text(case, "transaction_hash")? == hex::encode(tx.hash().as_slice()),
            "integrity",
            "transaction identity or length differs",
        )?;
        require(
            number(case, "witness_bytes")? == tx.witnesses().into_iter().map(|w| w.raw_data().len() as u64).sum::<u64>(),
            "integrity",
            "witness byte count differs",
        )?;
        require(tx.header_deps().is_empty(), "resolution", "archive lacks header resolutions")?;
        for dep in tx.cell_deps() {
            require(
                u8::from(dep.dep_type()) == 0 && points.contains(dep.out_point().as_slice()),
                "resolution",
                "transaction dependency is unresolved or a dep group",
            )?;
        }
        let resolved = array(case, "inputs")?.iter().map(cell).collect::<Result<Vec<_>>>()?;
        require(resolved.len() == tx.inputs().len(), "resolution", "resolved input list is incomplete")?;
        let mut seen = BTreeSet::new();
        for (input, resolved) in tx.inputs().into_iter().zip(&resolved) {
            require(
                input.previous_output() == resolved.point
                    && seen.insert(resolved.point.as_slice().to_vec())
                    && !points.contains(resolved.point.as_slice()),
                "resolution",
                "input identity/order is duplicated, aliased or substituted",
            )?;
        }
        let input_capacity = resolved
            .iter()
            .try_fold(0u64, |total, cell| total.checked_add(u64::from(cell.output.capacity())))
            .ok_or_else(|| error("resolution", "input capacity overflows"))?;
        let output_capacity = tx
            .outputs()
            .into_iter()
            .try_fold(0u64, |total, output| total.checked_add(u64::from(output.capacity())))
            .ok_or_else(|| error("resolution", "output capacity overflows"))?;
        require(
            input_capacity.checked_sub(output_capacity) == Some(number(case, "fee")?),
            "integrity",
            "archived fee differs from resolved capacities",
        )?;
        match text(case, "outcome")? {
            "accepted" => require(number(case, "transaction_cycles")? > 0, "parse", "accepted case has no execution cycles")?,
            "rejected" => require(
                !array(case, "expected_groups")?.is_empty()
                    && array(case, "expected_groups")?.iter().all(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                    && text(case, "error")?.contains("ValidationFailure")
                    && case["transaction_cycles"].is_null(),
                "parse",
                "rejected case lacks a complete policy-rejection expectation",
            )?,
            _ => return Err(error("parse", "unknown archived outcome")),
        }
        inputs.push(resolved);
        transactions.push(tx);
    }
    Ok(Archive { report, programs, dependencies, cases, local_observations, transactions, inputs })
}
