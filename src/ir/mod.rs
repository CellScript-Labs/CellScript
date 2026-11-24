mod call_target;
pub use call_target::IrCallTarget;

use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::resolve::{FunctionDef, ModuleResolver, TypeDef};
use crate::runtime_errors::CellScriptRuntimeError;
use cellscript_artifact_checker::runtime_contract::RuntimeHelper;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

mod aggregate_storage;
mod assignments;
mod call_lowering;
mod constant_values;
mod expressions;
mod projections;
mod scalar_types;
mod tail_values;

mod aggregate_expressions;
mod branches;
mod cell_operations;
mod cell_patterns;
mod checks;
mod collection_calls;
mod control_flow;
mod data_declarations;
mod entries;
mod flow_states;
mod handle_calls;
mod relations;
mod script_calls;
mod statements;
mod stdlib_calls;
mod witness_calls;

mod bindings;
mod builtins;
mod fixed_layout;
pub(crate) use fixed_layout::{checked_fixed_size, fixed_layout_width, validate_fixed_layout};
mod generic_contract;
mod link_types;
mod nested;
mod nominal;
mod type_text;
mod type_visit;
mod validate;
pub use generic_contract::{IrGenericContract, IrGenericDeclaration, IrGenericKind, IrGenericParameter, IrGenericValueParameter};
pub use nested::NestedViewType;
pub(crate) use nominal::resolved_types_equal;
pub(crate) use type_visit::visit_instruction_types;
pub(crate) use validate::validate_structure;
pub mod allocated;
mod optimize;
use allocated::IrAllocatedValue;
mod policy;
mod value_aggregates;
pub use bindings::{IrCellBinding, IrCellBindingRole, IrCellMembership, IrCellSource};
pub(crate) use optimize::{optimize_exact_cell_data_equality, optimize_source_byte_equality};
pub(crate) use policy::{bind_artifact_policy, bind_single_entry_cells};

/// CKB temporal domains use exact `u64` wire bits and occupy one machine word.
/// Keep this inventory in shared IR so codegen, entry-witness metadata, and
/// generated builders cannot classify the same source type differently.
pub(crate) const CKB_TEMPORAL_SCALAR_TYPE_NAMES: &[&str] = &[
    "EpochNumber",
    "EpochDuration",
    "BlockNumber",
    "EpochLength",
    "TimestampMillis",
    "EncodedSince",
    "DecodedSince",
    "AbsoluteBlockSince",
    "AbsoluteEpochSince",
    "AbsoluteTimestampSince",
    "RelativeBlockSince",
    "RelativeEpochSince",
    "RelativeTimestampSince",
];

pub(crate) fn is_ckb_temporal_scalar_name(name: &str) -> bool {
    CKB_TEMPORAL_SCALAR_TYPE_NAMES.contains(&name) || name.starts_with("Since<Absolute, ") || name.starts_with("Since<Relative, ")
}

/// CKB semantic hash domains remain physically represented by one 32-byte
/// value while the type checker prevents accidental domain interchange.
pub(crate) const CKB_FIXED_HASH_DOMAIN_TYPE_NAMES: &[&str] = &["ScriptHash", "SighashAllDigest"];

pub(crate) fn is_ckb_fixed_hash_domain_name(name: &str) -> bool {
    CKB_FIXED_HASH_DOMAIN_TYPE_NAMES.contains(&name)
}

/// Exact prelude labels whose intrinsic physical representation cannot be
/// acquired by a source declaration or import alias. Qualified declaration
/// labels remain ordinary nominal keys; this does not reserve every prelude
/// spelling (for example, an ordinary OutPoint record remains supported).
pub(crate) fn is_reserved_builtin_type_name(name: &str) -> bool {
    CKB_TEMPORAL_SCALAR_TYPE_NAMES.contains(&name)
        || is_ckb_fixed_hash_domain_name(name)
        || crate::zk_contract::fixed_width(name).is_some()
        || matches!(
            name,
            crate::script_handle_contract::EXACT_SCRIPT_HANDLE_TYPE | crate::script_handle_contract::DEPLOYMENT_LINE_HANDLE_TYPE
        )
}

/// Raw exact-width reads return byte arrays, independently of their physical
/// width matching a digest. No implicit hash-domain conversion is admitted.
pub(crate) fn is_raw_bytes32_runtime_result(helper: RuntimeHelper, ty: &IrType) -> bool {
    matches!(
        helper,
        RuntimeHelper::CkbCellDataBytes32
            | RuntimeHelper::CkbCellLockArgs32
            | RuntimeHelper::CkbCellTypeArgs32
            | RuntimeHelper::CkbWitnessBytes32
            | RuntimeHelper::CkbWitnessLockExact32
            | RuntimeHelper::CkbWitnessInputTypeExact32
            | RuntimeHelper::CkbWitnessOutputTypeExact32
    ) && matches!(ty, IrType::Array(item, 32) if item.as_ref() == &IrType::U8)
}

/// Typed Cell and ScriptRef properties retain the full-Script hash domain.
/// Raw SourceView hash calls and data hashes keep their ordinary Hash result.
pub(crate) fn is_ckb_script_hash_property_result(helper: RuntimeHelper, ty: &IrType) -> bool {
    matches!(helper, RuntimeHelper::CkbCellLockHash | RuntimeHelper::CkbCellTypeHash)
        && matches!(ty, IrType::Named(name) if name == CKB_SCRIPT_HASH_TYPE)
}

pub(crate) fn is_ckb_sighash_all_result(helper: RuntimeHelper, ty: &IrType) -> bool {
    helper == RuntimeHelper::CkbSighashAllZeroLock && matches!(ty, IrType::Named(name) if name == CKB_SIGHASH_ALL_DIGEST_TYPE)
}

pub(crate) const CKB_WITNESS_OWNER_RAW: u64 = cellscript_artifact_checker::view_types::WitnessBytesOwner::Raw.tag();
pub(crate) const CKB_WITNESS_OWNER_LOCK: u64 = cellscript_artifact_checker::view_types::WitnessBytesOwner::Lock.tag();
pub(crate) const CKB_WITNESS_OWNER_ENTRY: u64 = cellscript_artifact_checker::view_types::WitnessBytesOwner::Entry.tag();
pub(crate) const CKB_WITNESS_OWNER_OUTPUT_TYPE: u64 = cellscript_artifact_checker::view_types::WitnessBytesOwner::OutputType.tag();

pub(crate) fn witness_bytes_view_parts(ty: &IrType) -> Option<(&str, u64, u64)> {
    let IrType::WitnessBytes(view) = ty else {
        return None;
    };
    Some((view.owner.as_str(), view.owner.tag(), view.maximum))
}

#[derive(Debug, Clone)]
pub struct IrModule {
    pub name: String,
    pub items: Vec<IrItem>,
    pub entry_selection: IrEntrySelection,
    pub external_type_defs: Vec<IrTypeDef>,
    pub external_callable_abis: Vec<IrCallableAbi>,
    pub enum_fixed_sizes: HashMap<String, usize>,
    pub enum_layouts: HashMap<String, IrEnumLayout>,
    pub generic_contracts: BTreeMap<String, IrGenericContract>,
    /// Source ownership survives entry pruning and imported helper merging.
    /// None records an ambiguous lowered name; it cannot supply open admission.
    pub source_type_origins: BTreeMap<String, Option<DeclarationIdentity>>,
    /// Resolved specialization arguments; output labels are not identity keys.
    pub nominal_identities: BTreeMap<String, Option<crate::resolve::ResolvedTypeIdentity>>,
    /// Resolved source spelling to unique lowered callable; never serialized.
    pub source_callable_targets: BTreeMap<String, String>,
}

/// Artifact-local entry choice, separate from the retained callable set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum IrEntrySelection {
    #[default]
    Implicit,
    Action(String),
    Lock(String),
    Artifact(crate::artifact::ArtifactDeclaration),
}

/// The callable selected by an implicit or explicit single-entry contract.
///
/// Keep the selection in shared IR so emitted code and semantic metadata cannot
/// disagree about which callable the entry wrapper invokes. A module may retain
/// other actions as dependencies; their presence does not imply dispatch.
#[derive(Debug, Clone, Copy)]
pub(crate) enum IrEntryPoint<'a> {
    Action(&'a IrAction),
    Lock(&'a IrLock),
}

impl<'a> IrEntryPoint<'a> {
    pub(crate) fn name(self) -> &'a str {
        match self {
            Self::Action(action) => &action.name,
            Self::Lock(lock) => &lock.name,
        }
    }

    pub(crate) fn params(self) -> &'a [IrParam] {
        match self {
            Self::Action(action) => &action.params,
            Self::Lock(lock) => &lock.params,
        }
    }

    #[cfg(not(feature = "wasm"))]
    pub(crate) fn kind(self) -> &'static str {
        match self {
            Self::Action(_) => "action",
            Self::Lock(_) => "lock",
        }
    }

    #[cfg(not(feature = "wasm"))]
    pub(crate) fn script_role(self) -> &'static str {
        match self {
            Self::Action(_) => "type",
            Self::Lock(_) => "lock",
        }
    }

    #[cfg(not(feature = "wasm"))]
    pub(crate) fn trigger(self) -> &'a str {
        match self {
            Self::Action(_) => "type-group",
            Self::Lock(_) => "lock-group",
        }
    }
}

impl IrModule {
    /// Explicit scopes select exactly their named callable, even when other
    /// actions are retained as dependencies. Unscoped compilation preserves
    /// Runtime entry precedence: `main`, the first no-argument
    /// action, the first action, then the first Lock Script.
    pub(crate) fn resolved_entry(&self) -> Option<IrEntryPoint<'_>> {
        match &self.entry_selection {
            IrEntrySelection::Action(name) => {
                return self.items.iter().find_map(|item| match item {
                    IrItem::Action(action) if action.name == *name => Some(IrEntryPoint::Action(action)),
                    _ => None,
                });
            }
            IrEntrySelection::Lock(name) => {
                return self.items.iter().find_map(|item| match item {
                    IrItem::Lock(lock) if lock.name == *name => Some(IrEntryPoint::Lock(lock)),
                    _ => None,
                });
            }
            IrEntrySelection::Artifact(_) => return None,
            IrEntrySelection::Implicit => {}
        }
        let actions = || {
            self.items.iter().filter_map(|item| match item {
                IrItem::Action(action) => Some(action),
                _ => None,
            })
        };
        actions()
            .find(|action| action.name == "main")
            .or_else(|| actions().find(|action| action.params.is_empty()))
            .or_else(|| actions().next())
            .map(IrEntryPoint::Action)
            .or_else(|| {
                self.items.iter().find_map(|item| match item {
                    IrItem::Lock(lock) => Some(IrEntryPoint::Lock(lock)),
                    _ => None,
                })
            })
    }

    pub(crate) fn validate_entry_selection(&self) -> Result<()> {
        let (kind, name) = match &self.entry_selection {
            IrEntrySelection::Implicit => return Ok(()),
            IrEntrySelection::Action(name) => ("action", name),
            IrEntrySelection::Lock(name) => ("lock", name),
            IrEntrySelection::Artifact(declaration) => return policy::validate_artifact_selection(self, declaration),
        };
        let matches = self
            .items
            .iter()
            .filter(|item| match (kind, item) {
                ("action", IrItem::Action(action)) => action.name == *name,
                ("lock", IrItem::Lock(lock)) => lock.name == *name,
                _ => false,
            })
            .count();
        if matches != 1 || name.is_empty() {
            return Err(CompileError::without_span(format!(
                "explicit artifact entry {kind} '{name}' must identify exactly one retained callable; found {matches}"
            ))
            .with_code(crate::error::DiagnosticCode::E2101));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct IrEnumLayout {
    pub name: String,
    pub value_abilities: Vec<ValueAbility>,
    pub tag_width: usize,
    pub encoded_size: usize,
    pub variants: Vec<IrEnumVariantLayout>,
    pub contains_linear_payload: bool,
}

impl IrEnumLayout {
    pub fn has_payload(&self) -> bool {
        self.variants.iter().any(|variant| !variant.fields.is_empty())
    }
}

#[derive(Debug, Clone)]
pub struct IrEnumVariantLayout {
    pub name: String,
    pub tag: u8,
    pub payload_width: usize,
    pub fields: Vec<IrEnumFieldLayout>,
}

#[derive(Debug, Clone)]
pub struct IrEnumFieldLayout {
    pub index: usize,
    pub ty: IrType,
    pub offset: usize,
    pub width: usize,
    pub linear: bool,
}

impl IrEnumFieldLayout {
    /// Serialized width does not choose a scalar representation for a record,
    /// tuple or array. Linear payloads retain their existing local handle ABI.
    pub(crate) fn has_scalar_storage(&self) -> bool {
        self.linear
            || (self.width <= 8
                && (matches!(self.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64)
                    || matches!(&self.ty, IrType::Named(name) if is_ckb_temporal_scalar_name(name))))
    }
}

#[derive(Debug, Clone)]
pub struct IrCallableAbi {
    pub name: String,
    pub params: Vec<IrParam>,
    pub type_hash_param_indices: BTreeSet<usize>,
}

#[derive(Debug, Clone)]
pub enum IrItem {
    TypeDef(IrTypeDef),
    Invariant(IrInvariant),
    Action(IrAction),
    Function(IrFunction),
    Lock(IrLock),
}

#[derive(Debug, Clone)]
pub struct IrTypeDef {
    pub name: String,
    pub type_id: Option<String>,
    pub default_hash_type: Option<String>,
    pub capacity_floor_shannons: Option<u64>,
    pub kind: IrTypeKind,
    pub fields: Vec<IrField>,
    pub capabilities: Vec<Capability>,
    pub value_abilities: Vec<ValueAbility>,
    pub claim_output: Option<IrType>,
    pub flow_states: Option<Vec<String>>,
    pub flow_state_field: Option<String>,
    pub flow_rules: Vec<IrFlowRule>,
    pub flow_initial_state: Option<String>,
    pub flow_terminal_states: Vec<String>,
    pub flow_terminal_discharge: Option<String>,
    pub flow_state_model: Option<String>,
    pub flow_audit_warnings: Vec<String>,
    pub validity_predicates: Vec<IrValidityPredicate>,
    /// Identity policy for v0.15 cell identity system
    pub identity: IrIdentityPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrValidityEvidence {
    CheckedStatic,
    CheckedRuntime,
    BuilderEvidenceRequired,
}

#[derive(Debug, Clone)]
pub struct IrValidityPredicate {
    pub expression: Expr,
    pub message: Option<Expr>,
    pub rendered: String,
    pub evidence: IrValidityEvidence,
    pub dependencies: Vec<String>,
    pub runtime_checked_on_create: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrInvariant {
    pub name: String,
    pub trigger: Option<String>,
    pub scope: Option<String>,
    pub reads: Vec<AggregateTarget>,
    pub aggregates: Vec<IrAggregateInvariant>,
    pub quantifiers: Vec<BoundedQuantifier>,
    pub assert_count: usize,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrAggregateInvariant {
    pub kind: AggregateInvariantKind,
    pub target: AggregateTarget,
    pub scope: String,
    pub argument: Option<String>,
    pub relation: Option<AggregateRelation>,
    pub rhs: Option<AggregateTarget>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrFlowRule {
    pub from: String,
    pub to: String,
    pub from_index: usize,
    pub to_index: usize,
}

#[derive(Debug, Clone)]
pub struct IrStateTransitionEdge {
    pub input_binding: Option<String>,
    pub output_binding: Option<String>,
    pub type_name: String,
    pub field_name: String,
    pub from: String,
    pub to: String,
    pub from_index: usize,
    pub to_index: usize,
}

/// Explanatory participant-role candidate retained for ProtocolGraph.
///
/// These records never participate in verifier lowering or capability
/// entailment.  The graph renderer resolves them deterministically and keeps
/// the source attribution so field-name hints cannot be mistaken for
/// authorization evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrProtocolRoleCandidate {
    pub role: String,
    pub source: String,
    pub strength: String,
    pub actor_binding: Option<String>,
    pub actor_source: Option<String>,
    pub cell_binding: Option<String>,
    pub field_name: Option<String>,
    pub predicate: Option<String>,
    pub span: Option<Span>,
}

/// Destruction policy in IR, mirroring the AST-level DestructionPolicy
/// but simplified for codegen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrDestructionPolicy {
    /// Bare `destroy` — legacy v0.14, same as SingletonType
    Default,
    /// `destroy_singleton_type` — proves absence of same-TypeHash output
    SingletonType,
    /// `destroy_unique` — uses TYPE_ID to identify cell
    Unique { identity: String },
    /// `destroy_instance` — identifies by specific field
    Instance { identity_field: String },
    /// `burn_amount` — proves quantity delta, not output absence
    BurnAmount { field: String },
}

/// Identity policy in IR, mirroring the AST-level IdentityPolicy
/// but simplified for codegen and metadata emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrIdentityPolicy {
    /// No identity tracking (default)
    None,
    /// CKB TYPE_ID based identity
    CkbTypeId,
    /// Field-based identity
    Field(String),
    /// Script args based identity
    ScriptArgs,
    /// Singleton type identity
    SingletonType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrTypeKind {
    Resource,
    Shared,
    Receipt,
    Struct,
}

#[derive(Debug, Clone)]
pub struct IrField {
    pub name: String,
    pub ty: IrType,
    pub offset: usize,
    pub fixed_size: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IrType {
    U8,
    U16,
    U32,
    I32,
    U64,
    U128,
    Bool,
    Unit,
    Address,
    Hash,
    Array(Box<IrType>, usize),
    Tuple(Vec<IrType>),
    Named(String),
    Applied(Box<TypeApplication<IrType>>),
    Selection(Box<crate::cell_selection::SelectionType>),
    /// Stack/Molecule vector family. Storage ownership and borrowing remain
    /// operation/provenance contracts, separate from the element type. None
    /// is the constructor's inference hole; it has no source type argument.
    Vector(Option<Box<IrType>>),
    Context(crate::ast::ContextView<Box<IrType>>),
    Bounded(Box<BoundedCollectionType<IrType>>),
    Commitment(Box<IrType>),
    Opening(Box<IrType>),
    OpenHandle(Box<OpenHandleType>),
    WitnessBytes(cellscript_artifact_checker::view_types::WitnessBytesViewType),
    Nested(NestedViewType),
    Ref(Box<IrType>),
    MutRef(Box<IrType>),
}

impl IrType {
    /// Structured views and stack vectors retain their pointer/length helper
    /// transport. This does not grant packed layout or Cell ownership.
    pub(crate) fn has_structured_span_abi(&self) -> bool {
        match self {
            Self::Context(_) | Self::Vector(_) | Self::Nested(_) | Self::Bounded(_) | Self::Opening(_) | Self::WitnessBytes(_) => true,
            Self::Ref(inner) | Self::MutRef(inner) => inner.has_structured_span_abi(),
            _ => false,
        }
    }

    pub fn context(view: crate::ast::ContextView<IrType>) -> Self {
        Self::Context(view.map(|schema| Box::new(schema.clone())))
    }

    pub(crate) fn context_constructor(&self) -> Option<&'static str> {
        let Self::Context(view) = self else { return None };
        Some(view.constructor())
    }

    pub fn vector(element: IrType) -> Self {
        Self::Vector(Some(Box::new(element)))
    }

    pub fn vector_element(&self) -> Option<&IrType> {
        match self {
            Self::Vector(Some(element)) => Some(element),
            Self::Ref(inner) | Self::MutRef(inner) => inner.vector_element(),
            _ => None,
        }
    }
    pub fn applied(constructor: impl Into<String>, arguments: Vec<TypeArgument<IrType>>) -> Self {
        Self::Applied(Box::new(TypeApplication::new(constructor, arguments)))
    }
    pub fn application(&self) -> Option<&TypeApplication<IrType>> {
        match self {
            Self::Applied(application) => Some(application),
            _ => None,
        }
    }
}

/// IR Action
#[derive(Debug, Clone)]
pub struct IrAction {
    pub name: String,
    /// Explicit roles retain declaration order in either coordinate system.
    pub ordered_cell_roles: bool,
    /// Entry selection supplies this policy after source lowering.
    pub entry_cells: Option<IrEntryCells>,
    /// Canonical source-declared disposition plans from the Edition 2027
    /// frontend. The generated body remains the execution authority; these
    /// records prevent typed expansion from re-inferring a weaker legacy
    /// meaning from the lowered operations.
    pub source_dispositions: Vec<IrSourceDisposition>,
    /// Actual source relation occurrences with their executable CFG anchors.
    pub successor_relations: Vec<IrSuccessorRelation>,
    /// Type-checked metadata-only declarations. These are intentionally not
    /// inserted into executable blocks.
    pub audit_claims: Vec<IrAuditClaim>,
    pub params: Vec<IrParam>,
    pub return_type: Option<IrType>,
    pub state_transition_edges: Vec<IrStateTransitionEdge>,
    pub protocol_role_candidates: Vec<IrProtocolRoleCandidate>,
    pub body: IrBody,
    pub effects: EffectFacts,
    pub scheduler_hints: SchedulerHints,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrEntryCells {
    pub bindings: crate::artifact::EntryCellBindings,
    pub coverage: crate::artifact::EntryCoverage,
    pub layout: Option<String>,
}

/// Ordinary source function with inferred external-operation facts.
#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<IrParam>,
    pub return_type: Option<IrType>,
    pub body: IrBody,
    pub effects: EffectFacts,
}

/// IR Lock
#[derive(Debug, Clone)]
pub struct IrLock {
    pub name: String,
    pub effects: EffectFacts,
    pub params: Vec<IrParam>,
    pub audit_claims: Vec<IrAuditClaim>,
    pub body: IrBody,
}

#[derive(Debug, Clone)]
pub struct IrAuditClaim {
    pub name: String,
    pub evidence: String,
    pub subject: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrSourceDisposition {
    pub kind: IrSourceDispositionKind,
    pub input_binding: Option<String>,
    pub output_binding: Option<String>,
    pub data_fields: Vec<(String, String)>,
    pub lock_expression: Option<String>,
    pub lock_treatment: IrSourceLockTreatment,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrSourceLockTreatment {
    None,
    Same,
    ExactAddress,
    ExactScriptHash,
}

impl IrSourceLockTreatment {
    fn from_source(treatment: &ReplaceLockTreatment) -> Self {
        match treatment {
            ReplaceLockTreatment::Same => Self::Same,
            ReplaceLockTreatment::Exact(_) => Self::ExactAddress,
            ReplaceLockTreatment::ExactHash(_) => Self::ExactScriptHash,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IrSuccessorRelation {
    pub occurrence: usize,
    pub input_binding: String,
    pub output_binding: String,
    pub input_local_id: usize,
    pub output_local_id: usize,
    pub data_fields: Vec<(String, String)>,
    pub lock_treatment: IrSourceLockTreatment,
    pub lock_expression: Option<String>,
    pub consume_block: BlockId,
    pub create_block: BlockId,
    pub success_block: BlockId,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum IrSourceDispositionKind {
    PooledInput { pool_id: String, accounting_obligation: String },
    PoolResult { pool_id: String, accounting_obligation: String },
    Retired { absence_policy: String },
    Fresh { identity_policy: String },
}

fn source_lock_expression(treatment: &ReplaceLockTreatment) -> Option<&Expr> {
    match treatment {
        ReplaceLockTreatment::Same => None,
        ReplaceLockTreatment::Exact(value) | ReplaceLockTreatment::ExactHash(value) => Some(value),
    }
}

pub(crate) fn source_disposition_id(entry_id: &str, source: &IrSourceDisposition) -> String {
    match &source.kind {
        IrSourceDispositionKind::PooledInput { pool_id, .. } => format!(
            "disposition:{entry_id}:pooled-input:{pool_id}:{}",
            source.input_binding.as_deref().expect("validated pooled input")
        ),
        IrSourceDispositionKind::PoolResult { pool_id, .. } => format!(
            "disposition:{entry_id}:pool-result:{pool_id}:{}",
            source.output_binding.as_deref().expect("validated pool output")
        ),
        IrSourceDispositionKind::Retired { .. } => {
            format!("disposition:{entry_id}:retired:{}", source.input_binding.as_deref().expect("validated retirement input"))
        }
        IrSourceDispositionKind::Fresh { .. } => {
            format!("disposition:{entry_id}:fresh:{}", source.output_binding.as_deref().expect("validated fresh output"))
        }
    }
}

fn source_identity_policy(policy: &IdentityPolicy) -> String {
    match policy {
        IdentityPolicy::None => "none".to_string(),
        IdentityPolicy::CkbTypeId => "ckb-type-id".to_string(),
        IdentityPolicy::Field(field) => format!("field:{field}"),
        IdentityPolicy::ScriptArgs => "script-args".to_string(),
        IdentityPolicy::SingletonType => "singleton-type".to_string(),
    }
}

fn source_absence_policy(policy: &DestructionPolicy) -> String {
    match policy {
        DestructionPolicy::SingletonType => "same-type-hash-output-absent".to_string(),
        DestructionPolicy::Unique { .. } => "same-ckb-type-id-output-absent".to_string(),
        DestructionPolicy::Instance { identity_field } => format!("same-field-identity-output-absent:{identity_field}"),
        DestructionPolicy::Default | DestructionPolicy::BurnAmount { .. } => {
            unreachable!("native retirement requires an exact absence policy")
        }
    }
}

#[derive(Debug, Clone)]
pub struct IrParam {
    pub name: String,
    pub ty: IrType,
    pub is_mut: bool,
    pub is_ref: bool,
    pub is_read_ref: bool,
    pub source: ParamSource,
    pub binding: IrVar,
}

#[derive(Debug, Clone)]
pub struct IrBody {
    /// Resolved physical locations; later phases must not allocate ordinals.
    pub cell_bindings: Vec<IrCellBinding>,
    pub consume_set: Vec<CellPattern>,
    pub read_refs: Vec<CellPattern>,
    pub create_set: Vec<CreatePattern>,
    pub mutate_set: Vec<MutatePattern>,
    pub write_intents: Vec<WriteIntent>,
    pub bounded_collection_ops: Vec<IrBoundedCollectionOp>,
    pub borrow_regions: Vec<IrBorrowRegion>,
    /// External verifier calls whose target identity is bound in the emitted
    /// machine code and whose trust claim must be supplied by Cell.toml.
    pub trusted_external_calls: Vec<IrTrustedExternalCall>,
    /// Source-level `require`/Edition 2027 `enforce` conditions that were
    /// lowered to an explicit fail-closed branch. These bindings let the
    /// semantic foundation distinguish the claim itself from auxiliary
    /// ProofPlan/runtime obligations.
    pub enforced_claims: Vec<IrEnforcedClaim>,
    pub zk_origins: Vec<cellscript_artifact_checker::zk_profile::ZkSourceOrigin>,
    pub blocks: Vec<IrBlock>,
}

/// Effective control-flow reachability for literal boolean branches. The
/// original block set and IDs remain intact for source maps and ABI bindings.
pub(crate) fn effective_reachable_blocks(body: &IrBody) -> HashSet<BlockId> {
    let mut visited = HashSet::new();
    let mut pending = body.blocks.first().map(|block| block.id).into_iter().collect::<Vec<_>>();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let Some(block) = body.blocks.iter().find(|block| block.id == id) else { continue };
        if block.runtime_error.is_some() {
            continue;
        }
        match &block.terminator {
            IrTerminator::Return(_) => {}
            IrTerminator::Jump(target) => pending.push(*target),
            IrTerminator::Branch { cond: IrOperand::Const(IrConst::Bool(value)), then_block, else_block } => {
                pending.push(if *value { *then_block } else { *else_block });
            }
            IrTerminator::Branch { then_block, else_block, .. } => {
                pending.push(*then_block);
                pending.push(*else_block);
            }
        }
    }
    visited
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrTrustedExternalCall {
    pub operation: String,
    pub adapter: String,
    pub code_hash: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct IrEnforcedClaim {
    pub statement: String,
    pub condition_block: BlockId,
    pub success_block: BlockId,
    pub failure_block: BlockId,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrBorrowRegion {
    pub root: String,
    pub path: Vec<String>,
    pub binding: String,
    pub root_type: String,
    pub view_type: String,
    /// Retain the lexical alias even when only a nested reborrow uses it.
    pub view_binding: IrVar,
    pub start_block: BlockId,
    pub start_instruction: usize,
    pub end_block: Option<BlockId>,
    pub end_instruction: Option<usize>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrBoundedCollectionOp {
    pub operation: String,
    pub binding: String,
    pub collection_binding: String,
    pub collection_type: IrType,
    pub element_type: IrType,
    pub max_elements: usize,
    pub source: ParamSource,
    pub output_type: Option<String>,
    /// Predicates retained from the type-checked bounded body. A selected
    /// runtime contract executes them once per element; fail-closed shapes
    /// retain them only as audit evidence.
    pub predicates: Vec<String>,
    /// Restricted outer numeric `+=` updates executed once per element. These
    /// make aggregate conservation auditable without admitting arbitrary loop
    /// mutation or Cell effects.
    pub accumulator_updates: Vec<String>,
    /// The single type-checked create template retained for `create_each`.
    pub create_template: Option<IrBoundedCreateTemplate>,
    /// Positive consensus-runtime contract selected for this exact shape.
    /// `None` keeps the operation on the explicit fail-closed path.
    pub runtime_contract: Option<String>,
    /// Exact encoded element width for the first executable bounded runtime
    /// contracts. Dynamic element schemas remain fail-closed.
    pub element_width: Option<usize>,
    /// Declared positive capacity floor for an executable bounded output.
    pub output_capacity_floor_shannons: Option<u64>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrBoundedCreateTemplate {
    pub output_type: String,
    pub fields: Vec<(String, String)>,
    pub lock: Option<String>,
}

pub const BOUNDED_CONSUME_EACH_FAIL_CLOSED_HELPER: &str = "__cellscript_bounded_consume_each_fail_closed";
pub const BOUNDED_CREATE_EACH_FAIL_CLOSED_HELPER: &str = "__cellscript_bounded_create_each_fail_closed";
pub const BOUNDED_TYPE_GROUP_INPUTS_V1: &str = "bounded-type-group-inputs-v1";
pub const BOUNDED_OUTPUT_PLAN_V1: &str = "bounded-output-plan-v1";
pub const BOUNDED_OUTPUT_PLAN_MAGIC_V1: &[u8; 8] = b"CSBPLv1\0";
const BOUNDED_CELL_RUNTIME_MAX_DATA_BYTES: usize = 512;
pub const BOUNDED_OUTPUT_PLAN_MAX_BYTES: usize = 4084;

fn direct_bounded_plan_field<'a>(rendered: &'a str, binding: &str) -> Option<&'a str> {
    let field = rendered.strip_prefix(binding)?.strip_prefix('.')?;
    (!field.is_empty() && !field.contains('.')).then_some(field)
}

fn direct_bounded_plan_field_expr<'a>(expr: &'a Expr, binding: &str) -> Option<&'a str> {
    let Expr::FieldAccess(access) = expr else { return None };
    let Expr::Identifier(base) = access.expr.as_ref() else { return None };
    (base == binding).then_some(access.field.as_str())
}

/// Recognized source/runtime contracts with intentionally unavailable execution.
/// Keep this classification shared by metadata and the emitter: a deferred
/// value-producing call must terminate, never return an error as a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrDeferredRuntimeFeature {
    CkbSighashAll,
    CkbSinceConstraints,
    CkbOccupiedCapacityFloor,
}

impl IrDeferredRuntimeFeature {
    pub fn from_helper(helper: &str) -> Option<Self> {
        Self::from_runtime_helper(cellscript_artifact_checker::runtime_contract::RuntimeHelper::from_symbol(helper)?)
    }

    pub fn from_runtime_helper(helper: cellscript_artifact_checker::runtime_contract::RuntimeHelper) -> Option<Self> {
        use cellscript_artifact_checker::runtime_contract::DeferredFeature;
        Some(match helper.deferred()? {
            DeferredFeature::CkbSighashAll => Self::CkbSighashAll,
            DeferredFeature::CkbSinceConstraints => Self::CkbSinceConstraints,
            DeferredFeature::CkbOccupiedCapacityFloor => Self::CkbOccupiedCapacityFloor,
        })
    }

    pub fn from_source_name(name: &str) -> Option<Self> {
        match name {
            "env::sighash_all" => Some(Self::CkbSighashAll),
            _ => None,
        }
    }

    pub const fn feature(self) -> &'static str {
        match self {
            Self::CkbSighashAll => "ckb-sighash-all-deferred",
            Self::CkbSinceConstraints => "ckb-since-constraints-deferred",
            Self::CkbOccupiedCapacityFloor => "ckb-occupied-capacity-deferred",
        }
    }

    pub const fn runtime_error(self) -> CellScriptRuntimeError {
        match self {
            Self::CkbSighashAll => CellScriptRuntimeError::SighashAllUnsupported,
            Self::CkbSinceConstraints => CellScriptRuntimeError::SinceConstraintsUnsupported,
            Self::CkbOccupiedCapacityFloor => CellScriptRuntimeError::OccupiedCapacityUnsupported,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CellPattern {
    pub operation: String,
    pub type_hash: Option<[u8; 32]>,
    pub binding: String,
    pub fields: Vec<(String, IrOperand)>,
}

#[derive(Debug, Clone)]
pub struct CreatePattern {
    pub operation: String,
    pub ty: String,
    pub binding: String,
    pub fields: Vec<(String, IrOperand)>,
    pub lock: Option<IrOperand>,
    pub identity: IrIdentityPolicy,
}

#[derive(Debug, Clone)]
pub struct MutatePattern {
    pub operation: String,
    pub ty: String,
    pub binding: String,
    pub fields: Vec<String>,
    pub preserved_fields: Vec<String>,
    pub transitions: Vec<MutateFieldTransition>,
    pub input_index: usize,
    pub output_index: usize,
    pub preserve_type_hash: bool,
    pub preserve_lock_hash: bool,
}

#[derive(Debug, Clone)]
pub struct WriteIntent {
    pub operation: String,
    pub ty: String,
    pub binding: String,
    pub index: usize,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MutateFieldTransition {
    pub field: String,
    pub op: MutateTransitionOp,
    pub operand: IrOperand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutateTransitionOp {
    Set,
    Add,
    Sub,
    Append,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellMetadataField {
    LockHash,
    Capacity,
}

#[derive(Debug, Clone)]
pub struct IrBlock {
    pub id: BlockId,
    pub instructions: Vec<IrInstruction>,
    pub terminator: IrTerminator,
    pub runtime_error: Option<CellScriptRuntimeError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone)]
pub enum IrInstruction {
    LoadConst {
        dest: IrVar,
        value: IrConst,
    },
    LoadVar {
        dest: IrVar,
        name: String,
    },
    StoreVar {
        name: String,
        src: IrOperand,
    },
    Binary {
        dest: IrVar,
        op: BinaryOp,
        left: IrOperand,
        right: IrOperand,
    },
    Unary {
        dest: IrVar,
        op: UnaryOp,
        operand: IrOperand,
    },
    FieldAccess {
        dest: IrVar,
        obj: IrOperand,
        field: String,
    },
    Index {
        dest: IrVar,
        arr: IrOperand,
        idx: IrOperand,
    },
    Length {
        dest: IrVar,
        operand: IrOperand,
    },
    TypeHash {
        dest: IrVar,
        operand: IrOperand,
    },
    AllocatedValue(IrAllocatedValue),
    CollectionNew {
        dest: IrVar,
        ty: IrType,
        capacity: Option<IrOperand>,
    },
    CollectionCapacity {
        dest: IrVar,
        collection: IrOperand,
    },
    CollectionPush {
        collection: IrOperand,
        value: IrOperand,
    },
    CollectionExtend {
        collection: IrOperand,
        slice: IrOperand,
    },
    CollectionClear {
        collection: IrOperand,
    },
    CollectionContains {
        dest: IrVar,
        collection: IrOperand,
        value: IrOperand,
    },
    CollectionRemove {
        dest: IrVar,
        collection: IrOperand,
        index: IrOperand,
    },
    CollectionInsert {
        collection: IrOperand,
        index: IrOperand,
        value: IrOperand,
    },
    CollectionSet {
        collection: IrOperand,
        index: IrOperand,
        value: IrOperand,
    },
    CollectionPop {
        dest: IrVar,
        collection: IrOperand,
    },
    CollectionReverse {
        collection: IrOperand,
    },
    CollectionTruncate {
        collection: IrOperand,
        len: IrOperand,
    },
    CollectionSwap {
        collection: IrOperand,
        left: IrOperand,
        right: IrOperand,
    },
    /// Load one canonically ordered Cell from the current Type Script group's
    /// input view. `found` is false only for CKB_INDEX_OUT_OF_BOUND. A
    /// successful load at `index >= max_elements` fails with the stable
    /// bounded-cardinality runtime error.
    BoundedCellLoad {
        dest: IrVar,
        found: IrVar,
        index: IrOperand,
        max_elements: usize,
        element_type: String,
        element_width: usize,
    },
    /// Source-committed complete fixed-role selection and correspondence.
    VerifyExactCellRoles {
        dest: Option<IrVar>,
        contract: crate::cell_selection::Contract,
    },
    ReadExactCellRole {
        dest: IrVar,
        selection: IrOperand,
        role: u32,
        read: crate::cell_selection::Read,
        index: Option<IrOperand>,
    },
    /// Decode one element from the canonical versioned Molecule FixVec carried
    /// by the BoundedList witness parameter.
    BoundedPlanLoad {
        dest: IrVar,
        found: IrVar,
        plan: IrOperand,
        index: IrOperand,
        max_elements: usize,
        element_type: String,
        element_width: usize,
    },
    /// Verify the GroupOutput at the plan-relative index against the single
    /// type-checked create template, including lock and capacity policy.
    BoundedOutputVerify {
        index: IrOperand,
        pattern: CreatePattern,
        capacity_floor_shannons: u64,
    },
    /// Prove that the canonical GroupOutput view ends exactly where the plan
    /// ends, rejecting both missing and extra outputs.
    BoundedOutputEnd {
        index: IrOperand,
    },
    Call {
        dest: Option<IrVar>,
        func: IrCallTarget,
        args: Vec<IrOperand>,
    },
    ReadRef {
        dest: IrVar,
        ty: String,
    },
    Move {
        dest: IrVar,
        src: IrOperand,
    },
    Tuple {
        dest: IrVar,
        fields: Vec<IrOperand>,
    },
    EnumConstruct {
        dest: IrVar,
        enum_name: String,
        variant: String,
        fields: Vec<IrOperand>,
    },
    EnumTag {
        dest: IrVar,
        operand: IrOperand,
        enum_name: String,
    },
    EnumPayload {
        dest: IrVar,
        operand: IrOperand,
        enum_name: String,
        variant: String,
        field_index: usize,
    },
    Consume {
        operand: IrOperand,
    },
    Create {
        dest: IrVar,
        pattern: CreatePattern,
    },
    Transfer {
        dest: IrVar,
        operand: IrOperand,
        to: IrOperand,
    },
    Destroy {
        operand: IrOperand,
        policy: IrDestructionPolicy,
    },
    Claim {
        dest: IrVar,
        receipt: IrOperand,
    },
    Settle {
        dest: IrVar,
        operand: IrOperand,
    },
    CreateUnique {
        dest: IrVar,
        pattern: CreatePattern,
        identity: IrIdentityPolicy,
    },
    ReplaceUnique {
        dest: IrVar,
        operand: IrOperand,
        pattern: CreatePattern,
        identity: IrIdentityPolicy,
    },
    CellMetadataEquality {
        left: IrOperand,
        right: IrOperand,
        field: CellMetadataField,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IrVar {
    pub id: usize,
    pub name: String,
    pub ty: IrType,
}

#[derive(Debug, Clone)]
pub enum IrOperand {
    Var(IrVar),
    Const(IrConst),
}

#[derive(Debug, Clone)]
pub enum IrConst {
    Unit,
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Bool(bool),
    Address([u8; 32]),
    Hash([u8; 32]),
    Array(Vec<IrConst>),
}

#[derive(Debug, Clone)]
pub enum IrTerminator {
    Return(Option<IrOperand>),
    Jump(BlockId),
    Branch { cond: IrOperand, then_block: BlockId, else_block: BlockId },
}

pub use cellscript_artifact_checker::effect_facts::EffectFacts;

#[derive(Debug, Clone)]
pub struct SchedulerHints {
    pub parallelizable: bool,
    pub touches_shared: Vec<[u8; 32]>,
    pub estimated_cycles: u64,
}

impl Default for SchedulerHints {
    fn default() -> Self {
        Self { parallelizable: true, touches_shared: Vec::new(), estimated_cycles: 1000 }
    }
}

pub struct IrGenerator<'a> {
    nested_resolver: Option<&'a ModuleResolver>,
    nested_source_records: HashMap<String, StructDef>,
    nested_descriptions: crate::nested_values::Descriptions,
    cell_role_catalogs: HashMap<String, CellRolesDef>,
    collection_pools: HashMap<String, CollectionsDef>,
    module: IrModule,
    var_counter: usize,
    /// Locals whose storage can be reassigned; a new binding must snapshot them.
    mutable_local_ids: HashSet<usize>,
    block_counter: usize,
    aggregate_fields: HashMap<usize, HashMap<String, IrVar>>,
    schema_field_roots: HashMap<usize, (usize, String)>,
    /// Lexical branch ancestry in which a field value was materialized.
    /// Reuse requires a prefix of the current path; a shallower value from
    /// another arm does not dominate a nested sibling arm.
    aggregate_field_contexts: HashMap<usize, Vec<u32>>,
    branch_path: Vec<u32>,
    branch_epoch: u32,
    aggregate_elements: HashMap<usize, Vec<IrVar>>,
    /// Cumulative local-aggregate rebuild components emitted for this module.
    /// Bounds pre-budget lowering work before any post-lowering budget runs.
    aggregate_rebuild_parts: usize,
    mutated_fields: HashMap<usize, BTreeSet<String>>,
    mutated_field_transitions: HashMap<usize, BTreeMap<String, MutateFieldTransition>>,
    transition_param_ids: HashSet<usize>,
    transition_coverable_value_ids: HashSet<usize>,
    type_fields: HashMap<String, HashMap<String, IrType>>,
    type_field_order: HashMap<String, Vec<String>>,
    type_kinds: HashMap<String, IrTypeKind>,
    type_capacity_floors: HashMap<String, u64>,
    type_identities: HashMap<String, IrIdentityPolicy>,
    receipt_claim_outputs: HashMap<String, Option<IrType>>,
    flow_states: HashMap<String, Vec<String>>,
    flow_state_fields: HashMap<String, String>,
    flow_rules: HashMap<String, Vec<IrFlowRule>>,
    flow_initial_states: HashMap<String, String>,
    flow_terminal_states: HashMap<String, Vec<String>>,
    flow_state_models: HashMap<String, String>,
    flow_audit_warnings: HashMap<String, Vec<String>>,
    type_validity: HashMap<String, Vec<IrValidityPredicate>>,
    enum_variants: HashMap<String, HashMap<String, u64>>,
    enum_definitions: HashMap<String, EnumDef>,
    constants: HashMap<String, (Expr, IrType)>,
    function_param_types: HashMap<String, Vec<IrType>>,
    external_function_param_types: HashMap<String, Vec<IrType>>,
    function_return_types: HashMap<String, Option<IrType>>,
    external_function_return_types: HashMap<String, Option<IrType>>,
    call_target_labels: HashMap<String, String>,
    lowering_lock_entry: bool,
    borrow_regions: Vec<IrBorrowRegion>,
    trusted_external_calls: Vec<IrTrustedExternalCall>,
    enforced_claims: Vec<IrEnforcedClaim>,
    successor_relations: Vec<IrSuccessorRelation>,
    zk_origins: Vec<cellscript_artifact_checker::zk_profile::ZkSourceOrigin>,
    loop_targets: Vec<LoopTarget>,
    errors: RefCell<Vec<CompileError>>,
}

#[derive(Clone)]
struct LoopTarget {
    label: Option<String>,
    break_block: BlockId,
    continue_block: BlockId,
}

struct LoweredExpr {
    operand: IrOperand,
    current: Option<BlockId>,
}

fn stmts_contain_loop_control(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Break(_) | Stmt::Continue(_) => true,
        Stmt::If(statement) => {
            stmts_contain_loop_control(&statement.then_branch)
                || statement.else_branch.as_deref().is_some_and(stmts_contain_loop_control)
        }
        Stmt::For(statement) => stmts_contain_loop_control(&statement.body),
        Stmt::While(statement) => stmts_contain_loop_control(&statement.body),
        Stmt::Borrow(statement) => stmts_contain_loop_control(&statement.body),
        Stmt::Expr(Expr::Block(statements)) | Stmt::Discard(Expr::Block(statements)) => stmts_contain_loop_control(statements),
        Stmt::Expr(Expr::If(branch)) | Stmt::Discard(Expr::If(branch)) => {
            expr_contains_loop_control(&branch.then_branch) || expr_contains_loop_control(&branch.else_branch)
        }
        Stmt::Expr(Expr::Match(match_expr)) | Stmt::Discard(Expr::Match(match_expr)) => {
            match_expr.arms.iter().any(|arm| expr_contains_loop_control(&arm.value))
        }
        Stmt::Let(statement) => expr_contains_loop_control(&statement.value),
        Stmt::Return(statement) => statement.value.as_ref().is_some_and(expr_contains_loop_control),
        Stmt::Expr(_) | Stmt::Discard(_) => false,
    })
}

fn expr_contains_loop_control(expr: &Expr) -> bool {
    match expr {
        Expr::Block(statements) => stmts_contain_loop_control(statements),
        Expr::If(branch) => expr_contains_loop_control(&branch.then_branch) || expr_contains_loop_control(&branch.else_branch),
        Expr::Match(match_expr) => match_expr.arms.iter().any(|arm| expr_contains_loop_control(&arm.value)),
        _ => false,
    }
}

fn collect_protocol_role_equalities_from_statements<'a>(statements: &'a [Stmt], equalities: &mut Vec<&'a BinaryExpr>) {
    for statement in statements {
        match statement {
            Stmt::Let(statement) => collect_protocol_role_equalities_from_expr(&statement.value, equalities),
            Stmt::Expr(expr) | Stmt::Discard(expr) => collect_protocol_role_equalities_from_expr(expr, equalities),
            Stmt::Return(statement) => {
                if let Some(value) = &statement.value {
                    collect_protocol_role_equalities_from_expr(value, equalities);
                }
            }
            Stmt::If(statement) => {
                collect_protocol_role_equalities_from_expr(&statement.condition, equalities);
                collect_protocol_role_equalities_from_statements(&statement.then_branch, equalities);
                if let Some(else_branch) = &statement.else_branch {
                    collect_protocol_role_equalities_from_statements(else_branch, equalities);
                }
            }
            Stmt::For(statement) => {
                collect_protocol_role_equalities_from_expr(&statement.iterable, equalities);
                collect_protocol_role_equalities_from_statements(&statement.body, equalities);
            }
            Stmt::While(statement) => {
                collect_protocol_role_equalities_from_expr(&statement.condition, equalities);
                collect_protocol_role_equalities_from_statements(&statement.body, equalities);
            }
            Stmt::Borrow(statement) => collect_protocol_role_equalities_from_statements(&statement.body, equalities),
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }
}

fn collect_protocol_role_equalities_from_expr<'a>(expr: &'a Expr, equalities: &mut Vec<&'a BinaryExpr>) {
    match expr {
        Expr::Require(require) => collect_protocol_role_condition_equalities(&require.condition, equalities),
        Expr::RequireBlock(block) => {
            for condition in &block.expressions {
                collect_protocol_role_condition_equalities(condition, equalities);
            }
        }
        Expr::Assert(assertion) => collect_protocol_role_condition_equalities(&assertion.condition, equalities),
        Expr::Block(statements) => collect_protocol_role_equalities_from_statements(statements, equalities),
        Expr::If(branch) => {
            collect_protocol_role_equalities_from_expr(&branch.condition, equalities);
            collect_protocol_role_equalities_from_expr(&branch.then_branch, equalities);
            collect_protocol_role_equalities_from_expr(&branch.else_branch, equalities);
        }
        Expr::Match(match_expr) => {
            collect_protocol_role_equalities_from_expr(&match_expr.expr, equalities);
            for arm in &match_expr.arms {
                collect_protocol_role_equalities_from_expr(&arm.value, equalities);
            }
        }
        _ => {}
    }
}

fn collect_protocol_role_condition_equalities<'a>(expr: &'a Expr, equalities: &mut Vec<&'a BinaryExpr>) {
    match expr {
        Expr::Binary(binary) if binary.op == BinaryOp::Eq => equalities.push(binary),
        Expr::Binary(binary) if matches!(binary.op, BinaryOp::And | BinaryOp::Or) => {
            collect_protocol_role_condition_equalities(&binary.left, equalities);
            collect_protocol_role_condition_equalities(&binary.right, equalities);
        }
        Expr::Unary(unary) => collect_protocol_role_condition_equalities(&unary.expr, equalities),
        _ => {}
    }
}

fn protocol_role_field_access(expr: &Expr) -> Option<(&str, &str)> {
    let Expr::FieldAccess(access) = expr else {
        return None;
    };
    let Expr::Identifier(binding) = access.expr.as_ref() else {
        return None;
    };
    Some((binding, &access.field))
}

fn protocol_role_actor_param(param: &Param) -> bool {
    matches!(&param.ty, Type::Address) && matches!(param.source, ParamSource::Default | ParamSource::Witness | ParamSource::LockArgs)
}

fn protocol_role_param_source(source: ParamSource) -> &'static str {
    match source {
        ParamSource::Default => "entry-witness-default",
        ParamSource::Witness => "witness",
        ParamSource::LockArgs => "lock_args",
        ParamSource::Input => "input",
        ParamSource::Output => "output",
        ParamSource::Protected => "protected",
    }
}

fn protocol_role_source_rank(source: &str) -> u8 {
    match source {
        "verification-predicate" => 0,
        "witness-binding" | "lock-args-binding" => 1,
        "field-name" => 2,
        _ => u8::MAX,
    }
}

fn protocol_role_name(name: &str) -> Option<String> {
    const ROLE_NAMES: &[&str] = &[
        "admin",
        "arbiter",
        "authority",
        "beneficiary",
        "borrower",
        "buyer",
        "claimant",
        "counterparty",
        "depositor",
        "initiator",
        "issuer",
        "lender",
        "maker",
        "minter",
        "operator",
        "owner",
        "participant",
        "payee",
        "payer",
        "proposer",
        "recipient",
        "refundee",
        "seller",
        "sender",
        "signer",
        "sponsor",
        "taker",
        "withdrawer",
    ];
    let normalized = name.to_ascii_lowercase();
    normalized.split('_').find(|part| ROLE_NAMES.binary_search(part).is_ok()).map(str::to_string)
}

struct IrImportContext {
    type_fields: HashMap<String, HashMap<String, IrType>>,
    type_field_order: HashMap<String, Vec<String>>,
    type_kinds: HashMap<String, IrTypeKind>,
    receipt_claim_outputs: HashMap<String, Option<IrType>>,
    flow_states: HashMap<String, Vec<String>>,
    type_validity: HashMap<String, Vec<IrValidityPredicate>>,
    enum_definitions: HashMap<String, EnumDef>,
    external_function_param_types: HashMap<String, Vec<IrType>>,
    external_function_return_types: HashMap<String, Option<IrType>>,
    call_target_labels: HashMap<String, String>,
}

impl<'scope> IrGenerator<'scope> {
    pub fn new(module_name: String) -> Self {
        Self {
            nested_resolver: None,
            nested_source_records: HashMap::new(),
            nested_descriptions: Default::default(),
            cell_role_catalogs: HashMap::new(),
            collection_pools: HashMap::new(),
            module: IrModule {
                name: module_name,
                items: Vec::new(),
                entry_selection: IrEntrySelection::Implicit,
                external_type_defs: Vec::new(),
                external_callable_abis: Vec::new(),
                enum_fixed_sizes: HashMap::new(),
                enum_layouts: HashMap::new(),
                generic_contracts: BTreeMap::new(),
                source_type_origins: BTreeMap::new(),
                nominal_identities: BTreeMap::new(),
                source_callable_targets: BTreeMap::new(),
            },
            var_counter: 0,
            mutable_local_ids: HashSet::new(),
            block_counter: 0,
            aggregate_fields: HashMap::new(),
            schema_field_roots: HashMap::new(),
            aggregate_field_contexts: HashMap::new(),
            branch_path: Vec::new(),
            branch_epoch: 0,
            aggregate_elements: HashMap::new(),
            aggregate_rebuild_parts: 0,
            mutated_fields: HashMap::new(),
            mutated_field_transitions: HashMap::new(),
            transition_param_ids: HashSet::new(),
            transition_coverable_value_ids: HashSet::new(),
            type_fields: HashMap::new(),
            type_field_order: HashMap::new(),
            type_kinds: HashMap::new(),
            type_capacity_floors: HashMap::new(),
            type_identities: HashMap::new(),
            receipt_claim_outputs: HashMap::new(),
            flow_states: HashMap::new(),
            flow_state_fields: HashMap::new(),
            flow_rules: HashMap::new(),
            flow_initial_states: HashMap::new(),
            flow_terminal_states: HashMap::new(),
            flow_state_models: HashMap::new(),
            flow_audit_warnings: HashMap::new(),
            type_validity: HashMap::new(),
            enum_variants: HashMap::new(),
            enum_definitions: HashMap::new(),
            constants: HashMap::new(),
            function_param_types: HashMap::new(),
            external_function_param_types: HashMap::new(),
            function_return_types: HashMap::new(),
            external_function_return_types: HashMap::new(),
            call_target_labels: HashMap::new(),
            lowering_lock_entry: false,
            borrow_regions: Vec::new(),
            trusted_external_calls: Vec::new(),
            enforced_claims: Vec::new(),
            successor_relations: Vec::new(),
            zk_origins: Vec::new(),
            loop_targets: Vec::new(),
            errors: RefCell::new(Vec::new()),
        }
    }

    pub fn with_type_fields(module_name: String, type_fields: HashMap<String, HashMap<String, IrType>>) -> Self {
        let mut generator = Self::new(module_name);
        generator.type_fields = type_fields;
        generator
    }

    fn with_import_context(module_name: String, context: IrImportContext, resolver: &'scope ModuleResolver) -> Self {
        let IrImportContext {
            type_fields,
            type_field_order,
            type_kinds,
            receipt_claim_outputs,
            flow_states,
            type_validity,
            enum_definitions,
            external_function_param_types,
            external_function_return_types,
            call_target_labels,
        } = context;
        let mut generator = Self::with_type_fields(module_name, type_fields);
        generator.nested_resolver = Some(resolver);
        generator.type_field_order = type_field_order;
        generator.type_kinds.extend(type_kinds);
        generator.receipt_claim_outputs.extend(receipt_claim_outputs);
        generator.flow_states.extend(flow_states);
        generator.type_validity.extend(type_validity);
        for (name, mut definition) in enum_definitions {
            definition.name = name.clone();
            generator.enum_variants.insert(
                name.clone(),
                definition.variants.iter().enumerate().map(|(index, variant)| (variant.name.clone(), index as u64)).collect(),
            );
            generator.enum_definitions.insert(name, definition);
        }
        generator.external_function_param_types = external_function_param_types;
        generator.external_function_return_types = external_function_return_types;
        generator.module.source_callable_targets = call_target_labels.iter().map(|(a, b)| (a.clone(), b.clone())).collect();
        generator.call_target_labels = call_target_labels;
        generator
    }

    pub fn generate(self, ast: &Module) -> Result<IrModule> {
        self.generate_diagnostics(ast)
            .map_err(|errors| errors.into_iter().next().unwrap_or_else(|| CompileError::without_span("IR lowering failed")))
    }

    pub fn generate_diagnostics(self, ast: &Module) -> std::result::Result<IrModule, Vec<CompileError>> {
        crate::generics::validate_specialization_origins(ast).map_err(|error| vec![error])?;
        let mut ir = self.lower_diagnostics(ast)?;
        crate::effects::populate(&mut ir).map_err(|error| vec![error])?;
        Ok(ir)
    }

    pub(crate) fn lower_diagnostics(mut self, ast: &Module) -> std::result::Result<IrModule, Vec<CompileError>> {
        for (name, identity) in crate::resolve::nominal_identities(ast, None) {
            self.module.nominal_identities.entry(name).or_insert(identity);
        }
        self.prepare_nested_types(ast);
        self.type_fields = self
            .type_fields
            .iter()
            .map(|(name, fields)| {
                (name.clone(), fields.iter().map(|(field, ty)| (field.clone(), self.resolve_projection_type(ty))).collect())
            })
            .collect();
        self.external_function_param_types = self
            .external_function_param_types
            .iter()
            .map(|(name, params)| (name.clone(), params.iter().map(|ty| self.resolve_projection_type(ty)).collect()))
            .collect();
        self.external_function_return_types = self
            .external_function_return_types
            .iter()
            .map(|(name, ty)| (name.clone(), ty.as_ref().map(|ty| self.resolve_projection_type(ty))))
            .collect();
        self.collection_pools = ast
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Collections(def) => Some((def.name.clone(), def.clone())),
                _ => None,
            })
            .collect();
        let validity_constant_names = ast
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Const(constant) => Some(constant.name.as_str()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        for item in &ast.items {
            let declaration = match item {
                Item::Resource(value) => Some((&value.name, &value.fields)),
                Item::Shared(value) => Some((&value.name, &value.fields)),
                Item::Receipt(value) => Some((&value.name, &value.fields)),
                Item::Struct(value) => Some((&value.name, &value.fields)),
                _ => None,
            };
            if let Some((name, fields)) = declaration {
                self.type_field_order.insert(name.clone(), fields.iter().map(|field| field.name.clone()).collect());
            }
            if let Item::Const(c) = item {
                self.constants.insert(c.name.clone(), (c.value.clone(), self.convert_type(&c.ty)));
            }
            match item {
                Item::Resource(r) => {
                    self.type_kinds.insert(r.name.clone(), IrTypeKind::Resource);
                    if let Some(floor) = &r.capacity_floor {
                        self.type_capacity_floors.insert(r.name.clone(), floor.shannons);
                    }
                    self.type_identities.insert(r.name.clone(), Self::lower_identity_policy(&r.identity));
                    self.type_fields.insert(
                        r.name.clone(),
                        r.fields.iter().map(|field| (field.name.clone(), self.convert_type(&field.ty))).collect(),
                    );
                    self.type_validity.insert(
                        r.name.clone(),
                        ir_validity_predicates_with_constants(r.validity.as_ref(), &r.fields, &validity_constant_names),
                    );
                }
                Item::Shared(s) => {
                    self.type_kinds.insert(s.name.clone(), IrTypeKind::Shared);
                    if let Some(floor) = &s.capacity_floor {
                        self.type_capacity_floors.insert(s.name.clone(), floor.shannons);
                    }
                    self.type_identities.insert(s.name.clone(), Self::lower_identity_policy(&s.identity));
                    self.type_fields.insert(
                        s.name.clone(),
                        s.fields.iter().map(|field| (field.name.clone(), self.convert_type(&field.ty))).collect(),
                    );
                    self.type_validity.insert(
                        s.name.clone(),
                        ir_validity_predicates_with_constants(s.validity.as_ref(), &s.fields, &validity_constant_names),
                    );
                }
                Item::Receipt(r) => {
                    self.type_kinds.insert(r.name.clone(), IrTypeKind::Receipt);
                    if let Some(floor) = &r.capacity_floor {
                        self.type_capacity_floors.insert(r.name.clone(), floor.shannons);
                    }
                    self.type_identities.insert(r.name.clone(), Self::lower_identity_policy(&r.identity));
                    self.receipt_claim_outputs.insert(r.name.clone(), r.claim_output.as_ref().map(|ty| self.convert_type(ty)));
                    self.type_fields.insert(
                        r.name.clone(),
                        r.fields.iter().map(|field| (field.name.clone(), self.convert_type(&field.ty))).collect(),
                    );
                    self.type_validity.insert(
                        r.name.clone(),
                        ir_validity_predicates_with_constants(r.validity.as_ref(), &r.fields, &validity_constant_names),
                    );
                }
                Item::Struct(s) => {
                    self.type_kinds.insert(s.name.clone(), IrTypeKind::Struct);
                    if let Some(floor) = &s.capacity_floor {
                        self.type_capacity_floors.insert(s.name.clone(), floor.shannons);
                    }
                    self.type_identities.insert(s.name.clone(), IrIdentityPolicy::None);
                    self.type_fields.insert(
                        s.name.clone(),
                        s.fields.iter().map(|field| (field.name.clone(), self.convert_type(&field.ty))).collect(),
                    );
                    self.type_validity.insert(
                        s.name.clone(),
                        ir_validity_predicates_with_constants(s.validity.as_ref(), &s.fields, &validity_constant_names),
                    );
                }
                Item::Enum(e) => {
                    self.enum_definitions.insert(e.name.clone(), e.clone());
                    self.enum_variants.insert(
                        e.name.clone(),
                        e.variants.iter().enumerate().map(|(index, variant)| (variant.name.clone(), index as u64)).collect(),
                    );
                }
                Item::Action(action) => {
                    let params = action.params.iter().map(|param| self.convert_type(&param.ty)).collect::<Vec<_>>();
                    let return_type = action.return_type.as_ref().map(|ty| self.convert_type(ty));
                    self.function_param_types.insert(action.name.clone(), params.clone());
                    self.function_param_types.insert(format!("{}::{}", self.module.name, action.name), params);
                    self.function_return_types.insert(action.name.clone(), return_type.clone());
                    self.function_return_types.insert(format!("{}::{}", self.module.name, action.name), return_type);
                }
                Item::Function(function) => {
                    let params = function.params.iter().map(|param| self.convert_type(&param.ty)).collect::<Vec<_>>();
                    let return_type = function.return_type.as_ref().map(|ty| self.convert_type(ty));
                    self.function_param_types.insert(function.name.clone(), params.clone());
                    self.function_param_types.insert(format!("{}::{}", self.module.name, function.name), params);
                    self.function_return_types.insert(function.name.clone(), return_type.clone());
                    self.function_return_types.insert(format!("{}::{}", self.module.name, function.name), return_type);
                }
                Item::Lock(lock) => {
                    let params = lock.params.iter().map(|param| self.convert_type(&param.ty)).collect::<Vec<_>>();
                    self.function_param_types.insert(lock.name.clone(), params.clone());
                    self.function_param_types.insert(format!("{}::{}", self.module.name, lock.name), params);
                    self.function_return_types.insert(lock.name.clone(), Some(IrType::Bool));
                    self.function_return_types.insert(format!("{}::{}", self.module.name, lock.name), Some(IrType::Bool));
                }
                Item::CellRoles(def) => {
                    self.cell_role_catalogs.insert(def.name.clone(), def.clone());
                }
                Item::Flow(_) => {}
                _ => {}
            }
        }

        self.register_enum_layouts();
        self.register_flows(&ast.items);

        // Every declaration identity is available before any body lowers,
        // including forward declarations and imported alias arguments.
        for item in &ast.items {
            if matches!(item, Item::Resource(_) | Item::Shared(_) | Item::Receipt(_) | Item::Struct(_) | Item::Enum(_))
                && let Some(name) = item.name()
            {
                self.module
                    .source_type_origins
                    .insert(name.into(), Some(DeclarationIdentity { module: ast.name.clone(), name: name.into() }));
            }
        }
        for item in &ast.items {
            match item {
                Item::Resource(r) => {
                    let ir_item = IrItem::TypeDef(self.gen_resource(r));
                    self.module.items.push(ir_item);
                }
                Item::Shared(s) => {
                    let ir_item = IrItem::TypeDef(self.gen_shared(s));
                    self.module.items.push(ir_item);
                }
                Item::Receipt(r) => {
                    let ir_item = IrItem::TypeDef(self.gen_receipt(r));
                    self.module.items.push(ir_item);
                }
                Item::Struct(s) => {
                    let ir_item = IrItem::TypeDef(self.gen_struct(s));
                    self.module.items.push(ir_item);
                }
                Item::Invariant(invariant) => {
                    let ir_item = IrItem::Invariant(self.gen_invariant(invariant));
                    self.module.items.push(ir_item);
                }
                Item::Const(_) | Item::Enum(_) => {}
                Item::Action(a) => {
                    let ir_item = IrItem::Action(self.gen_action(a));
                    self.module.items.push(ir_item);
                }
                Item::Function(f) => {
                    let ir_item = IrItem::Function(self.gen_function(f));
                    self.module.items.push(ir_item);
                }
                Item::Lock(l) => {
                    let ir_item = IrItem::Lock(self.gen_lock(l));
                    self.module.items.push(ir_item);
                }
                Item::DataShape(_) | Item::CellRoles(_) | Item::Collections(_) => {}
                Item::Flow(_) => {}
                Item::Use(_) => {}
            }
        }
        for item in &ast.items {
            let (name, kind) = match item {
                Item::Struct(def) => (&def.name, IrGenericKind::Struct),
                Item::Enum(def) => (&def.name, IrGenericKind::Enum),
                Item::Function(def) => (&def.name, IrGenericKind::Function),
                _ => continue,
            };
            match IrGenericContract::from_ast(ast, name, kind) {
                Ok(Some(contract)) => {
                    self.module.generic_contracts.insert(name.clone(), contract);
                }
                Ok(None) => {}
                Err(error) => self.errors.borrow_mut().push(error),
            }
        }
        if self.errors.borrow().is_empty() {
            Ok(self.module)
        } else {
            Err(self.errors.into_inner())
        }
    }

    fn convert_type(&self, ty: &Type) -> IrType {
        self.resolve_projection_type(&ast_type_to_ir(ty))
    }

    fn lower_signature_and_body(
        &mut self,
        params: &[Param],
        outputs: &[crate::ast::ActionOutput],
        stmts: &[Stmt],
        return_type: Option<IrType>,
        core_input_bindings: &HashSet<String>,
    ) -> (Vec<IrParam>, IrBody) {
        self.mutable_local_ids.clear();
        self.borrow_regions.clear();
        self.enforced_claims.clear();
        self.zk_origins.clear();
        self.loop_targets.clear();
        let mut vars = HashMap::new();
        let mut ir_params = params
            .iter()
            .map(|param| {
                let binding = self.new_var(param.name.clone(), self.convert_type(&param.ty));
                vars.insert(param.name.clone(), binding.clone());
                IrParam {
                    name: param.name.clone(),
                    ty: self.convert_type(&param.ty),
                    is_mut: param.is_mut,
                    is_ref: param.is_ref,
                    is_read_ref: param.is_read_ref,
                    source: param.source,
                    binding,
                }
            })
            .collect::<Vec<_>>();
        for output in outputs {
            let binding = self.new_var(output.name.clone(), self.convert_type(&output.ty));
            vars.insert(output.name.clone(), binding.clone());
            ir_params.push(IrParam {
                name: output.name.clone(),
                ty: self.convert_type(&output.ty),
                is_mut: false,
                is_ref: false,
                is_read_ref: false,
                source: ParamSource::Output,
                binding,
            });
        }
        self.mutable_local_ids.extend(ir_params.iter().filter(|param| param.is_mut).map(|param| param.binding.id));
        self.transition_param_ids = ir_params.iter().map(|param| param.binding.id).collect();
        let mut blocks = Vec::new();
        let entry = self.push_block(&mut blocks);
        let tail_expr_returns = return_type.is_some();
        let _ = self.lower_stmts(stmts, entry, &mut blocks, &mut vars, return_type.as_ref(), tail_expr_returns);
        let consume_set = self.collect_consume_patterns(&blocks, &ir_params, core_input_bindings);
        let mut read_refs = self.collect_read_ref_param_patterns(&ir_params);
        read_refs.extend(self.collect_read_ref_patterns(&blocks));
        let create_set = self.collect_create_patterns(&blocks, &ir_params);
        let mutate_set = self.collect_mutate_param_patterns(&ir_params, consume_set.len(), create_set.len());
        let write_intents = Self::collect_write_intents(&create_set, &mutate_set);
        let mut bounded_collection_ops = collect_bounded_collection_ops(stmts, params);
        for operation in &mut bounded_collection_ops {
            if operation.operation == "consume_each"
                && let Some(width) =
                    self.bounded_consume_runtime_width(&operation.element_type, operation.source, operation.max_elements)
            {
                operation.runtime_contract = Some(BOUNDED_TYPE_GROUP_INPUTS_V1.to_string());
                operation.element_width = Some(width);
            } else if operation.operation == "create_each"
                && let Some(width) = self.bounded_create_runtime_width(operation)
            {
                operation.runtime_contract = Some(BOUNDED_OUTPUT_PLAN_V1.to_string());
                operation.element_width = Some(width);
                operation.output_capacity_floor_shannons =
                    operation.output_type.as_deref().and_then(|output_type| self.type_capacity_floors.get(output_type).copied());
            }
        }
        let borrow_regions = std::mem::take(&mut self.borrow_regions);
        let trusted_external_calls = std::mem::take(&mut self.trusted_external_calls);
        let enforced_claims = std::mem::take(&mut self.enforced_claims);
        self.transition_param_ids.clear();
        self.transition_coverable_value_ids.clear();

        (
            ir_params,
            IrBody {
                cell_bindings: Vec::new(),
                consume_set,
                read_refs,
                create_set,
                mutate_set,
                write_intents,
                bounded_collection_ops,
                borrow_regions,
                trusted_external_calls,
                enforced_claims,
                zk_origins: std::mem::take(&mut self.zk_origins),
                blocks,
            },
        )
    }

    fn new_var(&mut self, name: impl Into<String>, ty: IrType) -> IrVar {
        let id = self.var_counter;
        self.var_counter += 1;
        IrVar { id, name: name.into(), ty }
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.block_counter);
        self.block_counter += 1;
        id
    }

    fn push_block(&mut self, blocks: &mut Vec<IrBlock>) -> BlockId {
        let id = self.new_block();
        blocks.push(IrBlock { id, instructions: Vec::new(), terminator: IrTerminator::Return(None), runtime_error: None });
        id
    }

    fn block_mut<'a>(&self, blocks: &'a mut [IrBlock], id: BlockId) -> &'a mut IrBlock {
        &mut blocks[id.0]
    }

    fn operand_type(&self, operand: &IrOperand) -> IrType {
        match operand {
            IrOperand::Var(var) => var.ty.clone(),
            IrOperand::Const(value) => self.const_type(value),
        }
    }

    fn lower_match_pattern_operand(&mut self, pattern: &str, span: Span) -> Option<IrOperand> {
        if pattern == "_" {
            return Some(IrOperand::Const(IrConst::U64(0)));
        }
        if let Some((_, variant)) = self.lower_enum_variant(pattern) {
            return Some(variant);
        }
        if let Some(constant) = self.lower_constant(pattern, span) {
            return Some(constant);
        }
        self.record_error(format!("match pattern '{}' is not supported by IR lowering", pattern), span);
        None
    }

    fn record_error(&self, message: impl Into<String>, span: Span) {
        self.errors.borrow_mut().push(CompileError::new(message, span));
    }
}

fn ir_type_display(ty: &IrType) -> String {
    ty.source_text()
}

fn collection_item_ir_type(ty: &IrType) -> Option<IrType> {
    ty.vector_element().cloned()
}

const CKB_LOCK_SCRIPT_REF_TYPE: &str = "__ckb_lock_script_ref";
const CKB_TYPE_SCRIPT_REF_TYPE: &str = "__ckb_type_script_ref";
const CKB_INPUT_OUT_POINT_REF_TYPE: &str = "__ckb_input_out_point_ref";
const CKB_SCRIPT_ARGS_TYPE: &str = "ScriptArgs";
const CKB_SCRIPT_VALUE_TYPE: &str = "Script";
pub(crate) const CKB_SCRIPT_HASH_TYPE: &str = "ScriptHash";
const CKB_SIGHASH_ALL_DIGEST_TYPE: &str = "SighashAllDigest";

fn typed_view_property_runtime_helper(ty: &IrType, field: &str) -> Option<(&'static str, &'static str, IrType)> {
    if matches!(ty, IrType::Named(name) if name == CKB_INPUT_OUT_POINT_REF_TYPE) {
        return match field {
            "index" => Some(("__ckb_input_out_point_index", "typed_out_point_index", IrType::U32)),
            "tx_hash" => Some(("__ckb_input_out_point_tx_hash", "typed_out_point_tx_hash", IrType::Hash)),
            _ => None,
        };
    }
    let IrType::Context(view) = ty else { return None };
    use crate::ast::ContextView;
    match (view, field) {
        (ContextView::Input(_) | ContextView::Output(_) | ContextView::CellDep, "capacity") => {
            Some(("__ckb_cell_capacity", "typed_view_capacity", IrType::U64))
        }
        (ContextView::Input(_) | ContextView::Output(_) | ContextView::CellDep, "occupied_capacity") => {
            Some(("__ckb_cell_occupied_capacity", "typed_view_occupied_capacity", IrType::U64))
        }
        (ContextView::Input(_) | ContextView::Output(_) | ContextView::CellDep, "unoccupied_capacity") => {
            Some(("__ckb_cell_unoccupied_capacity", "typed_view_unoccupied_capacity", IrType::U64))
        }
        (ContextView::Input(_) | ContextView::Output(_) | ContextView::CellDep, "data_size") => {
            Some(("__ckb_cell_data_size", "typed_view_data_size", IrType::U64))
        }
        (ContextView::Input(_) | ContextView::Output(_) | ContextView::CellDep, "data_hash") => {
            Some(("__ckb_cell_data_hash_field", "typed_view_data_hash", IrType::Hash))
        }
        (ContextView::Output(_), "output_index") => Some(("__ckb_cell_output_index", "typed_view_output_index", IrType::U64)),
        (ContextView::Input(_), "since") => {
            Some(("__ckb_input_since_at", "typed_view_since", IrType::Named("EncodedSince".to_string())))
        }
        (ContextView::Input(_) | ContextView::Output(_) | ContextView::CellDep, "lock_hash") => {
            Some(("__ckb_cell_lock_hash", "typed_view_lock_hash", IrType::Named(CKB_SCRIPT_HASH_TYPE.to_string())))
        }
        (ContextView::Input(_) | ContextView::Output(_) | ContextView::CellDep, "type_hash") => {
            Some(("__ckb_cell_type_hash", "typed_view_type_hash", IrType::Named(CKB_SCRIPT_HASH_TYPE.to_string())))
        }
        (ContextView::WitnessArgs, "size") => Some(("__ckb_witness_size", "typed_witness_size", IrType::U64)),
        (ContextView::HeaderDep, "epoch_number") => {
            Some(("__ckb_header_dep_epoch_number", "typed_header_epoch_number", IrType::Named("EpochNumber".to_string())))
        }
        (ContextView::HeaderDep, "epoch_start_block_number") => Some((
            "__ckb_header_dep_epoch_start_block_number",
            "typed_header_epoch_start_block_number",
            IrType::Named("BlockNumber".to_string()),
        )),
        (ContextView::HeaderDep, "epoch_length") => {
            Some(("__ckb_header_dep_epoch_length", "typed_header_epoch_length", IrType::Named("EpochLength".to_string())))
        }
        (ContextView::HeaderDep, "block_number") => {
            Some(("__ckb_header_dep_block_number", "typed_header_block_number", IrType::Named("BlockNumber".to_string())))
        }
        (ContextView::HeaderDep, "timestamp") => {
            Some(("__ckb_header_dep_timestamp_millis", "typed_header_timestamp_millis", IrType::Named("TimestampMillis".to_string())))
        }
        _ => None,
    }
}

fn typed_view_property_wrapper(ty: &IrType, field: &str) -> Option<&'static str> {
    let IrType::Context(view) = ty else { return None };
    use crate::ast::ContextView;
    match (view, field) {
        (ContextView::Input(_), "out_point") => Some(CKB_INPUT_OUT_POINT_REF_TYPE),
        (ContextView::Input(_) | ContextView::Output(_) | ContextView::CellDep, "lock") => Some(CKB_LOCK_SCRIPT_REF_TYPE),
        (ContextView::Input(_) | ContextView::Output(_) | ContextView::CellDep, "type" | "type_script") => {
            Some(CKB_TYPE_SCRIPT_REF_TYPE)
        }
        _ => None,
    }
}

fn script_ref_property_runtime_helper(ty: &IrType, field: &str) -> Option<(&'static str, &'static str, IrType)> {
    let IrType::Named(name) = ty else {
        return None;
    };
    let lock_script = name == CKB_LOCK_SCRIPT_REF_TYPE;
    let type_script = name == CKB_TYPE_SCRIPT_REF_TYPE;
    if !lock_script && !type_script {
        return None;
    }
    match field {
        "hash" => Some((
            if lock_script { "__ckb_cell_lock_hash" } else { "__ckb_cell_type_hash" },
            if lock_script { "ckb_cell_lock_hash" } else { "ckb_cell_type_hash" },
            IrType::Named(CKB_SCRIPT_HASH_TYPE.to_string()),
        )),
        "code_hash" => Some((
            if lock_script { "__ckb_cell_lock_code_hash" } else { "__ckb_cell_type_code_hash" },
            if lock_script { "ckb_cell_lock_code_hash" } else { "ckb_cell_type_code_hash" },
            IrType::Hash,
        )),
        "hash_type" => Some((
            if lock_script { "__ckb_cell_lock_hash_type" } else { "__ckb_cell_type_hash_type" },
            if lock_script { "ckb_cell_lock_hash_type" } else { "ckb_cell_type_hash_type" },
            IrType::U64,
        )),
        "args_empty" => Some((
            if lock_script { "__ckb_cell_lock_args_empty" } else { "__ckb_cell_type_args_empty" },
            if lock_script { "ckb_cell_lock_args_empty" } else { "ckb_cell_type_args_empty" },
            IrType::Bool,
        )),
        "args32" => Some((
            if lock_script { "__ckb_cell_lock_args32" } else { "__ckb_cell_type_args32" },
            if lock_script { "ckb_cell_lock_args32" } else { "ckb_cell_type_args32" },
            IrType::Array(Box::new(IrType::U8), 32),
        )),
        _ => None,
    }
}

fn fixed_byte_width_for_script_args_var(var: &IrVar) -> Option<usize> {
    match &var.ty {
        IrType::Hash | IrType::Address => Some(32),
        IrType::Array(inner, len) if matches!(inner.as_ref(), IrType::U8) => Some(*len),
        _ => None,
    }
}

fn fixed_byte_width_for_script_args_operand(operand: &IrOperand, ty: &IrType) -> Option<usize> {
    match operand {
        IrOperand::Const(IrConst::Hash(_)) | IrOperand::Const(IrConst::Address(_)) => Some(32),
        IrOperand::Const(IrConst::Array(items)) => Some(items.len()),
        IrOperand::Var(var) => fixed_byte_width_for_script_args_var(var),
        _ => match ty {
            IrType::Hash | IrType::Address => Some(32),
            IrType::Array(inner, len) if matches!(inner.as_ref(), IrType::U8) => Some(*len),
            _ => None,
        },
    }
}

fn checked_cast_maximum(source: &IrType, target: &IrType) -> Option<IrConst> {
    let source_bits = unsigned_ir_bits(source)?;
    let target_bits = unsigned_ir_bits(target)?;
    if target_bits >= source_bits || source_bits > 64 {
        return None;
    }
    let maximum = if target_bits == 64 { u64::MAX } else { (1u64 << target_bits) - 1 };
    match source {
        IrType::U8 => u8::try_from(maximum).ok().map(IrConst::U8),
        IrType::U16 => u16::try_from(maximum).ok().map(IrConst::U16),
        IrType::U32 => u32::try_from(maximum).ok().map(IrConst::U32),
        IrType::U64 | IrType::Named(_) => Some(IrConst::U64(maximum)),
        _ => None,
    }
}

fn unsigned_ir_bits(ty: &IrType) -> Option<u32> {
    match ty {
        IrType::U8 => Some(8),
        IrType::U16 => Some(16),
        IrType::U32 => Some(32),
        IrType::U64 => Some(64),
        IrType::U128 => Some(128),
        IrType::Named(name) if name == "usize" => Some(64),
        _ => None,
    }
}

fn ir_validity_predicates(validity: Option<&ValidityBlock>, fields: &[Field]) -> Vec<IrValidityPredicate> {
    let Some(validity) = validity else {
        return Vec::new();
    };
    let field_names = fields.iter().map(|field| field.name.as_str()).collect::<HashSet<_>>();
    validity
        .predicates
        .iter()
        .filter_map(|predicate| {
            let Expr::Require(require) = predicate else {
                return None;
            };
            let mut dependencies = BTreeSet::new();
            collect_validity_dependencies(&require.condition, &field_names, &mut dependencies);
            let evidence = if validity_expr_uses_block_number(&require.condition) {
                IrValidityEvidence::BuilderEvidenceRequired
            } else if matches!(require.condition.as_ref(), Expr::Bool(true)) {
                IrValidityEvidence::CheckedStatic
            } else {
                IrValidityEvidence::CheckedRuntime
            };
            Some(IrValidityPredicate {
                expression: (*require.condition).clone(),
                message: require.message.as_deref().cloned(),
                rendered: crate::fmt::format_expression(&require.condition),
                evidence,
                dependencies: dependencies.into_iter().collect(),
                runtime_checked_on_create: evidence == IrValidityEvidence::CheckedRuntime,
                span: require.span,
            })
        })
        .collect()
}

fn ir_validity_predicates_with_constants(
    validity: Option<&ValidityBlock>,
    fields: &[Field],
    constant_names: &HashSet<&str>,
) -> Vec<IrValidityPredicate> {
    let mut predicates = ir_validity_predicates(validity, fields);
    for predicate in &mut predicates {
        let mut dependencies = predicate.dependencies.iter().cloned().collect::<BTreeSet<_>>();
        collect_validity_constant_dependencies(&predicate.expression, constant_names, &mut dependencies);
        predicate.dependencies = dependencies.into_iter().collect();
    }
    predicates
}

fn collect_validity_constant_dependencies(expr: &Expr, constants: &HashSet<&str>, dependencies: &mut BTreeSet<String>) {
    match expr {
        // Relations are lifecycle syntax and are rejected in validity
        // predicates before lowering; recurse defensively into their values.
        Expr::ReplaceRelation(relation) => {
            for value in replace_relation_inner_exprs(relation) {
                collect_validity_constant_dependencies(value, constants, dependencies);
            }
        }
        Expr::Identifier(name) => {
            if constants.contains(name.as_str()) {
                dependencies.insert(format!("constant:{name}"));
            }
        }
        Expr::Call(call) => {
            for arg in &call.args {
                collect_validity_constant_dependencies(arg, constants, dependencies);
            }
        }
        Expr::Binary(binary) => {
            collect_validity_constant_dependencies(&binary.left, constants, dependencies);
            collect_validity_constant_dependencies(&binary.right, constants, dependencies);
        }
        Expr::Unary(unary) => collect_validity_constant_dependencies(&unary.expr, constants, dependencies),
        Expr::FieldAccess(field) => collect_validity_constant_dependencies(&field.expr, constants, dependencies),
        Expr::Index(index) => {
            collect_validity_constant_dependencies(&index.expr, constants, dependencies);
            collect_validity_constant_dependencies(&index.index, constants, dependencies);
        }
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                collect_validity_constant_dependencies(item, constants, dependencies);
            }
        }
        Expr::Cast(cast) => collect_validity_constant_dependencies(&cast.expr, constants, dependencies),
        Expr::Range(range) => {
            collect_validity_constant_dependencies(&range.start, constants, dependencies);
            collect_validity_constant_dependencies(&range.end, constants, dependencies);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_validity_constant_dependencies(value, constants, dependencies);
            }
        }
        Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::ByteString(_)
        | Expr::Assign(_)
        | Expr::Create(_)
        | Expr::Consume(_)
        | Expr::Destroy(_)
        | Expr::ReadRef(_)
        | Expr::Claim(_)
        | Expr::Settle(_)
        | Expr::CreateUnique(_)
        | Expr::ReplaceUnique(_)
        | Expr::Assert(_)
        | Expr::Require(_)
        | Expr::RequireBlock(_)
        | Expr::Preserve(_)
        | Expr::Block(_)
        | Expr::If(_)
        | Expr::Match(_)
        | Expr::StdlibCall(_) => {}
    }
}

fn validity_expr_uses_block_number(expr: &Expr) -> bool {
    match expr {
        Expr::ReplaceRelation(relation) => {
            replace_relation_inner_exprs(relation).iter().any(|value| validity_expr_uses_block_number(value))
        }
        Expr::Call(call) => {
            matches!(call.func.as_ref(), Expr::Identifier(name) if name == "env::block_number")
                || validity_expr_uses_block_number(&call.func)
                || call.args.iter().any(validity_expr_uses_block_number)
        }
        Expr::Binary(binary) => validity_expr_uses_block_number(&binary.left) || validity_expr_uses_block_number(&binary.right),
        Expr::Unary(unary) => validity_expr_uses_block_number(&unary.expr),
        Expr::FieldAccess(field) => validity_expr_uses_block_number(&field.expr),
        Expr::Index(index) => validity_expr_uses_block_number(&index.expr) || validity_expr_uses_block_number(&index.index),
        Expr::Tuple(items) | Expr::Array(items) => items.iter().any(validity_expr_uses_block_number),
        Expr::Cast(cast) => validity_expr_uses_block_number(&cast.expr),
        Expr::Range(range) => validity_expr_uses_block_number(&range.start) || validity_expr_uses_block_number(&range.end),
        Expr::StructInit(init) => init.fields.iter().any(|(_, value)| validity_expr_uses_block_number(value)),
        Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::ByteString(_)
        | Expr::Identifier(_)
        | Expr::Assign(_)
        | Expr::Create(_)
        | Expr::Consume(_)
        | Expr::Destroy(_)
        | Expr::ReadRef(_)
        | Expr::Claim(_)
        | Expr::Settle(_)
        | Expr::CreateUnique(_)
        | Expr::ReplaceUnique(_)
        | Expr::Assert(_)
        | Expr::Require(_)
        | Expr::RequireBlock(_)
        | Expr::Preserve(_)
        | Expr::Block(_)
        | Expr::If(_)
        | Expr::Match(_)
        | Expr::StdlibCall(_) => false,
    }
}

fn collect_validity_dependencies(expr: &Expr, fields: &HashSet<&str>, dependencies: &mut BTreeSet<String>) {
    match expr {
        Expr::ReplaceRelation(relation) => {
            for value in replace_relation_inner_exprs(relation) {
                collect_validity_dependencies(value, fields, dependencies);
            }
        }
        Expr::Identifier(name) => {
            if fields.contains(name.as_str()) {
                dependencies.insert(format!("field:{name}"));
            }
        }
        Expr::Call(call) => {
            if let Expr::Identifier(name) = call.func.as_ref() {
                if name == "env::block_number" {
                    dependencies.insert("environment:env::block_number".to_string());
                    dependencies.insert("builder:header-dep-block-number-evidence".to_string());
                } else if !matches!(name.as_str(), "Address::zero" | "Hash::zero") {
                    dependencies.insert(format!("helper:{name}"));
                }
            }
            for arg in &call.args {
                collect_validity_dependencies(arg, fields, dependencies);
            }
        }
        Expr::Binary(binary) => {
            collect_validity_dependencies(&binary.left, fields, dependencies);
            collect_validity_dependencies(&binary.right, fields, dependencies);
        }
        Expr::Unary(unary) => collect_validity_dependencies(&unary.expr, fields, dependencies),
        Expr::FieldAccess(field) => collect_validity_dependencies(&field.expr, fields, dependencies),
        Expr::Index(index) => {
            collect_validity_dependencies(&index.expr, fields, dependencies);
            collect_validity_dependencies(&index.index, fields, dependencies);
        }
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                collect_validity_dependencies(item, fields, dependencies);
            }
        }
        Expr::Cast(cast) => collect_validity_dependencies(&cast.expr, fields, dependencies),
        Expr::Range(range) => {
            collect_validity_dependencies(&range.start, fields, dependencies);
            collect_validity_dependencies(&range.end, fields, dependencies);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_validity_dependencies(value, fields, dependencies);
            }
        }
        Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::ByteString(_)
        | Expr::Assign(_)
        | Expr::Create(_)
        | Expr::Consume(_)
        | Expr::Destroy(_)
        | Expr::ReadRef(_)
        | Expr::Claim(_)
        | Expr::Settle(_)
        | Expr::CreateUnique(_)
        | Expr::ReplaceUnique(_)
        | Expr::Assert(_)
        | Expr::Require(_)
        | Expr::RequireBlock(_)
        | Expr::Preserve(_)
        | Expr::Block(_)
        | Expr::If(_)
        | Expr::Match(_)
        | Expr::StdlibCall(_) => {}
    }
}

fn qualify_external_validity_predicates(type_def: &mut IrTypeDef, owner_module: &str, resolver: &ModuleResolver) {
    let field_names = type_def.fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>();
    let fields = field_names.iter().map(String::as_str).collect::<HashSet<_>>();
    for predicate in &mut type_def.validity_predicates {
        let mut external_constant_dependencies = BTreeSet::new();
        qualify_validity_dependencies(&mut predicate.expression, owner_module, resolver, &fields, &mut external_constant_dependencies);
        let mut dependencies = BTreeSet::new();
        collect_validity_dependencies(&predicate.expression, &fields, &mut dependencies);
        dependencies.extend(external_constant_dependencies);
        predicate.dependencies = dependencies.into_iter().collect();
    }
}

fn qualify_validity_dependencies(
    expr: &mut Expr,
    owner_module: &str,
    resolver: &ModuleResolver,
    fields: &HashSet<&str>,
    constant_dependencies: &mut BTreeSet<String>,
) {
    match expr {
        Expr::ReplaceRelation(relation) => {
            if let ReplaceLockTreatment::Exact(lock) | ReplaceLockTreatment::ExactHash(lock) = &mut relation.lock {
                qualify_validity_dependencies(lock, owner_module, resolver, fields, constant_dependencies);
            }
            if let ReplaceDataTreatment::Fields(treatments) = &mut relation.data {
                for treatment in treatments {
                    if let ReplaceFieldTreatment::Assign(_, value) = treatment {
                        qualify_validity_dependencies(value, owner_module, resolver, fields, constant_dependencies);
                    }
                }
            }
        }
        Expr::Call(call) => {
            if let Expr::Identifier(name) = call.func.as_mut()
                && let Some((callee_module, function)) = resolver.resolve_function_with_module(owner_module, name)
            {
                *name = format!("{}::{}", callee_module, function_def_name(&function));
            }
            qualify_validity_dependencies(&mut call.func, owner_module, resolver, fields, constant_dependencies);
            for arg in &mut call.args {
                qualify_validity_dependencies(arg, owner_module, resolver, fields, constant_dependencies);
            }
        }
        Expr::Binary(binary) => {
            qualify_validity_dependencies(&mut binary.left, owner_module, resolver, fields, constant_dependencies);
            qualify_validity_dependencies(&mut binary.right, owner_module, resolver, fields, constant_dependencies);
        }
        Expr::Unary(unary) => qualify_validity_dependencies(&mut unary.expr, owner_module, resolver, fields, constant_dependencies),
        Expr::FieldAccess(field) => {
            qualify_validity_dependencies(&mut field.expr, owner_module, resolver, fields, constant_dependencies)
        }
        Expr::Index(index) => {
            qualify_validity_dependencies(&mut index.expr, owner_module, resolver, fields, constant_dependencies);
            qualify_validity_dependencies(&mut index.index, owner_module, resolver, fields, constant_dependencies);
        }
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                qualify_validity_dependencies(item, owner_module, resolver, fields, constant_dependencies);
            }
        }
        Expr::Cast(cast) => qualify_validity_dependencies(&mut cast.expr, owner_module, resolver, fields, constant_dependencies),
        Expr::Range(range) => {
            qualify_validity_dependencies(&mut range.start, owner_module, resolver, fields, constant_dependencies);
            qualify_validity_dependencies(&mut range.end, owner_module, resolver, fields, constant_dependencies);
        }
        Expr::StructInit(init) => {
            for (_, value) in &mut init.fields {
                qualify_validity_dependencies(value, owner_module, resolver, fields, constant_dependencies);
            }
        }
        Expr::Identifier(name) if !fields.contains(name.as_str()) => {
            let source_name = name.clone();
            if let Some((constant_module, constant)) = resolver.resolve_constant_with_module(owner_module, &source_name) {
                constant_dependencies.insert(format!("constant:{constant_module}::{}", constant.name));
                *expr = constant.value;
                qualify_validity_dependencies(expr, owner_module, resolver, fields, constant_dependencies);
            }
        }
        Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::ByteString(_)
        | Expr::Identifier(_)
        | Expr::Assign(_)
        | Expr::Create(_)
        | Expr::Consume(_)
        | Expr::Destroy(_)
        | Expr::ReadRef(_)
        | Expr::Claim(_)
        | Expr::Settle(_)
        | Expr::CreateUnique(_)
        | Expr::ReplaceUnique(_)
        | Expr::Assert(_)
        | Expr::Require(_)
        | Expr::RequireBlock(_)
        | Expr::Preserve(_)
        | Expr::Block(_)
        | Expr::If(_)
        | Expr::Match(_)
        | Expr::StdlibCall(_) => {}
    }
}

pub fn generate(ast: &Module) -> Result<IrModule> {
    let generator = IrGenerator::new(ast.name.clone());
    generator.generate(ast)
}

pub fn generate_diagnostics(ast: &Module) -> std::result::Result<IrModule, Vec<CompileError>> {
    let generator = IrGenerator::new(ast.name.clone());
    generator.generate_diagnostics(ast)
}

pub fn generate_with_resolver(ast: &Module, resolver: &ModuleResolver, module_name: &str) -> Result<IrModule> {
    generate_with_resolver_diagnostics(ast, resolver, module_name).map_err(collapse_ir_errors)
}

pub fn generate_with_resolver_diagnostics(
    ast: &Module,
    resolver: &ModuleResolver,
    module_name: &str,
) -> std::result::Result<IrModule, Vec<CompileError>> {
    let mut ir = lower_with_resolver_diagnostics(ast, resolver, module_name)?;
    crate::effects::populate(&mut ir).map_err(|error| vec![error])?;
    Ok(ir)
}

pub(crate) fn lower_with_resolver_diagnostics(
    ast: &Module,
    resolver: &ModuleResolver,
    module_name: &str,
) -> std::result::Result<IrModule, Vec<CompileError>> {
    generate_with_resolver_diagnostics_inner(ast, resolver, module_name, true)
}

fn collapse_ir_errors(errors: Vec<CompileError>) -> CompileError {
    errors.into_iter().next().unwrap_or_else(|| CompileError::without_span("IR lowering failed"))
}

fn generate_with_resolver_inner(
    ast: &Module,
    resolver: &ModuleResolver,
    module_name: &str,
    include_external_callables: bool,
) -> Result<IrModule> {
    generate_with_resolver_diagnostics_inner(ast, resolver, module_name, include_external_callables).map_err(collapse_ir_errors)
}

#[derive(Default)]
struct ExternalCallableContext {
    function_param_types: HashMap<String, Vec<IrType>>,
    function_return_types: HashMap<String, Option<IrType>>,
    target_labels: HashMap<String, String>,
    abis: Vec<IrCallableAbi>,
    names: HashSet<String>,
}

fn generate_with_resolver_diagnostics_inner(
    ast: &Module,
    resolver: &ModuleResolver,
    module_name: &str,
    include_external_callables: bool,
) -> std::result::Result<IrModule, Vec<CompileError>> {
    let mut type_fields = HashMap::new();
    let mut type_kinds = HashMap::new();
    let mut receipt_claim_outputs = HashMap::new();
    let mut flow_states = HashMap::new();
    let mut external_type_defs = Vec::new();
    let mut external_type_names = HashSet::new();
    let mut external_enum_definitions = HashMap::new();
    let mut external_callables = ExternalCallableContext::default();

    let mut resolved_external_types: Vec<(String, String, TypeDef)> = Vec::new();
    let mut imported_generic_contracts = BTreeMap::new();
    let mut source_type_origins = BTreeMap::new();
    let mut pending_types = BTreeSet::new();

    let mut retain_type = |local_name: String, owner: String, type_def: TypeDef, identity: DeclarationIdentity| {
        source_type_origins.insert(local_name.clone(), Some(identity.clone()));
        if let Some(kind) = resolver_type_kind(&type_def) {
            type_kinds.insert(local_name.clone(), kind);
        }
        if let Some(output) = resolver_receipt_claim_output_to_ir(&type_def) {
            receipt_claim_outputs.insert(local_name.clone(), output);
        }
        if let Some(states) = resolver_flow_states_to_ir(&type_def) {
            flow_states.insert(local_name.clone(), states);
        }
        if let Some(fields) = resolver_type_fields_to_ir(&type_def) {
            type_fields.insert(local_name.clone(), fields);
        }
        if external_type_names.insert(local_name.clone()) {
            resolved_external_types.push((local_name, owner, type_def));
        }
    };

    for item in &ast.items {
        let Item::Use(use_stmt) = item else {
            continue;
        };

        for import in &use_stmt.imports {
            let local_name = import.alias.clone().unwrap_or_else(|| import.name.clone());
            let owner = use_stmt.module_path.join("::");
            if let Some(owner_ast) = resolver.module(&owner)
                && owner_ast.specializations.contains_key(&import.name)
            {
                let kind = match resolver.resolve_type(module_name, &local_name) {
                    Some(TypeDef::Struct(_)) => IrGenericKind::Struct,
                    Some(TypeDef::Enum(_)) => IrGenericKind::Enum,
                    _ => IrGenericKind::Function,
                };
                if let Some(contract) = IrGenericContract::from_ast(owner_ast, &import.name, kind).map_err(|error| vec![error])? {
                    imported_generic_contracts.insert(local_name.clone(), contract);
                }
            }
            if let Some(type_def) = resolver.resolve_type(module_name, &local_name) {
                // A Cell view can expose a row even when the reader has no
                // explicit import for that record. Follow the resolver's
                // field spelling so a local same-name record cannot stand in.
                if matches!(type_def, TypeDef::Resource(_) | TypeDef::Shared(_) | TypeDef::Receipt(_))
                    && let Some(fields) = resolver_type_fields_to_ir(&type_def)
                {
                    for field in fields.values() {
                        if matches!(field, IrType::Array(element, _) if matches!(element.as_ref(), IrType::Named(_))) {
                            nominal::collect_names(field, &mut pending_types);
                        }
                    }
                }
                let identity = resolver.resolve_type_identity(module_name, &local_name).ok_or_else(|| {
                    vec![CompileError::new(
                        format!("imported type '{local_name}' has no resolved defining declaration"),
                        use_stmt.span,
                    )
                    .with_code(crate::error::DiagnosticCode::E2111)]
                })?;
                let mut bindings = vec![local_name.clone()];
                if !resolver.module(&owner).is_some_and(|module| module.specializations.contains_key(&import.name)) {
                    bindings.push(format!("{owner}::{}", import.name));
                }
                for local_name in bindings {
                    retain_type(local_name, owner.clone(), type_def.clone(), identity.clone());
                }
            }
            if let Some((owner_module, function)) = resolver.resolve_function_with_module(module_name, &local_name) {
                register_external_callable_context(module_name, &owner_module, &local_name, &function, &mut external_callables)
                    .map_err(|error| vec![error])?;
            }
        }
    }

    let mut source_calls = collect_call_names(ast).into_iter().collect::<Vec<_>>();
    source_calls.sort();
    for call_name in source_calls {
        if let Some((owner, function)) = resolver.resolve_function_with_module(module_name, &call_name) {
            register_external_callable_context(module_name, &owner, &call_name, &function, &mut external_callables)
                .map_err(|error| vec![error])?;
        }
    }
    for ty in
        external_callables.function_param_types.values().flatten().chain(external_callables.function_return_types.values().flatten())
    {
        nominal::collect_names(ty, &mut pending_types);
    }
    for item in &ast.items {
        let (params, result) = match item {
            Item::Action(def) => (def.params.as_slice(), def.return_type.as_ref()),
            Item::Function(def) => (def.params.as_slice(), def.return_type.as_ref()),
            Item::Lock(def) => (def.params.as_slice(), Some(&def.return_type)),
            _ => continue,
        };
        for ty in params.iter().map(|param| &param.ty).chain(result) {
            nominal::collect_names(&ast_type_to_ir(ty), &mut pending_types);
        }
    }
    let mut visited_types = BTreeSet::new();
    while let Some(name) = pending_types.pop_first() {
        if !visited_types.insert(name.clone()) {
            continue;
        }
        let Some(definition) = resolver.resolve_type(module_name, &name) else { continue };
        let Some(identity) = resolver.resolve_type_identity(module_name, &name) else { continue };
        if identity.module != module_name
            && let Some(owner) = resolver.module(&identity.module)
            && owner.specializations.contains_key(&identity.name)
        {
            let kind = match &definition {
                TypeDef::Struct(_) => IrGenericKind::Struct,
                TypeDef::Enum(_) => IrGenericKind::Enum,
                _ => continue,
            };
            if let Some(contract) = IrGenericContract::from_ast(owner, &identity.name, kind).map_err(|error| vec![error])? {
                imported_generic_contracts.insert(name.clone(), contract);
            }
        }
        if let Some(fields) = resolver_type_fields_to_ir(&definition) {
            for ty in fields.values() {
                nominal::collect_names(ty, &mut pending_types);
            }
        }
        if let TypeDef::Enum(definition) = &definition {
            for ty in definition.variants.iter().flat_map(|variant| &variant.fields) {
                nominal::collect_names(&ast_type_to_ir(ty), &mut pending_types);
            }
        }
        if let Some(Some(output)) = resolver_receipt_claim_output_to_ir(&definition) {
            nominal::collect_names(&output, &mut pending_types);
        }
        if identity.module != module_name || name != definition.name() {
            retain_type(name, identity.module.clone(), definition, identity);
        }
    }

    for (local_name, owner_module, type_def) in resolved_external_types {
        if let TypeDef::Enum(mut enum_def) = type_def.clone() {
            enum_def.name = local_name.clone();
            external_enum_definitions.insert(local_name.clone(), enum_def);
        }
        if let Some(mut ir_type_def) = resolver_type_def_to_ir(&local_name, &type_def, &type_fields) {
            qualify_external_validity_predicates(&mut ir_type_def, &owner_module, resolver);
            external_type_defs.push(ir_type_def);
        }
    }
    let imported_type_validity = external_type_defs
        .iter()
        .map(|type_def| (type_def.name.clone(), type_def.validity_predicates.clone()))
        .collect::<HashMap<_, _>>();

    let mut call_names = HashSet::new();
    for predicates in imported_type_validity.values() {
        for predicate in predicates {
            collect_call_names_from_expr(&predicate.expression, &mut call_names);
        }
    }
    let mut ordered_call_names = call_names.into_iter().collect::<Vec<_>>();
    ordered_call_names.sort();
    for call_name in ordered_call_names {
        if let Some((owner_module, function)) = resolver.resolve_function_with_module(module_name, &call_name) {
            register_external_callable_context(module_name, &owner_module, &call_name, &function, &mut external_callables)
                .map_err(|error| vec![error])?;
        }
    }

    let ExternalCallableContext {
        function_param_types: external_function_param_types,
        function_return_types: external_function_return_types,
        target_labels: call_target_labels,
        abis: mut external_callable_abis,
        names: _,
    } = external_callables;
    let mut generator = IrGenerator::with_import_context(
        ast.name.clone(),
        IrImportContext {
            type_fields,
            type_field_order: external_type_defs
                .iter()
                .map(|definition| (definition.name.clone(), definition.fields.iter().map(|field| field.name.clone()).collect()))
                .collect(),
            type_kinds,
            receipt_claim_outputs,
            flow_states,
            type_validity: imported_type_validity,
            enum_definitions: external_enum_definitions,
            external_function_param_types,
            external_function_return_types,
            call_target_labels,
        },
        resolver,
    );
    generator.module.source_type_origins.extend(source_type_origins);
    generator.module.nominal_identities = crate::resolve::nominal_identities(ast, Some(resolver));
    for name in generator.module.source_type_origins.keys() {
        generator
            .module
            .nominal_identities
            .entry(name.clone())
            .or_insert_with(|| crate::resolve::resolved_type_identity(ast, Some(resolver), module_name, &Type::Named(name.clone())));
    }
    generator.prepare_nested_types(ast);
    for definition in &mut external_type_defs {
        for field in &mut definition.fields {
            field.ty = generator.resolve_projection_type(&field.ty);
        }
    }
    for abi in &mut external_callable_abis {
        for param in &mut abi.params {
            param.ty = generator.resolve_projection_type(&param.ty);
            param.binding.ty = param.ty.clone();
        }
    }
    let mut ir = generator.lower_diagnostics(ast)?;
    ir.generic_contracts.extend(imported_generic_contracts);
    ir.external_type_defs = external_type_defs;
    ir.external_callable_abis = external_callable_abis;
    if include_external_callables {
        append_external_callable_bodies(&mut ir, ast, resolver, module_name).map_err(|error| vec![error])?;
    }
    Ok(ir)
}

fn append_external_callable_bodies(ir: &mut IrModule, ast: &Module, resolver: &ModuleResolver, module_name: &str) -> Result<()> {
    let mut known_callables = ir_callable_names(ir);
    let mut imported_callables = HashSet::new();
    let mut pending = collect_call_names(ast)
        .into_iter()
        .map(|call_name| PendingExternalCall { lookup_module: module_name.to_string(), call_name })
        .collect::<Vec<_>>();
    let mut validity_calls = HashSet::new();
    for type_def in &ir.external_type_defs {
        for predicate in &type_def.validity_predicates {
            collect_call_names_from_expr(&predicate.expression, &mut validity_calls);
        }
    }
    pending
        .extend(validity_calls.into_iter().map(|call_name| PendingExternalCall { lookup_module: module_name.to_string(), call_name }));

    // Discovery uses sets, but emitted callable order is part of ELF identity.
    // Source aliases and randomized hash seeds must not choose body addresses.
    pending.sort_by(|left, right| (&left.lookup_module, &left.call_name).cmp(&(&right.lookup_module, &right.call_name)));

    while let Some(pending_call) = pending.pop() {
        let Some((owner_module, function)) =
            resolver.resolve_function_with_module(&pending_call.lookup_module, &pending_call.call_name)
        else {
            continue;
        };
        let symbol = function_def_name(&function).to_string();
        let label = callable_label_for(module_name, &owner_module, &symbol);
        if known_callables.contains(&label) {
            continue;
        }

        let import_key = format!("{}::{}", owner_module, symbol);
        if owner_module == module_name || !imported_callables.insert(import_key) {
            continue;
        }

        let Some(owner_ast) = resolver.module(&owner_module) else {
            continue;
        };
        let mut external_ir = generate_with_resolver_inner(owner_ast, resolver, &owner_module, false)?;
        link_types::prepare(ir, &mut external_ir)?;
        merge_external_type_defs(ir, &external_ir);
        merge_external_callable_abis(ir, &external_ir);
        for (name, contract) in &external_ir.generic_contracts {
            if contract.kind != IrGenericKind::Function {
                ir.generic_contracts.entry(name.clone()).or_insert_with(|| contract.clone());
            }
        }
        if let Some(contract) = external_ir.generic_contracts.get(&symbol) {
            ir.generic_contracts.insert(label.clone(), contract.clone());
        }
        for (name, size) in external_ir.enum_fixed_sizes {
            ir.enum_fixed_sizes.entry(name).or_insert(size);
        }
        for (name, layout) in external_ir.enum_layouts {
            ir.enum_layouts.entry(name).or_insert(layout);
        }
        // The dependency's IR calls have already been lowered. Recover their
        // resolved source bindings from its index rather than trying to parse
        // an assembly label as a source path. Aliases index one definition.
        let call_sources = external_ir.source_callable_targets.into_iter().map(|(source, label)| (label, source)).collect();

        if let Some(mut item) =
            external_ir.items.into_iter().find(|item| ir_item_callable_name(item).is_some_and(|name| name == symbol))
        {
            rewrite_external_callable_item_calls(&mut item, &owner_module, module_name, resolver, &call_sources, &mut pending);
            rename_ir_item_callable(&mut item, label.clone());
            if known_callables.insert(label) {
                ir.items.push(item);
            }
        }
    }

    Ok(())
}

fn register_external_callable_context(
    consumer_module: &str,
    owner_module: &str,
    source_name: &str,
    function: &FunctionDef,
    context: &mut ExternalCallableContext,
) -> Result<()> {
    let symbol = function_def_name(function);
    let full_path = format!("{}::{}", owner_module, symbol);
    let label = callable_label_for(consumer_module, owner_module, symbol);
    let params = function_def_param_types_ir(function);
    let return_type = function_def_return_type(function);

    for key in [source_name.to_string(), full_path.clone(), label.clone()] {
        context.function_param_types.insert(key.clone(), params.clone());
        context.function_return_types.insert(key, return_type.clone());
    }
    context.target_labels.insert(source_name.to_string(), label.clone());
    context.target_labels.entry(full_path).or_insert_with(|| label.clone());
    if owner_module != consumer_module {
        push_external_callable_abi(&mut context.abis, &mut context.names, label, function);
    }
    Ok(())
}

fn callable_label_for(consumer_module: &str, owner_module: &str, symbol: &str) -> String {
    if owner_module == consumer_module {
        symbol.to_string()
    } else {
        // Hex preserves the complete defining module, including separators and
        // Unicode. The first dot after the hex component separates the exact
        // source symbol. Source identifiers cannot contain dots, so this label
        // namespace also cannot collide with an ordinary source declaration.
        // Keep the symbol intact for existing structured mono-name consumers.
        format!("__cellscript.ext.{}.{}", hex::encode(owner_module.as_bytes()), symbol)
    }
}

fn function_def_name(function: &FunctionDef) -> &str {
    match function {
        FunctionDef::Action(action) => &action.name,
        FunctionDef::Function(function) => &function.name,
        FunctionDef::Lock(lock) => &lock.name,
    }
}

#[derive(Debug, Clone)]
struct PendingExternalCall {
    lookup_module: String,
    call_name: String,
}

fn ir_callable_names(ir: &IrModule) -> HashSet<String> {
    ir.items.iter().filter_map(ir_item_callable_name).map(str::to_string).collect()
}

fn ir_item_callable_name(item: &IrItem) -> Option<&str> {
    match item {
        IrItem::Action(action) => Some(&action.name),
        IrItem::Function(function) => Some(&function.name),
        IrItem::Lock(lock) => Some(&lock.name),
        IrItem::TypeDef(_) | IrItem::Invariant(_) => None,
    }
}

fn merge_external_type_defs(ir: &mut IrModule, external_ir: &IrModule) {
    for (name, identity) in &external_ir.nominal_identities {
        let retained = ir.nominal_identities.entry(name.clone()).or_insert_with(|| identity.clone());
        if retained != identity {
            *retained = None;
        }
    }
    for (name, origin) in &external_ir.source_type_origins {
        let retained = ir.source_type_origins.entry(name.clone()).or_insert_with(|| origin.clone());
        if retained != origin {
            *retained = None;
        }
    }
    let mut names = ir.external_type_defs.iter().map(|type_def| type_def.name.clone()).collect::<HashSet<_>>();
    for type_def in &external_ir.external_type_defs {
        if names.insert(type_def.name.clone()) {
            ir.external_type_defs.push(type_def.clone());
        }
    }
    for item in &external_ir.items {
        let IrItem::TypeDef(type_def) = item else {
            continue;
        };
        if names.insert(type_def.name.clone()) {
            ir.external_type_defs.push(type_def.clone());
        }
    }
}

fn merge_external_callable_abis(ir: &mut IrModule, external_ir: &IrModule) {
    let mut names = ir.external_callable_abis.iter().map(|abi| abi.name.clone()).collect::<HashSet<_>>();
    for abi in &external_ir.external_callable_abis {
        if names.insert(abi.name.clone()) {
            ir.external_callable_abis.push(abi.clone());
        }
    }
}

fn rename_ir_item_callable(item: &mut IrItem, label: String) {
    match item {
        IrItem::Action(action) => action.name = label,
        IrItem::Function(function) => function.name = label,
        IrItem::Lock(lock) => lock.name = label,
        IrItem::TypeDef(_) | IrItem::Invariant(_) => {}
    }
}

fn rewrite_external_callable_item_calls(
    item: &mut IrItem,
    owner_module: &str,
    entry_module: &str,
    resolver: &ModuleResolver,
    call_sources: &HashMap<String, String>,
    pending: &mut Vec<PendingExternalCall>,
) {
    match item {
        IrItem::Action(action) => {
            rewrite_external_body_calls(&mut action.body, owner_module, entry_module, resolver, call_sources, pending)
        }
        IrItem::Function(function) => {
            rewrite_external_body_calls(&mut function.body, owner_module, entry_module, resolver, call_sources, pending)
        }
        IrItem::Lock(lock) => rewrite_external_body_calls(&mut lock.body, owner_module, entry_module, resolver, call_sources, pending),
        IrItem::TypeDef(_) | IrItem::Invariant(_) => {}
    }
}

fn rewrite_external_body_calls(
    body: &mut IrBody,
    owner_module: &str,
    entry_module: &str,
    resolver: &ModuleResolver,
    call_sources: &HashMap<String, String>,
    pending: &mut Vec<PendingExternalCall>,
) {
    for block in &mut body.blocks {
        for instruction in &mut block.instructions {
            if let IrInstruction::Call { func: IrCallTarget::Function(func), .. } = instruction {
                let original = func.clone();
                let source_name = call_sources.get(&original).unwrap_or(&original);
                let Some((callee_module, function)) = resolver.resolve_function_with_module(owner_module, source_name) else {
                    continue;
                };
                let symbol = function_def_name(&function).to_string();
                *func = callable_label_for(entry_module, &callee_module, &symbol);
                if callee_module != entry_module {
                    pending.push(PendingExternalCall { lookup_module: owner_module.to_string(), call_name: source_name.clone() });
                }
            }
        }
    }
}

fn push_external_callable_abi(
    external_callable_abis: &mut Vec<IrCallableAbi>,
    external_callable_names: &mut HashSet<String>,
    name: String,
    function: &FunctionDef,
) {
    if !external_callable_names.insert(name.clone()) {
        return;
    }
    external_callable_abis.push(IrCallableAbi {
        name,
        params: function_def_params(function),
        type_hash_param_indices: BTreeSet::new(),
    });
}

fn function_def_params(function: &FunctionDef) -> Vec<IrParam> {
    let params = match function {
        FunctionDef::Action(action) => &action.params,
        FunctionDef::Function(function) => &function.params,
        FunctionDef::Lock(lock) => &lock.params,
    };
    params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let ty = ast_type_to_ir(&param.ty);
            IrParam {
                name: param.name.clone(),
                ty: ty.clone(),
                is_mut: param.is_mut,
                is_ref: param.is_ref,
                is_read_ref: param.is_read_ref,
                source: param.source,
                binding: IrVar { id: index, name: param.name.clone(), ty },
            }
        })
        .collect()
}

fn function_def_param_types_ir(function: &FunctionDef) -> Vec<IrType> {
    let params = match function {
        FunctionDef::Action(action) => &action.params,
        FunctionDef::Function(function) => &function.params,
        FunctionDef::Lock(lock) => &lock.params,
    };
    params.iter().map(|param| ast_type_to_ir(&param.ty)).collect()
}

fn collect_call_names(ast: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &ast.items {
        match item {
            Item::Action(action) => collect_call_names_from_stmts(&action.body, &mut names),
            Item::Function(function) => collect_call_names_from_stmts(&function.body, &mut names),
            Item::Lock(lock) => collect_call_names_from_stmts(&lock.body, &mut names),
            Item::Resource(resource) => collect_call_names_from_validity(resource.validity.as_ref(), &mut names),
            Item::Shared(shared) => collect_call_names_from_validity(shared.validity.as_ref(), &mut names),
            Item::Receipt(receipt) => collect_call_names_from_validity(receipt.validity.as_ref(), &mut names),
            Item::Struct(struct_def) => collect_call_names_from_validity(struct_def.validity.as_ref(), &mut names),
            Item::Collections(_)
            | Item::DataShape(_)
            | Item::CellRoles(_)
            | Item::Flow(_)
            | Item::Invariant(_)
            | Item::Const(_)
            | Item::Enum(_)
            | Item::Use(_) => {}
        }
    }
    names
}

fn collect_call_names_from_validity(validity: Option<&ValidityBlock>, names: &mut HashSet<String>) {
    if let Some(validity) = validity {
        for predicate in &validity.predicates {
            collect_call_names_from_expr(predicate, names);
        }
    }
}

fn collect_bounded_collection_ops(stmts: &[Stmt], params: &[Param]) -> Vec<IrBoundedCollectionOp> {
    let mut ops = Vec::new();
    collect_bounded_collection_ops_from_stmts(stmts, params, &mut ops);
    ops
}

fn collect_bounded_collection_ops_from_stmts(stmts: &[Stmt], params: &[Param], ops: &mut Vec<IrBoundedCollectionOp>) {
    for stmt in stmts {
        match stmt {
            Stmt::Expr(expr)
            | Stmt::Discard(expr)
            | Stmt::Let(LetStmt { value: expr, .. })
            | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                collect_bounded_collection_ops_from_expr(expr, params, ops);
            }
            Stmt::If(if_stmt) => {
                collect_bounded_collection_ops_from_expr(&if_stmt.condition, params, ops);
                collect_bounded_collection_ops_from_stmts(&if_stmt.then_branch, params, ops);
                if let Some(else_branch) = &if_stmt.else_branch {
                    collect_bounded_collection_ops_from_stmts(else_branch, params, ops);
                }
            }
            Stmt::For(for_stmt) => {
                collect_bounded_collection_ops_from_expr(&for_stmt.iterable, params, ops);
                collect_bounded_collection_ops_from_stmts(&for_stmt.body, params, ops);
            }
            Stmt::While(while_stmt) => {
                collect_bounded_collection_ops_from_expr(&while_stmt.condition, params, ops);
                collect_bounded_collection_ops_from_stmts(&while_stmt.body, params, ops);
            }
            Stmt::Borrow(borrow_stmt) => collect_bounded_collection_ops_from_stmts(&borrow_stmt.body, params, ops),
            Stmt::Return(ReturnStmt { value: None, .. }) => {}
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }
}

fn collect_bounded_collection_ops_from_expr(expr: &Expr, params: &[Param], ops: &mut Vec<IrBoundedCollectionOp>) {
    let Expr::Call(call) = expr else {
        return;
    };
    let Expr::Identifier(operation) = call.func.as_ref() else {
        return;
    };
    if !matches!(operation.as_str(), "__cellscript_consume_each" | "__cellscript_create_each") {
        return;
    }
    let [Expr::Identifier(binding), Expr::Identifier(collection_binding), Expr::Block(body)] = call.args.as_slice() else {
        return;
    };
    let Some(param) = params.iter().find(|param| param.name == *collection_binding) else {
        return;
    };
    let Some(collection) = parse_bounded_collection_type(&param.ty) else {
        return;
    };
    let output_type = body.iter().find_map(|stmt| match stmt {
        Stmt::Expr(Expr::Create(create)) | Stmt::Discard(Expr::Create(create)) => Some(create.ty.clone()),
        _ => None,
    });
    let predicates = body
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Expr(Expr::Require(require)) | Stmt::Discard(Expr::Require(require)) => {
                Some(vec![crate::fmt::format_expression(&require.condition)])
            }
            Stmt::Expr(Expr::RequireBlock(require_block)) | Stmt::Discard(Expr::RequireBlock(require_block)) => {
                Some(require_block.expressions.iter().map(crate::fmt::format_expression).collect())
            }
            _ => None,
        })
        .flatten()
        .collect();
    let accumulator_updates = body
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Expr(expr @ Expr::Assign(assign)) | Stmt::Discard(expr @ Expr::Assign(assign))
                if assign.op == AssignOp::AddAssign =>
            {
                Some(crate::fmt::format_expression(expr))
            }
            _ => None,
        })
        .collect();
    let create_template = body.iter().find_map(|stmt| match stmt {
        Stmt::Expr(Expr::Create(create)) | Stmt::Discard(Expr::Create(create)) => Some(IrBoundedCreateTemplate {
            output_type: create.ty.clone(),
            fields: create.fields.iter().map(|(field, value)| (field.clone(), crate::fmt::format_expression(value))).collect(),
            lock: create.lock.as_deref().map(crate::fmt::format_expression),
        }),
        _ => None,
    });
    ops.push(IrBoundedCollectionOp {
        operation: operation.trim_start_matches("__cellscript_").to_string(),
        binding: binding.clone(),
        collection_binding: collection_binding.clone(),
        collection_type: ast_type_to_ir(&param.ty),
        element_type: ast_type_to_ir(&collection.element_type),
        max_elements: collection.max_elements,
        source: param.source,
        output_type,
        predicates,
        accumulator_updates,
        create_template,
        runtime_contract: None,
        element_width: None,
        output_capacity_floor_shannons: None,
        span: call.span,
    });
}

fn function_def_return_type(function: &FunctionDef) -> Option<IrType> {
    match function {
        FunctionDef::Action(action) => action.return_type.as_ref().map(ast_type_to_ir),
        FunctionDef::Function(function) => function.return_type.as_ref().map(ast_type_to_ir),
        FunctionDef::Lock(_) => Some(IrType::Bool),
    }
}

pub(crate) fn ast_type_to_ir(ty: &Type) -> IrType {
    if let Some(view) = ty.as_witness_bytes_view() {
        return IrType::WitnessBytes(view);
    }
    if let Some(handle) = ty.as_open_handle() {
        return IrType::OpenHandle(Box::new(handle));
    }
    if let Some(value) = ty.commitment_value_type() {
        return IrType::Commitment(Box::new(ast_type_to_ir(value)));
    }
    if let Some(value) = ty.opening_value_type() {
        return IrType::Opening(Box::new(ast_type_to_ir(value)));
    }
    if let Some(collection) = parse_bounded_collection_type(ty) {
        return IrType::Bounded(Box::new(collection.map_element(ast_type_to_ir)));
    }
    if let Some(view) = ty.as_context_view() {
        return IrType::Context(view.map(|schema| Box::new(ast_type_to_ir(schema))));
    }
    match ty {
        Type::U8 => IrType::U8,
        Type::U16 => IrType::U16,
        Type::U32 => IrType::U32,
        Type::I32 => IrType::I32,
        Type::U64 => IrType::U64,
        Type::U128 => IrType::U128,
        Type::Bool => IrType::Bool,
        Type::Unit => IrType::Unit,
        Type::Address => IrType::Address,
        Type::Hash => IrType::Hash,
        Type::Array(elem, size) => IrType::Array(Box::new(ast_type_to_ir(elem)), *size),
        Type::Tuple(types) => IrType::Tuple(types.iter().map(ast_type_to_ir).collect()),
        Type::Named(name) if name == "Vec" => IrType::Vector(None),
        Type::Named(name) => IrType::Named(name.clone()),
        Type::Applied(application) if application.unary_type("Vec").is_some() => {
            IrType::Vector(application.unary_type("Vec").map(|element| Box::new(ast_type_to_ir(element))))
        }
        Type::Applied(application) => IrType::Applied(Box::new(application.map_types(ast_type_to_ir))),
        Type::Selection(selection) => IrType::Selection(selection.clone()),
        Type::Ref(inner) => IrType::Ref(Box::new(ast_type_to_ir(inner))),
        Type::MutRef(inner) => IrType::MutRef(Box::new(ast_type_to_ir(inner))),
    }
}

fn resolver_type_fields_to_ir(type_def: &TypeDef) -> Option<HashMap<String, IrType>> {
    let fields = match type_def {
        TypeDef::Resource(resource) => &resource.fields,
        TypeDef::Shared(shared) => &shared.fields,
        TypeDef::Receipt(receipt) => &receipt.fields,
        TypeDef::Struct(struct_def) => &struct_def.fields,
        TypeDef::Enum(_) => return None,
    };

    Some(fields.iter().map(|field| (field.name.clone(), ast_type_to_ir(&field.ty))).collect())
}

fn resolver_type_def_to_ir(
    local_name: &str,
    type_def: &TypeDef,
    type_fields: &HashMap<String, HashMap<String, IrType>>,
) -> Option<IrTypeDef> {
    match type_def {
        TypeDef::Resource(resource) => Some(IrTypeDef {
            name: local_name.to_string(),
            type_id: resource.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: resource.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: resource.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Resource,
            fields: layout_resolver_fields(&resource.fields, type_fields),
            capabilities: resource.capabilities.clone(),
            value_abilities: Vec::new(),
            claim_output: None,
            flow_states: None,
            flow_state_field: None,
            flow_rules: Vec::new(),
            flow_initial_state: None,
            flow_terminal_states: Vec::new(),
            flow_terminal_discharge: None,
            flow_state_model: None,
            flow_audit_warnings: Vec::new(),
            validity_predicates: ir_validity_predicates(resource.validity.as_ref(), &resource.fields),
            identity: lower_identity_policy_ast(&resource.identity),
        }),
        TypeDef::Shared(shared) => Some(IrTypeDef {
            name: local_name.to_string(),
            type_id: shared.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: shared.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: shared.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Shared,
            fields: layout_resolver_fields(&shared.fields, type_fields),
            capabilities: shared.capabilities.clone(),
            value_abilities: Vec::new(),
            claim_output: None,
            flow_states: None,
            flow_state_field: None,
            flow_rules: Vec::new(),
            flow_initial_state: None,
            flow_terminal_states: Vec::new(),
            flow_terminal_discharge: None,
            flow_state_model: None,
            flow_audit_warnings: Vec::new(),
            validity_predicates: ir_validity_predicates(shared.validity.as_ref(), &shared.fields),
            identity: lower_identity_policy_ast(&shared.identity),
        }),
        TypeDef::Receipt(receipt) => Some(IrTypeDef {
            name: local_name.to_string(),
            type_id: receipt.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: receipt.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: receipt.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Receipt,
            fields: layout_resolver_fields(&receipt.fields, type_fields),
            capabilities: receipt.capabilities.clone(),
            value_abilities: Vec::new(),
            claim_output: receipt.claim_output.as_ref().map(ast_type_to_ir),
            flow_states: None,
            flow_state_field: None,
            flow_rules: Vec::new(),
            flow_initial_state: None,
            flow_terminal_states: Vec::new(),
            flow_terminal_discharge: None,
            flow_state_model: None,
            flow_audit_warnings: Vec::new(),
            validity_predicates: ir_validity_predicates(receipt.validity.as_ref(), &receipt.fields),
            identity: lower_identity_policy_ast(&receipt.identity),
        }),
        TypeDef::Struct(struct_def) => Some(IrTypeDef {
            name: local_name.to_string(),
            type_id: struct_def.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: struct_def.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: struct_def.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Struct,
            fields: layout_resolver_fields(&struct_def.fields, type_fields),
            capabilities: Vec::new(),
            value_abilities: struct_def.abilities.clone(),
            claim_output: None,
            flow_states: None,
            flow_state_field: None,
            flow_rules: Vec::new(),
            flow_initial_state: None,
            flow_terminal_states: Vec::new(),
            flow_terminal_discharge: None,
            flow_state_model: None,
            flow_audit_warnings: Vec::new(),
            validity_predicates: ir_validity_predicates(struct_def.validity.as_ref(), &struct_def.fields),
            identity: IrIdentityPolicy::None,
        }),
        TypeDef::Enum(_) => None,
    }
}

fn lower_identity_policy_ast(policy: &IdentityPolicy) -> IrIdentityPolicy {
    match policy {
        IdentityPolicy::None => IrIdentityPolicy::None,
        IdentityPolicy::CkbTypeId => IrIdentityPolicy::CkbTypeId,
        IdentityPolicy::Field(path) => IrIdentityPolicy::Field(path.clone()),
        IdentityPolicy::ScriptArgs => IrIdentityPolicy::ScriptArgs,
        IdentityPolicy::SingletonType => IrIdentityPolicy::SingletonType,
    }
}

fn layout_resolver_fields(fields: &[Field], type_fields: &HashMap<String, HashMap<String, IrType>>) -> Vec<IrField> {
    let mut next_offset = Some(0usize);
    fields
        .iter()
        .map(|field| {
            let ty = ast_type_to_ir(&field.ty);
            let fixed_size = fixed_encoded_size_for_resolver_ir_type(&ty, type_fields, &mut HashSet::new());
            let offset = next_offset.unwrap_or(0);
            next_offset = next_offset.and_then(|current| fixed_size.and_then(|size| current.checked_add(size)));
            IrField { name: field.name.clone(), ty, offset, fixed_size }
        })
        .collect()
}

fn fixed_encoded_size_for_resolver_ir_type(
    ty: &IrType,
    type_fields: &HashMap<String, HashMap<String, IrType>>,
    seen: &mut HashSet<String>,
) -> Option<usize> {
    match ty {
        IrType::Commitment(_) => Some(32),
        IrType::Opening(value) => fixed_encoded_size_for_resolver_ir_type(value, type_fields, seen),
        IrType::Applied(_) | IrType::OpenHandle(_) | IrType::WitnessBytes(_) | IrType::Selection(_) => None,
        IrType::U8 | IrType::Bool => Some(1),
        IrType::U16 => Some(2),
        IrType::U32 => Some(4),
        IrType::I32 => Some(4),
        IrType::U64 => Some(8),
        IrType::U128 => Some(16),
        IrType::Address | IrType::Hash => Some(32),
        IrType::Array(inner, len) => {
            fixed_encoded_size_for_resolver_ir_type(inner, type_fields, seen).and_then(|inner_size| inner_size.checked_mul(*len))
        }
        IrType::Tuple(items) => items.iter().try_fold(0usize, |acc, item| {
            fixed_encoded_size_for_resolver_ir_type(item, type_fields, seen).and_then(|size| acc.checked_add(size))
        }),
        IrType::Unit => Some(0),
        IrType::Named(name) => {
            let base_name = name.as_str();
            if !seen.insert(base_name.to_string()) {
                return None;
            }
            let size = type_fields.get(base_name).and_then(|fields| {
                fields.values().try_fold(0usize, |acc, field_ty| {
                    fixed_encoded_size_for_resolver_ir_type(field_ty, type_fields, seen)
                        .and_then(|field_size| acc.checked_add(field_size))
                })
            });
            seen.remove(base_name);
            size
        }
        IrType::Context(_) | IrType::Vector(_) | IrType::Bounded(_) | IrType::Nested(_) | IrType::Ref(_) | IrType::MutRef(_) => None,
    }
}

fn resolver_type_kind(type_def: &TypeDef) -> Option<IrTypeKind> {
    match type_def {
        TypeDef::Resource(_) => Some(IrTypeKind::Resource),
        TypeDef::Shared(_) => Some(IrTypeKind::Shared),
        TypeDef::Receipt(_) => Some(IrTypeKind::Receipt),
        TypeDef::Struct(_) => Some(IrTypeKind::Struct),
        TypeDef::Enum(_) => None,
    }
}

fn resolver_receipt_claim_output_to_ir(type_def: &TypeDef) -> Option<Option<IrType>> {
    match type_def {
        TypeDef::Receipt(receipt) => Some(receipt.claim_output.as_ref().map(ast_type_to_ir)),
        TypeDef::Resource(_) | TypeDef::Shared(_) | TypeDef::Struct(_) | TypeDef::Enum(_) => None,
    }
}

fn resolver_flow_states_to_ir(type_def: &TypeDef) -> Option<Vec<String>> {
    match type_def {
        TypeDef::Resource(_) | TypeDef::Shared(_) | TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        TypeDef::Receipt(_) => None,
    }
}

fn const_usize_operand(operand: &IrOperand) -> Option<usize> {
    match operand {
        IrOperand::Const(IrConst::U8(value)) => Some(*value as usize),
        IrOperand::Const(IrConst::U16(value)) => Some(*value as usize),
        IrOperand::Const(IrConst::U32(value)) => Some(*value as usize),
        IrOperand::Const(IrConst::U64(value)) => usize::try_from(*value).ok(),
        _ => None,
    }
}

fn direct_field_access_root(field: &FieldAccessExpr) -> Option<(&str, &str)> {
    match field.expr.as_ref() {
        Expr::Identifier(root) => Some((root.as_str(), field.field.as_str())),
        _ => None,
    }
}

fn same_direct_field_access(expr: &Expr, root: &str, field_name: &str) -> bool {
    let Expr::FieldAccess(field) = expr else {
        return false;
    };
    matches!(field.expr.as_ref(), Expr::Identifier(name) if name == root) && field.field == field_name
}

fn call_target_is_min(expr: &Expr) -> bool {
    matches!(expr, Expr::Identifier(name) if name == "min" || name == "math_min")
}

fn is_verifier_coverable_output_field_type(ty: &IrType) -> bool {
    matches!(ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64 | IrType::Address | IrType::Hash)
        || matches!(ty, IrType::Array(inner, _) if matches!(inner.as_ref(), IrType::U8))
}

fn binding_pattern_label(pattern: &BindingPattern) -> &str {
    match pattern {
        BindingPattern::Name(name) => name.as_str(),
        BindingPattern::Wildcard => "_",
        BindingPattern::Tuple(_) => "tuple_item",
    }
}

pub(crate) fn type_hash_for_name(name: &str) -> [u8; 32] {
    crate::ckb_blake2b256(name.as_bytes())
}

fn stable_u64_tag(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325u64, |acc, byte| acc.wrapping_mul(0x100_0000_01b3).wrapping_add(byte as u64))
}

#[cfg(test)]
mod tests;
