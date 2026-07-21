use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptIdentity {
    pub code_hash: String,
    pub hash_type: String,
    pub args: String,
}

impl ScriptIdentity {
    pub fn canonicalized(mut self) -> anyhow::Result<Self> {
        self.code_hash = canonical_hex(&self.code_hash, Some(32), "script.code_hash")?;
        self.args = canonical_hex(&self.args, None, "script.args")?;
        self.hash_type = canonical_hash_type(&self.hash_type)?.to_string();
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiberAssetDescriptor {
    pub schema: String,
    pub contract: String,
    pub module: String,
    pub display_name: String,
    pub selected_type: String,
    pub selected_invariant: String,
    pub selected_field: String,
    pub compiler_version: String,
    pub metadata_schema_version: u32,
    pub source_hash: String,
    pub artifact_hash: String,
    pub artifact_format: String,
    pub target_profile: String,
    pub data_length_bytes: usize,
    pub amount_offset_bytes: usize,
    pub amount_width_bytes: usize,
    pub endianness: String,
    pub arithmetic: String,
    pub group_scope: String,
    pub owner_mode: String,
    pub owner_args_length_bytes: usize,
    pub authority_modes: Vec<String>,
    pub authority_args_lengths_bytes: Vec<usize>,
    pub owner_authorized_mint: bool,
    pub owner_authorized_burn: bool,
    pub non_owner_input_group_non_empty: bool,
    pub non_owner_output_group_non_empty: bool,
    pub non_owner_conservation_required: bool,
    pub payload_required: bool,
    pub witness_policy: String,
    pub runtime_helper: String,
}

pub(crate) fn canonical_hash_type(value: &str) -> anyhow::Result<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "data" => Ok("data"),
        "type" => Ok("type"),
        "data1" => Ok("data1"),
        "data2" => Ok("data2"),
        _ => anyhow::bail!("unsupported CKB Script hash_type '{value}'"),
    }
}

pub(crate) fn canonical_hex(value: &str, expected_bytes: Option<usize>, field: &str) -> anyhow::Result<String> {
    let raw = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .ok_or_else(|| anyhow::anyhow!("{field} must be 0x-prefixed hex"))?;
    if raw.len() % 2 != 0 || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        anyhow::bail!("{field} must contain an even number of hexadecimal digits");
    }
    if let Some(expected_bytes) = expected_bytes
        && raw.len() != expected_bytes * 2
    {
        anyhow::bail!("{field} must be {expected_bytes} bytes, got {}", raw.len() / 2);
    }
    Ok(format!("0x{}", raw.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_identity_is_canonical_and_closed() {
        let identity =
            ScriptIdentity { code_hash: format!("0x{}", "AA".repeat(32)), hash_type: "Data2".to_string(), args: "0xBEEF".to_string() }
                .canonicalized()
                .unwrap();
        assert_eq!(identity.code_hash, format!("0x{}", "aa".repeat(32)));
        assert_eq!(identity.hash_type, "data2");
        assert_eq!(identity.args, "0xbeef");
    }
}
