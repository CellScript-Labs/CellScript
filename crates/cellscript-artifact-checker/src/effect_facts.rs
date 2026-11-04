//! Closed effect facts and canonical wire sets. These descriptions carry no
//! compiler state and confer no lifecycle or artifact authority by themselves.
use serde::de::{Error, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

mod ipc;
pub mod runtime;

// A wire name is mandatory for each closed identity. Ordering uses those names,
// rather than Rust discriminants, so moving a variant cannot change hashes.
macro_rules! fact_vocabulary {
    ($name:ident { $( $variant:ident => $wire:literal ),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name { $( #[serde(rename = $wire)] $variant, )+ }
        impl $name {
            pub const ALL: &'static [Self] = &[ $( Self::$variant, )+ ];
            pub const fn wire_name(self) -> &'static str {
                match self { $( Self::$variant => $wire, )+ }
            }
        }
        impl Ord for $name {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering { self.wire_name().cmp(other.wire_name()) }
        }
        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
        }
    };
}

/// Bound each dimension before accepting untrusted serialized facts. The
/// vocabulary is closed; this also bounds parameter-relative source entries.
pub const MAX_FACTS_PER_DIMENSION: usize = 4096;

/// Parameter-relative sources are substituted at a resolved call edge. Group
/// qualification is semantic and never collapsed into the unqualified source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ContextSource {
    Input,
    GroupInput,
    Output,
    GroupOutput,
    CellDep,
    HeaderDep,
    /// Whole witness inventory. An indexed witness retains its Input,
    /// GroupInput, Output or GroupOutput source with ContextAccess::Witness.
    Witness,
    CurrentScript,
    Transaction,
    HeaderEvidence,
    Parameter {
        index: u16,
    },
}

impl<'de> Deserialize<'de> for ContextSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Serde's internally tagged *unit* variants ignore additional map
        // fields even with deny_unknown_fields. Empty struct variants enforce
        // the current exact object shape, including index being absent for
        // every non-parameter source. This is only the current wire grammar.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
        enum Wire {
            Input {},
            GroupInput {},
            Output {},
            GroupOutput {},
            CellDep {},
            HeaderDep {},
            Witness {},
            CurrentScript {},
            Transaction {},
            HeaderEvidence {},
            Parameter { index: u16 },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Input {} => Self::Input,
            Wire::GroupInput {} => Self::GroupInput,
            Wire::Output {} => Self::Output,
            Wire::GroupOutput {} => Self::GroupOutput,
            Wire::CellDep {} => Self::CellDep,
            Wire::HeaderDep {} => Self::HeaderDep,
            Wire::Witness {} => Self::Witness,
            Wire::CurrentScript {} => Self::CurrentScript,
            Wire::Transaction {} => Self::Transaction,
            Wire::HeaderEvidence {} => Self::HeaderEvidence,
            Wire::Parameter { index } => Self::Parameter { index },
        })
    }
}

impl ContextSource {
    fn wire_key(self) -> (&'static str, u16) {
        match self {
            Self::Input => ("input", 0),
            Self::GroupInput => ("group-input", 0),
            Self::Output => ("output", 0),
            Self::GroupOutput => ("group-output", 0),
            Self::CellDep => ("cell-dep", 0),
            Self::HeaderDep => ("header-dep", 0),
            Self::Witness => ("witness", 0),
            Self::CurrentScript => ("current-script", 0),
            Self::Transaction => ("transaction", 0),
            Self::HeaderEvidence => ("header-evidence", 0),
            Self::Parameter { index } => ("parameter", index),
        }
    }
}

impl Ord for ContextSource {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.wire_key().cmp(&other.wire_key())
    }
}
impl PartialOrd for ContextSource {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fact_vocabulary! {
    ContextAccess {
        CellData => "cell-data",
        Capacity => "capacity",
        OccupiedCapacity => "occupied-capacity",
        LockScript => "lock-script",
        TypeScript => "type-script",
        OutPoint => "out-point",
        Since => "since",
        Index => "index",
        Cardinality => "cardinality",
        RoleSelection => "role-selection",
        Header => "header",
        Transaction => "transaction",
        Script => "script",
        Witness => "witness",
        EnvironmentEvidence => "environment-evidence",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRead {
    pub source: ContextSource,
    pub access: ContextAccess,
}

fact_vocabulary! {
    LifecycleOperation {
        Consume => "consume",
        Destroy => "destroy",
        Create => "create",
        Replace => "replace",
        Transfer => "transfer",
        Claim => "claim",
        Settle => "settle",
    }
}

fact_vocabulary! {
    ProcessOperation {
        Spawn => "spawn",
        Exec => "exec",
        Wait => "wait",
        PipeCreate => "pipe-create",
        PipeRead => "pipe-read",
        PipeWrite => "pipe-write",
        Close => "close",
        InheritedDescriptors => "inherited-descriptors",
        ProcessId => "process-id",
    }
}

/// In-memory ordering is also the specified wire ordering. Deserialization
/// rejects duplicates and unsorted entries instead of silently normalizing them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct CanonicalSet<T: Ord>(BTreeSet<T>);

impl<T: Ord> Default for CanonicalSet<T> {
    fn default() -> Self {
        Self(BTreeSet::new())
    }
}

impl<T: Ord> CanonicalSet<T> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn contains(&self, value: &T) -> bool {
        self.0.contains(value)
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.iter()
    }
    pub fn is_subset(&self, other: &Self) -> bool {
        self.0.is_subset(&other.0)
    }

    pub fn insert(&mut self, value: T) -> Result<bool, FactSetError> {
        if self.0.contains(&value) {
            return Ok(false);
        }
        if self.0.len() == MAX_FACTS_PER_DIMENSION {
            return Err(FactSetError::Budget);
        }
        Ok(self.0.insert(value))
    }
}

impl<'de, T: Ord + Deserialize<'de>> Deserialize<'de> for CanonicalSet<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CanonicalVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Ord + Deserialize<'de>> Visitor<'de> for CanonicalVisitor<T> {
            type Value = CanonicalSet<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded, strictly ordered fact set")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
                let mut values = BTreeSet::new();
                while let Some(value) = sequence.next_element::<T>()? {
                    if values.len() == MAX_FACTS_PER_DIMENSION {
                        return Err(A::Error::custom("effect fact set exceeds its budget"));
                    }
                    if values.last().is_some_and(|last| last >= &value) {
                        return Err(A::Error::custom("effect fact set is duplicated or noncanonical"));
                    }
                    values.insert(value);
                }
                Ok(CanonicalSet(values))
            }
        }
        deserializer.deserialize_seq(CanonicalVisitor(std::marker::PhantomData))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactSetError {
    Budget,
    UnresolvedSource { parameter: u16 },
    MissingLifecycleConstituent { operation: LifecycleOperation, required: LifecycleOperation },
}

impl fmt::Display for FactSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Budget => formatter.write_str("effect facts exceed their bounded vocabulary budget"),
            Self::UnresolvedSource { parameter } => write!(formatter, "effect source parameter {parameter} has no resolved argument"),
            Self::MissingLifecycleConstituent { operation, required } => {
                write!(formatter, "lifecycle operation {} requires fact {}", operation.wire_name(), required.wire_name())
            }
        }
    }
}

impl std::error::Error for FactSetError {}

/// Defaults are for completed local computation or internal accumulators only.
/// No field has a serde default: missing evidence must never decode as empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "EffectFactsWire")]
pub struct EffectFacts {
    context_reads: CanonicalSet<ContextRead>,
    lifecycle_ops: CanonicalSet<LifecycleOperation>,
    process_ops: CanonicalSet<ProcessOperation>,
}

// One current wire shape, validated on construction. This is not an alternate
// model or a reader for an earlier generation; every field is mandatory.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectFactsWire {
    context_reads: CanonicalSet<ContextRead>,
    lifecycle_ops: CanonicalSet<LifecycleOperation>,
    process_ops: CanonicalSet<ProcessOperation>,
}

impl TryFrom<EffectFactsWire> for EffectFacts {
    type Error = FactSetError;
    fn try_from(wire: EffectFactsWire) -> Result<Self, Self::Error> {
        let facts = Self { context_reads: wire.context_reads, lifecycle_ops: wire.lifecycle_ops, process_ops: wire.process_ops };
        facts.validate()?;
        Ok(facts)
    }
}

impl EffectFacts {
    pub fn context_reads(&self) -> &CanonicalSet<ContextRead> {
        &self.context_reads
    }

    pub fn lifecycle_ops(&self) -> &CanonicalSet<LifecycleOperation> {
        &self.lifecycle_ops
    }

    pub fn process_ops(&self) -> &CanonicalSet<ProcessOperation> {
        &self.process_ops
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn read(source: ContextSource, access: ContextAccess) -> Self {
        Self { context_reads: CanonicalSet(BTreeSet::from([ContextRead { source, access }])), ..Self::empty() }
    }

    pub fn lifecycle(operation: LifecycleOperation) -> Self {
        use LifecycleOperation as L;
        // Record constituent operations as well as the specific lifecycle form.
        // The additional tag records the operation, not a correspondence proof.
        let operations = match operation {
            L::Consume => BTreeSet::from([L::Consume]),
            L::Create => BTreeSet::from([L::Create]),
            L::Destroy => BTreeSet::from([L::Consume, L::Destroy]),
            L::Replace | L::Transfer | L::Claim | L::Settle => BTreeSet::from([L::Consume, L::Create, operation]),
        };
        Self { lifecycle_ops: CanonicalSet(operations), ..Self::empty() }
    }

    pub fn process(operation: ProcessOperation) -> Self {
        Self { process_ops: CanonicalSet(BTreeSet::from([operation])), ..Self::empty() }
    }

    pub fn include_read(&mut self, source: ContextSource, access: ContextAccess) -> Result<(), FactSetError> {
        self.context_reads.insert(ContextRead { source, access })?;
        Ok(())
    }

    pub fn include_lifecycle(&mut self, operation: LifecycleOperation) -> Result<(), FactSetError> {
        for operation in Self::lifecycle(operation).lifecycle_ops.iter() {
            self.lifecycle_ops.insert(*operation)?;
        }
        Ok(())
    }

    pub fn include_process(&mut self, operation: ProcessOperation) -> Result<(), FactSetError> {
        self.process_ops.insert(operation)?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.context_reads.is_empty() && self.lifecycle_ops.is_empty() && self.process_ops.is_empty()
    }

    pub fn validate(&self) -> Result<(), FactSetError> {
        for operation in self.lifecycle_ops.iter() {
            for required in Self::lifecycle(*operation).lifecycle_ops.iter() {
                if !self.lifecycle_ops.contains(required) {
                    return Err(FactSetError::MissingLifecycleConstituent { operation: *operation, required: *required });
                }
            }
        }
        Ok(())
    }

    pub fn allows_observation(&self) -> bool {
        self.lifecycle_ops.is_empty() && self.process_ops.is_empty()
    }

    pub fn union(&self, other: &Self) -> Result<Self, FactSetError> {
        let mut result = self.clone();
        for read in other.context_reads.iter() {
            result.context_reads.insert(*read)?;
        }
        for operation in other.lifecycle_ops.iter() {
            result.lifecycle_ops.insert(*operation)?;
        }
        for operation in other.process_ops.iter() {
            result.process_ops.insert(*operation)?;
        }
        Ok(result)
    }

    /// This is only the effect ceiling. Independent lifecycle, signature,
    /// authorization and deployment obligations must still be satisfied.
    pub fn accepts(&self, candidate: &Self) -> bool {
        candidate.context_reads.is_subset(&self.context_reads)
            && candidate.lifecycle_ops.is_subset(&self.lifecycle_ops)
            && candidate.process_ops.is_subset(&self.process_ops)
    }

    /// Instantiate source parameters using the resolved call's immutable source
    /// bindings. Every read of one parameter uses the same binding.
    /// A symbolic source may be forwarded to a caller parameter explicitly;
    /// missing arguments are errors, never wildcards or empty observations.
    pub fn substitute_sources(&self, bindings: &BTreeMap<u16, ContextSource>) -> Result<Self, FactSetError> {
        let mut result = Self { context_reads: CanonicalSet::default(), ..self.clone() };
        for read in self.context_reads.iter() {
            let source = match read.source {
                ContextSource::Parameter { index } => {
                    bindings.get(&index).copied().ok_or(FactSetError::UnresolvedSource { parameter: index })?
                }
                source => source,
            };
            result.context_reads.insert(ContextRead { source, access: read.access })?;
        }
        Ok(result)
    }
}

impl fmt::Display for EffectFacts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return formatter.write_str("no external operations");
        }
        formatter.write_str("reads=[")?;
        for (index, read) in self.context_reads.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            let (kind, parameter) = read.source.wire_key();
            if matches!(read.source, ContextSource::Parameter { .. }) {
                write!(formatter, "{kind}({parameter})")?;
            } else {
                formatter.write_str(kind)?;
            }
            write!(formatter, ".{}", read.access.wire_name())?;
        }
        formatter.write_str("]; lifecycle=[")?;
        for (index, operation) in self.lifecycle_ops.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            formatter.write_str(operation.wire_name())?;
        }
        formatter.write_str("]; process=[")?;
        for (index, operation) in self.process_ops.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            formatter.write_str(operation.wire_name())?;
        }
        formatter.write_str("]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shared_wire_vectors_cover_validation_and_the_product_relation() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!("effect_facts_vectors.json")).unwrap();
        for case in vectors["valid"].as_array().unwrap() {
            let facts: EffectFacts = serde_json::from_value(case["value"].clone()).unwrap();
            assert_eq!(serde_json::to_value(facts).unwrap(), case["value"], "{}", case["name"]);
        }
        for case in vectors["invalid"].as_array().unwrap() {
            assert!(serde_json::from_value::<EffectFacts>(case["value"].clone()).is_err(), "{}", case["name"]);
        }
        for case in vectors["comparisons"].as_array().unwrap() {
            let required: EffectFacts = serde_json::from_value(case["required"].clone()).unwrap();
            let candidate: EffectFacts = serde_json::from_value(case["candidate"].clone()).unwrap();
            assert_eq!(required.accepts(&candidate), case["accepts"].as_bool().unwrap(), "{}", case["name"]);
        }
        let inventory = vectors["valid"].as_array().unwrap().iter().find(|case| case["name"] == "complete-vocabulary").unwrap();
        let facts: EffectFacts = serde_json::from_value(inventory["value"].clone()).unwrap();
        assert_eq!(facts.lifecycle_ops.iter().copied().collect::<BTreeSet<_>>(), LifecycleOperation::ALL.iter().copied().collect());
        assert_eq!(facts.process_ops.iter().copied().collect::<BTreeSet<_>>(), ProcessOperation::ALL.iter().copied().collect());
        assert_eq!(
            facts.context_reads.iter().map(|read| read.access).collect::<BTreeSet<_>>(),
            ContextAccess::ALL.iter().copied().collect()
        );
    }

    fn read() -> EffectFacts {
        EffectFacts::read(ContextSource::Input, ContextAccess::Capacity)
    }
    fn create() -> EffectFacts {
        EffectFacts::lifecycle(LifecycleOperation::Create)
    }
    fn write() -> EffectFacts {
        EffectFacts::process(ProcessOperation::PipeWrite)
    }

    #[test]
    fn independent_dimensions_survive_union_and_define_a_product_ceiling() {
        let values = [
            EffectFacts::empty(),
            read(),
            create(),
            write(),
            read().union(&create()).unwrap(),
            create().union(&write()).unwrap(),
            read().union(&create()).unwrap().union(&write()).unwrap(),
        ];
        for a in &values {
            assert!(a.accepts(a));
            assert_eq!(a.union(a).unwrap(), *a);
            assert_eq!(a.union(&EffectFacts::empty()).unwrap(), *a);
            for b in &values {
                assert_eq!(a.union(b).unwrap(), b.union(a).unwrap());
                assert!(a.union(b).unwrap().accepts(a));
                for c in &values {
                    assert_eq!(a.union(&b.union(c).unwrap()).unwrap(), a.union(b).unwrap().union(c).unwrap());
                    if a.accepts(b) && b.accepts(c) {
                        assert!(a.accepts(c));
                    }
                }
            }
        }
        assert!(!create().accepts(&read().union(&create()).unwrap()));
        assert!(!read().accepts(&write()));
        assert!(!write().is_empty());
        assert!(!write().allows_observation());
        assert!(read().allows_observation());
        // Dropping the replacement operation is below this effect ceiling.
        // It still cannot discharge a required successor/correspondence proof.
        let replacement = EffectFacts::lifecycle(LifecycleOperation::Replace);
        let consume_create = EffectFacts::lifecycle(LifecycleOperation::Consume).union(&create()).unwrap();
        assert!(replacement.accepts(&consume_create));
        assert!(!consume_create.accepts(&replacement));
    }

    #[test]
    fn wire_requires_all_dimensions_and_rejects_categories_and_extensions() {
        let empty = json!({"context_reads":[], "lifecycle_ops":[], "process_ops":[]});
        assert_eq!(serde_json::from_value::<EffectFacts>(empty.clone()).unwrap(), EffectFacts::empty());
        for field in ["context_reads", "lifecycle_ops", "process_ops"] {
            let mut missing = empty.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<EffectFacts>(missing).is_err());
        }
        for retired in ["Pure", "ReadOnly", "Creating", "Destroying", "Mutating"] {
            assert!(serde_json::from_value::<EffectFacts>(json!(retired)).is_err());
            let mut extended = empty.clone();
            extended["effect_class"] = json!(retired);
            assert!(serde_json::from_value::<EffectFacts>(extended).is_err());
        }
        assert!(serde_json::from_str::<EffectFacts>("null").is_err());
        let mut unknown = empty;
        unknown["process_ops"] = json!(["unregistered"]);
        assert!(serde_json::from_value::<EffectFacts>(unknown).is_err());
    }

    #[test]
    fn sets_reject_duplicates_disorder_and_oversized_input() {
        assert!(serde_json::from_str::<CanonicalSet<ProcessOperation>>(r#"["pipe-read","pipe-read"]"#).is_err());
        assert!(serde_json::from_str::<CanonicalSet<ProcessOperation>>(r#"["pipe-write","pipe-read"]"#).is_err());
        assert!(serde_json::from_str::<CanonicalSet<ProcessOperation>>(r#"["pipe-read","pipe-write"]"#).is_ok());
        let oversized = (0..=MAX_FACTS_PER_DIMENSION)
            .map(|index| ContextRead { source: ContextSource::Parameter { index: index as u16 }, access: ContextAccess::Capacity })
            .collect::<Vec<_>>();
        assert!(serde_json::from_value::<CanonicalSet<ContextRead>>(serde_json::to_value(oversized).unwrap()).is_err());
        let mut bounded = CanonicalSet::default();
        for index in 0..MAX_FACTS_PER_DIMENSION {
            bounded.insert(index).unwrap();
        }
        assert_eq!(bounded.insert(MAX_FACTS_PER_DIMENSION), Err(FactSetError::Budget));
        assert!(!bounded.insert(0).unwrap());
    }

    #[test]
    fn sources_substitute_without_losing_group_identity_or_inventing_reads() {
        let relative = EffectFacts::read(ContextSource::Parameter { index: 1 }, ContextAccess::CellData);
        assert_eq!(relative.substitute_sources(&BTreeMap::new()), Err(FactSetError::UnresolvedSource { parameter: 1 }));
        let instantiated = relative.substitute_sources(&BTreeMap::from([(1, ContextSource::GroupInput)])).unwrap();
        assert_eq!(instantiated, EffectFacts::read(ContextSource::GroupInput, ContextAccess::CellData));
        assert!(!EffectFacts::read(ContextSource::Input, ContextAccess::CellData).accepts(&instantiated));
        assert!(EffectFacts::empty().substitute_sources(&BTreeMap::new()).unwrap().is_empty());
        assert_eq!(
            relative.substitute_sources(&BTreeMap::from([(1, ContextSource::Parameter { index: 0 })])).unwrap(),
            EffectFacts::read(ContextSource::Parameter { index: 0 }, ContextAccess::CellData)
        );
    }

    #[test]
    fn canonical_wire_roundtrips_facts_and_preserves_all_dimensions() {
        let facts = read().union(&create()).unwrap().union(&write()).unwrap();
        let encoded = serde_json::to_string(&facts).unwrap();
        assert_eq!(
            encoded,
            r#"{"context_reads":[{"source":{"kind":"input"},"access":"capacity"}],"lifecycle_ops":["create"],"process_ops":["pipe-write"]}"#
        );
        assert_eq!(serde_json::from_str::<EffectFacts>(&encoded).unwrap(), facts);
        assert_eq!(serde_json::to_string(&facts).unwrap(), encoded);
    }

    #[test]
    fn union_budget_errors_do_not_mutate_inputs_and_missing_source_never_disappears() {
        let mut full = EffectFacts::empty();
        for index in 0..MAX_FACTS_PER_DIMENSION {
            full.context_reads
                .insert(ContextRead { source: ContextSource::Parameter { index: index as u16 }, access: ContextAccess::CellData })
                .unwrap();
        }
        let before = full.clone();
        assert_eq!(full.union(&read()), Err(FactSetError::Budget));
        assert_eq!(full, before);
        assert_eq!(
            full.substitute_sources(
                &(0..MAX_FACTS_PER_DIMENSION as u16).filter(|index| *index != 17).map(|index| (index, ContextSource::Input)).collect()
            ),
            Err(FactSetError::UnresolvedSource { parameter: 17 })
        );
        assert_eq!(full, before);
        // Many callee-relative reads may legitimately instantiate to one actual
        // source; this is set union, rather than evidence disappearing.
        assert_eq!(
            full.substitute_sources(&(0..MAX_FACTS_PER_DIMENSION as u16).map(|index| (index, ContextSource::Input)).collect())
                .unwrap(),
            EffectFacts::read(ContextSource::Input, ContextAccess::CellData)
        );
    }

    #[test]
    fn every_process_identity_has_a_closed_canonical_wire_name() {
        for (operation, name) in [
            (ProcessOperation::Spawn, "spawn"),
            (ProcessOperation::Exec, "exec"),
            (ProcessOperation::Wait, "wait"),
            (ProcessOperation::PipeCreate, "pipe-create"),
            (ProcessOperation::PipeRead, "pipe-read"),
            (ProcessOperation::PipeWrite, "pipe-write"),
            (ProcessOperation::Close, "close"),
            (ProcessOperation::InheritedDescriptors, "inherited-descriptors"),
            (ProcessOperation::ProcessId, "process-id"),
        ] {
            let facts = EffectFacts::process(operation);
            assert_eq!(serde_json::to_value(&facts).unwrap()["process_ops"], json!([name]));
            assert_eq!(serde_json::from_value::<EffectFacts>(serde_json::to_value(&facts).unwrap()).unwrap(), facts);
            assert!(!facts.is_empty());
            assert!(!facts.allows_observation());
        }
    }

    #[test]
    fn canonical_order_uses_wire_names_and_numeric_source_positions() {
        macro_rules! check_wire_order {
            ($vocabulary:ty) => {{
                let mut set = CanonicalSet::default();
                for value in <$vocabulary>::ALL.iter().rev() {
                    set.insert(*value).unwrap();
                }
                let mut names = <$vocabulary>::ALL.iter().map(|value| value.wire_name()).collect::<Vec<_>>();
                names.sort();
                assert_eq!(serde_json::to_value(&set).unwrap(), json!(names));
                let encoded = serde_json::to_string(&set).unwrap();
                assert_eq!(serde_json::from_str::<CanonicalSet<$vocabulary>>(&encoded).unwrap(), set);
            }};
        }
        check_wire_order!(ContextAccess);
        check_wire_order!(LifecycleOperation);
        check_wire_order!(ProcessOperation);

        let mut sources = CanonicalSet::default();
        for source in [
            ContextSource::Witness,
            ContextSource::Parameter { index: 12 },
            ContextSource::Input,
            ContextSource::Parameter { index: 2 },
            ContextSource::GroupInput,
        ] {
            sources.insert(source).unwrap();
        }
        assert_eq!(
            serde_json::to_value(&sources).unwrap(),
            json!([
                {"kind":"group-input"}, {"kind":"input"},
                {"kind":"parameter","index":2}, {"kind":"parameter","index":12}, {"kind":"witness"}
            ])
        );
        assert!(!EffectFacts::read(ContextSource::Input, ContextAccess::Witness)
            .accepts(&EffectFacts::read(ContextSource::GroupInput, ContextAccess::Witness)));
    }

    #[test]
    fn compound_lifecycle_wire_requires_all_constituent_facts() {
        for operation in LifecycleOperation::ALL {
            let facts = EffectFacts::lifecycle(*operation);
            facts.validate().unwrap();
            let value = serde_json::to_value(&facts).unwrap();
            assert_eq!(serde_json::from_value::<EffectFacts>(value.clone()).unwrap(), facts);
            if !matches!(operation, LifecycleOperation::Consume | LifecycleOperation::Create) {
                let mut omitted = value;
                omitted["lifecycle_ops"] = json!([operation.wire_name()]);
                assert!(serde_json::from_value::<EffectFacts>(omitted).is_err());
            }
        }
    }
}

pub(crate) mod check;

#[cfg(test)]
mod source_evidence_tests {
    use crate::{interface::InterfaceCallable, SourceCallableContract};
    #[test]
    fn source_effect_evidence_is_required_even_when_explicitly_unavailable() {
        let source = serde_json::to_value(SourceCallableContract::default()).unwrap();
        let interface = serde_json::to_value(InterfaceCallable::default()).unwrap();
        assert!(source.get("effects").unwrap().is_null());
        assert!(interface.get("effects").unwrap().is_null());
        assert!(serde_json::from_value::<SourceCallableContract>(source.clone()).is_ok());
        assert!(serde_json::from_value::<InterfaceCallable>(interface.clone()).is_ok());
        let mut absent = source.clone();
        absent.as_object_mut().unwrap().remove("effects");
        assert!(serde_json::from_value::<SourceCallableContract>(absent).is_err());
        let mut absent = interface.clone();
        absent.as_object_mut().unwrap().remove("effects");
        assert!(serde_json::from_value::<InterfaceCallable>(absent).is_err());
        for retired in ["Pure", "ReadOnly", "Creating", "Destroying", "Mutating"] {
            let mut source = source.clone();
            source["effects"] = retired.into();
            assert!(serde_json::from_value::<SourceCallableContract>(source).is_err());
            let mut interface = interface.clone();
            interface["effects"] = retired.into();
            assert!(serde_json::from_value::<InterfaceCallable>(interface).is_err());
        }
    }
}
