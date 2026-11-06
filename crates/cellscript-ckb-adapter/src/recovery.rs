//! Bounded client-side recovery over the existing ProtocolBundle pipeline.
//!
//! This module performs no RPC, signing, transaction serialization or economic
//! authorization. Applications commit to their own immutable intent and check
//! that intent against resolved candidate Cells before binding an attempt.

use anyhow::{bail, Result};
use ckb_types::{
    bytes::Bytes,
    core::{Capacity, TransactionView},
    packed::{CellOutput, Script, WitnessArgs},
    prelude::*,
    H256,
};
use serde::{Deserialize, Serialize};

use crate::{
    protocol_bundle_confirmation_evidence, protocol_bundle_live_resolution_evidence, protocol_bundle_ready_to_sign_evidence,
    protocol_bundle_submission_evidence, ProtocolBundleConfirmationEvidence, ProtocolBundleDependencyResolutionEvidence,
    ProtocolBundleLiveResolutionEvidence, ProtocolBundleMaterializationEvidence, ProtocolBundleReadyToSignEvidence,
    ProtocolBundleSignedTransactionEvidence, ProtocolBundleSubmissionEvidence, ProtocolBundleTxPoolEvidence,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoverySubject {
    Lock(Script),
    Type(Script),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryIntent {
    chain_id: String,
    genesis_hash: H256,
    commitment: H256,
    subjects: Vec<RecoverySubject>,
    maximum_fee_shannons: u64,
    maximum_occupied_capacity_shannons: u64,
}

impl RecoveryIntent {
    pub fn new(
        chain_id: String,
        genesis_hash: H256,
        commitment: H256,
        subjects: Vec<RecoverySubject>,
        maximum_fee_shannons: u64,
        maximum_occupied_capacity_shannons: u64,
    ) -> Result<Self> {
        if chain_id.is_empty() || subjects.is_empty() {
            bail!("recovery intent requires an exact chain and at least one full Script subject");
        }
        Ok(Self { chain_id, genesis_hash, commitment, subjects, maximum_fee_shannons, maximum_occupied_capacity_shannons })
    }

    pub fn commitment(&self) -> &H256 {
        &self.commitment
    }

    pub fn chain_id(&self) -> &str {
        &self.chain_id
    }
    pub fn genesis_hash(&self) -> &H256 {
        &self.genesis_hash
    }

    pub fn binding_hash(&self) -> H256 {
        let mut bytes = b"cellscript-recovery-intent-binding-v1\0".to_vec();
        bytes.extend_from_slice(&(self.chain_id.len() as u64).to_le_bytes());
        bytes.extend_from_slice(self.chain_id.as_bytes());
        bytes.extend_from_slice(self.genesis_hash.as_bytes());
        bytes.extend_from_slice(self.commitment.as_bytes());
        bytes.extend_from_slice(&(self.subjects.len() as u64).to_le_bytes());
        for subject in &self.subjects {
            let (tag, script) = match subject {
                RecoverySubject::Lock(script) => (0, script),
                RecoverySubject::Type(script) => (1, script),
            };
            bytes.push(tag);
            bytes.extend_from_slice(&(script.as_slice().len() as u64).to_le_bytes());
            bytes.extend_from_slice(script.as_slice());
        }
        bytes.extend_from_slice(&self.maximum_fee_shannons.to_le_bytes());
        bytes.extend_from_slice(&self.maximum_occupied_capacity_shannons.to_le_bytes());
        H256::from(ckb_hash::blake2b_256(&bytes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPolicy {
    pub maximum_rebuilds: u32,
    pub maximum_observations: u32,
    pub required_confirmations: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    AwaitingCandidate,
    DiscoveryUnavailable,
    Prepared,
    RebuildRequired,
    Signed,
    Submitting,
    SubmissionUnknown,
    Pending,
    Included,
    Confirmed,
    Reorged,
    ObservationUnavailable,
    ObservationLimitReached,
    Rejected,
    Cancelled,
    CancellationPendingObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsignedAttemptFailure {
    StaleInput,
    StaleQuote,
    RpcUnavailable,
    PreflightRejected,
    SignerRejected,
    SignerCancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryInclusion {
    pub block_hash: H256,
    pub block_number: u64,
    pub transaction_index: u32,
}

/// These are caller-supplied observations, not evidence produced by this
/// state machine. Unknown and unavailable never authorize a replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryObservation {
    Pending,
    Unknown,
    Included(RecoveryInclusion),
    Rejected { stage: RecoveryRejectionStage, reason: String },
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryRejectionStage {
    SignedExecution,
    TxPoolAdmission,
    TransactionStatus,
}

pub struct RecoveryCandidate<'a> {
    pub transaction: &'a TransactionView,
    pub materialization: &'a ProtocolBundleMaterializationEvidence,
    pub resolved_inputs: &'a [(CellOutput, Bytes)],
    pub live_resolution: &'a ProtocolBundleLiveResolutionEvidence,
    pub dependency_resolution: &'a ProtocolBundleDependencyResolutionEvidence,
}

/// Untrusted journal identity data. Deserializing this value grants no checked
/// receipt, signing, submission or replacement authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryJournalIdentity {
    schema: String,
    version: u32,
    chain_id: String,
    genesis_hash: String,
    intent_binding_hash: String,
    attempt: u32,
    policy: RecoveryPolicy,
    raw_transaction_hash: String,
    unsigned_serialized_transaction_hash: String,
    signed_serialized_transaction_hash: String,
}

#[derive(Debug)]
pub struct RecoverySubmissionSnapshot {
    identity: RecoveryJournalIdentity,
    unsigned: TransactionView,
    signed: TransactionView,
}

impl RecoverySubmissionSnapshot {
    pub fn identity(&self) -> &RecoveryJournalIdentity {
        &self.identity
    }
    pub fn unsigned_transaction(&self) -> &TransactionView {
        &self.unsigned
    }
    pub fn signed_transaction(&self) -> &TransactionView {
        &self.signed
    }
}

fn hash_string(hash: &H256) -> String {
    format!("0x{}", hex::encode(hash.as_bytes()))
}
fn transaction_bytes_hash(transaction: &TransactionView) -> String {
    hash_string(&H256::from(ckb_hash::blake2b_256(transaction.data().as_slice())))
}

impl RecoveryJournalIdentity {
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
    pub fn policy(&self) -> RecoveryPolicy {
        self.policy
    }
    pub fn intent_binding_hash(&self) -> &str {
        &self.intent_binding_hash
    }
    pub fn raw_transaction_hash(&self) -> &str {
        &self.raw_transaction_hash
    }
    pub fn signed_serialized_transaction_hash(&self) -> &str {
        &self.signed_serialized_transaction_hash
    }

    pub fn restore_observation_only(
        &self,
        expected_intent: &RecoveryIntent,
        expected_policy: RecoveryPolicy,
        expected_attempt: u32,
        unsigned: &TransactionView,
        signed: &TransactionView,
    ) -> Result<ObservationOnlyRecovery> {
        if self.schema != "cellscript-recovery-journal-identity-v1"
            || self.version != 1
            || self.chain_id != expected_intent.chain_id
            || self.genesis_hash != hash_string(&expected_intent.genesis_hash)
            || self.intent_binding_hash != hash_string(&expected_intent.binding_hash())
            || self.policy != expected_policy
            || self.attempt != expected_attempt
            || self.attempt > expected_policy.maximum_rebuilds
            || expected_policy.maximum_observations == 0
            || expected_policy.required_confirmations == 0
        {
            bail!("recovery journal has the wrong schema, chain, intent, attempt or policy");
        }
        let raw_hash = format!("0x{}", hex::encode(signed.hash().as_slice()));
        if unsigned.hash() != signed.hash()
            || self.raw_transaction_hash != raw_hash
            || self.unsigned_serialized_transaction_hash != transaction_bytes_hash(unsigned)
            || self.signed_serialized_transaction_hash != transaction_bytes_hash(signed)
        {
            bail!("recovery journal transaction identities do not match the original packed bytes");
        }
        if unsigned.witnesses().len() != signed.witnesses().len() {
            bail!("recovery journal changed witness count");
        }
        for (before, after) in unsigned.witnesses().into_iter().zip(signed.witnesses()) {
            let before = WitnessArgs::from_slice(&before.raw_data())?;
            let after = WitnessArgs::from_slice(&after.raw_data())?;
            if before.input_type().as_slice() != after.input_type().as_slice()
                || before.output_type().as_slice() != after.output_type().as_slice()
            {
                bail!("recovery journal changed compiler-owned witness fields");
            }
        }
        Ok(ObservationOnlyRecovery {
            identity: self.clone(),
            raw_hash: H256::from_slice(signed.hash().as_slice())?,
            state: RecoveryState::SubmissionUnknown,
            observations: 0,
            inclusion: None,
            reorgs_observed: 0,
            last_observation: None,
        })
    }
}

/// A restarted process can only follow the retained transaction. This type has
/// no candidate, signing, submission, cancellation or economic-retry methods.
#[derive(Debug)]
pub struct ObservationOnlyRecovery {
    identity: RecoveryJournalIdentity,
    raw_hash: H256,
    state: RecoveryState,
    observations: u32,
    inclusion: Option<RecoveryInclusion>,
    reorgs_observed: u32,
    last_observation: Option<RecoveryObservation>,
}

/// An observation-only depth calculation, not a checked ProtocolBundle
/// confirmation receipt or proof that an RPC was executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationOnlyConfirmation {
    pub original_transaction_hash: H256,
    pub inclusion: RecoveryInclusion,
    pub observed_tip_hash: H256,
    pub observed_tip_number: u64,
    pub confirmation_count: u64,
    pub required_confirmation_count: u64,
    pub required_depth_observed: bool,
}

impl ObservationOnlyRecovery {
    pub fn identity(&self) -> &RecoveryJournalIdentity {
        &self.identity
    }
    pub fn state(&self) -> RecoveryState {
        self.state
    }
    pub fn original_transaction_hash(&self) -> &H256 {
        &self.raw_hash
    }
    pub fn observations(&self) -> u32 {
        self.observations
    }
    pub fn reorgs_observed(&self) -> u32 {
        self.reorgs_observed
    }
    pub fn last_observation(&self) -> Option<&RecoveryObservation> {
        self.last_observation.as_ref()
    }

    pub fn observe(
        &mut self,
        chain_id: &str,
        genesis_hash: &H256,
        transaction_hash: &H256,
        observation: RecoveryObservation,
    ) -> Result<()> {
        if chain_id != self.identity.chain_id
            || hash_string(genesis_hash) != self.identity.genesis_hash
            || transaction_hash != &self.raw_hash
        {
            bail!("recovered observation belongs to another chain or original transaction");
        }
        if self.observations >= self.identity.policy.maximum_observations {
            self.state = RecoveryState::ObservationLimitReached;
            bail!("recovered original-transaction observation limit reached");
        }
        self.observations += 1;
        let (state, inclusion, reorg) = observation_transition(self.inclusion.as_ref(), &observation);
        self.state = state;
        self.inclusion = inclusion;
        if reorg {
            self.reorgs_observed = self.reorgs_observed.saturating_add(1);
        }
        self.last_observation = Some(observation);
        Ok(())
    }

    pub fn observe_confirmation(
        &mut self,
        chain_id: &str,
        genesis_hash: &H256,
        transaction_hash: &H256,
        inclusion: RecoveryInclusion,
        observed_tip_hash: H256,
        observed_tip_number: u64,
    ) -> Result<ObservationOnlyConfirmation> {
        let confirmation_count = observed_tip_number
            .checked_sub(inclusion.block_number)
            .and_then(|distance| distance.checked_add(1))
            .ok_or_else(|| anyhow::anyhow!("recovered confirmation tip precedes the inclusion block"))?;
        self.observe(chain_id, genesis_hash, transaction_hash, RecoveryObservation::Included(inclusion.clone()))?;
        let required_depth_observed = confirmation_count >= self.identity.policy.required_confirmations;
        if required_depth_observed {
            self.state = RecoveryState::Confirmed;
        }
        Ok(ObservationOnlyConfirmation {
            original_transaction_hash: self.raw_hash.clone(),
            inclusion,
            observed_tip_hash,
            observed_tip_number,
            confirmation_count,
            required_confirmation_count: self.identity.policy.required_confirmations,
            required_depth_observed,
        })
    }
}

fn observation_transition(
    previous: Option<&RecoveryInclusion>,
    observation: &RecoveryObservation,
) -> (RecoveryState, Option<RecoveryInclusion>, bool) {
    if matches!(observation, RecoveryObservation::Unavailable(_)) {
        return (RecoveryState::ObservationUnavailable, previous.cloned(), false);
    }
    let next = match observation {
        RecoveryObservation::Included(location) => Some(location.clone()),
        _ => None,
    };
    let reorg = previous.is_some() && previous != next.as_ref();
    let state = match observation {
        RecoveryObservation::Pending | RecoveryObservation::Unknown | RecoveryObservation::Included(_) if reorg => {
            RecoveryState::Reorged
        }
        RecoveryObservation::Pending => RecoveryState::Pending,
        RecoveryObservation::Unknown => RecoveryState::SubmissionUnknown,
        RecoveryObservation::Included(_) => RecoveryState::Included,
        RecoveryObservation::Rejected { .. } => RecoveryState::Rejected,
        RecoveryObservation::Unavailable(_) => unreachable!(),
    };
    (state, next, reorg)
}

#[derive(Debug, Clone)]
struct Attempt {
    unsigned: TransactionView,
    materialization: ProtocolBundleMaterializationEvidence,
    preparation: ProtocolBundleReadyToSignEvidence,
    signed: Option<(TransactionView, ProtocolBundleSignedTransactionEvidence)>,
    submission: Option<ProtocolBundleSubmissionEvidence>,
}

#[derive(Debug)]
pub struct ProtocolBundleRecovery {
    intent: RecoveryIntent,
    policy: RecoveryPolicy,
    state: RecoveryState,
    attempt: Option<Attempt>,
    rebuilds: u32,
    observations: u32,
    inclusion: Option<RecoveryInclusion>,
    reorgs_observed: u32,
    cancellation_requested: bool,
    last_unsigned_failure: Option<UnsignedAttemptFailure>,
    last_observation: Option<RecoveryObservation>,
    discovery_unavailable_reason: Option<String>,
}

impl ProtocolBundleRecovery {
    pub fn new(intent: RecoveryIntent, policy: RecoveryPolicy) -> Result<Self> {
        if policy.maximum_observations == 0 || policy.required_confirmations == 0 {
            bail!("recovery policy requires positive observation and confirmation bounds");
        }
        Ok(Self {
            intent,
            policy,
            state: RecoveryState::AwaitingCandidate,
            attempt: None,
            rebuilds: 0,
            observations: 0,
            inclusion: None,
            reorgs_observed: 0,
            cancellation_requested: false,
            last_unsigned_failure: None,
            last_observation: None,
            discovery_unavailable_reason: None,
        })
    }

    pub fn state(&self) -> RecoveryState {
        self.state
    }
    pub fn rebuilds(&self) -> u32 {
        self.rebuilds
    }
    pub fn observations(&self) -> u32 {
        self.observations
    }
    pub fn reorgs_observed(&self) -> u32 {
        self.reorgs_observed
    }
    pub fn cancellation_requested(&self) -> bool {
        self.cancellation_requested
    }
    pub fn last_unsigned_failure(&self) -> Option<UnsignedAttemptFailure> {
        self.last_unsigned_failure
    }
    pub fn last_observation(&self) -> Option<&RecoveryObservation> {
        self.last_observation.as_ref()
    }
    pub fn discovery_unavailable_reason(&self) -> Option<&str> {
        self.discovery_unavailable_reason.as_deref()
    }

    pub fn discovery_unavailable(&mut self, reason: String) -> Result<()> {
        if !matches!(
            self.state,
            RecoveryState::AwaitingCandidate | RecoveryState::DiscoveryUnavailable | RecoveryState::RebuildRequired
        ) {
            bail!("discovery failure cannot replace an existing prepared or signed attempt");
        }
        self.discovery_unavailable_reason = Some(reason);
        self.state = RecoveryState::DiscoveryUnavailable;
        Ok(())
    }

    /// This rejects changed intent and checks actual packed bytes through the
    /// ordinary live-resolution and signing-preparation functions. Economic
    /// predicates remain the application's responsibility.
    pub fn bind_candidate(&mut self, intent: &RecoveryIntent, candidate: RecoveryCandidate<'_>) -> Result<()> {
        if !matches!(
            self.state,
            RecoveryState::AwaitingCandidate | RecoveryState::DiscoveryUnavailable | RecoveryState::RebuildRequired
        ) {
            bail!("recovery cannot replace this attempt; observe the original signed transaction");
        }
        if intent != &self.intent {
            bail!("recovery candidate changes immutable intent or resource limits");
        }
        if self.attempt.is_some() && self.rebuilds >= self.policy.maximum_rebuilds {
            bail!("recovery rebuild limit reached");
        }
        let materialization = candidate.materialization;
        if candidate.transaction.outputs().len() != candidate.transaction.outputs_data().len() {
            bail!("recovery candidate must provide data for every output exactly once");
        }
        let mut output_capacity = 0u64;
        let mut occupied_capacity = 0u64;
        for (output, data) in candidate.transaction.outputs().into_iter().zip(candidate.transaction.outputs_data()) {
            output_capacity = output_capacity
                .checked_add(output.capacity().unpack())
                .ok_or_else(|| anyhow::anyhow!("recovery output capacity total overflow"))?;
            let occupied = output.occupied_capacity(Capacity::bytes(data.raw_data().len())?)?.as_u64();
            let capacity: u64 = output.capacity().unpack();
            if capacity < occupied {
                bail!("recovery candidate has an output below occupied capacity");
            }
            occupied_capacity =
                occupied_capacity.checked_add(occupied).ok_or_else(|| anyhow::anyhow!("recovery occupied capacity total overflow"))?;
        }
        if output_capacity != materialization.output_capacity_shannons
            || occupied_capacity != materialization.occupied_output_capacity_shannons
        {
            bail!("recovery capacity evidence differs from actual candidate outputs");
        }
        let genesis = format!("0x{}", hex::encode(self.intent.genesis_hash.as_bytes()));
        let actual_live = protocol_bundle_live_resolution_evidence(
            candidate.transaction,
            materialization,
            &self.intent.chain_id,
            &genesis,
            candidate.resolved_inputs,
        )?;
        if &actual_live != candidate.live_resolution {
            bail!("recovery resolved inputs do not match live-resolution evidence");
        }
        if actual_live.fee_shannons > self.intent.maximum_fee_shannons
            || materialization.occupied_output_capacity_shannons > self.intent.maximum_occupied_capacity_shannons
        {
            bail!("recovery candidate exceeds the immutable fee or occupied-capacity limit");
        }
        for subject in &self.intent.subjects {
            let present = candidate.resolved_inputs.iter().any(|(output, _)| match subject {
                RecoverySubject::Lock(script) => output.lock() == *script,
                RecoverySubject::Type(script) => output.type_().to_opt().as_ref() == Some(script),
            });
            if !present {
                bail!("recovery candidate omits or substitutes an exact Script subject");
            }
        }
        let preparation = protocol_bundle_ready_to_sign_evidence(
            candidate.transaction,
            materialization,
            candidate.live_resolution,
            candidate.dependency_resolution,
        )?;
        if self.attempt.is_some() {
            self.rebuilds += 1;
        }
        self.attempt = Some(Attempt {
            unsigned: candidate.transaction.clone(),
            materialization: materialization.clone(),
            preparation,
            signed: None,
            submission: None,
        });
        self.state = RecoveryState::Prepared;
        self.last_unsigned_failure = None;
        self.discovery_unavailable_reason = None;
        Ok(())
    }

    pub fn unsigned_failure(&mut self, failure: UnsignedAttemptFailure) -> Result<()> {
        if self.state != RecoveryState::Prepared {
            bail!("unsigned recovery failure requires a prepared, unsigned attempt");
        }
        self.state = match failure {
            UnsignedAttemptFailure::StaleInput | UnsignedAttemptFailure::StaleQuote => RecoveryState::RebuildRequired,
            UnsignedAttemptFailure::RpcUnavailable => RecoveryState::Prepared,
            UnsignedAttemptFailure::SignerCancelled => RecoveryState::Cancelled,
            UnsignedAttemptFailure::SignerRejected | UnsignedAttemptFailure::PreflightRejected => RecoveryState::Rejected,
        };
        self.last_unsigned_failure = Some(failure);
        if failure == UnsignedAttemptFailure::SignerCancelled {
            self.cancellation_requested = true;
        }
        Ok(())
    }

    pub fn bind_signed(&mut self, transaction: &TransactionView, evidence: &ProtocolBundleSignedTransactionEvidence) -> Result<()> {
        if self.state != RecoveryState::Prepared {
            bail!("recovery signing requires a prepared unsigned attempt");
        }
        let attempt = self.attempt.as_mut().expect("prepared attempt");
        let actual = crate::protocol_bundle::protocol_bundle_signed_transaction_evidence(
            &attempt.unsigned,
            transaction,
            &attempt.preparation,
            evidence.sdk_unlocker_count,
        )?;
        if &actual != evidence {
            bail!("recovery signing evidence differs from the actual signed bytes");
        }
        attempt.signed = Some((transaction.clone(), evidence.clone()));
        self.state = RecoveryState::Signed;
        Ok(())
    }

    /// Call before send_transaction. A timeout after this point must track the
    /// original hash; the transaction might already have been accepted.
    pub fn begin_submission(&mut self, tx_pool: &ProtocolBundleTxPoolEvidence) -> Result<()> {
        if self.state != RecoveryState::Signed {
            bail!("recovery submission requires the original signed attempt");
        }
        let attempt = self.attempt.as_mut().expect("signed attempt");
        let (transaction, signing) = attempt.signed.as_ref().expect("signed bytes");
        let hash = H256::from_slice(transaction.hash().as_slice())?;
        attempt.submission = Some(protocol_bundle_submission_evidence(transaction, signing, tx_pool, &hash)?);
        self.state = RecoveryState::Submitting;
        Ok(())
    }

    /// Obtain the exact validated bytes for a write-ahead journal. This does
    /// not send anything or advance the attempt; callers persist it before
    /// invoking begin_submission and the existing RPC submission API.
    pub fn submission_snapshot(&self, tx_pool: &ProtocolBundleTxPoolEvidence) -> Result<RecoverySubmissionSnapshot> {
        if self.state != RecoveryState::Signed {
            bail!("submission journal requires the original signed attempt");
        }
        let attempt = self.attempt.as_ref().expect("signed attempt");
        let (signed, signing) = attempt.signed.as_ref().expect("signed bytes");
        let raw_hash = H256::from_slice(signed.hash().as_slice())?;
        protocol_bundle_submission_evidence(signed, signing, tx_pool, &raw_hash)?;
        Ok(RecoverySubmissionSnapshot {
            identity: RecoveryJournalIdentity {
                schema: "cellscript-recovery-journal-identity-v1".into(),
                version: 1,
                chain_id: self.intent.chain_id.clone(),
                genesis_hash: hash_string(&self.intent.genesis_hash),
                intent_binding_hash: hash_string(&self.intent.binding_hash()),
                attempt: self.rebuilds,
                policy: self.policy,
                raw_transaction_hash: hash_string(&raw_hash),
                unsigned_serialized_transaction_hash: transaction_bytes_hash(&attempt.unsigned),
                signed_serialized_transaction_hash: transaction_bytes_hash(signed),
            },
            unsigned: attempt.unsigned.clone(),
            signed: signed.clone(),
        })
    }

    pub fn submitted(&mut self, submission: &ProtocolBundleSubmissionEvidence) -> Result<()> {
        if !matches!(self.state, RecoveryState::Submitting | RecoveryState::SubmissionUnknown) {
            bail!("recovery has no submission awaiting an acknowledgement");
        }
        if self.attempt.as_ref().and_then(|attempt| attempt.submission.as_ref()) != Some(submission) {
            bail!("recovery submission acknowledgement belongs to another transaction");
        }
        self.state = RecoveryState::Pending;
        Ok(())
    }

    pub fn submission_unknown(&mut self) -> Result<()> {
        if self.state != RecoveryState::Submitting {
            bail!("unknown submission requires an in-flight original submission");
        }
        self.state = RecoveryState::SubmissionUnknown;
        Ok(())
    }

    pub fn observe(&mut self, transaction_hash: &H256, observation: RecoveryObservation) -> Result<()> {
        let attempt = self.attempt.as_ref().ok_or_else(|| anyhow::anyhow!("recovery has no original transaction to observe"))?;
        let (transaction, _) =
            attempt.signed.as_ref().ok_or_else(|| anyhow::anyhow!("recovery has no signed transaction to observe"))?;
        if transaction.hash().as_slice() != transaction_hash.as_bytes() {
            bail!("recovery observation belongs to another transaction");
        }
        if self.observations >= self.policy.maximum_observations {
            self.state = RecoveryState::ObservationLimitReached;
            bail!("recovery observation limit reached; original transaction remains unresolved");
        }
        self.observations += 1;
        self.last_observation = Some(observation.clone());
        let (state, next_inclusion, reorg) = observation_transition(self.inclusion.as_ref(), &observation);
        if reorg {
            self.reorgs_observed = self.reorgs_observed.saturating_add(1);
        }
        self.inclusion = next_inclusion;
        self.state = state;
        Ok(())
    }

    pub fn confirmed(&mut self, confirmation: &ProtocolBundleConfirmationEvidence) -> Result<()> {
        if self.state == RecoveryState::ObservationLimitReached {
            bail!("recovery observation limit reached; confirmation requires caller intervention");
        }
        let attempt = self.attempt.as_ref().ok_or_else(|| anyhow::anyhow!("recovery has no submitted attempt"))?;
        let submission =
            attempt.submission.as_ref().ok_or_else(|| anyhow::anyhow!("recovery requires original submission evidence"))?;
        if confirmation.required_confirmation_count != self.policy.required_confirmations
            || confirmation.reorgs_observed != self.reorgs_observed
        {
            bail!("recovery confirmation changes the required depth or observed reorg count");
        }
        let actual = protocol_bundle_confirmation_evidence(
            submission,
            &attempt.materialization,
            &confirmation.network_chain_id,
            &confirmation.network_genesis_hash,
            &confirmation.block_hash,
            confirmation.block_number,
            confirmation.transaction_index,
            &confirmation.observed_tip_hash,
            confirmation.observed_tip_number,
            self.policy.required_confirmations,
            self.reorgs_observed,
        )?;
        if &actual != confirmation {
            bail!("recovery confirmation differs from the original transaction evidence");
        }
        let inclusion = RecoveryInclusion {
            block_hash: H256::from_slice(&hex::decode(confirmation.block_hash.trim_start_matches("0x"))?)?,
            block_number: confirmation.block_number,
            transaction_index: confirmation.transaction_index,
        };
        if self.inclusion.as_ref().is_some_and(|previous| previous != &inclusion) {
            bail!("recovery must observe the changed inclusion before confirming it");
        }
        self.inclusion = Some(inclusion);
        self.state = RecoveryState::Confirmed;
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<()> {
        if self.state == RecoveryState::Confirmed {
            bail!("confirmed operation cannot be cancelled");
        }
        self.cancellation_requested = true;
        self.state = if self.attempt.as_ref().is_some_and(|attempt| attempt.signed.is_some()) {
            RecoveryState::CancellationPendingObservation
        } else {
            RecoveryState::Cancelled
        };
        Ok(())
    }
}
