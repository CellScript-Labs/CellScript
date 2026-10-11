//! File transport for recomputing private open admission, never for importing
//! serialized checked proofs. Source compilation remains frozen and offline.
use super::*;
use crate::package::frozen_interface::{
    compile_module, freeze_code_catalog, freeze_package_sources, freeze_source_code_policy_with_histories, resolve_source_catalog,
    CodeCandidateInput, EntrySelection,
};
use cellscript_artifact_checker::code_origin::{check_type_code_history, TypeCodeHistoryStep};
use cellscript_artifact_checker::open_handle_policy::{AuthorizationSet, PolicyHeader, PolicyMember};
use std::io::Read;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    transaction_file: String,
    inputs: Vec<Cell>,
    dependencies: Vec<Cell>,
    bindings: Vec<Binding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cell {
    out_point: String,
    output: String,
    data: String,
}
struct CellBytes {
    point: [u8; 36],
    output: Vec<u8>,
    data: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    consumer: String,
    parameter: String,
    participant: String,
    consumer_module: Module,
    required: Module,
    candidates: Vec<Candidate>,
    policy_header: String,
    policy_members: Vec<String>,
    selection: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Module {
    package: String,
    environment: String,
    entry_kind: String,
    entry: String,
    opt_level: u8,
    #[serde(default)]
    target_profile: Option<String>,
}
impl Module {
    fn root(&self, base: &Path) -> Result<camino::Utf8PathBuf> {
        let path = confined_path(base, &self.package, "open source package")?;
        camino::Utf8PathBuf::from_path_buf(path).map_err(|_| invalid("source package path must be UTF-8"))
    }
    fn compile(&self, base: &Path) -> Result<FrozenPackageModule> {
        let selection = match self.entry_kind.as_str() {
            "action" => EntrySelection::Action(self.entry.clone()),
            "lock" => EntrySelection::Lock(self.entry.clone()),
            "artifact" => EntrySelection::Artifact(self.entry.clone()),
            _ => return Err(invalid("unknown frozen source entry kind")),
        };
        if self.opt_level > 3 || self.environment.is_empty() || self.entry.is_empty() {
            return Err(invalid("source module requires an environment, entry and optimization level 0..=3"));
        }
        compile_module(
            &self.root(base)?,
            &self.environment,
            crate::CompileOptions { opt_level: self.opt_level, target_profile: self.target_profile.clone(), ..Default::default() },
            selection,
        )
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    module: Module,
    creation_transaction_file: String,
    output_index: u32,
    selected_script: String,
    #[serde(default)]
    history: Vec<History>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    transaction_file: String,
    output_index: u32,
}
struct BytesBudget(usize);
impl BytesBudget {
    fn include(&mut self, bytes: usize) -> Result<()> {
        self.0 = self.0.checked_add(bytes).ok_or_else(|| invalid("open admission byte sum overflow"))?;
        if bytes > 4 * 1024 * 1024 || self.0 > 16 * 1024 * 1024 {
            return Err(invalid("open admission exceeds 4 MiB per byte input or 16 MiB aggregate"));
        }
        Ok(())
    }
    fn file(&mut self, base: &Path, path: &str) -> Result<Vec<u8>> {
        let path = confined_path(base, path, "open transaction")?;
        let bytes = read_file(&path, 4 * 1024 * 1024)?;
        self.include(bytes.len())?;
        Ok(bytes)
    }
    fn hex(&mut self, value: &str) -> Result<Vec<u8>> {
        // Check the encoded size before allocating the decoded buffer.
        self.include(value.len().saturating_sub(2) / 2)?;
        hex_bytes(value)
    }
    fn cells(&mut self, cells: &[Cell]) -> Result<Vec<CellBytes>> {
        cells
            .iter()
            .map(|cell| {
                Ok(CellBytes {
                    point: self.hex(&cell.out_point)?.try_into().map_err(|_| invalid("snapshot OutPoint must be 36 bytes"))?,
                    output: self.hex(&cell.output)?,
                    data: self.hex(&cell.data)?,
                })
            })
            .collect()
    }
}
fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    // Reject FIFOs/devices before opening; bounded reads alone would not keep
    // opening a named pipe from waiting for a writer.
    if !std::fs::metadata(path)?.is_file() {
        return Err(invalid("open admission requires regular input files"));
    }
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("open admission requires regular input files"));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid("open admission file exceeds its byte bound"));
    }
    Ok(bytes)
}

/// Recompile all declared package snapshots and independently check every
/// candidate before admitting the supplied transaction. Paths are confined to
/// each manifest's directory; no lock repinning or network resolution occurs.
pub fn check_protocol_bundle_with_open_handles_file(bundle_path: &Path, request_path: &Path) -> Result<ProtocolBundleReport> {
    let input: ProtocolBundleInput = serde_json::from_slice(&read_file(bundle_path, MAX_MANIFEST_BYTES as usize)?)?;
    let request: Request = serde_json::from_slice(&read_file(request_path, 16 * 1024 * 1024)?)?;
    if request.schema != "cellscript-protocol-open-admission-input-v1"
        || request.bindings.is_empty()
        || request.bindings.len() > 32
        || request.inputs.len() > 256
        || request.dependencies.len() > 64
        || request.bindings.iter().any(|binding| {
            binding.candidates.is_empty()
                || binding.candidates.len() > 32
                || binding.policy_members.len() != binding.candidates.len()
                || binding.candidates.iter().any(|candidate| candidate.history.len() > 32)
        })
    {
        return Err(invalid("unsupported open admission schema or cardinality"));
    }
    let base =
        std::fs::canonicalize(request_path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")))?;
    let mut budget = BytesBudget(0);
    let full = budget.file(&base, &request.transaction_file)?;
    let inputs = budget.cells(&request.inputs)?;
    let dependencies = budget.cells(&request.dependencies)?;
    let mut owners = Vec::new();
    for binding in &request.bindings {
        let header = PolicyHeader::decode(&budget.hex(&binding.policy_header)?).map_err(|e| invalid(&e.to_string()))?;
        let members = binding
            .policy_members
            .iter()
            .map(|member| PolicyMember::decode(&budget.hex(member)?).map_err(|e| invalid(&e.to_string())))
            .collect::<Result<Vec<_>>>()?;
        let wire = AuthorizationSet::new(header, &members).map_err(|e| invalid(&e.to_string()))?;
        let selection = budget.hex(&binding.selection)?;
        let mut raw = Vec::new();
        let mut scripts = Vec::new();
        let mut history_bytes = Vec::new();
        for candidate in &binding.candidates {
            raw.push(budget.file(&base, &candidate.creation_transaction_file)?);
            scripts.push(budget.hex(&candidate.selected_script)?);
            history_bytes
                .push(candidate.history.iter().map(|step| budget.file(&base, &step.transaction_file)).collect::<Result<Vec<_>>>()?);
        }
        let mut candidates = Vec::new();
        for ((candidate, raw_transaction), selected_script) in binding.candidates.iter().zip(raw).zip(scripts) {
            candidates.push(CodeCandidateInput {
                module: candidate.module.compile(&base)?,
                raw_transaction,
                output_index: candidate.output_index,
                selected_script,
            });
        }
        let catalog = freeze_code_catalog(binding.required.compile(&base)?, candidates, &CheckerBudgets::default())?;
        let mut histories = Vec::new();
        for ((candidate, checked_candidate), raw) in binding.candidates.iter().zip(catalog.candidates()).zip(&history_bytes) {
            if !candidate.history.is_empty() {
                let steps = candidate
                    .history
                    .iter()
                    .zip(raw)
                    .map(|(step, bytes)| TypeCodeHistoryStep { raw_transaction: bytes, output_index: step.output_index })
                    .collect::<Vec<_>>();
                histories.push(checked(check_type_code_history(
                    checked_candidate.target_origin(),
                    &steps,
                    &CheckerBudgets::default(),
                ))?);
            }
        }
        let sources = freeze_package_sources(&binding.consumer_module.root(&base)?, &binding.consumer_module.environment)?;
        let policy = freeze_source_code_policy_with_histories(resolve_source_catalog(sources, catalog)?, wire, histories)?;
        let consumer = binding.consumer_module.compile(&base)?;
        owners.push((consumer, policy, selection));
    }
    let bindings = request
        .bindings
        .iter()
        .zip(&owners)
        .map(|(binding, (consumer, policy, selection))| ProtocolOpenHandleBinding {
            consumer: &binding.consumer,
            parameter: &binding.parameter,
            participant: &binding.participant,
            consumer_module: consumer,
            policy,
            selection,
        })
        .collect::<Vec<_>>();
    let input_cells = inputs
        .iter()
        .map(|cell| SuppliedInputCell { out_point: cell.point, output: &cell.output, data: &cell.data })
        .collect::<Vec<_>>();
    let dependency_cells = dependencies
        .iter()
        .map(|cell| SuppliedDependencyCell { out_point: cell.point, output: &cell.output, data: &cell.data })
        .collect::<Vec<_>>();
    check_protocol_bundle_with_open_handles(
        &input,
        bundle_path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")),
        &ProtocolOpenTransaction {
            full_transaction: &full,
            inputs: &input_cells,
            dependencies: &dependency_cells,
            bindings: &bindings,
        },
    )
}
