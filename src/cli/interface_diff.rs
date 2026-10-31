//! Checked nominal compatibility uses complete independently verified bundles.
use super::commands::InterfaceDiffArgs;
use crate::error::{CompileError, CompileErrorCategory, Result};
use camino::Utf8Path;
use cellscript_artifact_checker::{
    interface::{project_bundle, CheckedModuleProjection, MAX_PROJECTION_FILE_BYTES},
    CheckerBudgets,
};
use serde::Serialize;
use std::{fs::File, io::Read, path::Path};

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    version: u32,
    evidence: &'static str,
    compatible: bool,
    required_module: String,
    candidate_module: String,
    required_contract_hash: String,
    candidate_contract_hash: String,
    required_artifact_hash: String,
    candidate_artifact_hash: String,
    incompatibility: Option<cellscript_artifact_checker::CheckerError>,
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let io_error = |error: std::io::Error| {
        CompileError::without_span(format!("failed to read checked interface bundle '{}': {error}", path.display()))
            .with_category(CompileErrorCategory::Io)
            .with_source(error)
    };
    let file = File::open(path).map_err(io_error)?;
    let mut bytes = Vec::new();
    file.take(MAX_PROJECTION_FILE_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(io_error)?;
    if bytes.len() > MAX_PROJECTION_FILE_BYTES {
        return Err(CompileError::without_span(format!("checked interface bundle file '{}' exceeds the byte budget", path.display()))
            .with_code("E2400"));
    }
    Ok(bytes)
}

fn project(path: &Path) -> Result<CheckedModuleProjection> {
    let artifact = Utf8Path::from_path(path).ok_or_else(|| CompileError::without_span("artifact path is not valid UTF-8"))?;
    let artifact_bytes = read_bounded(path)?;
    let metadata = read_bounded(crate::default_metadata_path_for_artifact(artifact).as_std_path())?;
    let lowering = read_bounded(crate::lowering_record_output_path_from_artifact(artifact).as_std_path())?;
    let source_map = read_bounded(crate::source_map_output_path_from_artifact(artifact).as_std_path())?;
    project_bundle(&artifact_bytes, &metadata, &lowering, &source_map, &CheckerBudgets::default()).map_err(|error| {
        CompileError::without_span(format!("checked interface bundle '{}': {error}", path.display())).with_code("E2400")
    })
}

pub(super) fn run(args: InterfaceDiffArgs) -> Result<()> {
    let required = project(&args.old)?;
    let candidate = project(&args.new)?;
    let incompatibility = required.check_required_contracts(&candidate).err();
    let report = Report {
        schema: "cellscript-checked-interface-compatibility-v1",
        version: 1,
        evidence: "independently-checked-module-contracts",
        compatible: incompatibility.is_none(),
        required_module: required.module().into(),
        candidate_module: candidate.module().into(),
        required_contract_hash: required.identity().into(),
        candidate_contract_hash: candidate.identity().into(),
        required_artifact_hash: required.artifact_report().artifact_hash.clone(),
        candidate_artifact_hash: candidate.artifact_report().artifact_hash.clone(),
        incompatibility,
    };
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| CompileError::without_span(format!("failed to serialize checked interface comparison: {error}")))?;
    if let Some(output) = &args.output {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output, format!("{json}\n"))?;
    }
    if !report.compatible && args.json {
        let details = serde_json::to_value(&report)
            .map_err(|error| CompileError::without_span(format!("failed to serialize checked interface comparison: {error}")))?;
        return Err(CompileError::without_span("independently checked interface contracts are incompatible")
            .with_code("E2501")
            .with_details(details));
    }
    if args.json || args.output.is_none() {
        println!("{json}");
    } else {
        println!("Checked interface compatibility: {}", if report.compatible { "compatible" } else { "breaking" });
    }
    if !report.compatible {
        return Err(CompileError::without_span("independently checked interface contracts are incompatible").with_code("E2501"));
    }
    Ok(())
}
