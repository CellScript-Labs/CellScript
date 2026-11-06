//! Content identities shared by the archive producer and offline VM replay.
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use serde_json::{json, Value};

pub const SCHEMA: &str = "cellscript-finance-vm-pilot-v2";
pub const CELL_DOMAIN: &str = "cellscript-finance-archive-cell-v1";
pub const CASE_DOMAIN: &str = "cellscript-finance-archive-case-v1";
pub const ARCHIVE_DOMAIN: &str = "cellscript-finance-archive-v1";
pub const OBSERVATION_DOMAIN: &str = "cellscript-finance-archive-local-observation-v1";

pub fn hash(bytes: &[u8]) -> String {
    hex::encode(cellscript_artifact_checker::ckb_blake2b256(bytes))
}

pub fn identity(domain: &str, value: &Value) -> Result<String, String> {
    cellscript_artifact_checker::canonical_hash(domain, value).map_err(|error| error.to_string())
}

pub fn cell(point: &packed::OutPoint, output: &packed::CellOutput, data: &Bytes) -> Value {
    let content = json!({"out_point":hex::encode(point.as_slice()),"output":hex::encode(output.as_slice()),"data":hex::encode(data)});
    let mut result = content.clone();
    result["output_hash"] = json!(hash(output.as_slice()));
    result["data_hash"] = json!(hex::encode(packed::CellOutput::calc_data_hash(data).as_slice()));
    result["content_hash"] = json!(identity(CELL_DOMAIN, &content).unwrap());
    result
}

pub fn case_identity(case: &Value) -> Result<String, String> {
    let mut content = case.clone();
    content.as_object_mut().ok_or_else(|| "case is not an object".to_string())?.remove("case_id");
    identity(CASE_DOMAIN, &content)
}

pub fn archive_identity(report: &Value) -> Result<String, String> {
    let mut content = report.clone();
    content.as_object_mut().ok_or_else(|| "report is not an object".to_string())?.remove("archive_id");
    identity(ARCHIVE_DOMAIN, &content)
}

pub fn observation_identity(observation: &Value) -> Result<String, String> {
    let mut content = observation.clone();
    content.as_object_mut().ok_or_else(|| "observation is not an object".to_string())?.remove("observation_id");
    identity(OBSERVATION_DOMAIN, &content)
}
