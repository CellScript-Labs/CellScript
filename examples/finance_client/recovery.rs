//! Application-owned economic checks around the public ProtocolBundle adapter.
//!
//! This example validates resolved transaction Cells, never a caller's quote.
//! It does not replace the executing financial Scripts or node verification.

use anyhow::{bail, Result};
use cellscript_ckb_adapter::{
    recovery::{ProtocolBundleRecovery, RecoveryCandidate, RecoveryIntent, RecoveryPolicy, RecoverySubject},
    ProtocolBundleLiveCellObservation,
};
use ckb_hash::blake2b_256;
use ckb_types::{
    bytes::Bytes,
    core::TransactionView,
    packed::{CellOutput, Script},
    prelude::*,
    H256,
};

#[path = "journal.rs"]
mod journal;
pub use journal::{FinanceJournal, JournalReference};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountEncoding {
    U64Le,
    U128Le,
    PoolShares { pool: [u8; 32], controller_code: [u8; 32] },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetAmountBound {
    pub asset: Script,
    pub encoding: AmountEncoding,
    pub amount: u128,
}

#[derive(Debug, Clone)]
pub struct FinanceIntent {
    operation_id: H256,
    operation: String,
    owner: Script,
    recipient: Script,
    debits: Vec<AssetAmountBound>,
    credits: Vec<AssetAmountBound>,
    subjects: Vec<RecoverySubject>,
    maximum_fee_shannons: u64,
    maximum_capacity_debit_shannons: u64,
    maximum_occupied_capacity_shannons: u64,
}

pub struct FinanceIntentParameters {
    pub operation_id: H256,
    pub operation: String,
    pub owner: Script,
    pub recipient: Script,
    pub debits: Vec<AssetAmountBound>,
    pub credits: Vec<AssetAmountBound>,
    pub subjects: Vec<RecoverySubject>,
    pub maximum_fee_shannons: u64,
    pub maximum_capacity_debit_shannons: u64,
    pub maximum_occupied_capacity_shannons: u64,
}

impl FinanceIntent {
    pub fn new(parameters: FinanceIntentParameters) -> Result<Self> {
        if parameters.operation_id == H256::from([0; 32])
            || parameters.operation.is_empty()
            || (parameters.debits.is_empty() && parameters.credits.is_empty())
        {
            bail!("financial intent requires an operation and at least one explicit debit or credit asset limit");
        }
        for bounds in [&parameters.debits, &parameters.credits] {
            for (index, bound) in bounds.iter().enumerate() {
                if bounds[..index].iter().any(|previous| previous.asset == bound.asset) {
                    bail!("financial intent has duplicate asset limits");
                }
                if let AmountEncoding::PoolShares { pool, controller_code } = bound.encoding {
                    let tail = [pool.as_slice(), controller_code.as_slice()].concat();
                    if bound.asset.args().raw_data().as_ref() != blake2b_256(&tail).as_slice() {
                        bail!("LP asset Script arguments do not commit to its exact pool/controller configuration");
                    }
                }
            }
        }
        if parameters
            .credits
            .iter()
            .any(|credit| parameters.debits.iter().any(|debit| debit.asset == credit.asset && debit.encoding != credit.encoding))
        {
            bail!("financial intent gives one asset conflicting amount profiles");
        }
        Ok(Self {
            operation_id: parameters.operation_id,
            operation: parameters.operation,
            owner: parameters.owner,
            recipient: parameters.recipient,
            debits: parameters.debits,
            credits: parameters.credits,
            subjects: parameters.subjects,
            maximum_fee_shannons: parameters.maximum_fee_shannons,
            maximum_capacity_debit_shannons: parameters.maximum_capacity_debit_shannons,
            maximum_occupied_capacity_shannons: parameters.maximum_occupied_capacity_shannons,
        })
    }

    /// A local intent commitment, not a transaction serializer or signature.
    pub fn recovery_intent(&self, chain_id: String, genesis_hash: H256) -> Result<RecoveryIntent> {
        let mut bytes = b"cellscript-finance-client-intent-v1\0".to_vec();
        bytes.extend_from_slice(self.operation_id.as_bytes());
        let append = |bytes: &mut Vec<u8>, part: &[u8]| {
            bytes.extend_from_slice(&(part.len() as u64).to_le_bytes());
            bytes.extend_from_slice(part);
        };
        append(&mut bytes, self.operation.as_bytes());
        append(&mut bytes, self.owner.as_slice());
        append(&mut bytes, self.recipient.as_slice());
        for bounds in [&self.debits, &self.credits] {
            bytes.extend_from_slice(&(bounds.len() as u64).to_le_bytes());
            for bound in bounds {
                append(&mut bytes, bound.asset.as_slice());
                bytes.push(match bound.encoding {
                    AmountEncoding::U64Le => 8,
                    AmountEncoding::U128Le => 16,
                    AmountEncoding::PoolShares { .. } => 80,
                });
                if let AmountEncoding::PoolShares { pool, controller_code } = bound.encoding {
                    bytes.extend_from_slice(&pool);
                    bytes.extend_from_slice(&controller_code);
                }
                bytes.extend_from_slice(&bound.amount.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&(self.subjects.len() as u64).to_le_bytes());
        for subject in &self.subjects {
            let (tag, script) = match subject {
                RecoverySubject::Lock(script) => (0, script),
                RecoverySubject::Type(script) => (1, script),
            };
            bytes.push(tag);
            append(&mut bytes, script.as_slice());
        }
        bytes.extend_from_slice(&self.maximum_fee_shannons.to_le_bytes());
        bytes.extend_from_slice(&self.maximum_capacity_debit_shannons.to_le_bytes());
        bytes.extend_from_slice(&self.maximum_occupied_capacity_shannons.to_le_bytes());
        RecoveryIntent::new(
            chain_id,
            genesis_hash,
            H256::from(blake2b_256(&bytes)),
            self.subjects.clone(),
            self.maximum_fee_shannons,
            self.maximum_occupied_capacity_shannons,
        )
    }

    pub fn verify_candidate(&self, transaction: &TransactionView, inputs: &[ProtocolBundleLiveCellObservation]) -> Result<()> {
        if transaction.inputs().len() != inputs.len() || transaction.outputs().len() != transaction.outputs_data().len() {
            bail!("financial candidate must resolve every input and output data exactly once");
        }
        for (input, resolved) in transaction.inputs().into_iter().zip(inputs) {
            if input.previous_output() != resolved.out_point {
                bail!("financial candidate resolved input belongs to another OutPoint");
            }
            if resolved.output.lock() == self.owner
                && let Some(asset) = resolved.output.type_().to_opt()
                && !self.debits.iter().any(|bound| bound.asset == asset)
            {
                bail!("financial candidate spends an undeclared owner asset; declare an explicit debit limit");
            }
        }
        let before = inputs.iter().map(|cell| (cell.output.clone(), cell.data.clone())).collect::<Vec<_>>();
        let after = transaction
            .outputs()
            .into_iter()
            .zip(transaction.outputs_data())
            .map(|(output, data)| (output, data.raw_data()))
            .collect::<Vec<_>>();
        for (output, data) in &after {
            if output.lock() != self.owner {
                continue;
            }
            let Some(asset) = output.type_().to_opt() else {
                continue;
            };
            let bound = self
                .debits
                .iter()
                .chain(&self.credits)
                .find(|bound| bound.asset == asset)
                .ok_or_else(|| anyhow::anyhow!("financial candidate credits owner capacity through an undeclared asset Type"))?;
            // A credit for another recipient still does not exempt an owner
            // output from the exact declared asset data codec.
            amount(&[(output.clone(), data.clone())], &self.owner, bound)?;
        }
        for bound in &self.debits {
            let before_amount = amount(&before, &self.owner, bound)?;
            let after_amount = amount(&after, &self.owner, bound)?;
            if before_amount.saturating_sub(after_amount) > bound.amount {
                bail!("financial candidate exceeds maximum spend");
            }
        }
        for bound in &self.credits {
            let before_amount = amount(&before, &self.recipient, bound)?;
            let after_amount = amount(&after, &self.recipient, bound)?;
            if after_amount.checked_sub(before_amount).is_none_or(|received| received < bound.amount) {
                bail!("financial candidate violates minimum received amount or recipient");
            }
        }
        let total = |cells: &[(CellOutput, Bytes)], lock: Option<&Script>| -> Result<u64> {
            cells.iter().filter(|(output, _)| lock.is_none_or(|lock| output.lock() == *lock)).try_fold(0u64, |sum, (output, _)| {
                sum.checked_add(output.capacity().unpack()).ok_or_else(|| anyhow::anyhow!("financial capacity total overflow"))
            })
        };
        let fee = total(&before, None)?
            .checked_sub(total(&after, None)?)
            .ok_or_else(|| anyhow::anyhow!("financial candidate creates capacity"))?;
        if fee > self.maximum_fee_shannons {
            bail!("financial candidate exceeds maximum fee");
        }
        if total(&before, Some(&self.owner))?.saturating_sub(total(&after, Some(&self.owner))?) > self.maximum_capacity_debit_shannons
        {
            bail!("financial candidate exceeds maximum owner capacity debit");
        }
        Ok(())
    }
}

fn amount(cells: &[(CellOutput, Bytes)], holder: &Script, bound: &AssetAmountBound) -> Result<u128> {
    cells.iter().filter(|(output, _)| output.lock() == *holder && output.type_().to_opt().as_ref() == Some(&bound.asset)).try_fold(
        0u128,
        |sum, (_, data)| {
            let value = match bound.encoding {
                AmountEncoding::U64Le => u128::from(u64::from_le_bytes(
                    data.as_ref().try_into().map_err(|_| anyhow::anyhow!("financial asset amount is not canonical u64"))?,
                )),
                AmountEncoding::U128Le => u128::from_le_bytes(
                    data.as_ref().try_into().map_err(|_| anyhow::anyhow!("financial asset amount is not canonical u128"))?,
                ),
                AmountEncoding::PoolShares { pool, controller_code } => {
                    if data.len() != 80 || data[16..48] != pool || data[48..80] != controller_code {
                        bail!("LP share data does not match the exact 80-byte pool/controller profile");
                    }
                    if bound.asset.args().raw_data().as_ref() != blake2b_256(&data[16..]).as_slice() {
                        bail!("LP share Script arguments do not commit to its complete configuration tail");
                    }
                    u128::from_le_bytes(data[..16].try_into().expect("checked LP amount width"))
                }
            };
            sum.checked_add(value).ok_or_else(|| anyhow::anyhow!("financial asset total overflow"))
        },
    )
}

/// Keep economic validation attached to every newly prepared/rebuilt attempt.
/// Subsequent signing/submission/observation methods use the adapter unchanged.
pub struct FinanceRecovery {
    intent: FinanceIntent,
    binding: RecoveryIntent,
    recovery: ProtocolBundleRecovery,
    inputs: Vec<ProtocolBundleLiveCellObservation>,
}

/// The journal-aware application entry point returns an observation-only type
/// for an already recorded operation. Do not invoke a signer in that branch.
pub enum FinanceRecoverySession {
    Fresh(Box<FinanceRecovery>),
    ObservationOnly(Box<cellscript_ckb_adapter::recovery::ObservationOnlyRecovery>),
}

impl FinanceRecovery {
    pub fn start_or_resume(
        intent: FinanceIntent,
        chain_id: String,
        genesis_hash: H256,
        policy: RecoveryPolicy,
        journal: &FinanceJournal,
    ) -> Result<FinanceRecoverySession> {
        let binding = intent.recovery_intent(chain_id.clone(), genesis_hash.clone())?;
        match journal.existing_attempt(&intent, &binding)? {
            Some(attempt) => Ok(FinanceRecoverySession::ObservationOnly(Box::new(journal.restore_observation_only(
                &intent,
                &chain_id,
                genesis_hash,
                policy,
                attempt,
            )?))),
            None => Ok(FinanceRecoverySession::Fresh(Box::new(Self::new(intent, chain_id, genesis_hash, policy)?))),
        }
    }

    /// Low-level in-memory construction. Restarting applications should use
    /// start_or_resume; this method alone provides no global signing fence.
    pub fn new(intent: FinanceIntent, chain_id: String, genesis_hash: H256, policy: RecoveryPolicy) -> Result<Self> {
        let binding = intent.recovery_intent(chain_id, genesis_hash)?;
        let recovery = ProtocolBundleRecovery::new(binding.clone(), policy)?;
        Ok(Self { intent, binding, recovery, inputs: Vec::new() })
    }

    pub fn prepare(&mut self, candidate: RecoveryCandidate<'_>, inputs: &[ProtocolBundleLiveCellObservation]) -> Result<()> {
        self.intent.verify_candidate(candidate.transaction, inputs)?;
        if inputs.len() != candidate.resolved_inputs.len()
            || inputs
                .iter()
                .zip(candidate.resolved_inputs)
                .any(|(economic, (output, data))| economic.output != *output || economic.data != *data)
        {
            bail!("financial checks and adapter must use the same resolved input Cells");
        }
        self.recovery.bind_candidate(&self.binding, candidate)?;
        self.inputs = inputs.to_vec();
        Ok(())
    }

    pub fn recovery(&self) -> &ProtocolBundleRecovery {
        &self.recovery
    }

    pub fn unsigned_failure(&mut self, failure: cellscript_ckb_adapter::recovery::UnsignedAttemptFailure) -> Result<()> {
        self.recovery.unsigned_failure(failure)
    }
    pub fn bind_signed(
        &mut self,
        transaction: &TransactionView,
        evidence: &cellscript_ckb_adapter::ProtocolBundleSignedTransactionEvidence,
    ) -> Result<()> {
        self.recovery.bind_signed(transaction, evidence)
    }
    pub fn begin_submission(
        &mut self,
        evidence: &cellscript_ckb_adapter::ProtocolBundleTxPoolEvidence,
        journal: &FinanceJournal,
    ) -> Result<JournalReference> {
        let snapshot = self.recovery.submission_snapshot(evidence)?;
        let reference = journal.persist(&self.intent, &self.binding, &self.inputs, &snapshot)?;
        self.recovery.begin_submission(evidence)?;
        Ok(reference)
    }
    pub fn submitted(&mut self, evidence: &cellscript_ckb_adapter::ProtocolBundleSubmissionEvidence) -> Result<()> {
        self.recovery.submitted(evidence)
    }
    pub fn submission_unknown(&mut self) -> Result<()> {
        self.recovery.submission_unknown()
    }
    pub fn observe(&mut self, hash: &H256, observation: cellscript_ckb_adapter::recovery::RecoveryObservation) -> Result<()> {
        self.recovery.observe(hash, observation)
    }
    pub fn confirmed(&mut self, evidence: &cellscript_ckb_adapter::ProtocolBundleConfirmationEvidence) -> Result<()> {
        self.recovery.confirmed(evidence)
    }
    pub fn cancel(&mut self) -> Result<()> {
        self.recovery.cancel()
    }
}
