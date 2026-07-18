use crate::error::Span;
use std::fmt;

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Item {
    Resource(ResourceDef),
    Shared(SharedDef),
    Receipt(ReceiptDef),
    Struct(StructDef),
    Flow(FlowDef),
    Invariant(InvariantDef),
    Const(ConstDef),
    Enum(EnumDef),
    Action(ActionDef),
    Function(FnDef),
    Lock(LockDef),
    Use(UseStmt),
}

#[derive(Debug, Clone)]
pub struct ResourceDef {
    pub name: String,
    pub type_id: Option<TypeIdentity>,
    pub identity: IdentityPolicy,
    pub default_hash_type: Option<HashTypeDecl>,
    pub capacity_floor: Option<CapacityFloorDecl>,
    pub capabilities: Vec<Capability>,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SharedDef {
    pub name: String,
    pub type_id: Option<TypeIdentity>,
    pub identity: IdentityPolicy,
    pub default_hash_type: Option<HashTypeDecl>,
    pub capacity_floor: Option<CapacityFloorDecl>,
    pub capabilities: Vec<Capability>,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ReceiptDef {
    pub name: String,
    pub type_id: Option<TypeIdentity>,
    pub identity: IdentityPolicy,
    pub default_hash_type: Option<HashTypeDecl>,
    pub capacity_floor: Option<CapacityFloorDecl>,
    pub claim_output: Option<Type>,
    pub capabilities: Vec<Capability>,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub type_id: Option<TypeIdentity>,
    pub default_hash_type: Option<HashTypeDecl>,
    pub capacity_floor: Option<CapacityFloorDecl>,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeIdentity {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashTypeDecl {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityFloorDecl {
    pub shannons: u64,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstDef {
    pub name: String,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<Type>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StateFieldPath {
    pub base: String,
    pub field: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub action: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FlowDef {
    pub name: Option<String>,
    pub target: StateFieldPath,
    pub initial_states: Vec<String>,
    pub terminal_states: Vec<String>,
    pub transitions: Vec<StateTransition>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    // v0.14 compat capability (Destroy is a protocol verb, prefer consume+burn kernel effects)
    Store,
    Destroy,
    // v0.15 kernel effect capabilities
    Create,
    Consume,
    Replace,
    Burn,
    Relock,
    RetargetType,
    ReadRef,
}

impl Capability {
    /// Version of the closed CellScript capability vocabulary.
    pub const REGISTRY_VERSION: u32 = 1;

    /// Canonical capability order used by parser-facing tools and metadata.
    pub const ALL: [Capability; 9] = [
        Self::Store,
        Self::Create,
        Self::Consume,
        Self::Destroy,
        Self::Replace,
        Self::Burn,
        Self::Relock,
        Self::RetargetType,
        Self::ReadRef,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::Destroy => "destroy",
            Self::Create => "create",
            Self::Consume => "consume",
            Self::Replace => "replace",
            Self::Burn => "burn",
            Self::Relock => "relock",
            Self::RetargetType => "retarget_type",
            Self::ReadRef => "read_ref",
        }
    }

    pub fn from_source_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|capability| capability.as_str() == name)
    }

    /// Returns true if this capability is a v0.14-era protocol verb
    /// that is not allowed in `--primitive-strict=0.15` mode.
    pub fn is_protocol_verb(self) -> bool {
        matches!(self, Self::Destroy)
    }

    /// Map a protocol capability to its kernel effect equivalents.
    pub fn kernel_effects(self) -> Vec<Capability> {
        match self {
            Self::Destroy => vec![Self::Consume, Self::Burn],
            other => vec![other],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InvariantDef {
    pub name: String,
    pub trigger: Option<String>,
    pub scope: Option<String>,
    pub reads: Vec<AggregateTarget>,
    pub aggregates: Vec<AggregateInvariant>,
    pub asserts: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateInvariantKind {
    Sum,
    Conserved,
    Delta,
    Distinct,
    Singleton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateRelation {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

/// Closed transaction/source view vocabulary used by invariants and aggregate
/// targets. `SelectedCells` represents the legacy `Type.field` aggregate form;
/// `TypeIdentity` represents the singleton `type_id` target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceView {
    Input,
    Output,
    GroupInput,
    GroupOutput,
    CellDep,
    HeaderDep,
    WitnessArgs,
    LockArgs,
    SelectedCells,
    TypeIdentity,
}

impl SourceView {
    pub const TRANSACTION_VIEWS: [Self; 8] = [
        Self::Input,
        Self::Output,
        Self::GroupInput,
        Self::GroupOutput,
        Self::CellDep,
        Self::HeaderDep,
        Self::WitnessArgs,
        Self::LockArgs,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "inputs",
            Self::Output => "outputs",
            Self::GroupInput => "group_inputs",
            Self::GroupOutput => "group_outputs",
            Self::CellDep => "cell_deps",
            Self::HeaderDep => "header_deps",
            Self::WitnessArgs => "witness",
            Self::LockArgs => "lock_args",
            Self::SelectedCells => "selected_cells",
            Self::TypeIdentity => "type_id",
        }
    }

    pub fn from_source_name(name: &str) -> Option<Self> {
        match name {
            "input" | "inputs" => Some(Self::Input),
            "output" | "outputs" => Some(Self::Output),
            "group_input" | "group_inputs" => Some(Self::GroupInput),
            "group_output" | "group_outputs" => Some(Self::GroupOutput),
            "cell_dep" | "cell_deps" => Some(Self::CellDep),
            "header_dep" | "header_deps" => Some(Self::HeaderDep),
            "witness" | "witness_args" => Some(Self::WitnessArgs),
            "lock_args" => Some(Self::LockArgs),
            "selected_cells" => Some(Self::SelectedCells),
            "type_id" => Some(Self::TypeIdentity),
            _ => None,
        }
    }

    pub const fn aggregate_scope(self) -> Option<&'static str> {
        match self {
            Self::GroupInput | Self::GroupOutput => Some("group"),
            Self::Input | Self::Output => Some("transaction"),
            _ => None,
        }
    }

    pub const fn proof_plan_read(self) -> Option<&'static str> {
        match self {
            Self::Input => Some("input"),
            Self::Output => Some("output"),
            Self::GroupInput => Some("group_input"),
            Self::GroupOutput => Some("group_output"),
            Self::CellDep => Some("cell_dep"),
            Self::HeaderDep => Some("header_dep"),
            Self::WitnessArgs => Some("witness"),
            Self::LockArgs => Some("lock_args"),
            Self::SelectedCells | Self::TypeIdentity => None,
        }
    }
}

/// Typed target shared by invariant read declarations and aggregate operands.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AggregateTarget {
    pub source: SourceView,
    pub type_name: Option<String>,
    pub field: Option<String>,
}

impl AggregateTarget {
    pub fn type_and_field(&self) -> Option<(&str, &str)> {
        Some((self.type_name.as_deref()?, self.field.as_deref()?))
    }
}

impl fmt::Display for AggregateTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.source {
            SourceView::SelectedCells => {
                if let Some(type_name) = &self.type_name {
                    formatter.write_str(type_name)?;
                } else {
                    formatter.write_str(self.source.as_str())?;
                }
            }
            SourceView::TypeIdentity => formatter.write_str(self.source.as_str())?,
            source => {
                formatter.write_str(source.as_str())?;
                if let Some(type_name) = &self.type_name {
                    write!(formatter, "<{type_name}>")?;
                }
            }
        }
        if let Some(field) = &self.field {
            write!(formatter, ".{field}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AggregateInvariant {
    pub kind: AggregateInvariantKind,
    pub target: AggregateTarget,
    pub scope: String,
    pub argument: Option<String>,
    pub relation: Option<AggregateRelation>,
    pub rhs: Option<AggregateTarget>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ActionDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub outputs: Vec<ActionOutput>,
    pub state_edges: Vec<ActionStateEdge>,
    pub body: Vec<Stmt>,
    pub effect: EffectClass,
    pub effect_declared: bool,
    pub scheduler_hint: Option<SchedulerHint>,
    pub doc_comment: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ActionOutput {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ActionStateEdge {
    pub path: StateFieldPath,
    pub to_path: StateFieldPath,
    pub from: String,
    pub to: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
    pub effect: EffectClass,
    pub effect_declared: bool,
    pub doc_comment: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LockDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Type,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct UseStmt {
    pub module_path: Vec<String>,
    pub imports: Vec<UseImport>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct UseImport {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub is_mut: bool,
    pub is_ref: bool,
    pub is_read_ref: bool,
    pub source: ParamSource,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamSource {
    Default,
    Input,
    Output,
    Protected,
    Witness,
    LockArgs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
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
    Array(Box<Type>, usize),
    Tuple(Vec<Type>),
    Named(String),
    Ref(Box<Type>),
    MutRef(Box<Type>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    Expr(Expr),
    Return(ReturnStmt),
    If(IfStmt),
    For(ForStmt),
    While(WhileStmt),
}

#[derive(Debug, Clone)]
pub enum BindingPattern {
    Name(String),
    Tuple(Vec<BindingPattern>),
    Wildcard,
}

#[derive(Debug, Clone)]
pub struct LetStmt {
    pub pattern: BindingPattern,
    pub ty: Option<Type>,
    pub value: Expr,
    pub is_mut: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_branch: Vec<Stmt>,
    pub else_branch: Option<Vec<Stmt>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub pattern: BindingPattern,
    pub iterable: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Integer(u64),
    Bool(bool),
    String(String),
    ByteString(Vec<u8>),
    Identifier(String),
    Assign(AssignExpr),
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    Call(CallExpr),
    FieldAccess(FieldAccessExpr),
    Index(IndexExpr),
    Create(CreateExpr),
    Consume(ConsumeExpr),
    Destroy(DestroyExpr),
    ReadRef(ReadRefExpr),
    Claim(ClaimExpr),
    Settle(SettleExpr),
    CreateUnique(CreateUniqueExpr),
    ReplaceUnique(ReplaceUniqueExpr),
    Assert(AssertExpr),
    Require(RequireExpr),
    RequireBlock(RequireBlockExpr),
    Preserve(PreserveExpr),
    Block(Vec<Stmt>),
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    If(IfExpr),
    Cast(CastExpr),
    Range(RangeExpr),
    StructInit(StructInitExpr),
    Match(MatchExpr),
    StdlibCall(StdlibCallExpr),
}

impl Expr {
    /// Return the source span of this expression.
    pub fn span(&self) -> Span {
        match self {
            Expr::Integer(_) => Span::default(), // primitives carry no span
            Expr::Bool(_) => Span::default(),
            Expr::String(_) => Span::default(),
            Expr::ByteString(_) => Span::default(),
            Expr::Identifier(_) => Span::default(),
            Expr::Assign(e) => e.span,
            Expr::Binary(e) => e.span,
            Expr::Unary(e) => e.span,
            Expr::Call(e) => e.span,
            Expr::FieldAccess(e) => e.span,
            Expr::Index(e) => e.span,
            Expr::Create(e) => e.span,
            Expr::Consume(e) => e.span,
            Expr::Destroy(e) => e.span,
            Expr::ReadRef(e) => e.span,
            Expr::Claim(e) => e.span,
            Expr::Settle(e) => e.span,
            Expr::CreateUnique(e) => e.span,
            Expr::ReplaceUnique(e) => e.span,
            Expr::Assert(e) => e.span,
            Expr::Require(e) => e.span,
            Expr::RequireBlock(e) => e.span,
            Expr::Preserve(e) => e.span,
            Expr::Block(_) => Span::default(),
            Expr::Tuple(_) => Span::default(),
            Expr::Array(_) => Span::default(),
            Expr::If(e) => e.span,
            Expr::Cast(e) => e.span,
            Expr::Range(e) => e.span,
            Expr::StructInit(e) => e.span,
            Expr::Match(e) => e.span,
            Expr::StdlibCall(e) => e.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssignExpr {
    pub target: Box<Expr>,
    pub op: AssignOp,
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
}

#[derive(Debug, Clone)]
pub struct BinaryExpr {
    pub op: BinaryOp,
    pub left: Box<Expr>,
    pub right: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub expr: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    Ref,
    Deref,
}

#[derive(Debug, Clone)]
pub struct CallExpr {
    pub func: Box<Expr>,
    pub type_args: Vec<Type>,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldAccessExpr {
    pub expr: Box<Expr>,
    pub field: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IndexExpr {
    pub expr: Box<Expr>,
    pub index: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CreateExpr {
    pub target: Option<String>,
    pub ty: String,
    pub fields: Vec<(String, Expr)>,
    pub lock: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConsumeExpr {
    pub expr: Box<Expr>,
    pub span: Span,
}

/// Cell identity policy for resource/shared/receipt declarations.
/// In v0.15, identity is a first-class primitive policy across
/// create, replace, and destroy flows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum IdentityPolicy {
    /// No identity tracking (default)
    #[default]
    None,
    /// CKB TYPE_ID based identity
    CkbTypeId,
    /// Field-based identity (e.g., identity field(id))
    Field(String),
    /// Script args based identity
    ScriptArgs,
    /// Singleton type identity (one cell per type script)
    SingletonType,
}

/// Destruction policy for the `destroy` expression.
/// In v0.15, bare `destroy` is deprecated in favor of explicit policies
/// that specify how the verifier proves destruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestructionPolicy {
    /// Bare `destroy cell` — legacy v0.14 compat, same as SingletonType
    Default,
    /// `destroy_singleton_type(cell)` — proves absence of same-TypeHash output
    SingletonType,
    /// `destroy_unique(cell, identity = type_id)` — uses TYPE_ID to identify cell
    Unique { identity: String },
    /// `destroy_instance(cell, identity_field = id)` — identifies by specific field
    Instance { identity_field: String },
    /// `burn_amount(cell, field = amount)` — proves quantity delta, not output absence
    BurnAmount { field: String },
}

#[derive(Debug, Clone)]
pub struct DestroyExpr {
    pub expr: Box<Expr>,
    pub policy: DestructionPolicy,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ReadRefExpr {
    pub ty: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClaimExpr {
    pub receipt: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SettleExpr {
    pub expr: Box<Expr>,
    pub span: Span,
}

/// Assert expression.
#[derive(Debug, Clone)]
pub struct AssertExpr {
    pub condition: Box<Expr>,
    pub message: Box<Expr>,
    pub span: Span,
}

/// Lock/action failure requirement expression.
#[derive(Debug, Clone)]
pub struct RequireExpr {
    pub condition: Box<Expr>,
    pub message: Option<Box<Expr>>,
    pub span: Span,
}

/// Anonymous require block: `require { expr; expr; }`
/// Desugars into independent atomic `require` statements.
#[derive(Debug, Clone)]
pub struct RequireBlockExpr {
    pub expressions: Vec<Expr>,
    pub span: Span,
}

/// Preserve sugar: `preserve output from input { field1, field2 }`
/// Desugars into `require output.field1 == input.field1; require output.field2 == input.field2;`
#[derive(Debug, Clone)]
pub struct PreserveExpr {
    pub output_name: String,
    pub input_name: String,
    pub fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IfExpr {
    pub condition: Box<Expr>,
    pub then_branch: Box<Expr>,
    pub else_branch: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CastExpr {
    pub expr: Box<Expr>,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RangeExpr {
    pub start: Box<Expr>,
    pub end: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructInitExpr {
    pub ty: String,
    pub fields: Vec<(String, Expr)>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub expr: Box<Expr>,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectClass {
    Pure,
    ReadOnly,
    Mutating,
    Creating,
    Destroying,
}

impl EffectClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pure => "Pure",
            Self::ReadOnly => "ReadOnly",
            Self::Mutating => "Mutating",
            Self::Creating => "Creating",
            Self::Destroying => "Destroying",
        }
    }
}

/// Stdlib call expression: `std::namespace::name(args)` or `std::namespace::name(args) { field1, field2 }`
///
/// Each stdlib pattern has a canonical expansion into core CellScript.
/// Constraint patterns expand to `require` constraints or canonical verifier metadata checks.
/// Lifecycle patterns expand to `consume` plus explicit output and verifier constraints.
#[derive(Debug, Clone)]
pub struct StdlibCallExpr {
    pub namespace: String,
    pub name: String,
    pub args: Vec<Expr>,
    /// Optional preserve-style field list for lifecycle patterns
    pub preserve_fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SchedulerHint {
    pub parallelizable: bool,
    pub estimated_cycles: u64,
}

/// `create_unique<T>(identity = ckb_type_id) { ... } with_lock(addr)`
/// Identity-aware cell creation that enforces TYPE_ID or other identity rules.
#[derive(Debug, Clone)]
pub struct CreateUniqueExpr {
    pub ty: String,
    pub fields: Vec<(String, Expr)>,
    pub lock: Option<Box<Expr>>,
    pub identity: IdentityPolicy,
    pub span: Span,
}

/// `replace_unique<T>(identity = ckb_type_id) { ... }`
/// Identity-aware cell replacement that enforces identity preservation.
#[derive(Debug, Clone)]
pub struct ReplaceUniqueExpr {
    pub expr: Box<Expr>,
    pub ty: String,
    pub fields: Vec<(String, Expr)>,
    pub identity: IdentityPolicy,
    pub span: Span,
}
