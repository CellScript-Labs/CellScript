//! Hashing helpers shared by migrated evidence generators.

use anyhow::{bail, Context, Result};
use blake2b_ref::Blake2bBuilder;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::shared::python_json_compact;

pub fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

pub fn decode_hex0x(value: &str) -> Result<Vec<u8>> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value)).with_context(|| format!("invalid hexadecimal value: {value}"))
}

pub fn bytes32(value: &str) -> Result<[u8; 32]> {
    let bytes = decode_hex0x(value)?;
    bytes.try_into().map_err(|bytes: Vec<u8>| anyhow::anyhow!("expected Byte32, got {} bytes", bytes.len()))
}

pub fn personalized_blake2b256(personalization: &[u8], chunks: &[&[u8]]) -> Result<[u8; 32]> {
    if personalization.len() > 16 {
        bail!("BLAKE2b personalization exceeds 16 bytes");
    }
    let mut state = Blake2bBuilder::new(32).personal(personalization).build();
    for chunk in chunks {
        state.update(chunk);
    }
    let mut digest = [0_u8; 32];
    state.finalize(&mut digest);
    Ok(digest)
}

pub fn ckb_blake2b256(bytes: &[u8]) -> Result<[u8; 32]> {
    personalized_blake2b256(b"ckb-default-hash", &[bytes])
}

pub fn canonical_report_hash(personalization: &[u8], label: &str, value: &Value) -> Result<String> {
    let canonical = python_json_compact(value)?;
    let digest = personalized_blake2b256(personalization, &[label.as_bytes(), b"\0", canonical.as_bytes()])?;
    Ok(hex0x(&digest))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn nonzero_hex32(value: &Value) -> bool {
    let Some(value) = value.as_str() else {
        return false;
    };
    let Some(raw) = value.strip_prefix("0x") else {
        return false;
    };
    if raw.len() != 64 {
        return false;
    }
    hex::decode(raw).is_ok_and(|bytes| bytes.iter().any(|byte| *byte != 0))
}
