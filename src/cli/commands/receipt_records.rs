use super::analysis_reports::protocol_graph_json;
use super::{hash_json_value, metadata_schema_versions_json, CompileReceiptVerificationReport};
use crate::error::Result;
use crate::CompileMetadata;
use base64::Engine;
use std::path::Path;

pub(super) fn compile_receipt_json(metadata: &CompileMetadata) -> Result<serde_json::Value> {
    let proof_plan_hash = hash_json_value("proof_plan", &metadata.runtime.proof_plan)?;
    let protocol_graph = protocol_graph_json(metadata);
    let protocol_graph_hash = hash_json_value("protocol_graph", &protocol_graph)?;
    let template_layout_hash = if metadata.template_layouts.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(hash_json_value("template_layouts", &metadata.template_layouts)?)
    };
    Ok(serde_json::json!({
        "schema": "cellscript-compile-receipt-v4",
        "compiler_version": metadata.compiler_version,
        "edition": metadata.edition,
        "compatibility_profile": metadata.compatibility_profile,
        "compatibility_profile_hash": hash_json_value("compatibility_profile", &metadata.compatibility_profile)?,
        "rust_toolchain": cellscript_rust_toolchain(),
        "target": metadata.artifact_format,
        "target_profile": metadata.target_profile.name,
        "source_hash": metadata.source_hash,
        "source_content_hash": metadata.source_content_hash,
        "ast_normalised_hash": serde_json::Value::Null,
        "ir_normalised_hash": serde_json::Value::Null,
        "normalisation_status": "ast-ir-normalised-hashes-not-yet-emitted",
        "proof_plan_hash": proof_plan_hash,
        "protocol_graph_hash": protocol_graph_hash,
        "template_layout_hash": template_layout_hash,
        "artifact_hash": metadata.artifact_hash,
        "metadata_hash": hash_json_value("metadata", metadata)?,
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": metadata_schema_versions_json(metadata),
        "signatures": [],
    }))
}

pub(super) fn verify_compile_receipt_against_metadata(
    receipt: &serde_json::Value,
    metadata: &CompileMetadata,
) -> Result<CompileReceiptVerificationReport> {
    validate_compile_receipt_schema(receipt)?;
    let expected = compile_receipt_json(metadata)?;
    for pointer in [
        "/compiler_version",
        "/edition",
        "/compatibility_profile",
        "/compatibility_profile_hash",
        "/rust_toolchain",
        "/target",
        "/target_profile",
        "/source_hash",
        "/source_content_hash",
        "/ast_normalised_hash",
        "/ir_normalised_hash",
        "/normalisation_status",
        "/proof_plan_hash",
        "/protocol_graph_hash",
        "/template_layout_hash",
        "/artifact_hash",
        "/metadata_hash",
        "/metadata_schema_version",
        "/metadata_schema_versions",
    ] {
        if receipt.pointer(pointer) != expected.pointer(pointer) {
            return Err(crate::error::CompileError::without_span(format!(
                "compile receipt field '{}' does not match validated metadata/artifact evidence",
                pointer.trim_start_matches('/')
            )));
        }
    }
    verify_compile_receipt_signatures(receipt)
}

pub(super) fn validate_compile_receipt_schema(receipt: &serde_json::Value) -> Result<()> {
    match receipt.get("schema").and_then(serde_json::Value::as_str) {
        Some("cellscript-compile-receipt-v4") => Ok(()),
        Some(schema) => Err(crate::error::CompileError::without_span(format!(
            "unsupported compile receipt schema '{}'; expected cellscript-compile-receipt-v4",
            schema
        ))),
        None => Err(crate::error::CompileError::without_span("compile receipt is missing schema")),
    }
}

pub(super) fn compile_receipt_payload_hash(receipt: &serde_json::Value) -> Result<String> {
    let mut unsigned = receipt.clone();
    let object = unsigned
        .as_object_mut()
        .ok_or_else(|| crate::error::CompileError::without_span("compile receipt must be a JSON object before it can be hashed"))?;
    object.remove("signatures");
    hash_json_value("compile receipt signing payload", &unsigned)
}

fn verify_compile_receipt_signatures(receipt: &serde_json::Value) -> Result<CompileReceiptVerificationReport> {
    let payload_hash = compile_receipt_payload_hash(receipt)?;
    let signatures = match receipt.get("signatures") {
        None => return Ok(CompileReceiptVerificationReport { payload_hash, signatures_verified: 0, unsigned_advisory: true }),
        Some(serde_json::Value::Array(signatures)) => signatures,
        Some(_) => {
            return Err(crate::error::CompileError::without_span("compile receipt signatures field must be an array when present"));
        }
    };
    if signatures.is_empty() {
        return Ok(CompileReceiptVerificationReport { payload_hash, signatures_verified: 0, unsigned_advisory: true });
    }

    for (index, signature) in signatures.iter().enumerate() {
        let prefix = format!("compile receipt signatures[{index}]");
        let algorithm = signature
            .get("algorithm")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("{prefix}.algorithm is required")))?;
        if algorithm != "ed25519" {
            return Err(crate::error::CompileError::without_span(format!(
                "{prefix}.algorithm '{}' is unsupported; expected ed25519",
                algorithm
            )));
        }
        let signature_payload_hash = signature
            .get("payload_hash")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("{prefix}.payload_hash is required")))?;
        if signature_payload_hash != payload_hash {
            return Err(crate::error::CompileError::without_span(format!(
                "{prefix}.payload_hash does not match current compile receipt payload hash"
            )));
        }
        let public_key = decode_prefixed_base64_field(signature, "public_key", "ed25519-pk:", &prefix)?;
        let signature_bytes = decode_prefixed_base64_field(signature, "signature", "ed25519-sig:", &prefix)?;
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key)
            .verify(payload_hash.as_bytes(), &signature_bytes)
            .map_err(|_| crate::error::CompileError::without_span(format!("{prefix}.signature failed Ed25519 verification")))?;
    }

    Ok(CompileReceiptVerificationReport { payload_hash, signatures_verified: signatures.len(), unsigned_advisory: false })
}

fn decode_prefixed_base64_field(value: &serde_json::Value, field: &str, prefix: &str, diagnostic_prefix: &str) -> Result<Vec<u8>> {
    let text = value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| crate::error::CompileError::without_span(format!("{diagnostic_prefix}.{field} is required")))?;
    let encoded = text
        .strip_prefix(prefix)
        .ok_or_else(|| crate::error::CompileError::without_span(format!("{diagnostic_prefix}.{field} must start with '{prefix}'")))?;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded).map_err(|error| {
        crate::error::CompileError::without_span(format!("{diagnostic_prefix}.{field} has invalid base64url payload: {error}"))
    })
}

pub(super) fn read_ed25519_pkcs8_key_arg(key: &str) -> Result<Vec<u8>> {
    let key = key.trim();
    if key.is_empty() {
        return Err(crate::error::CompileError::without_span("receipt signing key must be non-empty"));
    }
    let path = key.strip_prefix('@').unwrap_or(key);
    if Path::new(path).exists() {
        return std::fs::read(path)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to read Ed25519 key '{}': {}", path, error)));
    }
    let encoded = key.strip_prefix("ed25519-pkcs8:").or_else(|| key.strip_prefix("base64:")).unwrap_or(key);
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded))
        .map_err(|error| {
            crate::error::CompileError::without_span(format!(
                "receipt signing key must be a file path or base64 Ed25519 PKCS#8 DER: {}",
                error
            ))
        })
}

pub(super) fn validate_receipt_signature_role(role: &str) -> Result<&str> {
    match role {
        "compiler" | "publisher" => Ok(role),
        other => Err(crate::error::CompileError::without_span(format!(
            "unsupported receipt signature role '{}'; expected compiler or publisher",
            other
        ))),
    }
}

fn cellscript_rust_toolchain() -> String {
    include_str!("../../../Cargo.toml")
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("rust-version")
                .and_then(|rest| rest.split_once('='))
                .map(|(_, value)| value.trim().trim_matches('"').to_string())
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}
