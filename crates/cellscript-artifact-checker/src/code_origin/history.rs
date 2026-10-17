//! Bounded Type-ID code lineage from canonical transaction bytes. A consumer's
//! committed policy authorizes the resulting history identity; this checker
//! does not infer signatures, inclusion, confirmations or live Cell status.
use super::{invalid, molecule, CheckedTargetCodeCellOrigin};
use crate::{canonical_bytes, canonical_hash, ckb_blake2b256, hex_encode, CheckerBudgets, CheckerError, CheckerRejectionCode};
use serde::Serialize;
use std::collections::BTreeSet;

pub struct TypeCodeHistoryStep<'a> {
    pub raw_transaction: &'a [u8],
    pub output_index: u32,
}

#[derive(Debug)]
pub struct CheckedTypeCodeHistory {
    target: String,
    line: [u8; 32],
    tip: [u8; 32],
    sequence: u64,
    record: Record,
}

#[derive(Debug, Clone, Serialize)]
struct Record {
    schema: &'static str,
    type_script_hash: String,
    custody_lock_hash: String,
    steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
struct Step {
    transaction_hash: String,
    output_index: u32,
    artifact_hash: String,
}

impl CheckedTypeCodeHistory {
    pub fn target_identity(&self) -> &str {
        &self.target
    }
    pub fn deployment_line(&self) -> [u8; 32] {
        self.line
    }
    pub fn history_tip(&self) -> [u8; 32] {
        self.tip
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        canonical_bytes(&self.record)
    }

    /// Carry an already checked code lineage to another independently checked
    /// Script instance of that exact code output. Only the selected instance
    /// may differ; this cannot retarget the history to a replacement code Cell.
    /// Instance authorization is supplied separately by the caller's policy.
    pub fn bind_instance(
        &self,
        original: &CheckedTargetCodeCellOrigin,
        instance: &CheckedTargetCodeCellOrigin,
    ) -> Result<Self, CheckerError> {
        let old = original.origin();
        let new = instance.origin();
        if self.target != original.identity()
            || original.runtime_contract() != instance.runtime_contract()
            || old.selected_hash_type() != 1
            || new.selected_hash_type() != 1
            || old.transaction_hash() != new.transaction_hash()
            || old.output_index() != new.output_index()
            || old.artifact_hash() != new.artifact_hash()
            || old.type_script_hash() != new.type_script_hash()
            || new.type_script_hash() != Some(hex_encode(&self.line).as_str())
        {
            return Err(invalid("instance lineage differs from its original checked code, target, Type identity or OutPoint"));
        }
        Ok(Self {
            target: instance.identity().into(),
            line: self.line,
            tip: self.tip,
            sequence: self.sequence,
            record: self.record.clone(),
        })
    }
}

/// Independently validate 1..=32 creation/replacement steps with <=4 MiB per
/// transaction and <=16 MiB shared, before parsing. This profile preserves a
/// built-in Type-ID Script and its custody Lock through every replacement.
/// Every successor consumes the exact preceding OutPoint; the last output is
/// the actual checked artifact's creation output. Arbitrary Type Scripts,
/// truncated histories, forks, repeated transactions and changed custody reject.
/// Policy admission must commit the resulting line/tip/sequence explicitly.
pub fn check_type_code_history(
    target: &CheckedTargetCodeCellOrigin,
    steps: &[TypeCodeHistoryStep<'_>],
    budgets: &CheckerBudgets,
) -> Result<CheckedTypeCodeHistory, CheckerError> {
    if steps.is_empty() || steps.len() > 32 {
        return Err(invalid("Type-hash history requires 1..=32 actual creation/replacement transactions"));
    }
    let mut total = 0usize;
    for step in steps {
        total = total.checked_add(step.raw_transaction.len()).ok_or_else(|| invalid("Type-hash history size overflow"))?;
        if step.raw_transaction.len() > 4 * 1024 * 1024
            || step.raw_transaction.len() as u64 > budgets.record_bytes
            || total > 16 * 1024 * 1024
        {
            return Err(CheckerError::new(CheckerRejectionCode::V2400BudgetExceeded, "Type-hash history byte budget exceeded"));
        }
    }
    let origin = target.origin();
    if origin.selected_hash_type() != 1 || target.runtime_contract().target_profile != "ckb-type-hash" {
        return Err(invalid("Type-hash history requires an independently checked ckb-type-hash artifact"));
    }
    let mut type_script = Vec::new();
    let mut custody = Vec::new();
    let mut previous = [0u8; 36];
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    for (sequence, step) in steps.iter().enumerate() {
        let tx = molecule::transaction(step.raw_transaction)?;
        let output = tx.cells.get(step.output_index as usize).ok_or_else(|| invalid("Type-hash history output is absent"))?;
        let script = output.type_script.as_ref().ok_or_else(|| invalid("Type-hash history output has no Type Script"))?;
        let tx_hash = ckb_blake2b256(step.raw_transaction);
        if !seen.insert(tx_hash) {
            return Err(invalid("Type-hash history repeats a transaction"));
        }
        if sequence == 0 {
            let mut type_id = [0u8; 32];
            type_id[25..].copy_from_slice(b"TYPE_ID");
            if script.code_hash != type_id || script.hash_type != 1 || script.args.len() != 32 {
                return Err(invalid("Type-hash history requires the built-in TYPE_ID Script with 32-byte args"));
            }
            let first = tx.inputs.get(..44).ok_or_else(|| invalid("Type-ID creation requires its actual first CellInput"))?;
            let mut seed = [0u8; 52];
            seed[..44].copy_from_slice(first);
            seed[44..].copy_from_slice(&u64::from(step.output_index).to_le_bytes());
            if script.args != ckb_blake2b256(&seed) {
                return Err(invalid("Type-ID genesis args differ from the first CellInput and output index"));
            }
            type_script.extend_from_slice(script.bytes);
            custody.extend_from_slice(output.lock.bytes);
        } else if script.bytes != type_script
            || output.lock.bytes != custody
            || tx.inputs.chunks_exact(44).filter(|input| input[8..] == previous).count() != 1
        {
            return Err(invalid("Type-hash successor changes identity/custody or does not consume its exact predecessor"));
        }
        if tx.cells.iter().filter(|cell| cell.type_script.as_ref().is_some_and(|s| s.bytes == type_script)).count() != 1 {
            return Err(invalid("Type-hash history must create exactly one output in the selected Type-ID group"));
        }
        previous[..32].copy_from_slice(&tx_hash);
        previous[32..].copy_from_slice(&step.output_index.to_le_bytes());
        records.push(Step {
            transaction_hash: hex_encode(&tx_hash),
            output_index: step.output_index,
            artifact_hash: hex_encode(&ckb_blake2b256(tx.data[step.output_index as usize])),
        });
    }
    let last = records.last().expect("bounded nonempty history");
    let line = ckb_blake2b256(&type_script);
    if last.transaction_hash != origin.transaction_hash()
        || last.output_index != origin.output_index()
        || last.artifact_hash != origin.artifact_hash()
        || origin.type_script_hash() != Some(hex_encode(&line).as_str())
    {
        return Err(invalid("Type-hash history tip differs from the independently checked code output"));
    }
    let sequence = (records.len() - 1) as u64;
    let record = Record {
        schema: "cellscript-type-code-history-v1",
        type_script_hash: hex_encode(&line),
        custody_lock_hash: hex_encode(&ckb_blake2b256(&custody)),
        steps: records,
    };
    let digest = canonical_hash("cellscript-type-code-history-id-v1", &record)?;
    let mut tip = [0u8; 32];
    for (slot, pair) in tip.iter_mut().zip(digest.as_bytes().chunks_exact(2)) {
        let nibble = |byte: u8| if byte <= b'9' { byte - b'0' } else { byte - b'a' + 10 };
        *slot = nibble(pair[0]) * 16 + nibble(pair[1]);
    }
    Ok(CheckedTypeCodeHistory { target: target.identity().into(), line, tip, sequence, record })
}
