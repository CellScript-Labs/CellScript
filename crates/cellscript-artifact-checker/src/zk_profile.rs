//! Versioned compiler/builder contract. Cryptographic bytes are in `zk`.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZkSourceOrigin {
    pub policy: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
    /// Implicit transaction reads are owned by this exact source call/span.
    pub fields: Vec<String>,
}

pub fn statement_origins() -> Vec<String> {
    [
        "domain=argument[6]",
        "action=argument[7]",
        "script_hash=ckb.current_script_hash",
        "old_data_hash=GroupInput[0].data_hash",
        "new_data_hash=GroupOutput[0].data_hash",
        "input_transaction_hash=GroupInput[0].previous_output.tx_hash",
        "input_output_index=GroupInput[0].previous_output.index",
        "transaction_hash=ckb.raw_transaction_hash",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZkVerifierContract {
    pub schema: String,
    pub entry: String,
    pub policy: String,
    pub profile: String,
    pub profile_hash: String,
    pub exact_handle_hash: String,
    pub verification_key_hash: String,
    pub domain: String,
    pub action: String,
    pub proof_parameter: u32,
    pub dependency_local: u32,
    pub proof_bytes: u32,
    pub request_bytes: u32,
    pub public_input_count: u32,
    pub max_calls: u32,
    pub max_cycles: u64,
    pub witness_placement: String,
    pub context_binding: String,
    pub source: ZkSourceOrigin,
}

/// Reconstruct from typed operands, independently of compiler metadata.
pub fn contracts(typed: &crate::TypedSemanticRecord) -> Result<Vec<ZkVerifierContract>, String> {
    use crate::TypedSemanticConstant as C;
    let mut records = Vec::new();
    for entry in &typed.entries {
        for operation in entry.blocks.iter().flat_map(|block| &block.operations) {
            if operation.call.as_ref().is_none_or(|call| call.target != "__zk_require_transition") {
                continue;
            }
            let args = &operation.operands;
            if args.len() != 7 {
                return Err("ZK argument count".into());
            }
            let hash = |index: usize| match &args[index].constant {
                Some(C::Hash(value)) if value.len() == 64 => Ok(value.clone()),
                _ => Err("ZK constant identity missing".to_string()),
            };
            let Some(C::Array(bytes)) = &args[0].constant else {
                return Err("ZK policy name missing".into());
            };
            let policy: String = bytes
                .iter()
                .map(|byte| match byte {
                    C::U8(value) => value.parse::<u8>().map(char::from).map_err(|_| "ZK policy byte".to_string()),
                    _ => Err("ZK policy encoding".into()),
                })
                .collect::<Result<_, _>>()?;
            if policy.is_empty() || policy.len() > 64 || !policy.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err("ZK policy name".into());
            }
            let [source] = entry.zk_origins.as_slice() else {
                return Err("missing unique ZK source origin".into());
            };
            if source.policy != policy || source.end <= source.start || source.line == 0 || source.fields != statement_origins() {
                return Err("ZK source origin differs from registered statement".into());
            }
            records.push(ZkVerifierContract {
                source: source.clone(),
                schema: "cellscript-zk-verifier-contract-v2".into(),
                entry: entry.id.clone(),
                policy,
                profile: crate::zk::PROFILE.into(),
                profile_hash: crate::hex_encode(&crate::zk::PROFILE_ID),
                exact_handle_hash: hash(3)?,
                verification_key_hash: hash(4)?,
                domain: hash(5)?,
                action: hash(6)?,
                proof_parameter: args[1].local.ok_or("ZK proof local missing")?,
                dependency_local: args[2].local.ok_or("ZK dependency local missing")?,
                proof_bytes: 128,
                request_bytes: 464,
                public_input_count: 15,
                max_calls: 1,
                max_cycles: 250_000_000,
                witness_placement: "entry-WitnessArgs.input_type".into(),
                context_binding: "single-Type-group-1-to-1;script;data-hashes;input-outpoint;full-raw-transaction-hash".into(),
            });
        }
    }
    Ok(records)
}
