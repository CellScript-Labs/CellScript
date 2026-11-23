use super::super::output::{read_json_value, CommandOutcome};
use super::receipt_records::{
    compile_receipt_json, compile_receipt_payload_hash, read_ed25519_pkcs8_key_arg, validate_compile_receipt_schema,
    validate_receipt_signature_role, verify_compile_receipt_against_metadata,
};
use super::{CommandExecutor, ReceiptArgs, SignReceiptArgs, VerifyReceiptArgs};
use crate::error::Result;
use crate::{compile_path, resolve_input_path, validate_artifact_metadata, CompileMetadata, CompileOptions};
use base64::Engine;
use camino::Utf8Path;
use colored::Colorize;
use ring::signature::KeyPair;
use std::path::PathBuf;

impl CommandExecutor {
    pub(super) fn receipt(args: ReceiptArgs) -> Result<()> {
        let input_path = args.input.clone().unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let input_path = resolve_input_path(input)?;
        let compile_result = compile_path(
            &input_path,
            CompileOptions {
                edition: crate::CURRENT_EDITION,
                target: args.target.clone(),
                target_profile: args.target_profile.clone(),
                ..CompileOptions::default()
            },
        )?;
        let receipt = compile_receipt_json(&compile_result.metadata)?;
        if let Some(parent) = args.output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &args.output,
            serde_json::to_vec_pretty(&receipt).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize compile receipt: {}", error))
            })?,
        )?;
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "receipt": args.output.display().to_string(),
                "schema": receipt["schema"],
                "artifact_hash": receipt["artifact_hash"],
                "metadata_hash": receipt["metadata_hash"],
                "signature_count": 0,
                "unsigned_advisory": true,
            }),
            human_lines: vec![
                "Compile receipt written".green().to_string(),
                format!("  Receipt: {}", args.output.display()),
                format!("  Artifact hash: {}", receipt["artifact_hash"].as_str().unwrap_or("missing")),
                format!("  Metadata hash: {}", receipt["metadata_hash"].as_str().unwrap_or("missing")),
                "  Signatures: unsigned advisory".to_string(),
            ],
        }
        .emit(args.json)
    }

    pub(super) fn sign_receipt(args: SignReceiptArgs) -> Result<()> {
        let mut receipt = read_json_value(&args.receipt)?;
        validate_compile_receipt_schema(&receipt)?;
        let payload_hash = compile_receipt_payload_hash(&receipt)?;
        let key_bytes = read_ed25519_pkcs8_key_arg(&args.key)?;
        let key_pair = ring::signature::Ed25519KeyPair::from_pkcs8(&key_bytes)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to load Ed25519 private key: {:?}", error)))?;
        let signature = key_pair.sign(payload_hash.as_bytes());
        let signature_entry = serde_json::json!({
            "role": validate_receipt_signature_role(&args.role)?,
            "algorithm": "ed25519",
            "public_key": format!("ed25519-pk:{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key_pair.public_key().as_ref())),
            "payload_hash": payload_hash,
            "signature": format!("ed25519-sig:{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.as_ref())),
        });
        let object = receipt.as_object_mut().ok_or_else(|| {
            crate::error::CompileError::without_span("compile receipt must be a JSON object before it can be signed")
        })?;
        match object.get_mut("signatures") {
            Some(serde_json::Value::Array(signatures)) => signatures.push(signature_entry.clone()),
            Some(_) => {
                return Err(crate::error::CompileError::without_span(
                    "compile receipt signatures field must be an array before it can be signed",
                ));
            }
            None => {
                object.insert("signatures".to_string(), serde_json::json!([signature_entry.clone()]));
            }
        }

        let output_path = args.output.unwrap_or_else(|| args.receipt.clone());
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &output_path,
            serde_json::to_vec_pretty(&receipt)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize signed receipt: {}", error)))?,
        )?;

        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "receipt": output_path.display().to_string(),
                "role": signature_entry["role"],
                "algorithm": signature_entry["algorithm"],
                "public_key": signature_entry["public_key"],
                "payload_hash": signature_entry["payload_hash"],
            }),
            human_lines: vec![
                "Compile receipt signed".green().to_string(),
                format!("  Receipt: {}", output_path.display()),
                format!("  Role: {}", signature_entry["role"].as_str().unwrap_or("unknown")),
                format!("  Payload hash: {}", signature_entry["payload_hash"].as_str().unwrap_or("missing")),
            ],
        }
        .emit(args.json)
    }

    pub(super) fn verify_receipt(args: VerifyReceiptArgs) -> Result<()> {
        let receipt = read_json_value(&args.receipt)?;
        let artifact_bytes = std::fs::read(&args.artifact).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to read artifact '{}': {}", args.artifact.display(), error))
        })?;
        let metadata_bytes = std::fs::read(&args.metadata).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to read metadata '{}': {}", args.metadata.display(), error))
        })?;
        let metadata: CompileMetadata = serde_json::from_slice(&metadata_bytes).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to parse metadata '{}': {}", args.metadata.display(), error))
        })?;
        let validated = validate_artifact_metadata(artifact_bytes, metadata)?;
        let report = verify_compile_receipt_against_metadata(&receipt, &validated.metadata)?;

        let signature_line = if report.unsigned_advisory {
            "  Signatures: unsigned advisory".to_string()
        } else {
            format!("  Signatures verified: {}", report.signatures_verified)
        };
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "receipt": args.receipt.display().to_string(),
                "metadata": args.metadata.display().to_string(),
                "artifact": args.artifact.display().to_string(),
                "payload_hash": report.payload_hash,
                "signatures_verified": report.signatures_verified,
                "unsigned_advisory": report.unsigned_advisory,
            }),
            human_lines: vec![
                "Compile receipt verification succeeded".green().to_string(),
                format!("  Receipt: {}", args.receipt.display()),
                format!("  Artifact: {}", args.artifact.display()),
                format!("  Metadata: {}", args.metadata.display()),
                format!("  Payload hash: {}", report.payload_hash),
                signature_line,
            ],
        }
        .emit(args.json)
    }
}
