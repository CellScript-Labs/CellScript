//! Write-ahead, write-once records for original financial submissions.
//!
//! Restoring a record grants observation only, never a checked receipt or the
//! authority to sign, submit or rebuild another economic transaction.

use super::*;
use anyhow::Context;
use cellscript_ckb_adapter::recovery::{ObservationOnlyRecovery, RecoveryJournalIdentity, RecoverySubmissionSnapshot};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalReference {
    schema: String,
    version: u32,
    intent_binding_hash: String,
    attempt: u32,
    raw_transaction_hash: String,
    signed_serialized_transaction_hash: String,
    payload_hash: String,
}

#[derive(Debug)]
pub struct FinanceJournal {
    directory: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalWire {
    schema: String,
    version: u32,
    payload_hash: String,
    payload: JournalPayload,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalPayload {
    intent: IntentWire,
    identity: RecoveryJournalIdentity,
    unsigned_transaction: String,
    signed_transaction: String,
    resolved_inputs: Vec<InputWire>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputWire {
    out_point: String,
    output: String,
    data: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundWire {
    asset: String,
    width: u8,
    amount: String,
    share_configuration: Option<ShareConfigurationWire>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShareConfigurationWire {
    pool: String,
    controller_code: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubjectWire {
    role: String,
    script: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntentWire {
    operation_id: String,
    operation: String,
    owner: String,
    recipient: String,
    debits: Vec<BoundWire>,
    credits: Vec<BoundWire>,
    subjects: Vec<SubjectWire>,
    maximum_fee_shannons: u64,
    maximum_capacity_debit_shannons: u64,
    maximum_occupied_capacity_shannons: u64,
}

fn encoded(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}
fn decoded(value: &str) -> Result<Vec<u8>> {
    let bytes = hex::decode(value.strip_prefix("0x").ok_or_else(|| anyhow::anyhow!("journal hex must have a 0x prefix"))?)?;
    if encoded(&bytes) != value {
        bail!("journal hex must be canonical lowercase encoding");
    }
    Ok(bytes)
}
fn script(value: &str) -> Result<Script> {
    Script::from_slice(&decoded(value)?).map_err(|error| anyhow::anyhow!("invalid journal Script: {error}"))
}
fn payload_digest(payload: &JournalPayload) -> Result<String> {
    Ok(format!(
        "0x{}",
        cellscript_artifact_checker::canonical_hash(
            "cellscript-finance-submission-journal-payload-v1",
            &serde_json::to_value(payload)?,
        )?
    ))
}

impl IntentWire {
    fn from_intent(intent: &FinanceIntent) -> Self {
        let bounds = |bounds: &[AssetAmountBound]| {
            bounds
                .iter()
                .map(|bound| BoundWire {
                    asset: encoded(bound.asset.as_slice()),
                    width: match bound.encoding {
                        AmountEncoding::U64Le => 8,
                        AmountEncoding::U128Le => 16,
                        AmountEncoding::PoolShares { .. } => 80,
                    },
                    amount: bound.amount.to_string(),
                    share_configuration: match bound.encoding {
                        AmountEncoding::PoolShares { pool, controller_code } => {
                            Some(ShareConfigurationWire { pool: encoded(&pool), controller_code: encoded(&controller_code) })
                        }
                        _ => None,
                    },
                })
                .collect()
        };
        Self {
            operation_id: encoded(intent.operation_id.as_bytes()),
            operation: intent.operation.clone(),
            owner: encoded(intent.owner.as_slice()),
            recipient: encoded(intent.recipient.as_slice()),
            debits: bounds(&intent.debits),
            credits: bounds(&intent.credits),
            subjects: intent
                .subjects
                .iter()
                .map(|subject| match subject {
                    RecoverySubject::Lock(script) => SubjectWire { role: "lock".into(), script: encoded(script.as_slice()) },
                    RecoverySubject::Type(script) => SubjectWire { role: "type".into(), script: encoded(script.as_slice()) },
                })
                .collect(),
            maximum_fee_shannons: intent.maximum_fee_shannons,
            maximum_capacity_debit_shannons: intent.maximum_capacity_debit_shannons,
            maximum_occupied_capacity_shannons: intent.maximum_occupied_capacity_shannons,
        }
    }

    fn into_intent(self) -> Result<FinanceIntent> {
        let bounds = |bounds: Vec<BoundWire>| {
            bounds
                .into_iter()
                .map(|bound| {
                    let amount: u128 = bound.amount.parse()?;
                    if amount.to_string() != bound.amount {
                        bail!("journal amount must be canonical unsigned decimal");
                    }
                    Ok(AssetAmountBound {
                        asset: script(&bound.asset)?,
                        amount,
                        encoding: match (bound.width, bound.share_configuration) {
                            (8, None) => AmountEncoding::U64Le,
                            (16, None) => AmountEncoding::U128Le,
                            (80, Some(configuration)) => AmountEncoding::PoolShares {
                                pool: decoded(&configuration.pool)?
                                    .try_into()
                                    .map_err(|_| anyhow::anyhow!("journal share pool must be 32 bytes"))?,
                                controller_code: decoded(&configuration.controller_code)?
                                    .try_into()
                                    .map_err(|_| anyhow::anyhow!("journal controller code must be 32 bytes"))?,
                            },
                            _ => bail!("unsupported journal amount width"),
                        },
                    })
                })
                .collect::<Result<Vec<_>>>()
        };
        let subjects = self
            .subjects
            .into_iter()
            .map(|subject| match subject.role.as_str() {
                "lock" => Ok(RecoverySubject::Lock(script(&subject.script)?)),
                "type" => Ok(RecoverySubject::Type(script(&subject.script)?)),
                _ => bail!("unsupported journal Script subject role"),
            })
            .collect::<Result<Vec<_>>>()?;
        FinanceIntent::new(FinanceIntentParameters {
            operation_id: H256::from_slice(&decoded(&self.operation_id)?)?,
            operation: self.operation,
            owner: script(&self.owner)?,
            recipient: script(&self.recipient)?,
            debits: bounds(self.debits)?,
            credits: bounds(self.credits)?,
            subjects,
            maximum_fee_shannons: self.maximum_fee_shannons,
            maximum_capacity_debit_shannons: self.maximum_capacity_debit_shannons,
            maximum_occupied_capacity_shannons: self.maximum_occupied_capacity_shannons,
        })
    }
}

impl FinanceJournal {
    pub fn new(directory: impl AsRef<Path>) -> Result<Self> {
        fs::create_dir_all(directory.as_ref())?;
        if !fs::symlink_metadata(directory.as_ref())?.file_type().is_dir() {
            bail!("journal directory must be a real directory");
        }
        Ok(Self { directory: fs::canonicalize(directory.as_ref())? })
    }

    /// Stable idempotency key: changing economic limits must not create a new
    /// storage slot for the same authorized operation after a process restart.
    pub fn operation_key(intent: &FinanceIntent) -> String {
        let mut bytes = b"cellscript-finance-journal-operation-v1\0".to_vec();
        bytes.extend_from_slice(intent.operation_id.as_bytes());
        hex::encode(blake2b_256(&bytes))
    }

    fn paths(&self, intent: &FinanceIntent) -> (PathBuf, PathBuf) {
        let key = Self::operation_key(intent);
        (self.directory.join(format!("{key}.journal.json")), self.directory.join(format!("{key}.reference.json")))
    }

    pub(super) fn existing_attempt(&self, intent: &FinanceIntent, binding: &RecoveryIntent) -> Result<Option<u32>> {
        let (record, reference) = self.paths(intent);
        let absent = |path: &Path| match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(error),
            Ok(_) => Ok(false),
        };
        if absent(&record)? && absent(&reference)? {
            return Ok(None);
        }
        let reference: JournalReference = serde_json::from_slice(&read_bounded(&reference)?)?;
        if reference.schema != "cellscript-finance-journal-reference-v1"
            || reference.version != 1
            || reference.intent_binding_hash != encoded(binding.binding_hash().as_bytes())
        {
            bail!("existing operation has another immutable chain or financial intent");
        }
        Ok(Some(reference.attempt))
    }

    pub fn persist(
        &self,
        intent: &FinanceIntent,
        binding: &RecoveryIntent,
        inputs: &[ProtocolBundleLiveCellObservation],
        snapshot: &RecoverySubmissionSnapshot,
    ) -> Result<JournalReference> {
        if &intent.recovery_intent(binding.chain_id().into(), binding.genesis_hash().clone())? != binding {
            bail!("journal financial intent differs from the original adapter intent");
        }
        intent.verify_candidate(snapshot.signed_transaction(), inputs)?;
        snapshot.identity().restore_observation_only(
            binding,
            snapshot.identity().policy(),
            snapshot.identity().attempt(),
            snapshot.unsigned_transaction(),
            snapshot.signed_transaction(),
        )?;
        let payload = JournalPayload {
            intent: IntentWire::from_intent(intent),
            identity: snapshot.identity().clone(),
            unsigned_transaction: encoded(snapshot.unsigned_transaction().data().as_slice()),
            signed_transaction: encoded(snapshot.signed_transaction().data().as_slice()),
            resolved_inputs: inputs
                .iter()
                .map(|cell| InputWire {
                    out_point: encoded(cell.out_point.as_slice()),
                    output: encoded(cell.output.as_slice()),
                    data: encoded(&cell.data),
                })
                .collect(),
        };
        let payload_hash = payload_digest(&payload)?;
        let reference = JournalReference {
            schema: "cellscript-finance-journal-reference-v1".into(),
            version: 1,
            intent_binding_hash: encoded(binding.binding_hash().as_bytes()),
            attempt: snapshot.identity().attempt(),
            raw_transaction_hash: snapshot.identity().raw_transaction_hash().into(),
            signed_serialized_transaction_hash: snapshot.identity().signed_serialized_transaction_hash().into(),
            payload_hash: payload_hash.clone(),
        };
        let record = JournalWire { schema: "cellscript-finance-submission-journal-v1".into(), version: 1, payload_hash, payload };
        let (record_path, reference_path) = self.paths(intent);
        self.install_once(&record_path, &serde_json::to_vec(&record)?)?;
        self.install_once(&reference_path, &serde_json::to_vec(&reference)?)?;
        Ok(reference)
    }

    pub fn restore_observation_only(
        &self,
        expected_intent: &FinanceIntent,
        chain_id: &str,
        genesis_hash: H256,
        policy: RecoveryPolicy,
        expected_attempt: u32,
    ) -> Result<ObservationOnlyRecovery> {
        let binding = expected_intent.recovery_intent(chain_id.into(), genesis_hash)?;
        let (record_path, reference_path) = self.paths(expected_intent);
        let reference: JournalReference = serde_json::from_slice(&read_bounded(&reference_path)?)?;
        if reference.schema != "cellscript-finance-journal-reference-v1"
            || reference.version != 1
            || reference.intent_binding_hash != encoded(binding.binding_hash().as_bytes())
            || reference.attempt != expected_attempt
        {
            bail!("journal reference has the wrong schema, chain, intent or attempt");
        }
        let wire: JournalWire = serde_json::from_slice(&read_bounded(&record_path)?)?;
        if wire.schema != "cellscript-finance-submission-journal-v1"
            || wire.version != 1
            || wire.payload_hash != reference.payload_hash
            || payload_digest(&wire.payload)? != reference.payload_hash
            || wire.payload.identity.raw_transaction_hash() != reference.raw_transaction_hash
            || wire.payload.identity.signed_serialized_transaction_hash() != reference.signed_serialized_transaction_hash
        {
            bail!("journal payload does not match the durable original reference");
        }
        let stored_intent = wire.payload.intent.into_intent()?;
        if stored_intent.recovery_intent(chain_id.into(), binding.genesis_hash().clone())? != binding {
            bail!("journal substituted immutable financial constraints");
        }
        let unsigned = ckb_types::packed::Transaction::from_slice(&decoded(&wire.payload.unsigned_transaction)?)
            .map_err(|error| anyhow::anyhow!("invalid unsigned journal transaction: {error}"))?
            .into_view();
        let signed = ckb_types::packed::Transaction::from_slice(&decoded(&wire.payload.signed_transaction)?)
            .map_err(|error| anyhow::anyhow!("invalid signed journal transaction: {error}"))?
            .into_view();
        let inputs = wire
            .payload
            .resolved_inputs
            .into_iter()
            .map(|cell| {
                Ok(ProtocolBundleLiveCellObservation {
                    out_point: ckb_types::packed::OutPoint::from_slice(&decoded(&cell.out_point)?)
                        .map_err(|error| anyhow::anyhow!("invalid journal OutPoint: {error}"))?,
                    output: CellOutput::from_slice(&decoded(&cell.output)?)
                        .map_err(|error| anyhow::anyhow!("invalid journal CellOutput: {error}"))?,
                    data: Bytes::from(decoded(&cell.data)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        stored_intent.verify_candidate(&signed, &inputs)?;
        wire.payload.identity.restore_observation_only(&binding, policy, expected_attempt, &unsigned, &signed)
    }

    fn install_once(&self, target: &Path, bytes: &[u8]) -> Result<()> {
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            bail!("submission journal exceeds its byte limit");
        }
        let mut pending = None;
        for _ in 0..100 {
            let name =
                self.directory.join(format!(".pending-{}-{}", std::process::id(), NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&name) {
                Ok(file) => {
                    pending = Some((name, file));
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let (temporary, mut file) = pending.ok_or_else(|| anyhow::anyhow!("cannot allocate a unique submission journal temporary"))?;
        let cleanup = TemporaryFile(temporary);
        file.write_all(bytes)?;
        file.sync_all()?;
        match fs::hard_link(&cleanup.0, target) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if read_bounded(target)? != bytes {
                    bail!("an immutable original submission already exists for this intent");
                }
            }
            Err(error) => return Err(error).context("cannot atomically install submission journal without replacement"),
        }
        File::open(&self.directory)?.sync_all()?;
        drop(cleanup);
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}

struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_JOURNAL_BYTES {
        bail!("submission journal must be a bounded regular file");
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(MAX_JOURNAL_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_JOURNAL_BYTES {
        bail!("submission journal grew beyond its byte limit");
    }
    Ok(bytes)
}
