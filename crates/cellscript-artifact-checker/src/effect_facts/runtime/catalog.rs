//! Explicit exhaustive descriptions. Registering a helper requires a decision;
//! there is no symbol-prefix classification or missing-arm empty contract.
use super::*;
use super::{ContextAccess as A, ContextSource as S, ProcessOperation as P, ReadSource as R};

impl RuntimeHelper {
    pub fn fact_contract(self) -> RuntimeEffectContract {
        use RuntimeHelper as H;
        let template = match self {
            H::CkbExecCellDepU8Args | H::CkbExecCellDepHex4 => {
                RuntimeEffectTemplate { reads: &[], processes: &[P::Exec], result: SourceResult::Value }
            }
            H::CkbTransactionU32Le
            | H::CkbTransactionBlake2bGather
            | H::CkbTransactionHash
            | H::CkbRawTransactionHashWithoutCellDeps => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Fixed(S::Transaction), access: A::Transaction }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbWitnessBlake2bSelectChunks
            | H::CkbWitnessBytes32
            | H::CkbWitnessBlake2bSpan
            | H::CkbWitnessLockExact32
            | H::CkbWitnessInputTypeExact32
            | H::CkbWitnessOutputTypeExact32
            | H::CkbWitnessBoundedSize
            | H::CkbWitnessBoundedU8
            | H::CkbWitnessBoundedU32Le
            | H::CkbWitnessBoundedU64Le
            | H::CkbWitnessBoundedBlake2b
            | H::CkbWitnessSize
            | H::CkbWitnessU8
            | H::CkbWitnessU32Le
            | H::CkbWitnessU64Le
            | H::CkbRequireWitnessSizeAtLeast
            | H::CellscriptWitnessBoundedResolve => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::Witness }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbSpawnWaitCellDepHex4 => {
                RuntimeEffectTemplate { reads: &[], processes: &[P::Spawn, P::Wait], result: SourceResult::Value }
            }
            H::CkbCurrentScriptHash | H::CkbRequireCurrentScriptArgsEmpty => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbCommitmentOpen
            | H::CkbCurrentRole
            | H::CkbScriptHash
            | H::CkbSinceToRaw
            | H::CkbEpochNumberToU64
            | H::CkbEpochDurationToU64
            | H::CkbBlockNumberToU64
            | H::CkbEpochLengthToU64
            | H::CkbTimestampMillisToU64
            | H::CkbCellOutputIndex
            | H::C256RequireU128ProductLte
            | H::C256RequireU128ProductEq
            | H::C256RequireU128Sum2ProductsLte
            | H::C256RequireU128Sum2ProductsEq
            | H::CellscriptAbort
            | H::CellscriptBlake2bCompress
            | H::CellscriptMulU128ToU256
            | H::CellscriptLoadU64Le
            | H::CellscriptMemcmpFixed
            | H::CellscriptMemzeroFixed
            | H::CellscriptAddU256
            | H::CellscriptRequireExactSize
            | H::CellscriptRequireMinSize
            | H::CkbRequireSha256dMerkleRoot
            | H::CellscriptMemcpyFixed
            | H::CellscriptSha256Compress
            | H::CellscriptSha256Fixed
            | H::CkbHashBlake2b
            | H::CkbHashBlake2bVar
            | H::CkbHashChain
            | H::CkbHashPair
            | H::CkbHashSha256
            | H::CkbHashSha256Pair
            | H::CkbHashSha256d
            | H::CkbHashSha256dPair
            | H::CkbEpochAdd
            | H::CkbEpochDuration
            | H::CkbEpochSub
            | H::CkbSinceAsAbsoluteBlock
            | H::CkbSinceAsAbsoluteEpoch
            | H::CkbSinceAsAbsoluteTimestamp
            | H::CkbSinceAsRelativeBlock
            | H::CkbSinceAsRelativeEpoch
            | H::CkbSinceAsRelativeTimestamp
            | H::CkbSinceBlockAbsolute
            | H::CkbSinceBlockRelative
            | H::CkbSinceDecode
            | H::CkbSinceEpochAbsolute
            | H::CkbSinceEpochRelative
            | H::CkbSinceFromRawChecked
            | H::CkbSinceIsDisabled
            | H::CkbSinceIsRelative
            | H::CkbSinceMetric
            | H::CkbSinceTimestampAbsolute
            | H::CkbSinceTimestampRelative
            | H::CkbSinceValue
            | H::CkbHashBlake2bPacked
            | H::CkbHashDataPacked => RuntimeEffectTemplate::local(),
            H::CkbCellCapacity => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::Capacity }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbCellOccupiedCapacity => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::OccupiedCapacity }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbCellUnoccupiedCapacity => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::Capacity },
                    RuntimeRead::Field { source: R::Argument(0), access: A::OccupiedCapacity },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbInputOutPointIndex
            | H::CkbInputOutPointTxHashLow
            | H::CkbInputOutPointTxHash
            | H::CkbRequireInputOutPointTxHash
            | H::CkbRequireInputOutPoint => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::OutPoint }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbCellLockHashLow
            | H::CkbCellLockHash
            | H::CkbCellLockCodeHash
            | H::CkbCellLockHashType
            | H::CkbCellLockArgsEmpty
            | H::CkbCellLockArgs32
            | H::CkbRequireCellLockHash
            | H::CkbRequireCellLockArgsEmpty
            | H::CkbRequireCellLockArgs32
            | H::CkbRequireCellLockArgsExact
            | H::CkbRequireCellLockArgsPrefix32
            | H::CkbRequireCellLockArgsSuffix32
            | H::CkbRequireCellLockScriptHashType
            | H::CkbCellLockSize
            | H::CkbCellLockU8 => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::LockScript }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbCellTypeHashLow
            | H::CkbCellTypeHash
            | H::CkbCellTypeCodeHash
            | H::CkbCellTypeHashType
            | H::CkbCellTypeArgsEmpty
            | H::CkbCellTypeArgs32
            | H::CkbRequireCellTypeHash
            | H::CkbRequireCellTypeArgsEmpty
            | H::CkbRequireCellTypeArgs32
            | H::CkbRequireCellTypeArgsExact
            | H::CkbRequireCellTypeArgsPrefix32
            | H::CkbRequireCellTypeArgsSuffix32
            | H::CkbRequireCellTypeScriptHashType
            | H::CkbCellHasType
            | H::DaoHasDaoType
            | H::XudtOwnerModeInputTypeHash
            | H::XudtRequireOwnerModeInputType
            | H::XudtRequireOwnerModeTypeArgs
            | H::CkbCellTypeSize
            | H::CkbCellTypeU8 => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbCellDataHashField
            | H::CkbCellDataBytes32
            | H::CkbCellDataBlake2bSpan
            | H::CkbRequireCellDataHash
            | H::CkbCellDataSize
            | H::CkbCellDataU32Le
            | H::CkbCellDataU64Le
            | H::DaoIsDepositData
            | H::DaoIsWithdrawalRequestData
            | H::XudtAmountLow
            | H::XudtAmountHigh
            | H::CkbCellDataU8 => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::CellData }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbCellDataEqual => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::CellData },
                    RuntimeRead::Field { source: R::Argument(1), access: A::CellData },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbSourceBytesEqual => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Bytes { source: R::Argument(0), kind_argument: 5 },
                    RuntimeRead::Bytes { source: R::Argument(2), kind_argument: 6 },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbSourceBytesEqualMemory | H::CellscriptExactReadCached => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Bytes { source: R::Argument(0), kind_argument: 4 }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbSourceBytesZero => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Bytes { source: R::Argument(0), kind_argument: 3 }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbCellCount => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::Cardinality }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::DaoAccumulatedRate
            | H::DaoInputAccumulatedRate
            | H::CkbHeaderDepEpochNumber
            | H::CkbHeaderDepEpochStartBlockNumber
            | H::CkbHeaderDepEpochLength
            | H::CkbHeaderDepBlockNumber
            | H::CkbHeaderDepTimestampMillis => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::Header }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::DaoRequireHeaderDepForInput => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::Header },
                    RuntimeRead::Field { source: R::Argument(1), access: A::Header },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::DaoRequireInputSinceAtLeast | H::DaoRequireInputRelativeEpochSinceAtLeast | H::CkbInputSinceAt => {
                RuntimeEffectTemplate {
                    reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::Since }],
                    processes: &[],
                    result: SourceResult::Value,
                }
            }
            H::XudtRequireOwnerModeTypeArgsCurrentScript => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbWitnessCount => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Fixed(S::Input), access: A::Witness }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::EnvCurrentTimepoint | H::CkbHeaderEpochNumber | H::CkbHeaderEpochStartBlockNumber | H::CkbHeaderEpochLength => {
                RuntimeEffectTemplate {
                    reads: &[RuntimeRead::Field { source: R::Fixed(S::HeaderDep), access: A::Header }],
                    processes: &[],
                    result: SourceResult::Value,
                }
            }
            H::CkbSpawnWithFd1 => RuntimeEffectTemplate { reads: &[], processes: &[P::Spawn], result: SourceResult::Value },
            H::CkbSourceCellDep => RuntimeEffectTemplate { reads: &[], processes: &[], result: SourceResult::Fixed(S::CellDep) },
            H::CkbSourceGroupInput => RuntimeEffectTemplate { reads: &[], processes: &[], result: SourceResult::Fixed(S::GroupInput) },
            H::CkbSourceGroupOutput => {
                RuntimeEffectTemplate { reads: &[], processes: &[], result: SourceResult::Fixed(S::GroupOutput) }
            }
            H::CkbSourceHeaderDep => RuntimeEffectTemplate { reads: &[], processes: &[], result: SourceResult::Fixed(S::HeaderDep) },
            H::CkbSourceInput => RuntimeEffectTemplate { reads: &[], processes: &[], result: SourceResult::Fixed(S::Input) },
            H::CkbSourceOutput => RuntimeEffectTemplate { reads: &[], processes: &[], result: SourceResult::Fixed(S::Output) },
            H::CkbWait => RuntimeEffectTemplate { reads: &[], processes: &[P::Wait], result: SourceResult::Value },
            H::CkbProcessId => RuntimeEffectTemplate { reads: &[], processes: &[P::ProcessId], result: SourceResult::Value },
            H::CkbPipe => RuntimeEffectTemplate { reads: &[], processes: &[P::PipeCreate], result: SourceResult::Value },
            H::CkbPipeWrite => RuntimeEffectTemplate { reads: &[], processes: &[P::PipeWrite], result: SourceResult::Value },
            H::CkbPipeRead => RuntimeEffectTemplate { reads: &[], processes: &[P::PipeRead], result: SourceResult::Value },
            H::CkbInheritedFd => {
                RuntimeEffectTemplate { reads: &[], processes: &[P::InheritedDescriptors], result: SourceResult::Value }
            }
            H::CkbClose => RuntimeEffectTemplate { reads: &[], processes: &[P::Close], result: SourceResult::Value },
            H::ZkRequireTransition => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::Field { source: R::Fixed(S::Transaction), access: A::Transaction },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::OutPoint },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::Cardinality },
                ],
                processes: &[P::PipeCreate, P::Spawn, P::PipeWrite, P::Close, P::Wait],
                result: SourceResult::Value,
            },
            H::Bip340RequireSignature | H::Bip340RequireSignatureFromCellDep => RuntimeEffectTemplate {
                reads: &[],
                processes: &[P::PipeCreate, P::Spawn, P::PipeWrite, P::Close, P::Wait],
                result: SourceResult::Value,
            },
            H::BoundedConsumeEachFailClosed | H::BoundedCreateEachFailClosed => {
                return RuntimeEffectContract::Failure(crate::runtime_contract::KnownRuntimeFailure::CollectionRuntimeUnsupported)
            }
            H::CkbSighashAll => return RuntimeEffectContract::Deferred(DeferredFeature::CkbSighashAll),
            H::CkbRequireMaturity => return RuntimeEffectContract::Deferred(DeferredFeature::CkbSinceConstraints),
            H::CkbRequireTime => return RuntimeEffectContract::Deferred(DeferredFeature::CkbSinceConstraints),
            H::CkbRequireEpochAfter => return RuntimeEffectContract::Deferred(DeferredFeature::CkbSinceConstraints),
            H::CkbRequireEpochRelative => return RuntimeEffectContract::Deferred(DeferredFeature::CkbSinceConstraints),
            H::CkbOccupiedCapacity => return RuntimeEffectContract::Deferred(DeferredFeature::CkbOccupiedCapacityFloor),
            H::CkbInputSince => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::Since }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellLockExactHandle => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::LockScript }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellTypeExactHandle => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellDepExactVerifierHandle => RuntimeEffectTemplate {
                reads: &[RuntimeRead::Field { source: R::Argument(0), access: A::CellData }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireBoundedCellDepDataHash => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::Cardinality },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::XudtRequireGroupAmountConserved => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::Cardinality },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::XudtRequireGroupAmountMinted => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::Cardinality },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::XudtRequireGroupAmountBurned => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::Cardinality },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CellscriptRequireFungibleTypeGroupV1 => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::GroupOutput), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::Field { source: R::Fixed(S::Input), access: A::LockScript },
                    RuntimeRead::Field { source: R::Fixed(S::Input), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::Input), access: A::Cardinality },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbSighashAllZeroLock => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::Transaction), access: A::Transaction },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::Witness },
                    RuntimeRead::Field { source: R::Fixed(S::GroupInput), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::Input), access: A::Witness },
                    RuntimeRead::Field { source: R::Fixed(S::Input), access: A::Cardinality },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellDepOpenScriptHandle => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::Transaction), access: A::Transaction },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::Cardinality },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellDepOpenVerifierHandle => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::Transaction), access: A::Transaction },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::Cardinality },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellDepOpenScriptHandleFromArgs => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::Transaction), access: A::Transaction },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellDepOpenVerifierHandleFromArgs => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::Transaction), access: A::Transaction },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireOpenScriptParticipantFromArgs => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::Transaction), access: A::Transaction },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::Field { source: R::Argument(2), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(2), access: A::TypeScript },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireInstanceScriptParticipantFromArgs => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Fixed(S::Transaction), access: A::Transaction },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::CellData },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Fixed(S::CellDep), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::Field { source: R::Argument(2), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(2), access: A::TypeScript },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellLockDeploymentLineHandle => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(1), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(1), access: A::CellData },
                    RuntimeRead::Field { source: R::Argument(2), access: A::CellData },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellTypeDeploymentLineHandle => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(1), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(1), access: A::CellData },
                    RuntimeRead::Field { source: R::Argument(2), access: A::CellData },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireCellDepDeploymentLineVerifierHandle => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::CellData },
                    RuntimeRead::Field { source: R::Argument(1), access: A::CellData },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireMetapointRelative => RuntimeEffectTemplate {
                reads: &[RuntimeRead::InputOutPoint { argument: 0 }, RuntimeRead::InputOutPoint { argument: 1 }],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireLockTypeMetapointPairs => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Argument(0), access: A::RoleSelection },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::InputOutPoint { argument: 0 },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireTypeLockMetapointPairs => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Argument(0), access: A::RoleSelection },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::InputOutPoint { argument: 0 },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireLockTypeMetapointPairsFromI32Data => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Argument(0), access: A::RoleSelection },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::InputOutPoint { argument: 0 },
                    RuntimeRead::Field { source: R::Argument(0), access: A::CellData },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireTypeLockMetapointPairsFromI32Data => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Argument(0), access: A::RoleSelection },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::InputOutPoint { argument: 0 },
                    RuntimeRead::Field { source: R::Argument(0), access: A::CellData },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireLockTypeMetapointPairsFromI32DataFiltered => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Argument(0), access: A::RoleSelection },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::InputOutPoint { argument: 0 },
                    RuntimeRead::Field { source: R::Argument(0), access: A::CellData },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireTypeLockMetapointPairsFromI32DataFiltered => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Argument(0), access: A::RoleSelection },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::InputOutPoint { argument: 0 },
                    RuntimeRead::Field { source: R::Argument(0), access: A::CellData },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CkbRequireLockMatchMasterOutPointPairsFromData => RuntimeEffectTemplate {
                reads: &[
                    RuntimeRead::Field { source: R::Argument(0), access: A::CellData },
                    RuntimeRead::Field { source: R::Argument(0), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(0), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Argument(0), access: A::RoleSelection },
                    RuntimeRead::Field { source: R::Argument(1), access: A::CellData },
                    RuntimeRead::Field { source: R::Argument(1), access: A::LockScript },
                    RuntimeRead::Field { source: R::Argument(1), access: A::TypeScript },
                    RuntimeRead::Field { source: R::Argument(1), access: A::Cardinality },
                    RuntimeRead::Field { source: R::Argument(1), access: A::RoleSelection },
                    RuntimeRead::Field { source: R::Fixed(S::CurrentScript), access: A::Script },
                    RuntimeRead::InputOutPoint { argument: 0 },
                ],
                processes: &[],
                result: SourceResult::Value,
            },
            H::CellscriptCurrentScriptRoleAt
            | H::CellscriptRequireCellMembership
            | H::CellscriptBlake2bSegments
            | H::CellscriptBlake2bTransactionSpan
            | H::CellscriptBlake2bPrefixedTransactionSpan => return RuntimeEffectContract::MachineOnly(self),
        };
        RuntimeEffectContract::Available(template)
    }
}
