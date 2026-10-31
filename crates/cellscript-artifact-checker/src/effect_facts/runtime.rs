//! Runtime operation descriptions for fact inference. These are not artifact
//! evidence: consumers must establish each argument's source and source-kind
//! value from their own resolved operations before instantiating a description.
use super::{CanonicalSet, ContextAccess, ContextSource, EffectFacts, FactSetError, ProcessOperation};
use crate::runtime_contract::{DeferredFeature, RuntimeHelper};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadSource {
    Fixed(ContextSource),
    Argument(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeRead {
    /// MetaPoint output indices are local values; only input MetaPoints load an OutPoint.
    InputOutPoint {
        argument: u16,
    },
    Field {
        source: ReadSource,
        access: ContextAccess,
    },
    /// Kind is a bound scalar argument in the exact-read/byte-comparison ABI.
    Bytes {
        source: ReadSource,
        kind_argument: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceResult {
    Value,
    Fixed(ContextSource),
    Argument(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteSourceKind {
    CellData,
    Witness,
    CellLock,
    CellType,
}

impl ByteSourceKind {
    /// Exact byte-source discriminants emitted by the compiler's runtime ABI.
    pub fn from_wire(value: u64) -> Result<Self, RuntimeFactError> {
        match value {
            0 => Ok(Self::CellData),
            1 => Ok(Self::Witness),
            2 => Ok(Self::CellLock),
            3 => Ok(Self::CellType),
            other => Err(RuntimeFactError::InvalidByteSourceKind(other)),
        }
    }
    pub const fn access(self) -> ContextAccess {
        match self {
            Self::CellData => ContextAccess::CellData,
            Self::Witness => ContextAccess::Witness,
            Self::CellLock => ContextAccess::LockScript,
            Self::CellType => ContextAccess::TypeScript,
        }
    }
}

/// Source and kind bindings are normalized evidence, not raw integers guessed
/// to be SourceViews. An absent binding is never an empty observation.
#[derive(Debug, Clone, Default)]
pub struct RuntimeArgumentFacts {
    pub sources: BTreeMap<u16, CanonicalSet<ContextSource>>,
    pub byte_kinds: BTreeMap<u16, ByteSourceKind>,
    /// Closed source classes justified by a source-bearing formal type.
    pub source_kinds: BTreeMap<u16, CanonicalSet<ContextSource>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeEffectTemplate {
    pub reads: &'static [RuntimeRead],
    pub processes: &'static [ProcessOperation],
    pub result: SourceResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEffectContract {
    Available(RuntimeEffectTemplate),
    /// Missing executable semantics retain their exact deferred failure. They
    /// cannot be inferred as a completed empty effect contract.
    Deferred(DeferredFeature),
    Failure(crate::runtime_contract::KnownRuntimeFailure),
    /// Private pointer/descriptor ABI requires the enclosing machine contract;
    /// it is never admitted as a standalone typed semantic call.
    MachineOnly(RuntimeHelper),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeFactError {
    Deferred(DeferredFeature),
    NonReturningFailure,
    MissingSource(u16),
    MissingSourceKind(u16),
    InvalidSourceKind(ContextSource),
    MachineOnly(&'static str),
    EmptySource(u16),
    MissingByteSourceKind(u16),
    InvalidByteSourceKind(u64),
    Facts(FactSetError),
}
impl From<FactSetError> for RuntimeFactError {
    fn from(error: FactSetError) -> Self {
        Self::Facts(error)
    }
}
impl fmt::Display for RuntimeFactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deferred(feature) => write!(formatter, "runtime effect evidence is unavailable for {}", feature.feature()),
            Self::NonReturningFailure => write!(formatter, "operation always terminates with its registered runtime failure"),
            Self::MissingSourceKind(index) => write!(formatter, "runtime argument {index} has no bound source class"),
            Self::InvalidSourceKind(source) => write!(formatter, "unsupported source class {source:?}"),
            Self::MachineOnly(symbol) => write!(formatter, "private helper {symbol} requires enclosing machine evidence"),
            Self::MissingSource(index) => write!(formatter, "runtime argument {index} has no resolved source evidence"),
            Self::EmptySource(index) => write!(formatter, "runtime argument {index} has empty source evidence"),
            Self::MissingByteSourceKind(index) => write!(formatter, "runtime argument {index} has no bound byte-source kind"),
            Self::InvalidByteSourceKind(value) => write!(formatter, "unrecognized byte-source kind {value}"),
            Self::Facts(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for RuntimeFactError {}

impl RuntimeEffectTemplate {
    pub const fn local() -> Self {
        Self { reads: &[], processes: &[], result: SourceResult::Value }
    }

    pub fn instantiate(&self, arguments: &RuntimeArgumentFacts) -> Result<EffectFacts, RuntimeFactError> {
        let mut result = EffectFacts::empty();
        for operation in self.processes {
            result.include_process(*operation)?;
        }
        for read in self.reads {
            if let RuntimeRead::InputOutPoint { argument } = *read {
                let sources = arguments.sources.get(&argument).ok_or(RuntimeFactError::MissingSource(argument))?;
                if sources.is_empty() {
                    return Err(RuntimeFactError::EmptySource(argument));
                }
                for source in sources.iter() {
                    let kinds = if matches!(source, ContextSource::Parameter { .. }) {
                        arguments.source_kinds.get(&argument).ok_or(RuntimeFactError::MissingSourceKind(argument))?.clone()
                    } else {
                        let mut kinds = CanonicalSet::default();
                        kinds.insert(*source)?;
                        kinds
                    };
                    if kinds.is_empty() {
                        return Err(RuntimeFactError::MissingSourceKind(argument));
                    }
                    for kind in kinds.iter() {
                        match kind {
                            ContextSource::Input | ContextSource::GroupInput => {
                                result.include_read(*source, ContextAccess::OutPoint)?
                            }
                            ContextSource::Output | ContextSource::GroupOutput => {}
                            other => return Err(RuntimeFactError::InvalidSourceKind(*other)),
                        }
                    }
                }
                continue;
            }
            let (source, access) = match *read {
                RuntimeRead::InputOutPoint { .. } => continue,
                RuntimeRead::Field { source, access } => (source, access),
                RuntimeRead::Bytes { source, kind_argument } => (
                    source,
                    arguments.byte_kinds.get(&kind_argument).ok_or(RuntimeFactError::MissingByteSourceKind(kind_argument))?.access(),
                ),
            };
            match source {
                ReadSource::Fixed(source) => result.include_read(source, access)?,
                ReadSource::Argument(index) => {
                    let sources = arguments.sources.get(&index).ok_or(RuntimeFactError::MissingSource(index))?;
                    if sources.is_empty() {
                        return Err(RuntimeFactError::EmptySource(index));
                    }
                    for source in sources.iter() {
                        result.include_read(*source, access)?;
                    }
                }
            }
        }
        Ok(result)
    }

    /// None means a proven ordinary return value; failed source resolution is
    /// an error. Source constructors can return a view without reading it.
    pub fn result_sources(&self, arguments: &RuntimeArgumentFacts) -> Result<Option<CanonicalSet<ContextSource>>, RuntimeFactError> {
        match self.result {
            SourceResult::Value => Ok(None),
            SourceResult::Fixed(source) => {
                let mut sources = CanonicalSet::default();
                sources.insert(source)?;
                Ok(Some(sources))
            }
            SourceResult::Argument(index) => {
                let sources = arguments.sources.get(&index).ok_or(RuntimeFactError::MissingSource(index))?;
                if sources.is_empty() {
                    return Err(RuntimeFactError::EmptySource(index));
                }
                Ok(Some(sources.clone()))
            }
        }
    }
}

impl RuntimeEffectContract {
    pub fn instantiate(&self, arguments: &RuntimeArgumentFacts) -> Result<EffectFacts, RuntimeFactError> {
        match self {
            Self::Available(template) => template.instantiate(arguments),
            Self::Deferred(feature) => Err(RuntimeFactError::Deferred(*feature)),
            Self::Failure(_) => Err(RuntimeFactError::NonReturningFailure),
            Self::MachineOnly(helper) => Err(RuntimeFactError::MachineOnly(helper.symbol())),
        }
    }
}

mod catalog;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_contract::{RuntimeAvailability, RuntimeReturnAbi};

    #[test]
    fn helper_fact_inventory_matches_explicit_availability_and_failure_abi() {
        for helper in RuntimeHelper::ALL {
            match (helper.availability(), helper.fact_contract()) {
                (RuntimeAvailability::Implemented, RuntimeEffectContract::Available(_)) => {}
                (RuntimeAvailability::Implemented, RuntimeEffectContract::MachineOnly(identity)) => assert_eq!(*helper, identity),
                (RuntimeAvailability::Deferred(expected), RuntimeEffectContract::Deferred(actual)) => {
                    assert_eq!(expected, actual);
                    assert_eq!(helper.return_abi(), RuntimeReturnAbi::NoReturn);
                    assert_eq!(helper.failure(), Some(expected.failure()));
                }
                (RuntimeAvailability::Fails(expected), RuntimeEffectContract::Failure(actual)) => {
                    assert_eq!(expected, actual);
                    assert_eq!(helper.return_abi(), RuntimeReturnAbi::NoReturn);
                    assert_eq!(helper.failure(), Some(expected.failure()));
                }
                (availability, facts) => panic!("{}: {availability:?} disagrees with {facts:?}", helper.symbol()),
            }
        }
    }

    #[test]
    fn byte_read_discriminants_preserve_the_observed_context_field() {
        let read = RuntimeEffectTemplate {
            reads: &[RuntimeRead::Bytes { source: ReadSource::Argument(0), kind_argument: 1 }],
            processes: &[],
            result: SourceResult::Value,
        };
        let mut arguments = RuntimeArgumentFacts::default();
        let mut sources = CanonicalSet::default();
        sources.insert(ContextSource::Input).unwrap();
        arguments.sources.insert(0, sources);
        assert!(read.instantiate(&arguments).is_err());
        for (wire, access) in
            [(0, ContextAccess::CellData), (1, ContextAccess::Witness), (2, ContextAccess::LockScript), (3, ContextAccess::TypeScript)]
        {
            arguments.byte_kinds.insert(1, ByteSourceKind::from_wire(wire).unwrap());
            assert_eq!(read.instantiate(&arguments).unwrap(), EffectFacts::read(ContextSource::Input, access));
        }
        assert!(ByteSourceKind::from_wire(4).is_err());
    }

    #[test]
    fn zk_budget_metering_does_not_invent_a_transaction_observation() {
        let facts = RuntimeHelper::ZkRequireTransition.fact_contract().instantiate(&RuntimeArgumentFacts::default()).unwrap();
        assert!(facts
            .context_reads()
            .contains(&super::super::ContextRead { source: ContextSource::Transaction, access: ContextAccess::Transaction }));
        assert!(!facts
            .context_reads()
            .contains(&super::super::ContextRead { source: ContextSource::Transaction, access: ContextAccess::EnvironmentEvidence }));
        assert!(facts.process_ops().contains(&ProcessOperation::Spawn));
    }
    #[test]
    fn participant_templates_bind_ir_argument_two_before_machine_abi_expansion() {
        let mut arguments = RuntimeArgumentFacts::default();
        let mut sources = CanonicalSet::default();
        sources.insert(ContextSource::GroupOutput).unwrap();
        // IR arguments are (code CellDep, handle, participant); codegen adds
        // the handle byte length before loading participant into machine a3.
        arguments.sources.insert(2, sources);
        for helper in
            [RuntimeHelper::CkbRequireOpenScriptParticipantFromArgs, RuntimeHelper::CkbRequireInstanceScriptParticipantFromArgs]
        {
            let facts = helper.fact_contract().instantiate(&arguments).unwrap();
            for access in [ContextAccess::LockScript, ContextAccess::TypeScript] {
                assert!(facts.context_reads().contains(&super::super::ContextRead { source: ContextSource::GroupOutput, access }));
            }
            assert!(helper.fact_contract().instantiate(&RuntimeArgumentFacts::default()).is_err());
        }
    }
}
