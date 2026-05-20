use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::resolve::{FunctionDef, ModuleResolver, TypeDef};
use crate::runtime_errors::CellScriptRuntimeError;
use crate::syscalls::{
    checked_runtime_helper_spec, fail_closed_helper_spec, low_level_value_class_for_raw_symbol, LowLevelValueClass, SyscallKind,
};
use crate::types::lifecycle_effect_keys;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct IrModule {
    pub name: String,
    pub items: Vec<IrItem>,
    pub external_type_defs: Vec<IrTypeDef>,
    pub external_callable_abis: Vec<IrCallableAbi>,
    pub enum_fixed_sizes: HashMap<String, usize>,
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
    PureFn(IrPureFn),
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
    pub claim_output: Option<IrType>,
    pub flow_states: Option<Vec<String>>,
    pub flow_state_field: Option<String>,
    pub flow_rules: Vec<IrFlowRule>,
    /// Identity policy for v0.15 cell identity system
    pub identity: IrIdentityPolicy,
}

#[derive(Debug, Clone)]
pub struct IrInvariant {
    pub name: String,
    pub trigger: Option<String>,
    pub scope: Option<String>,
    pub reads: Vec<String>,
    pub aggregates: Vec<IrAggregateInvariant>,
    pub assert_count: usize,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrAggregateInvariant {
    pub kind: AggregateInvariantKind,
    pub target: String,
    pub scope: String,
    pub argument: Option<String>,
    pub relation: Option<AggregateRelation>,
    pub rhs: Option<String>,
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
    U64,
    U128,
    Bool,
    Unit,
    Address,
    Hash,
    Array(Box<IrType>, usize),
    Tuple(Vec<IrType>),
    Named(String),
    Ref(Box<IrType>),
    MutRef(Box<IrType>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrValueKind {
    Domain,
    Bool,
    SyscallStatus,
    HelperStatus,
    ErrorCode,
    ExitStatus,
}

impl IrValueKind {
    fn is_status_like(self) -> bool {
        matches!(self, IrValueKind::SyscallStatus | IrValueKind::HelperStatus | IrValueKind::ErrorCode | IrValueKind::ExitStatus)
    }
}

impl IrType {
    pub fn value_kind(&self) -> IrValueKind {
        match self {
            IrType::Bool => IrValueKind::Bool,
            _ => IrValueKind::Domain,
        }
    }
}

/// IR Action
#[derive(Debug, Clone)]
pub struct IrAction {
    pub name: String,
    pub params: Vec<IrParam>,
    pub return_type: Option<IrType>,
    pub state_transition_edges: Vec<IrStateTransitionEdge>,
    pub body: IrBody,
    pub effect_class: EffectClass,
    pub scheduler_hints: SchedulerHints,
}

/// IR pure helper function
#[derive(Debug, Clone)]
pub struct IrPureFn {
    pub name: String,
    pub params: Vec<IrParam>,
    pub return_type: Option<IrType>,
    pub body: IrBody,
}

/// IR Lock
#[derive(Debug, Clone)]
pub struct IrLock {
    pub name: String,
    pub params: Vec<IrParam>,
    pub body: IrBody,
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
    pub consume_set: Vec<CellPattern>,
    pub read_refs: Vec<CellPattern>,
    pub create_set: Vec<CreatePattern>,
    pub mutate_set: Vec<MutatePattern>,
    pub write_intents: Vec<WriteIntent>,
    pub blocks: Vec<IrBlock>,
}

#[derive(Debug, Clone)]
pub struct CellPattern {
    pub operation: String,
    pub type_hash: Option<[u8; 32]>,
    pub binding: String,
    pub fields: Vec<(String, IrOperand)>,
    pub destruction_policy: Option<IrDestructionPolicy>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone)]
pub enum IrInstruction {
    LoadConst { dest: IrVar, value: IrConst },
    LoadVar { dest: IrVar, name: String },
    StoreVar { name: String, src: IrOperand },
    Binary { dest: IrVar, op: BinaryOp, left: IrOperand, right: IrOperand },
    Unary { dest: IrVar, op: UnaryOp, operand: IrOperand },
    FieldAccess { dest: IrVar, obj: IrOperand, field: String },
    Index { dest: IrVar, arr: IrOperand, idx: IrOperand },
    Length { dest: IrVar, operand: IrOperand },
    TypeHash { dest: IrVar, operand: IrOperand },
    CollectionNew { dest: IrVar, ty: String, capacity: Option<IrOperand> },
    CollectionCapacity { dest: IrVar, collection: IrOperand },
    CollectionPush { collection: IrOperand, value: IrOperand },
    CollectionExtend { collection: IrOperand, slice: IrOperand },
    CollectionClear { collection: IrOperand },
    CollectionContains { dest: IrVar, collection: IrOperand, value: IrOperand },
    CollectionRemove { dest: IrVar, collection: IrOperand, index: IrOperand },
    CollectionInsert { collection: IrOperand, index: IrOperand, value: IrOperand },
    CollectionSet { collection: IrOperand, index: IrOperand, value: IrOperand },
    CollectionPop { dest: IrVar, collection: IrOperand },
    CollectionReverse { collection: IrOperand },
    CollectionTruncate { collection: IrOperand, len: IrOperand },
    CollectionSwap { collection: IrOperand, left: IrOperand, right: IrOperand },
    Call { dest: Option<IrVar>, func: String, args: Vec<IrOperand> },
    ReadRef { dest: IrVar, ty: String },
    Move { dest: IrVar, src: IrOperand },
    Cast { dest: IrVar, src: IrOperand },
    Tuple { dest: IrVar, fields: Vec<IrOperand> },
    Consume { operand: IrOperand },
    Create { dest: IrVar, pattern: CreatePattern },
    Transfer { dest: IrVar, operand: IrOperand, to: IrOperand },
    Destroy { operand: IrOperand, policy: IrDestructionPolicy },
    Claim { dest: IrVar, receipt: IrOperand },
    Settle { dest: IrVar, operand: IrOperand },
    CreateUnique { dest: IrVar, pattern: CreatePattern, identity: IrIdentityPolicy },
    ReplaceUnique { dest: IrVar, operand: IrOperand, pattern: CreatePattern, identity: IrIdentityPolicy },
    CellMetadataEquality { left: IrOperand, right: IrOperand, field: CellMetadataField },
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
    Abort(CellScriptRuntimeError),
    Jump(BlockId),
    Branch { cond: IrOperand, then_block: BlockId, else_block: BlockId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectClass {
    Pure,
    ReadOnly,
    Mutating,
    Creating,
    Destroying,
}

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

#[derive(Default)]
struct EffectFootprint {
    has_read_ref: bool,
    has_consume: bool,
    has_create: bool,
}

#[derive(Debug, Clone, Copy)]
struct StraightLineLifecycleOnly;

pub struct IrGenerator {
    module: IrModule,
    var_counter: usize,
    block_counter: usize,
    aggregate_fields: HashMap<usize, HashMap<String, IrVar>>,
    schema_field_roots: HashMap<usize, (usize, String)>,
    aggregate_elements: HashMap<usize, Vec<IrVar>>,
    mutated_fields: HashMap<usize, BTreeSet<String>>,
    mutated_field_transitions: HashMap<usize, BTreeMap<String, MutateFieldTransition>>,
    transition_param_ids: HashSet<usize>,
    transition_coverable_value_ids: HashSet<usize>,
    type_fields: HashMap<String, HashMap<String, IrType>>,
    type_kinds: HashMap<String, IrTypeKind>,
    receipt_claim_outputs: HashMap<String, Option<IrType>>,
    flow_states: HashMap<String, Vec<String>>,
    flow_state_fields: HashMap<String, String>,
    flow_rules: HashMap<String, Vec<IrFlowRule>>,
    enum_variants: HashMap<String, HashMap<String, u64>>,
    constants: HashMap<String, (Type, Expr)>,
    function_effects: HashMap<String, EffectClass>,
    external_function_effects: HashMap<String, EffectClass>,
    function_return_types: HashMap<String, Option<IrType>>,
    external_function_return_types: HashMap<String, Option<IrType>>,
    errors: Vec<CompileError>,
}

struct LoweredExpr {
    operand: IrOperand,
    current: Option<BlockId>,
}

impl IrGenerator {
    pub fn new(module_name: String) -> Self {
        Self {
            module: IrModule {
                name: module_name,
                items: Vec::new(),
                external_type_defs: Vec::new(),
                external_callable_abis: Vec::new(),
                enum_fixed_sizes: HashMap::new(),
            },
            var_counter: 0,
            block_counter: 0,
            aggregate_fields: HashMap::new(),
            schema_field_roots: HashMap::new(),
            aggregate_elements: HashMap::new(),
            mutated_fields: HashMap::new(),
            mutated_field_transitions: HashMap::new(),
            transition_param_ids: HashSet::new(),
            transition_coverable_value_ids: HashSet::new(),
            type_fields: HashMap::new(),
            type_kinds: HashMap::new(),
            receipt_claim_outputs: HashMap::new(),
            flow_states: HashMap::new(),
            flow_state_fields: HashMap::new(),
            flow_rules: HashMap::new(),
            enum_variants: HashMap::new(),
            constants: HashMap::new(),
            function_effects: HashMap::new(),
            external_function_effects: HashMap::new(),
            function_return_types: HashMap::new(),
            external_function_return_types: HashMap::new(),
            errors: Vec::new(),
        }
    }

    pub fn with_type_fields(module_name: String, type_fields: HashMap<String, HashMap<String, IrType>>) -> Self {
        let mut generator = Self::new(module_name);
        generator.type_fields = type_fields;
        generator
    }

    pub fn with_import_context(
        module_name: String,
        type_fields: HashMap<String, HashMap<String, IrType>>,
        type_kinds: HashMap<String, IrTypeKind>,
        receipt_claim_outputs: HashMap<String, Option<IrType>>,
        flow_states: HashMap<String, Vec<String>>,
        external_function_effects: HashMap<String, EffectClass>,
        external_function_return_types: HashMap<String, Option<IrType>>,
    ) -> Self {
        let mut generator = Self::with_type_fields(module_name, type_fields);
        generator.type_kinds.extend(type_kinds);
        generator.receipt_claim_outputs.extend(receipt_claim_outputs);
        generator.flow_states.extend(flow_states);
        generator.external_function_effects = external_function_effects;
        generator.external_function_return_types = external_function_return_types;
        generator
    }

    pub fn generate(mut self, ast: &Module) -> Result<IrModule> {
        for item in &ast.items {
            if let Item::Const(c) = item {
                self.constants.insert(c.name.clone(), (c.ty.clone(), c.value.clone()));
            }
            match item {
                Item::Resource(r) => {
                    self.type_kinds.insert(r.name.clone(), IrTypeKind::Resource);
                    self.type_fields.insert(
                        r.name.clone(),
                        r.fields.iter().map(|field| (field.name.clone(), Self::convert_type(&field.ty))).collect(),
                    );
                }
                Item::Shared(s) => {
                    self.type_kinds.insert(s.name.clone(), IrTypeKind::Shared);
                    self.type_fields.insert(
                        s.name.clone(),
                        s.fields.iter().map(|field| (field.name.clone(), Self::convert_type(&field.ty))).collect(),
                    );
                }
                Item::Receipt(r) => {
                    self.type_kinds.insert(r.name.clone(), IrTypeKind::Receipt);
                    self.receipt_claim_outputs.insert(r.name.clone(), r.claim_output.as_ref().map(Self::convert_type));
                    self.type_fields.insert(
                        r.name.clone(),
                        r.fields.iter().map(|field| (field.name.clone(), Self::convert_type(&field.ty))).collect(),
                    );
                }
                Item::Struct(s) => {
                    self.type_kinds.insert(s.name.clone(), IrTypeKind::Struct);
                    self.type_fields.insert(
                        s.name.clone(),
                        s.fields.iter().map(|field| (field.name.clone(), Self::convert_type(&field.ty))).collect(),
                    );
                }
                Item::Enum(e) => {
                    self.enum_variants.insert(
                        e.name.clone(),
                        e.variants.iter().enumerate().map(|(index, variant)| (variant.name.clone(), index as u64)).collect(),
                    );
                    if e.variants.iter().all(|variant| variant.fields.is_empty()) && e.variants.len() <= u8::MAX as usize + 1 {
                        self.module.enum_fixed_sizes.insert(e.name.clone(), 1);
                    }
                }
                Item::Action(action) => {
                    let return_type = action.return_type.as_ref().map(ast_type_to_ir);
                    self.function_return_types.insert(action.name.clone(), return_type.clone());
                    self.function_return_types.insert(format!("{}::{}", self.module.name, action.name), return_type);
                }
                Item::Function(function) => {
                    let return_type = function.return_type.as_ref().map(ast_type_to_ir);
                    self.function_return_types.insert(function.name.clone(), return_type.clone());
                    self.function_return_types.insert(format!("{}::{}", self.module.name, function.name), return_type);
                }
                Item::Lock(lock) => {
                    self.function_return_types.insert(lock.name.clone(), Some(IrType::Bool));
                    self.function_return_types.insert(format!("{}::{}", self.module.name, lock.name), Some(IrType::Bool));
                }
                Item::Flow(_) => {}
                _ => {}
            }
        }

        self.register_flows(&ast.items);
        self.infer_module_function_effects(&ast.items);

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
                    let inferred_effect = self.analyze_body_effect_class(&f.body);
                    if inferred_effect != EffectClass::Pure {
                        self.record_error(format!("fn '{}' must be pure; inferred effect is {:?}", f.name, inferred_effect), f.span);
                    }
                    let ir_item = IrItem::PureFn(self.gen_function(f));
                    self.module.items.push(ir_item);
                }
                Item::Lock(l) => {
                    let ir_item = IrItem::Lock(self.gen_lock(l));
                    self.module.items.push(ir_item);
                }
                Item::Flow(_) => {}
                Item::Use(_) => {}
            }
        }
        if let Some(error) = self.errors.into_iter().next() {
            Err(error)
        } else {
            Ok(self.module)
        }
    }

    fn register_flows(&mut self, items: &[Item]) {
        for item in items {
            let Item::Flow(machine) = item else {
                continue;
            };
            let type_name = machine.target.base.clone();
            let field_name = machine.target.field.clone();
            let states = self.flow_states_for_decl(machine);
            let rules = machine
                .transitions
                .iter()
                .filter_map(|transition| {
                    let from = self.canonical_state_name(&type_name, &transition.from);
                    let to = self.canonical_state_name(&type_name, &transition.to);
                    let from_index = states.iter().position(|state| state == &from)?;
                    let to_index = states.iter().position(|state| state == &to)?;
                    Some(IrFlowRule { from, to, from_index, to_index })
                })
                .collect::<Vec<_>>();

            self.flow_states.insert(type_name.clone(), states);
            self.flow_state_fields.insert(type_name.clone(), field_name);
            self.flow_rules.insert(type_name, rules);
        }
    }

    fn flow_states_for_decl(&self, machine: &FlowDef) -> Vec<String> {
        if let Some(fields) = self.type_fields.get(&machine.target.base) {
            if let Some(IrType::Named(enum_name)) = fields.get(&machine.target.field) {
                if let Some(variants) = self.enum_variants.get(enum_name) {
                    let mut ordered = variants.iter().map(|(name, ordinal)| (*ordinal, name.clone())).collect::<Vec<_>>();
                    ordered.sort_by_key(|(ordinal, _)| *ordinal);
                    return ordered.into_iter().map(|(_, name)| name).collect();
                }
            }
        }

        let mut states = Vec::new();
        for transition in &machine.transitions {
            for raw in [&transition.from, &transition.to] {
                let state = self.canonical_state_name(&machine.target.base, raw);
                if !states.iter().any(|existing| existing == &state) {
                    states.push(state);
                }
            }
        }
        states
    }

    fn canonical_state_name(&self, _type_name: &str, raw: &str) -> String {
        raw.rsplit_once("::").map_or(raw, |(_, state)| state).to_string()
    }

    fn gen_resource(&mut self, resource: &ResourceDef) -> IrTypeDef {
        IrTypeDef {
            name: resource.name.clone(),
            type_id: resource.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: resource.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: resource.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Resource,
            fields: self.layout_fields(&resource.fields),
            capabilities: resource.capabilities.clone(),
            claim_output: None,
            flow_states: self.flow_states.get(&resource.name).cloned(),
            flow_state_field: self.flow_state_fields.get(&resource.name).cloned(),
            flow_rules: self.flow_rules.get(&resource.name).cloned().unwrap_or_default(),
            identity: Self::lower_identity_policy(&resource.identity),
        }
    }

    fn gen_shared(&mut self, shared: &SharedDef) -> IrTypeDef {
        IrTypeDef {
            name: shared.name.clone(),
            type_id: shared.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: shared.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: shared.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Shared,
            fields: self.layout_fields(&shared.fields),
            capabilities: shared.capabilities.clone(),
            claim_output: None,
            flow_states: self.flow_states.get(&shared.name).cloned(),
            flow_state_field: self.flow_state_fields.get(&shared.name).cloned(),
            flow_rules: self.flow_rules.get(&shared.name).cloned().unwrap_or_default(),
            identity: Self::lower_identity_policy(&shared.identity),
        }
    }

    fn gen_receipt(&mut self, receipt: &ReceiptDef) -> IrTypeDef {
        IrTypeDef {
            name: receipt.name.clone(),
            type_id: receipt.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: receipt.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: receipt.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Receipt,
            fields: self.layout_fields(&receipt.fields),
            capabilities: receipt.capabilities.clone(),
            claim_output: receipt.claim_output.as_ref().map(Self::convert_type),
            flow_states: self.flow_states.get(&receipt.name).cloned(),
            flow_state_field: self.flow_state_fields.get(&receipt.name).cloned(),
            flow_rules: self.flow_rules.get(&receipt.name).cloned().unwrap_or_default(),
            identity: Self::lower_identity_policy(&receipt.identity),
        }
    }

    fn gen_struct(&mut self, struct_def: &StructDef) -> IrTypeDef {
        IrTypeDef {
            name: struct_def.name.clone(),
            type_id: struct_def.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: struct_def.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: struct_def.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Struct,
            fields: self.layout_fields(&struct_def.fields),
            capabilities: Vec::new(),
            claim_output: None,
            flow_states: self.flow_states.get(&struct_def.name).cloned(),
            flow_state_field: self.flow_state_fields.get(&struct_def.name).cloned(),
            flow_rules: self.flow_rules.get(&struct_def.name).cloned().unwrap_or_default(),
            identity: IrIdentityPolicy::None,
        }
    }

    fn gen_invariant(&self, invariant: &InvariantDef) -> IrInvariant {
        IrInvariant {
            name: invariant.name.clone(),
            trigger: invariant.trigger.clone(),
            scope: invariant.scope.clone(),
            reads: invariant.reads.clone(),
            aggregates: invariant
                .aggregates
                .iter()
                .map(|aggregate| IrAggregateInvariant {
                    kind: aggregate.kind,
                    target: aggregate.target.clone(),
                    scope: aggregate.scope.clone(),
                    argument: aggregate.argument.clone(),
                    relation: aggregate.relation,
                    rhs: aggregate.rhs.clone(),
                    span: aggregate.span,
                })
                .collect(),
            assert_count: invariant.asserts.len(),
            span: invariant.span,
        }
    }

    fn infer_module_function_effects(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Action(action) => {
                    self.function_effects.insert(action.name.clone(), EffectClass::Pure);
                    self.function_effects.insert(format!("{}::{}", self.module.name, action.name), EffectClass::Pure);
                }
                Item::Function(function) => {
                    self.function_effects.insert(function.name.clone(), EffectClass::Pure);
                    self.function_effects.insert(format!("{}::{}", self.module.name, function.name), EffectClass::Pure);
                }
                _ => {}
            }
        }

        for _ in 0..items.len().saturating_add(1) {
            let mut changed = false;
            for item in items {
                match item {
                    Item::Action(action) => {
                        let inferred = self.analyze_effect_class(action);
                        let declared = self.convert_effect_class(action.effect);
                        let effective =
                            if action.effect_declared && self.effect_covers(declared, inferred) { declared } else { inferred };
                        if self.function_effects.get(&action.name).copied() != Some(effective) {
                            self.function_effects.insert(action.name.clone(), effective);
                            self.function_effects.insert(format!("{}::{}", self.module.name, action.name), effective);
                            changed = true;
                        }
                    }
                    Item::Function(function) => {
                        let inferred = self.analyze_body_effect_class(&function.body);
                        if self.function_effects.get(&function.name).copied() != Some(inferred) {
                            self.function_effects.insert(function.name.clone(), inferred);
                            self.function_effects.insert(format!("{}::{}", self.module.name, function.name), inferred);
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn gen_function(&mut self, function: &FnDef) -> IrPureFn {
        self.var_counter = 0;
        self.block_counter = 0;
        self.aggregate_fields.clear();
        self.schema_field_roots.clear();
        self.aggregate_elements.clear();
        self.mutated_fields.clear();
        self.mutated_field_transitions.clear();
        self.transition_param_ids.clear();
        self.transition_coverable_value_ids.clear();
        let (params, body) = self.lower_signature_and_body(
            &function.params,
            &[],
            &function.body,
            function.return_type.as_ref().map(Self::convert_type),
            &HashSet::new(),
        );

        IrPureFn { name: function.name.clone(), params, return_type: function.return_type.as_ref().map(Self::convert_type), body }
    }

    fn layout_fields(&self, fields: &[Field]) -> Vec<IrField> {
        let mut next_offset = Some(0usize);
        fields
            .iter()
            .map(|field| {
                let ty = Self::convert_type(&field.ty);
                let fixed_size = self.fixed_encoded_size(&ty);
                let offset = next_offset.unwrap_or(0);
                next_offset = next_offset.and_then(|current| fixed_size.and_then(|size| current.checked_add(size)));
                IrField { name: field.name.clone(), ty, offset, fixed_size }
            })
            .collect()
    }

    fn fixed_encoded_size(&self, ty: &IrType) -> Option<usize> {
        self.fixed_encoded_size_with_seen(ty, &mut HashSet::new())
    }

    fn fixed_encoded_size_with_seen(&self, ty: &IrType, seen: &mut HashSet<String>) -> Option<usize> {
        match ty {
            IrType::U8 | IrType::Bool => Some(1),
            IrType::U16 => Some(2),
            IrType::U32 => Some(4),
            IrType::U64 => Some(8),
            IrType::U128 => Some(16),
            IrType::Address | IrType::Hash => Some(32),
            IrType::Array(inner, len) => {
                self.fixed_encoded_size_with_seen(inner, seen).and_then(|inner_size| inner_size.checked_mul(*len))
            }
            IrType::Tuple(items) => items
                .iter()
                .try_fold(0usize, |acc, item| self.fixed_encoded_size_with_seen(item, seen).and_then(|size| acc.checked_add(size))),
            IrType::Unit => Some(0),
            IrType::Named(name) => {
                let base_name = name.split('<').next().unwrap_or(name.as_str());
                if let Some(size) = self.module.enum_fixed_sizes.get(base_name).copied() {
                    return Some(size);
                }
                if !seen.insert(base_name.to_string()) {
                    return None;
                }
                let size = self.type_fields.get(base_name).and_then(|fields| {
                    fields.values().try_fold(0usize, |acc, field_ty| {
                        self.fixed_encoded_size_with_seen(field_ty, seen).and_then(|field_size| acc.checked_add(field_size))
                    })
                });
                seen.remove(base_name);
                size
            }
            IrType::Ref(_) | IrType::MutRef(_) => None,
        }
    }

    fn gen_action(&mut self, action: &ActionDef) -> IrAction {
        self.var_counter = 0;
        self.block_counter = 0;
        self.aggregate_fields.clear();
        self.schema_field_roots.clear();
        self.aggregate_elements.clear();
        self.mutated_fields.clear();
        self.mutated_field_transitions.clear();
        self.transition_param_ids.clear();
        self.transition_coverable_value_ids.clear();
        let core_input_bindings = action_core_input_binding_names(action);
        let (params, body) = self.lower_signature_and_body(
            &action.params,
            &action.outputs,
            &action.body,
            action.return_type.as_ref().map(Self::convert_type),
            &core_input_bindings,
        );

        let mut effect_class = self.analyze_effect_class(action);
        if params.iter().any(|param| param.is_read_ref) && effect_class == EffectClass::Pure {
            effect_class = EffectClass::ReadOnly;
        }
        if params.iter().any(|param| self.param_mutates_shared_state(param)) {
            effect_class = EffectClass::Mutating;
        }
        let declared_effect_class = self.convert_effect_class(action.effect);
        if action.effect_declared && !self.effect_covers(declared_effect_class, effect_class) {
            self.record_error(
                format!(
                    "declared effect {:?} is too weak for action '{}'; inferred effect is {:?}",
                    declared_effect_class, action.name, effect_class
                ),
                action.span,
            );
        }
        let touches_shared = self.infer_touches_shared(&params, &body);
        let estimated_cycles = self.estimate_cycles(&body);

        IrAction {
            name: action.name.clone(),
            params,
            return_type: action.return_type.as_ref().map(Self::convert_type),
            state_transition_edges: self.action_state_transition_edges(action),
            body,
            effect_class: if action.effect_declared { declared_effect_class } else { effect_class },
            scheduler_hints: action
                .scheduler_hint
                .as_ref()
                .map(|hint| SchedulerHints {
                    parallelizable: hint.parallelizable,
                    touches_shared: touches_shared.clone(),
                    estimated_cycles: hint.estimated_cycles.max(estimated_cycles),
                })
                .unwrap_or(SchedulerHints { parallelizable: touches_shared.is_empty(), touches_shared, estimated_cycles }),
        }
    }

    fn action_state_transition_edges(&self, action: &ActionDef) -> Vec<IrStateTransitionEdge> {
        let mut edges = Vec::new();
        for state_edge in &action.state_edges {
            let path = &state_edge.path;
            let Some(param) = action.params.iter().find(|param| param.name == path.base) else {
                continue;
            };
            let Some(type_name) = Self::named_type_name_from_ast_type(&param.ty) else {
                continue;
            };
            if let Some(mut lowered) = self.state_transition_edge_for(type_name, &path.field, &state_edge.from, &state_edge.to) {
                lowered.input_binding = Some(path.base.clone());
                lowered.output_binding = Some(state_edge.to_path.base.clone());
                edges.push(lowered);
            }
        }

        let mut unique = Vec::new();
        for state_edge in edges {
            if !unique.iter().any(|existing: &IrStateTransitionEdge| {
                existing.type_name == state_edge.type_name
                    && existing.field_name == state_edge.field_name
                    && existing.input_binding == state_edge.input_binding
                    && existing.output_binding == state_edge.output_binding
                    && existing.from_index == state_edge.from_index
                    && existing.to_index == state_edge.to_index
            }) {
                unique.push(state_edge);
            }
        }
        unique
    }

    fn state_transition_edge_for(&self, type_name: &str, field_name: &str, from: &str, to: &str) -> Option<IrStateTransitionEdge> {
        if self.flow_state_fields.get(type_name).is_some_and(|field| field != field_name) {
            return None;
        }
        let states = self.flow_states.get(type_name)?;
        let from = self.canonical_state_name(type_name, from);
        let to = self.canonical_state_name(type_name, to);
        let from_index = states.iter().position(|state| state == &from)?;
        let to_index = states.iter().position(|state| state == &to)?;
        Some(IrStateTransitionEdge {
            input_binding: None,
            output_binding: None,
            type_name: type_name.to_string(),
            field_name: field_name.to_string(),
            from,
            to,
            from_index,
            to_index,
        })
    }

    fn gen_lock(&mut self, lock: &LockDef) -> IrLock {
        self.var_counter = 0;
        self.block_counter = 0;
        self.aggregate_fields.clear();
        self.schema_field_roots.clear();
        self.aggregate_elements.clear();
        self.mutated_fields.clear();
        self.mutated_field_transitions.clear();
        self.transition_param_ids.clear();
        self.transition_coverable_value_ids.clear();
        let (params, body) = self.lower_signature_and_body(&lock.params, &[], &lock.body, Some(IrType::Bool), &HashSet::new());

        IrLock { name: lock.name.clone(), params, body }
    }

    fn convert_type(ty: &Type) -> IrType {
        match ty {
            Type::U8 => IrType::U8,
            Type::U16 => IrType::U16,
            Type::U32 => IrType::U32,
            Type::U64 => IrType::U64,
            Type::U128 => IrType::U128,
            Type::Bool => IrType::Bool,
            Type::Unit => IrType::Unit,
            Type::Address => IrType::Address,
            Type::Hash => IrType::Hash,
            Type::Array(elem, size) => IrType::Array(Box::new(Self::convert_type(elem)), *size),
            Type::Tuple(types) => IrType::Tuple(types.iter().map(Self::convert_type).collect()),
            Type::Named(name) if name == "usize" || name == "isize" => IrType::U64,
            Type::Named(name) => IrType::Named(name.clone()),
            Type::Ref(inner) => IrType::Ref(Box::new(Self::convert_type(inner))),
            Type::MutRef(inner) => IrType::MutRef(Box::new(Self::convert_type(inner))),
        }
    }

    fn analyze_effect_class(&self, action: &ActionDef) -> EffectClass {
        if !action_core_input_binding_names(action).is_empty() {
            return EffectClass::Mutating;
        }
        self.analyze_body_effect_class(&action.body)
    }

    fn analyze_body_effect_class(&self, body: &[Stmt]) -> EffectClass {
        let mut footprint = EffectFootprint::default();

        for stmt in body {
            self.check_stmt_effects(stmt, &mut footprint);
        }

        match (footprint.has_consume, footprint.has_create, footprint.has_read_ref) {
            (true, true, _) => EffectClass::Mutating,
            (true, false, _) => EffectClass::Destroying,
            (false, true, _) => EffectClass::Creating,
            (false, false, true) => EffectClass::ReadOnly,
            (false, false, false) => EffectClass::Pure,
        }
    }

    fn effect_covers(&self, declared: EffectClass, inferred: EffectClass) -> bool {
        matches!(
            (declared, inferred),
            (EffectClass::Pure, EffectClass::Pure)
                | (EffectClass::ReadOnly, EffectClass::Pure | EffectClass::ReadOnly)
                | (EffectClass::Creating, EffectClass::Pure | EffectClass::ReadOnly | EffectClass::Creating)
                | (EffectClass::Destroying, EffectClass::Pure | EffectClass::ReadOnly | EffectClass::Destroying)
                | (EffectClass::Mutating, _)
        )
    }

    fn check_stmt_effects(&self, stmt: &Stmt, footprint: &mut EffectFootprint) {
        match stmt {
            Stmt::Expr(expr) | Stmt::Let(LetStmt { value: expr, .. }) => {
                self.check_expr_effects(expr, footprint);
            }
            Stmt::Return(Some(expr)) => {
                self.check_expr_effects(expr, footprint);
            }
            Stmt::If(if_stmt) => {
                self.check_expr_effects(&if_stmt.condition, footprint);
                for stmt in &if_stmt.then_branch {
                    self.check_stmt_effects(stmt, footprint);
                }
                if let Some(ref else_branch) = if_stmt.else_branch {
                    for stmt in else_branch {
                        self.check_stmt_effects(stmt, footprint);
                    }
                }
            }
            Stmt::For(for_stmt) => {
                self.check_expr_effects(&for_stmt.iterable, footprint);
                for stmt in &for_stmt.body {
                    self.check_stmt_effects(stmt, footprint);
                }
            }
            Stmt::While(while_stmt) => {
                self.check_expr_effects(&while_stmt.condition, footprint);
                for stmt in &while_stmt.body {
                    self.check_stmt_effects(stmt, footprint);
                }
            }
            _ => {}
        }
    }

    fn check_expr_effects(&self, expr: &Expr, footprint: &mut EffectFootprint) {
        match expr {
            Expr::Consume(consume) => {
                footprint.has_consume = true;
                self.check_expr_effects(&consume.expr, footprint);
            }
            Expr::Create(create) => {
                footprint.has_create = true;
                for (_, value) in &create.fields {
                    self.check_expr_effects(value, footprint);
                }
                if let Some(lock) = &create.lock {
                    self.check_expr_effects(lock, footprint);
                }
            }
            Expr::Destroy(destroy) => {
                footprint.has_consume = true;
                self.check_expr_effects(&destroy.expr, footprint);
            }
            Expr::ReadRef(_) => {
                footprint.has_read_ref = true;
            }
            Expr::Assert(assert_expr) => {
                self.check_expr_effects(&assert_expr.condition, footprint);
            }
            Expr::Require(require_expr) => {
                self.check_expr_effects(&require_expr.condition, footprint);
                if let Some(message) = &require_expr.message {
                    self.check_expr_effects(message, footprint);
                }
            }
            Expr::RequireBlock(require_block) => {
                for expr in &require_block.expressions {
                    self.check_expr_effects(expr, footprint);
                }
            }
            Expr::Preserve(_) => {
                // preserve is pure sugar; desugared requires carry no side effects beyond verification
            }
            Expr::StdlibCall(call) => {
                let qualified = format!("std::{}::{}", call.namespace, call.name);
                match qualified.as_str() {
                    "std::lifecycle::transfer" | "std::receipt::claim" | "std::lifecycle::settle" => {
                        self.apply_effect_to_footprint(EffectClass::Mutating, footprint);
                    }
                    _ => {
                        // constraint patterns are verification-only
                    }
                }
                for arg in &call.args {
                    self.check_expr_effects(arg, footprint);
                }
            }
            Expr::Assign(assign) => {
                self.check_expr_effects(&assign.target, footprint);
                self.check_expr_effects(&assign.value, footprint);
            }
            Expr::Binary(bin) => {
                self.check_expr_effects(&bin.left, footprint);
                self.check_expr_effects(&bin.right, footprint);
            }
            Expr::Unary(unary) => {
                self.check_expr_effects(&unary.expr, footprint);
            }
            Expr::Call(call) => {
                if let Expr::Identifier(name) = call.func.as_ref() {
                    if let Some(effect) = self.function_effects.get(name).copied() {
                        self.apply_effect_to_footprint(effect, footprint);
                    } else if let Some(effect) = self.external_function_effects.get(name).copied() {
                        self.apply_effect_to_footprint(effect, footprint);
                    }
                }
                for arg in &call.args {
                    self.check_expr_effects(arg, footprint);
                }
            }
            Expr::FieldAccess(field) => {
                self.check_expr_effects(&field.expr, footprint);
            }
            Expr::Index(index) => {
                self.check_expr_effects(&index.expr, footprint);
                self.check_expr_effects(&index.index, footprint);
            }
            Expr::If(if_expr) => {
                self.check_expr_effects(&if_expr.condition, footprint);
                self.check_expr_effects(&if_expr.then_branch, footprint);
                self.check_expr_effects(&if_expr.else_branch, footprint);
            }
            Expr::Cast(cast) => {
                self.check_expr_effects(&cast.expr, footprint);
            }
            Expr::Range(range) => {
                self.check_expr_effects(&range.start, footprint);
                self.check_expr_effects(&range.end, footprint);
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    self.check_expr_effects(value, footprint);
                }
            }
            Expr::Match(match_expr) => {
                self.check_expr_effects(&match_expr.expr, footprint);
                for arm in &match_expr.arms {
                    self.check_expr_effects(&arm.value, footprint);
                }
            }
            Expr::Block(stmts) => {
                for stmt in stmts {
                    self.check_stmt_effects(stmt, footprint);
                }
            }
            Expr::Tuple(elems) | Expr::Array(elems) => {
                for elem in elems {
                    self.check_expr_effects(elem, footprint);
                }
            }
            Expr::Transfer(transfer) => {
                footprint.has_consume = true;
                self.check_expr_effects(&transfer.expr, footprint);
                self.check_expr_effects(&transfer.to, footprint);
            }
            Expr::Claim(claim) => {
                footprint.has_consume = true;
                self.check_expr_effects(&claim.receipt, footprint);
            }
            Expr::Settle(settle) => {
                footprint.has_consume = true;
                self.check_expr_effects(&settle.expr, footprint);
            }
            Expr::CreateUnique(cu) => {
                footprint.has_create = true;
                for (_, value) in &cu.fields {
                    self.check_expr_effects(value, footprint);
                }
                if let Some(lock) = &cu.lock {
                    self.check_expr_effects(lock, footprint);
                }
            }
            Expr::ReplaceUnique(ru) => {
                footprint.has_consume = true;
                footprint.has_create = true;
                self.check_expr_effects(&ru.expr, footprint);
                for (_, value) in &ru.fields {
                    self.check_expr_effects(value, footprint);
                }
            }
            _ => {}
        }
    }

    fn apply_effect_to_footprint(&self, effect: EffectClass, footprint: &mut EffectFootprint) {
        match effect {
            EffectClass::Pure => {}
            EffectClass::ReadOnly => footprint.has_read_ref = true,
            EffectClass::Creating => footprint.has_create = true,
            EffectClass::Destroying => footprint.has_consume = true,
            EffectClass::Mutating => {
                footprint.has_consume = true;
                footprint.has_create = true;
            }
        }
    }

    fn convert_effect_class(&self, effect: crate::ast::EffectClass) -> EffectClass {
        match effect {
            crate::ast::EffectClass::Pure => EffectClass::Pure,
            crate::ast::EffectClass::ReadOnly => EffectClass::ReadOnly,
            crate::ast::EffectClass::Mutating => EffectClass::Mutating,
            crate::ast::EffectClass::Creating => EffectClass::Creating,
            crate::ast::EffectClass::Destroying => EffectClass::Destroying,
        }
    }

    fn lower_signature_and_body(
        &mut self,
        params: &[Param],
        outputs: &[crate::ast::ActionOutput],
        stmts: &[Stmt],
        tail_return_type: Option<IrType>,
        core_input_bindings: &HashSet<String>,
    ) -> (Vec<IrParam>, IrBody) {
        let mut vars = HashMap::new();
        let mut ir_params = params
            .iter()
            .map(|param| {
                let binding = self.new_var(param.name.clone(), Self::convert_type(&param.ty));
                vars.insert(param.name.clone(), binding.clone());
                IrParam {
                    name: param.name.clone(),
                    ty: Self::convert_type(&param.ty),
                    is_mut: param.is_mut,
                    is_ref: param.is_ref,
                    is_read_ref: param.is_read_ref,
                    source: param.source,
                    binding,
                }
            })
            .collect::<Vec<_>>();
        for output in outputs {
            let binding = self.new_var(output.name.clone(), Self::convert_type(&output.ty));
            vars.insert(output.name.clone(), binding.clone());
            ir_params.push(IrParam {
                name: output.name.clone(),
                ty: Self::convert_type(&output.ty),
                is_mut: false,
                is_ref: false,
                is_read_ref: false,
                source: ParamSource::Output,
                binding,
            });
        }
        self.transition_param_ids = ir_params.iter().map(|param| param.binding.id).collect();
        let lifecycle_certificate = match certify_straight_line_lifecycle_only(stmts) {
            Ok(certificate) => certificate,
            Err(err) => {
                self.errors.push(err);
                StraightLineLifecycleOnly
            }
        };
        let mut blocks = Vec::new();
        let entry = self.push_block(&mut blocks);
        let _ = self.lower_stmts(stmts, entry, &mut blocks, &mut vars, tail_return_type.as_ref());
        let consume_set =
            self.collect_straight_line_consume_patterns(&blocks, &ir_params, core_input_bindings, &lifecycle_certificate);
        let mut read_refs = self.collect_read_ref_param_patterns(&ir_params);
        read_refs.extend(self.collect_read_ref_patterns(&blocks));
        let create_set = self.collect_straight_line_create_patterns(&blocks, &ir_params, &lifecycle_certificate);
        let mutate_set = self.collect_mutate_param_patterns(&ir_params, consume_set.len(), create_set.len());
        let write_intents = Self::collect_write_intents(&create_set, &mutate_set);
        self.transition_param_ids.clear();
        self.transition_coverable_value_ids.clear();

        (ir_params, IrBody { consume_set, read_refs, create_set, mutate_set, write_intents, blocks })
    }

    fn collect_write_intents(create_set: &[CreatePattern], mutate_set: &[MutatePattern]) -> Vec<WriteIntent> {
        let create_intents = create_set.iter().enumerate().map(|(index, pattern)| WriteIntent {
            operation: pattern.operation.clone(),
            ty: pattern.ty.clone(),
            binding: pattern.binding.clone(),
            index,
            fields: pattern.fields.iter().map(|(field, _)| field.clone()).collect(),
        });
        let mutate_intents = mutate_set.iter().map(|pattern| WriteIntent {
            operation: pattern.operation.clone(),
            ty: pattern.ty.clone(),
            binding: pattern.binding.clone(),
            index: pattern.output_index,
            fields: pattern.fields.clone(),
        });

        create_intents.chain(mutate_intents).collect()
    }

    fn collect_straight_line_consume_patterns(
        &self,
        blocks: &[IrBlock],
        params: &[IrParam],
        core_input_bindings: &HashSet<String>,
        _certificate: &StraightLineLifecycleOnly,
    ) -> Vec<CellPattern> {
        let mut patterns = params
            .iter()
            .filter(|param| core_input_bindings.contains(param.name.as_str()))
            .filter_map(|param| self.cell_pattern_from_var(&param.binding, "input"))
            .collect::<Vec<_>>();
        for block in blocks {
            for instruction in &block.instructions {
                if let IrInstruction::Consume { operand } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(operand, "consume") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Transfer { operand, .. } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(operand, "transfer") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Destroy { operand, policy } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(operand, "destroy") {
                        let mut pattern = pattern;
                        pattern.destruction_policy = Some(policy.clone());
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::ReplaceUnique { operand, .. } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(operand, "replace_unique") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Claim { receipt, .. } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(receipt, "claim") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Settle { operand, .. } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(operand, "settle") {
                        patterns.push(pattern);
                    }
                }
            }
        }
        patterns
    }

    fn collect_read_ref_patterns(&self, blocks: &[IrBlock]) -> Vec<CellPattern> {
        let mut patterns = Vec::new();
        for block in blocks {
            for instruction in &block.instructions {
                if let IrInstruction::ReadRef { dest, ty } = instruction {
                    patterns.push(CellPattern {
                        operation: "read_ref".to_string(),
                        type_hash: Some(type_hash_for_name(ty)),
                        binding: dest.name.clone(),
                        fields: Vec::new(),
                        destruction_policy: None,
                    });
                }
            }
        }
        patterns
    }

    fn collect_read_ref_param_patterns(&self, params: &[IrParam]) -> Vec<CellPattern> {
        params
            .iter()
            .filter(|param| param.is_read_ref)
            .filter_map(|param| {
                Self::named_type_name_from_ir_type(&param.ty).map(|type_name| CellPattern {
                    operation: "read_ref".to_string(),
                    type_hash: Some(type_hash_for_name(type_name)),
                    binding: param.name.clone(),
                    fields: Vec::new(),
                    destruction_policy: None,
                })
            })
            .collect()
    }

    fn collect_straight_line_create_patterns(
        &self,
        blocks: &[IrBlock],
        params: &[IrParam],
        _certificate: &StraightLineLifecycleOnly,
    ) -> Vec<CreatePattern> {
        let output_bindings =
            params.iter().filter(|param| param.source == ParamSource::Output).map(|param| param.name.clone()).collect::<HashSet<_>>();
        let mut patterns = params
            .iter()
            .filter(|param| param.source == ParamSource::Output)
            .filter_map(|param| self.create_pattern_from_var(&param.binding, "output"))
            .collect::<Vec<_>>();
        for block in blocks {
            for instruction in &block.instructions {
                if let IrInstruction::Create { pattern, .. } = instruction {
                    if pattern.operation == "output" && output_bindings.contains(&pattern.binding) {
                        if let Some(existing) = patterns.iter_mut().find(|existing| existing.binding == pattern.binding) {
                            *existing = pattern.clone();
                        } else {
                            patterns.push(pattern.clone());
                        }
                    } else {
                        patterns.push(pattern.clone());
                    }
                } else if let IrInstruction::CreateUnique { pattern, .. } = instruction {
                    patterns.push(pattern.clone());
                } else if let IrInstruction::ReplaceUnique { pattern, .. } = instruction {
                    patterns.push(pattern.clone());
                } else if let IrInstruction::Transfer { dest, to, .. } = instruction {
                    if let Some(pattern) = self.create_pattern_from_var_with_lock(dest, "transfer", Some(to.clone())) {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Claim { dest, .. } = instruction {
                    if let Some(pattern) = self.create_pattern_from_var(dest, "claim") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Settle { dest, .. } = instruction {
                    if let Some(pattern) = self.create_pattern_from_var(dest, "settle") {
                        patterns.push(pattern);
                    }
                }
            }
        }
        patterns
    }

    fn collect_mutate_param_patterns(&self, params: &[IrParam], consume_count: usize, create_count: usize) -> Vec<MutatePattern> {
        let mut patterns = Vec::new();
        for param in params {
            if !(param.is_mut || matches!(param.ty, IrType::MutRef(_))) {
                continue;
            }
            let Some(type_name) = Self::named_type_name_from_ir_type(&param.ty) else {
                continue;
            };
            if !matches!(self.type_kinds.get(type_name), Some(IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt)) {
                continue;
            }
            let fields = self
                .mutated_fields
                .get(&param.binding.id)
                .map(|fields| fields.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let mutated = fields.iter().cloned().collect::<BTreeSet<_>>();
            let preserved_fields = self
                .type_fields
                .get(type_name)
                .map(|fields| {
                    fields
                        .keys()
                        .filter(|field| !mutated.contains(*field))
                        .cloned()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let transitions = self
                .mutated_field_transitions
                .get(&param.binding.id)
                .map(|fields| fields.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let mutation_index = patterns.len();
            patterns.push(MutatePattern {
                operation: "mutate".to_string(),
                ty: type_name.to_string(),
                binding: param.name.clone(),
                fields,
                preserved_fields,
                transitions,
                input_index: consume_count + mutation_index,
                output_index: create_count + mutation_index,
                preserve_type_hash: true,
                preserve_lock_hash: true,
            });
        }
        patterns
    }

    fn cell_pattern_from_operand(&self, operand: &IrOperand, operation: &str) -> Option<CellPattern> {
        let IrOperand::Var(var) = operand else {
            return None;
        };
        self.cell_pattern_from_var(var, operation)
    }

    fn cell_pattern_from_var(&self, var: &IrVar, operation: &str) -> Option<CellPattern> {
        let type_name = match &var.ty {
            IrType::Named(name) => Some(name.as_str()),
            IrType::Ref(inner) | IrType::MutRef(inner) => match inner.as_ref() {
                IrType::Named(name) => Some(name.as_str()),
                _ => None,
            },
            _ => None,
        }?;
        Some(CellPattern {
            operation: operation.to_string(),
            type_hash: Some(type_hash_for_name(type_name)),
            binding: var.name.clone(),
            fields: Vec::new(),
            destruction_policy: None,
        })
    }

    fn create_pattern_from_var(&self, var: &IrVar, operation: &str) -> Option<CreatePattern> {
        self.create_pattern_from_var_with_lock(var, operation, None)
    }

    fn create_pattern_from_var_with_lock(&self, var: &IrVar, operation: &str, lock: Option<IrOperand>) -> Option<CreatePattern> {
        let type_name = match &var.ty {
            IrType::Named(name) => Some(name.as_str()),
            IrType::Ref(inner) | IrType::MutRef(inner) => match inner.as_ref() {
                IrType::Named(name) => Some(name.as_str()),
                _ => None,
            },
            _ => None,
        }?;
        let fields = self
            .aggregate_fields
            .get(&var.id)
            .map(|field_vars| {
                let mut fields =
                    field_vars.iter().map(|(field, var)| (field.clone(), IrOperand::Var(var.clone()))).collect::<Vec<_>>();
                fields.sort_by(|(left, _), (right, _)| left.cmp(right));
                fields
            })
            .unwrap_or_default();
        Some(CreatePattern {
            operation: operation.to_string(),
            ty: type_name.to_string(),
            binding: var.name.clone(),
            fields,
            lock,
            identity: IrIdentityPolicy::None,
        })
    }

    fn named_type_name_from_ir_type(ty: &IrType) -> Option<&str> {
        match ty {
            IrType::Named(name) => Some(name.as_str()),
            IrType::Ref(inner) | IrType::MutRef(inner) => Self::named_type_name_from_ir_type(inner),
            _ => None,
        }
    }

    fn claim_output_type_for_operand(&self, operand: &IrOperand) -> IrType {
        let ty = self.operand_type(operand);
        Self::named_type_name_from_ir_type(&ty)
            .and_then(|name| self.receipt_claim_outputs.get(name))
            .and_then(Clone::clone)
            .unwrap_or(IrType::U64)
    }

    fn materialize_matching_output_fields(
        &mut self,
        source: &IrOperand,
        output_ty: &IrType,
        active: BlockId,
        blocks: &mut Vec<IrBlock>,
    ) -> HashMap<String, IrVar> {
        let (IrOperand::Var(source_var), Some(output_type_name)) = (source, Self::named_type_name_from_ir_type(output_ty)) else {
            return HashMap::new();
        };
        let Some(output_fields) = self.type_fields.get(output_type_name).cloned() else {
            return HashMap::new();
        };

        let mut field_names = output_fields.keys().cloned().collect::<Vec<_>>();
        field_names.sort();
        let mut field_vars = HashMap::new();
        for field_name in field_names {
            let Some(output_field_ty) = output_fields.get(&field_name) else {
                continue;
            };
            let Some(source_field_ty) = self.lookup_field_ir_type(&source_var.ty, &field_name) else {
                continue;
            };
            if &source_field_ty != output_field_ty || !is_verifier_coverable_output_field_type(output_field_ty) {
                continue;
            }
            if let Some(field_var) = self.materialize_schema_field(source_var, &field_name, active, blocks) {
                field_vars.insert(field_name, field_var);
            }
        }
        field_vars
    }

    fn named_type_name_from_ast_type(ty: &Type) -> Option<&str> {
        match ty {
            Type::Named(name) => Some(name.as_str()),
            Type::Ref(inner) | Type::MutRef(inner) => Self::named_type_name_from_ast_type(inner),
            _ => None,
        }
    }

    fn infer_touches_shared(&self, params: &[IrParam], body: &IrBody) -> Vec<[u8; 32]> {
        let shared_hashes = self
            .type_kinds
            .iter()
            .filter_map(|(name, kind)| (*kind == IrTypeKind::Shared).then_some(type_hash_for_name(name)))
            .collect::<Vec<_>>();
        let mut hashes = Vec::new();
        for param in params {
            if let Some(type_name) = self.param_shared_type_name(param) {
                hashes.push(type_hash_for_name(type_name));
            }
        }
        for pattern in body.read_refs.iter().chain(body.consume_set.iter()) {
            if let Some(type_hash) = pattern.type_hash {
                if shared_hashes.contains(&type_hash) {
                    hashes.push(type_hash);
                }
            }
        }
        for pattern in &body.create_set {
            if self.type_kinds.get(&pattern.ty) == Some(&IrTypeKind::Shared) {
                hashes.push(type_hash_for_name(&pattern.ty));
            }
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::Call { dest: Some(dest), .. } = instruction {
                    self.collect_shared_type_hashes_from_type(&dest.ty, &mut hashes);
                }
            }
        }
        hashes.sort();
        hashes.dedup();
        hashes
    }

    fn param_shared_type_name<'a>(&self, param: &'a IrParam) -> Option<&'a str> {
        let type_name = Self::named_type_name_from_ir_type(&param.ty)?;
        (self.type_kinds.get(type_name) == Some(&IrTypeKind::Shared)).then_some(type_name)
    }

    fn param_mutates_shared_state(&self, param: &IrParam) -> bool {
        if self.param_shared_type_name(param).is_none() {
            return false;
        }
        param.is_mut || matches!(param.ty, IrType::MutRef(_))
    }

    fn collect_shared_type_hashes_from_type(&self, ty: &IrType, hashes: &mut Vec<[u8; 32]>) {
        match ty {
            IrType::Named(name) if self.type_kinds.get(name) == Some(&IrTypeKind::Shared) => {
                hashes.push(type_hash_for_name(name));
            }
            IrType::Ref(inner) | IrType::MutRef(inner) | IrType::Array(inner, _) => {
                self.collect_shared_type_hashes_from_type(inner, hashes);
            }
            IrType::Tuple(items) => {
                for item in items {
                    self.collect_shared_type_hashes_from_type(item, hashes);
                }
            }
            _ => {}
        }
    }

    fn estimate_cycles(&self, body: &IrBody) -> u64 {
        let instruction_count = body.blocks.iter().map(|block| block.instructions.len() as u64).sum::<u64>();
        let branch_count = body
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, IrTerminator::Abort(_) | IrTerminator::Jump(_) | IrTerminator::Branch { .. }))
            .count() as u64;
        let cell_ops = (body.consume_set.len() + body.read_refs.len() + body.create_set.len()) as u64;
        instruction_count
            .saturating_mul(8)
            .saturating_add(branch_count.saturating_mul(4))
            .saturating_add(cell_ops.saturating_mul(128))
            .saturating_add(32)
    }

    fn lower_stmts(
        &mut self,
        stmts: &[Stmt],
        mut current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        tail_return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        for (index, stmt) in stmts.iter().enumerate() {
            if let Some(expected_ty) = tail_return_type.filter(|_| index + 1 == stmts.len()) {
                if let Stmt::Expr(expr) = stmt {
                    let lowered = self.lower_expr_with_expected_type(expr, expected_ty, current, blocks, vars);
                    let active = lowered.current?;
                    self.block_mut(blocks, active).terminator = IrTerminator::Return(Some(lowered.operand));
                    return None;
                }
                if let Stmt::If(if_stmt) = stmt {
                    return self.lower_if_stmt(if_stmt, current, blocks, vars, Some(expected_ty));
                }
            }
            let next = self.lower_stmt(stmt, current, blocks, vars, tail_return_type)?;
            current = next;
        }

        Some(current)
    }

    fn lower_stmt(
        &mut self,
        stmt: &Stmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        expected_return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        match stmt {
            Stmt::Let(let_stmt) => {
                let transition_coverable = matches!(let_stmt.pattern, BindingPattern::Name(_))
                    && self.transition_expr_is_coverable_u64(&let_stmt.value, vars);
                let lowered = match &let_stmt.ty {
                    Some(declared_ty) => {
                        self.lower_expr_with_expected_type(&let_stmt.value, &Self::convert_type(declared_ty), current, blocks, vars)
                    }
                    None => self.lower_expr(&let_stmt.value, current, blocks, vars),
                };
                let active = lowered.current?;
                let block = self.block_mut(blocks, active);
                if let (BindingPattern::Name(name), Some(declared_ty)) = (&let_stmt.pattern, &let_stmt.ty) {
                    let declared_ty = Self::convert_type(declared_ty);
                    let var = if let_stmt.is_mut {
                        let owned = self.materialize_owned_operand(name, lowered.operand, block);
                        if owned.ty == declared_ty {
                            owned
                        } else {
                            self.materialize_operand_with_type(name, IrOperand::Var(owned), declared_ty, block)
                        }
                    } else {
                        self.materialize_operand_with_type(name, lowered.operand, declared_ty, block)
                    };
                    if transition_coverable && var.ty == IrType::U64 {
                        self.transition_coverable_value_ids.insert(var.id);
                    }
                    vars.insert(name.clone(), var);
                    return Some(active);
                }
                if let (BindingPattern::Name(name), true) = (&let_stmt.pattern, let_stmt.is_mut) {
                    let var = self.materialize_owned_operand(name, lowered.operand, block);
                    if transition_coverable && var.ty == IrType::U64 {
                        self.transition_coverable_value_ids.insert(var.id);
                    }
                    vars.insert(name.clone(), var);
                    return Some(active);
                }
                let bound = self.bind_pattern(&let_stmt.pattern, lowered.operand, block, vars);
                if transition_coverable && bound.as_ref().is_some_and(|var| var.ty == IrType::U64) {
                    let bound = bound.expect("checked above");
                    self.transition_coverable_value_ids.insert(bound.id);
                }
                Some(active)
            }
            Stmt::Expr(expr) => self.lower_expr(expr, current, blocks, vars).current,
            Stmt::Return(None) => {
                self.block_mut(blocks, current).terminator = IrTerminator::Return(None);
                None
            }
            Stmt::Return(Some(expr)) => {
                let lowered = if let Some(expected_ty) = expected_return_type {
                    self.lower_expr_with_expected_type(expr, expected_ty, current, blocks, vars)
                } else {
                    self.lower_expr(expr, current, blocks, vars)
                };
                let active = lowered.current?;
                self.block_mut(blocks, active).terminator = IrTerminator::Return(Some(lowered.operand));
                None
            }
            Stmt::If(if_stmt) => self.lower_if_stmt(if_stmt, current, blocks, vars, None),
            Stmt::For(for_stmt) => self.lower_for_stmt(for_stmt, current, blocks, vars),
            Stmt::While(while_stmt) => self.lower_while_stmt(while_stmt, current, blocks, vars),
        }
    }

    fn bind_pattern(
        &mut self,
        pattern: &BindingPattern,
        value: IrOperand,
        block: &mut IrBlock,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<IrVar> {
        match pattern {
            BindingPattern::Name(name) => {
                let var = self.materialize_operand(name, value, block);
                vars.insert(name.clone(), var.clone());
                Some(var)
            }
            BindingPattern::Tuple(items) => {
                let base_var = match &value {
                    IrOperand::Var(var) => Some(var.clone()),
                    IrOperand::Const(_) => None,
                };
                for (index, item) in items.iter().enumerate() {
                    if matches!(item, BindingPattern::Wildcard) {
                        continue;
                    }
                    let field = index.to_string();
                    let tuple_name = format!("{}_{}", binding_pattern_label(item), index);
                    let projected = base_var
                        .as_ref()
                        .and_then(|var| self.aggregate_fields.get(&var.id).and_then(|fields| fields.get(&field)).cloned())
                        .map(IrOperand::Var)
                        .or_else(|| {
                            let base_var = base_var.as_ref()?;
                            let field_ty = self.lookup_field_ir_type(&base_var.ty, &field)?;
                            let field_var = self.new_var(tuple_name, field_ty);
                            block.instructions.push(IrInstruction::FieldAccess {
                                dest: field_var.clone(),
                                obj: IrOperand::Var(base_var.clone()),
                                field,
                            });
                            Some(IrOperand::Var(field_var))
                        });

                    if let Some(projected) = projected {
                        self.bind_pattern(item, projected, block, vars);
                    } else {
                        self.record_error("tuple binding requires a lowered tuple aggregate", Span::default());
                    }
                }
                base_var
            }
            BindingPattern::Wildcard => None,
        }
    }

    fn materialize_operand(&mut self, name: &str, operand: IrOperand, block: &mut IrBlock) -> IrVar {
        match operand {
            IrOperand::Var(var) => var,
            IrOperand::Const(value) => {
                let ty = self.const_type(&value);
                let var = self.new_var(name.to_string(), ty);
                block.instructions.push(IrInstruction::LoadConst { dest: var.clone(), value });
                var
            }
        }
    }

    fn materialize_operand_with_type(&mut self, name: &str, operand: IrOperand, ty: IrType, block: &mut IrBlock) -> IrVar {
        match operand {
            IrOperand::Var(var) if var.ty == ty => var,
            IrOperand::Var(var) => {
                let dest = self.new_var(name.to_string(), ty);
                let source = IrOperand::Var(var);
                block.instructions.push(IrInstruction::Cast { dest: dest.clone(), src: source.clone() });
                self.copy_aggregate_metadata(&source, dest.id);
                dest
            }
            IrOperand::Const(value) => {
                let dest = self.new_var(name.to_string(), ty);
                if let Some(value) = Self::cast_const(&value, &dest.ty) {
                    block.instructions.push(IrInstruction::LoadConst { dest: dest.clone(), value });
                } else {
                    self.record_error(
                        format!("constant materialization from {:?} to {:?} is not supported", value, dest.ty),
                        Span::default(),
                    );
                    block.instructions.push(IrInstruction::LoadConst { dest: dest.clone(), value: IrConst::U64(0) });
                }
                dest
            }
        }
    }

    fn materialize_owned_operand(&mut self, name: &str, operand: IrOperand, block: &mut IrBlock) -> IrVar {
        match operand {
            IrOperand::Var(var) => {
                let dest = self.new_var(name.to_string(), var.ty.clone());
                let source = IrOperand::Var(var);
                block.instructions.push(IrInstruction::Move { dest: dest.clone(), src: source.clone() });
                self.copy_aggregate_metadata(&source, dest.id);
                dest
            }
            IrOperand::Const(value) => {
                let ty = self.const_type(&value);
                let dest = self.new_var(name.to_string(), ty);
                block.instructions.push(IrInstruction::LoadConst { dest: dest.clone(), value });
                dest
            }
        }
    }

    fn lower_expr(
        &mut self,
        expr: &Expr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        match expr {
            Expr::Integer(value) => LoweredExpr { operand: IrOperand::Const(IrConst::U64(*value)), current: Some(current) },
            Expr::Bool(value) => LoweredExpr { operand: IrOperand::Const(IrConst::Bool(*value)), current: Some(current) },
            Expr::Identifier(name) => {
                if let Some(var) = vars.get(name).cloned() {
                    LoweredExpr { operand: IrOperand::Var(var), current: Some(current) }
                } else if let Some(constant) = self.lower_constant(name, Span::default()) {
                    LoweredExpr { operand: constant, current: Some(current) }
                } else if let Some(zero) = self.lower_zero_value(name) {
                    LoweredExpr { operand: zero, current: Some(current) }
                } else if let Some(enum_variant) = self.lower_enum_variant(name) {
                    LoweredExpr { operand: enum_variant, current: Some(current) }
                } else if let Some(flow_state) = self.lower_flow_state_name(name) {
                    LoweredExpr { operand: flow_state, current: Some(current) }
                } else {
                    self.record_error(format!("IR lowering encountered unresolved identifier '{}'", name), Span::default());
                    LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
                }
            }
            Expr::Assign(assign) => self.lower_assign_expr(assign, current, blocks, vars),
            Expr::Binary(binary) => {
                let left = self.lower_expr(&binary.left, current, blocks, vars);
                let Some(active) = left.current else {
                    return left;
                };
                let right = self.lower_expr(&binary.right, active, blocks, vars);
                let Some(active) = right.current else {
                    return right;
                };
                let (left_operand, right_operand) = self.coerce_binary_integer_operands(
                    binary.op,
                    left.operand,
                    right.operand,
                    binary.span,
                    self.block_mut(blocks, active),
                );
                if let Err(message) = self.validate_binary_operands(binary.op, &left_operand, &right_operand) {
                    self.record_error(message, binary.span);
                }
                let dest = self.new_var("tmp", self.binary_result_type(binary.op, &left_operand, &right_operand));
                self.block_mut(blocks, active).instructions.push(IrInstruction::Binary {
                    dest: dest.clone(),
                    op: binary.op,
                    left: left_operand,
                    right: right_operand,
                });
                LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
            }
            Expr::Unary(unary) => {
                let operand = self.lower_expr(&unary.expr, current, blocks, vars);
                let Some(active) = operand.current else {
                    return operand;
                };
                if unary.op == UnaryOp::Neg {
                    self.record_error("unary negation is not supported for unsigned integer types", unary.span);
                }
                let dest = self.new_var("tmp", self.unary_result_type(unary.op, &operand.operand));
                let block = self.block_mut(blocks, active);
                block.instructions.push(IrInstruction::Unary { dest: dest.clone(), op: unary.op, operand: operand.operand });
                LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
            }
            Expr::Call(call) => {
                if let Some(lowered) = self.try_lower_builtin_call(call, current, blocks, vars) {
                    return lowered;
                }
                let mut active = current;
                // If the call target is a field access (e.g. obj.method()), lower the receiver
                // expression first so that any side effects (e.g. consume) are not dropped.
                if let Expr::FieldAccess(field) = call.func.as_ref() {
                    let lowered = self.lower_expr(&field.expr, active, blocks, vars);
                    let Some(next) = lowered.current else {
                        return lowered;
                    };
                    active = next;
                }
                let mut args = Vec::with_capacity(call.args.len());
                for arg in &call.args {
                    let lowered = self.lower_expr(arg, active, blocks, vars);
                    let Some(next) = lowered.current else {
                        return lowered;
                    };
                    active = next;
                    args.push(lowered.operand);
                }
                let func = match call.func.as_ref() {
                    Expr::Identifier(name) => self.lower_call_target_name(name),
                    Expr::FieldAccess(field) => field.field.clone(),
                    _ => "__expr_call".to_string(),
                };
                let source_func = match call.func.as_ref() {
                    Expr::Identifier(name) => name.as_str(),
                    Expr::FieldAccess(field) => field.field.as_str(),
                    _ => "__expr_call",
                };
                match self.call_return_type(source_func, &func) {
                    Some(Some(return_type)) => {
                        let dest = self.new_var("call_tmp", return_type);
                        self.block_mut(blocks, active).instructions.push(IrInstruction::Call { dest: Some(dest.clone()), func, args });
                        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
                    }
                    Some(None) => {
                        self.block_mut(blocks, active).instructions.push(IrInstruction::Call { dest: None, func, args });
                        LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) }
                    }
                    None => {
                        self.record_error(format!("call '{}' has no known return type during IR lowering", source_func), call.span);
                        LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) }
                    }
                }
            }
            Expr::ReadRef(read_ref) => self.lower_read_ref_expr(read_ref, current, blocks),
            Expr::Create(create) => self.lower_create_expr(create, current, blocks, vars),
            Expr::Consume(consume) => self.lower_consume_expr(consume, current, blocks, vars),
            Expr::Transfer(transfer) => self.lower_transfer_expr(transfer, current, blocks, vars),
            Expr::Destroy(destroy) => self.lower_destroy_expr(destroy, current, blocks, vars),
            Expr::Claim(claim) => self.lower_claim_expr(claim, current, blocks, vars),
            Expr::Settle(settle) => self.lower_settle_expr(settle, current, blocks, vars),
            Expr::CreateUnique(cu) => self.lower_create_unique_expr(cu, current, blocks, vars),
            Expr::ReplaceUnique(ru) => self.lower_replace_unique_expr(ru, current, blocks, vars),
            Expr::Assert(assert_expr) => self.lower_assert_expr(assert_expr, current, blocks, vars),
            Expr::Require(require_expr) => self.lower_require_expr(require_expr, current, blocks, vars),
            Expr::RequireBlock(require_block) => self.lower_require_block_expr(require_block, current, blocks, vars),
            Expr::Preserve(preserve_expr) => self.lower_preserve_expr(preserve_expr, current, blocks, vars),
            Expr::StructInit(init) => self.lower_struct_init(init, current, blocks, vars),
            Expr::FieldAccess(field) => self.lower_field_access(field, current, blocks, vars),
            Expr::Index(index) => self.lower_index_expr(index, current, blocks, vars),
            Expr::Block(stmts) => self.lower_tail_block_value(stmts, current, blocks, vars, None),
            Expr::If(if_expr) => self.lower_if_expr(if_expr, current, blocks, vars),
            Expr::Match(match_expr) => self.lower_match_expr(match_expr, current, blocks, vars),
            Expr::Cast(cast) => {
                let lowered = self.lower_expr(&cast.expr, current, blocks, vars);
                let Some(active) = lowered.current else {
                    return lowered;
                };
                let target_ty = Self::convert_type(&cast.ty);
                match &lowered.operand {
                    IrOperand::Const(value) => {
                        if let Some(converted) = Self::cast_const(value, &target_ty) {
                            LoweredExpr { operand: IrOperand::Const(converted), current: Some(active) }
                        } else {
                            self.record_error(
                                format!("constant cast from {:?} to {:?} is not supported", value, target_ty),
                                cast.span,
                            );
                            LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) }
                        }
                    }
                    IrOperand::Var(var) if var.ty == target_ty => lowered,
                    IrOperand::Var(_) if target_ty == IrType::U128 => {
                        self.record_error("cast to u128 from a non-constant expression is not yet supported".to_string(), cast.span);
                        LoweredExpr { operand: IrOperand::Const(IrConst::U128(0)), current: Some(active) }
                    }
                    operand => {
                        let block = self.block_mut(blocks, active);
                        let dest = self.materialize_operand_with_type("cast", operand.clone(), target_ty, block);
                        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
                    }
                }
            }
            Expr::Array(items) => self.lower_array_expr(items, current, blocks, vars),
            Expr::Tuple(items) => self.lower_tuple_expr(items, current, blocks, vars),
            Expr::String(_) => {
                self.record_error("string literals are only supported in metadata positions such as assert messages", Span::default());
                LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
            }
            Expr::ByteString(bytes) => LoweredExpr {
                operand: IrOperand::Const(IrConst::Array(bytes.iter().copied().map(IrConst::U8).collect())),
                current: Some(current),
            },
            Expr::Range(_) => {
                self.record_error("range expressions are only supported as for-loop iterables", Span::default());
                LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
            }
            Expr::StdlibCall(call) => self.lower_stdlib_call(call, current, blocks, vars),
        }
    }

    fn lower_tail_block_value(
        &mut self,
        stmts: &[Stmt],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        expected_ty: Option<&IrType>,
    ) -> LoweredExpr {
        let Some((last, prefix)) = stmts.split_last() else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(current) };
        };

        let mut active = current;
        for stmt in prefix {
            let Some(next) = self.lower_stmt(stmt, active, blocks, vars, expected_ty) else {
                return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None };
            };
            active = next;
        }

        match last {
            Stmt::Expr(expr) => {
                if let Some(expected_ty) = expected_ty {
                    self.lower_expr_with_expected_type(expr, expected_ty, active, blocks, vars)
                } else {
                    self.lower_expr(expr, active, blocks, vars)
                }
            }
            Stmt::If(if_stmt) if if_stmt.else_branch.is_some() => self.lower_if_stmt_value(if_stmt, active, blocks, vars, expected_ty),
            stmt => {
                let next = self.lower_stmt(stmt, active, blocks, vars, expected_ty);
                LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: next }
            }
        }
    }

    fn const_type(&self, value: &IrConst) -> IrType {
        match value {
            IrConst::U8(_) => IrType::U8,
            IrConst::U16(_) => IrType::U16,
            IrConst::U32(_) => IrType::U32,
            IrConst::U64(_) => IrType::U64,
            IrConst::U128(_) => IrType::U128,
            IrConst::Unit => IrType::Unit,
            IrConst::Bool(_) => IrType::Bool,
            IrConst::Address(_) => IrType::Address,
            IrConst::Hash(_) => IrType::Hash,
            IrConst::Array(items) => {
                IrType::Array(Box::new(items.first().map(|item| self.const_type(item)).unwrap_or(IrType::U8)), items.len())
            }
        }
    }

    fn ir_numeric_width(ty: &IrType) -> Option<u32> {
        match ty {
            IrType::U8 => Some(8),
            IrType::U16 => Some(16),
            IrType::U32 => Some(32),
            IrType::U64 => Some(64),
            IrType::U128 => Some(128),
            _ => None,
        }
    }

    fn coerce_binary_integer_operands(
        &mut self,
        op: BinaryOp,
        left: IrOperand,
        right: IrOperand,
        span: Span,
        block: &mut IrBlock,
    ) -> (IrOperand, IrOperand) {
        let left_ty = self.operand_type(&left);
        let right_ty = self.operand_type(&right);
        if left_ty == right_ty {
            return (left, right);
        }
        if matches!(op, BinaryOp::Add | BinaryOp::Sub) && left_ty == IrType::U128 && right_ty == IrType::U64 {
            return (left, right);
        }
        if matches!(op, BinaryOp::Add) && left_ty == IrType::U64 && right_ty == IrType::U128 {
            return (left, right);
        }

        match (&left, &right) {
            (IrOperand::Const(value), _) => {
                let expected = if right_ty == IrType::U128 && matches!(op, BinaryOp::Add) { IrType::U64 } else { right_ty.clone() };
                if let Some(converted) = Self::cast_const(value, &expected) {
                    return (IrOperand::Const(converted), right);
                }
            }
            (_, IrOperand::Const(value)) => {
                let expected =
                    if left_ty == IrType::U128 && matches!(op, BinaryOp::Add | BinaryOp::Sub) { IrType::U64 } else { left_ty.clone() };
                if let Some(converted) = Self::cast_const(value, &expected) {
                    return (left, IrOperand::Const(converted));
                }
            }
            _ => {}
        }

        // Implicit integer widening: widen the narrower variable operand to the
        // wider type so that mixed-width arithmetic like u64 * u16 compiles.
        // Only widen to types smaller than u128; u128 arithmetic has its own
        // dedicated delta path above.
        let lw = Self::ir_numeric_width(&left_ty);
        let rw = Self::ir_numeric_width(&right_ty);
        if let (Some(lw), Some(rw)) = (lw, rw) {
            let target_is_u128 = lw < rw && matches!(right_ty, IrType::U128) || lw >= rw && matches!(left_ty, IrType::U128);
            if !target_is_u128 {
                if lw < rw {
                    let widened = self.new_var("widened", right_ty);
                    block.instructions.push(IrInstruction::Cast { dest: widened.clone(), src: left });
                    return (IrOperand::Var(widened), right);
                } else {
                    let widened = self.new_var("widened", left_ty);
                    block.instructions.push(IrInstruction::Cast { dest: widened.clone(), src: right });
                    return (left, IrOperand::Var(widened));
                }
            }
        }

        self.record_error(format!("IR lowering encountered mixed-width {:?} operands {:?} and {:?}", op, left_ty, right_ty), span);
        (left, right)
    }

    fn validate_binary_operands(&self, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> std::result::Result<(), String> {
        let left_ty = self.operand_type(left);
        let right_ty = self.operand_type(right);
        let types_match = left_ty == right_ty;
        let both_integer = Self::ir_numeric_width(&left_ty).is_some() && Self::ir_numeric_width(&right_ty).is_some();
        let widenable = both_integer && !types_match;
        match op {
            BinaryOp::Add => {
                let supported = (types_match && left_ty != IrType::U128)
                    || (left_ty == IrType::U128 && right_ty == IrType::U64)
                    || (left_ty == IrType::U64 && right_ty == IrType::U128)
                    || (widenable && left_ty != IrType::U128 && right_ty != IrType::U128);
                if supported {
                    Ok(())
                } else {
                    Err(format!("unsupported arithmetic operand types {:?} and {:?}", left_ty, right_ty))
                }
            }
            BinaryOp::Sub => {
                let supported = (types_match && left_ty != IrType::U128)
                    || (left_ty == IrType::U128 && right_ty == IrType::U64)
                    || (widenable && left_ty != IrType::U128 && right_ty != IrType::U128);
                if supported {
                    Ok(())
                } else {
                    Err(format!("unsupported arithmetic operand types {:?} and {:?}", left_ty, right_ty))
                }
            }
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if (types_match && left_ty != IrType::U128) || (widenable && left_ty != IrType::U128 && right_ty != IrType::U128) {
                    Ok(())
                } else {
                    Err(format!("unsupported arithmetic operand types {:?} and {:?}", left_ty, right_ty))
                }
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                if (types_match && left_ty != IrType::U128) || (widenable && left_ty != IrType::U128 && right_ty != IrType::U128) {
                    Ok(())
                } else {
                    Err(format!("unsupported ordering operand types {:?} and {:?}", left_ty, right_ty))
                }
            }
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::And | BinaryOp::Or => Ok(()),
        }
    }

    fn binary_result_type(&self, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> IrType {
        match op {
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => IrType::Bool,
            BinaryOp::Add if self.operand_type(left) == IrType::U128 || self.operand_type(right) == IrType::U128 => IrType::U128,
            BinaryOp::Sub if self.operand_type(left) == IrType::U128 && self.operand_type(right) != IrType::U128 => IrType::U128,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                let left_ty = self.operand_type(left);
                let right_ty = self.operand_type(right);
                if left_ty == right_ty {
                    return left_ty;
                }
                // Return the wider type for mixed-width operands.
                let lw = Self::ir_numeric_width(&left_ty);
                let rw = Self::ir_numeric_width(&right_ty);
                match (lw, rw) {
                    (Some(lw), Some(rw)) if lw >= rw => left_ty,
                    (Some(_), Some(_)) => right_ty,
                    _ => left_ty,
                }
            }
            BinaryOp::And | BinaryOp::Or => IrType::Bool,
        }
    }

    fn unary_result_type(&self, op: UnaryOp, operand: &IrOperand) -> IrType {
        match op {
            UnaryOp::Not => IrType::Bool,
            UnaryOp::Neg => self.operand_type(operand),
            UnaryOp::Ref | UnaryOp::Deref => match operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            },
        }
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
        blocks.push(IrBlock { id, instructions: Vec::new(), terminator: IrTerminator::Return(None) });
        id
    }

    fn block_mut<'a>(&self, blocks: &'a mut [IrBlock], id: BlockId) -> &'a mut IrBlock {
        &mut blocks[id.0]
    }

    fn lower_if_stmt(
        &mut self,
        if_stmt: &IfStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        tail_return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        let lowered_cond = self.lower_expr(&if_stmt.condition, current, blocks, vars);
        let cond = lowered_cond.operand;
        let current = lowered_cond.current?;

        let then_block = self.push_block(blocks);
        let else_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Branch { cond, then_block, else_block };

        let mut then_vars = vars.clone();
        let then_exit = self.lower_stmts(&if_stmt.then_branch, then_block, blocks, &mut then_vars, tail_return_type);

        let mut else_vars = vars.clone();
        let else_exit = if let Some(else_branch) = &if_stmt.else_branch {
            self.lower_stmts(else_branch, else_block, blocks, &mut else_vars, tail_return_type)
        } else {
            Some(else_block)
        };

        if then_exit.is_none() && else_exit.is_none() {
            return None;
        }

        let join = self.push_block(blocks);
        if let Some(exit) = then_exit {
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }
        if let Some(exit) = else_exit {
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }
        Some(join)
    }

    fn ensure_join_aggregate_slots(&mut self, dest: &IrVar, branch_values: &[&IrOperand]) {
        match &dest.ty {
            IrType::Tuple(items) => {
                if self.aggregate_fields.contains_key(&dest.id) {
                    return;
                }
                let mut slots = HashMap::new();
                for (index, ty) in items.iter().enumerate() {
                    let field = index.to_string();
                    slots.insert(field.clone(), self.new_var(format!("{}_{}", dest.name, field), ty.clone()));
                }
                self.aggregate_fields.insert(dest.id, slots);
            }
            IrType::Named(name) => {
                if self.aggregate_fields.contains_key(&dest.id) {
                    return;
                }
                let Some(fields) = self.type_fields.get(name).cloned() else {
                    return;
                };
                let has_aggregate_source = branch_values
                    .iter()
                    .any(|operand| matches!(operand, IrOperand::Var(var) if self.aggregate_fields.contains_key(&var.id)));
                if !has_aggregate_source {
                    return;
                }
                let mut slots = HashMap::new();
                for (field, ty) in fields {
                    slots.insert(field.clone(), self.new_var(format!("{}_{}", dest.name, field), ty));
                }
                self.aggregate_fields.insert(dest.id, slots);
            }
            IrType::Array(inner, len) => {
                if self.aggregate_elements.contains_key(&dest.id) {
                    return;
                }
                let has_aggregate_source = branch_values
                    .iter()
                    .any(|operand| matches!(operand, IrOperand::Var(var) if self.aggregate_elements.contains_key(&var.id)));
                if !has_aggregate_source {
                    return;
                }
                let elements =
                    (0..*len).map(|index| self.new_var(format!("{}_{}", dest.name, index), (**inner).clone())).collect::<Vec<_>>();
                self.aggregate_elements.insert(dest.id, elements);
            }
            _ => {}
        }
    }

    fn aggregate_field_operand(&self, source: &IrOperand, field: &str) -> Option<IrOperand> {
        let IrOperand::Var(var) = source else {
            return None;
        };
        self.aggregate_fields.get(&var.id).and_then(|fields| fields.get(field)).cloned().map(IrOperand::Var)
    }

    fn lower_join_value_into_dest(&mut self, dest: &IrVar, src: IrOperand, block_id: BlockId, blocks: &mut Vec<IrBlock>) {
        self.block_mut(blocks, block_id).instructions.push(IrInstruction::Move { dest: dest.clone(), src: src.clone() });

        if let Some(fields) = self.aggregate_fields.get(&dest.id).cloned() {
            let mut ordered_fields = fields.into_iter().collect::<Vec<_>>();
            ordered_fields.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (field, field_dest) in ordered_fields {
                if let Some(field_src) = self.aggregate_field_operand(&src, &field) {
                    self.block_mut(blocks, block_id)
                        .instructions
                        .push(IrInstruction::Move { dest: field_dest.clone(), src: field_src.clone() });
                    self.copy_aggregate_metadata(&field_src, field_dest.id);
                } else if let IrOperand::Var(src_var) = &src {
                    self.block_mut(blocks, block_id).instructions.push(IrInstruction::FieldAccess {
                        dest: field_dest,
                        obj: IrOperand::Var(src_var.clone()),
                        field,
                    });
                }
            }
            if matches!(dest.ty, IrType::Tuple(_)) {
                let mut fields = self.aggregate_fields.get(&dest.id).cloned().unwrap_or_default().into_iter().collect::<Vec<_>>();
                fields.sort_by_key(|(field, _)| field.parse::<usize>().unwrap_or(usize::MAX));
                let fields = fields.into_iter().map(|(_, var)| IrOperand::Var(var)).collect::<Vec<_>>();
                self.block_mut(blocks, block_id).instructions.push(IrInstruction::Tuple { dest: dest.clone(), fields });
            }
        }

        if let Some(elements) = self.aggregate_elements.get(&dest.id).cloned() {
            for (index, element_dest) in elements.into_iter().enumerate() {
                if let Some(element_src) = match &src {
                    IrOperand::Var(src_var) => {
                        self.aggregate_elements.get(&src_var.id).and_then(|elements| elements.get(index)).cloned().map(IrOperand::Var)
                    }
                    IrOperand::Const(_) => None,
                } {
                    self.block_mut(blocks, block_id)
                        .instructions
                        .push(IrInstruction::Move { dest: element_dest.clone(), src: element_src.clone() });
                    self.copy_aggregate_metadata(&element_src, element_dest.id);
                } else {
                    self.block_mut(blocks, block_id).instructions.push(IrInstruction::Index {
                        dest: element_dest,
                        arr: src.clone(),
                        idx: IrOperand::Const(IrConst::U64(index as u64)),
                    });
                }
            }
        }
    }

    fn lower_if_stmt_value(
        &mut self,
        if_stmt: &IfStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        expected_ty: Option<&IrType>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&if_stmt.condition, current, blocks, vars);
        let cond = lowered_cond.operand;
        let Some(current) = lowered_cond.current else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None };
        };
        let Some(else_branch) = &if_stmt.else_branch else {
            let next = self.lower_if_stmt(if_stmt, current, blocks, vars, None);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: next };
        };

        let then_block = self.push_block(blocks);
        let else_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Branch { cond, then_block, else_block };

        let mut then_vars = vars.clone();
        let then_lowered = self.lower_tail_block_value(&if_stmt.then_branch, then_block, blocks, &mut then_vars, expected_ty);
        let mut else_vars = vars.clone();
        let else_lowered = self.lower_tail_block_value(else_branch, else_block, blocks, &mut else_vars, expected_ty);

        if then_lowered.current.is_none() && else_lowered.current.is_none() {
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None };
        }

        let result_ty = match (expected_ty, then_lowered.current.is_some(), else_lowered.current.is_some()) {
            (Some(expected_ty), _, _) => expected_ty.clone(),
            (None, true, _) => self.operand_type(&then_lowered.operand),
            (None, false, true) => self.operand_type(&else_lowered.operand),
            (None, false, false) => return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None },
        };
        let dest = self.new_var("if_tmp", result_ty);
        let join = self.push_block(blocks);
        let branch_values = [
            then_lowered.current.as_ref().map(|_| &then_lowered.operand),
            else_lowered.current.as_ref().map(|_| &else_lowered.operand),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        self.ensure_join_aggregate_slots(&dest, &branch_values);

        if let Some(exit) = then_lowered.current {
            self.lower_join_value_into_dest(&dest, then_lowered.operand, exit, blocks);
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }

        if let Some(exit) = else_lowered.current {
            self.lower_join_value_into_dest(&dest, else_lowered.operand, exit, blocks);
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }

        LoweredExpr { operand: IrOperand::Var(dest), current: Some(join) }
    }

    fn lower_while_stmt(
        &mut self,
        while_stmt: &WhileStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<BlockId> {
        let cond_entry = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Jump(cond_entry);

        let lowered_cond = self.lower_expr(&while_stmt.condition, cond_entry, blocks, vars);
        let cond = lowered_cond.operand;
        let cond_exit = lowered_cond.current?;

        let body_block = self.push_block(blocks);
        let exit_block = self.push_block(blocks);
        self.block_mut(blocks, cond_exit).terminator = IrTerminator::Branch { cond, then_block: body_block, else_block: exit_block };

        let mut body_vars = vars.clone();
        let body_exit = self.lower_stmts(&while_stmt.body, body_block, blocks, &mut body_vars, None);
        if let Some(exit) = body_exit {
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(cond_entry);
        }

        Some(exit_block)
    }

    fn lower_for_stmt(
        &mut self,
        for_stmt: &ForStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<BlockId> {
        match &for_stmt.iterable {
            Expr::Range(range) => self.lower_for_range_stmt(for_stmt, range, current, blocks, vars),
            _ => {
                let lowered = self.lower_expr(&for_stmt.iterable, current, blocks, vars);
                let active = lowered.current?;
                self.lower_for_iterable_stmt(for_stmt, lowered.operand, active, blocks, vars)
            }
        }
    }

    fn lower_for_iterable_stmt(
        &mut self,
        for_stmt: &ForStmt,
        iterable: IrOperand,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<BlockId> {
        if let IrOperand::Var(iterable_var) = &iterable {
            if let Some(elements) = self.aggregate_elements.get(&iterable_var.id).cloned() {
                return self.lower_for_local_fixed_array_stmt(for_stmt, elements, current, blocks, vars);
            }
            if let IrType::Array(_, len) = &iterable_var.ty {
                let len = *len;
                return self.lower_for_static_array_pointer_stmt(for_stmt, iterable, len, current, blocks, vars);
            }
        }

        let Some(item_ty) = self.iter_item_type(&iterable) else {
            self.record_error("for-loop iterable has no lowered item type", for_stmt.span);
            return Some(current);
        };

        let index_var = self.new_var("iter_index", IrType::U64);
        let length_var = self.new_var("iter_len", IrType::U64);
        {
            let block = self.block_mut(blocks, current);
            block.instructions.push(IrInstruction::Move { dest: index_var.clone(), src: IrOperand::Const(IrConst::U64(0)) });
            block.instructions.push(IrInstruction::Length { dest: length_var.clone(), operand: iterable.clone() });
        }

        let cond_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Jump(cond_block);

        let cond_var = self.new_var("iter_cond", IrType::Bool);
        {
            let block = self.block_mut(blocks, cond_block);
            block.instructions.push(IrInstruction::Binary {
                dest: cond_var.clone(),
                op: BinaryOp::Lt,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Var(length_var.clone()),
            });
        }

        let body_block = self.push_block(blocks);
        let exit_block = self.push_block(blocks);
        self.block_mut(blocks, cond_block).terminator =
            IrTerminator::Branch { cond: IrOperand::Var(cond_var), then_block: body_block, else_block: exit_block };

        let item_var = self.new_var("iter_item", item_ty);
        self.block_mut(blocks, body_block).instructions.push(IrInstruction::Index {
            dest: item_var.clone(),
            arr: iterable,
            idx: IrOperand::Var(index_var.clone()),
        });

        let mut body_vars = vars.clone();
        self.bind_pattern(&for_stmt.pattern, IrOperand::Var(item_var), self.block_mut(blocks, body_block), &mut body_vars);
        let body_exit = self.lower_stmts(&for_stmt.body, body_block, blocks, &mut body_vars, None);
        if let Some(exit) = body_exit {
            let next_index = self.new_var("iter_next", IrType::U64);
            let block = self.block_mut(blocks, exit);
            block.instructions.push(IrInstruction::Binary {
                dest: next_index.clone(),
                op: BinaryOp::Add,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Const(IrConst::U64(1)),
            });
            block.instructions.push(IrInstruction::Move { dest: index_var, src: IrOperand::Var(next_index) });
            block.terminator = IrTerminator::Jump(cond_block);
        }

        Some(exit_block)
    }

    fn lower_for_static_array_pointer_stmt(
        &mut self,
        for_stmt: &ForStmt,
        iterable: IrOperand,
        len: usize,
        mut current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<BlockId> {
        let Some(item_ty) = self.iter_item_type(&iterable) else {
            self.record_error("for-loop iterable has no lowered item type", for_stmt.span);
            return Some(current);
        };

        for index in 0..len {
            let item_var = self.new_var(format!("iter_item_{}", index), item_ty.clone());
            self.block_mut(blocks, current).instructions.push(IrInstruction::Index {
                dest: item_var.clone(),
                arr: iterable.clone(),
                idx: IrOperand::Const(IrConst::U64(index as u64)),
            });

            let mut body_vars = vars.clone();
            self.bind_pattern(&for_stmt.pattern, IrOperand::Var(item_var), self.block_mut(blocks, current), &mut body_vars);
            current = self.lower_stmts(&for_stmt.body, current, blocks, &mut body_vars, None)?;
        }

        Some(current)
    }

    fn lower_for_local_fixed_array_stmt(
        &mut self,
        for_stmt: &ForStmt,
        elements: Vec<IrVar>,
        mut current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<BlockId> {
        for (index, element_var) in elements.into_iter().enumerate() {
            let item_var = self.new_var(format!("iter_item_{}", index), element_var.ty.clone());
            self.block_mut(blocks, current)
                .instructions
                .push(IrInstruction::Move { dest: item_var.clone(), src: IrOperand::Var(element_var.clone()) });
            self.copy_aggregate_metadata(&IrOperand::Var(element_var), item_var.id);

            let mut body_vars = vars.clone();
            self.bind_pattern(&for_stmt.pattern, IrOperand::Var(item_var), self.block_mut(blocks, current), &mut body_vars);
            current = self.lower_stmts(&for_stmt.body, current, blocks, &mut body_vars, None)?;
        }

        Some(current)
    }

    fn lower_for_range_stmt(
        &mut self,
        for_stmt: &ForStmt,
        range: &RangeExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<BlockId> {
        let lowered_start = self.lower_expr(&range.start, current, blocks, vars);
        let start = lowered_start.operand;
        let active = lowered_start.current?;

        let lowered_end = self.lower_expr(&range.end, active, blocks, vars);
        let end = lowered_end.operand;
        let active = lowered_end.current?;

        let index_var = self.new_var("for_index", IrType::U64);
        let end_var = self.new_var("for_end", IrType::U64);
        {
            let block = self.block_mut(blocks, active);
            block.instructions.push(IrInstruction::Move { dest: index_var.clone(), src: start });
            block.instructions.push(IrInstruction::Move { dest: end_var.clone(), src: end });
        }

        let cond_block = self.push_block(blocks);
        self.block_mut(blocks, active).terminator = IrTerminator::Jump(cond_block);

        let cond_var = self.new_var("for_cond", IrType::Bool);
        {
            let block = self.block_mut(blocks, cond_block);
            block.instructions.push(IrInstruction::Binary {
                dest: cond_var.clone(),
                op: BinaryOp::Lt,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Var(end_var.clone()),
            });
        }

        let body_block = self.push_block(blocks);
        let exit_block = self.push_block(blocks);
        self.block_mut(blocks, cond_block).terminator =
            IrTerminator::Branch { cond: IrOperand::Var(cond_var), then_block: body_block, else_block: exit_block };

        let mut body_vars = vars.clone();
        self.bind_pattern(&for_stmt.pattern, IrOperand::Var(index_var.clone()), self.block_mut(blocks, body_block), &mut body_vars);
        let body_exit = self.lower_stmts(&for_stmt.body, body_block, blocks, &mut body_vars, None);
        if let Some(exit) = body_exit {
            let next_index = self.new_var("for_next", IrType::U64);
            let block = self.block_mut(blocks, exit);
            block.instructions.push(IrInstruction::Binary {
                dest: next_index.clone(),
                op: BinaryOp::Add,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Const(IrConst::U64(1)),
            });
            block.instructions.push(IrInstruction::Move { dest: index_var, src: IrOperand::Var(next_index) });
            block.terminator = IrTerminator::Jump(cond_block);
        }

        Some(exit_block)
    }

    fn lower_assert_expr(
        &mut self,
        assert_expr: &AssertExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&assert_expr.condition, current, blocks, vars);
        let Some(active) = lowered_cond.current else {
            return lowered_cond;
        };
        let cond = lowered_cond.operand;

        let ok_block = self.push_block(blocks);
        let fail_block = self.push_block(blocks);
        self.block_mut(blocks, active).terminator = IrTerminator::Branch { cond, then_block: ok_block, else_block: fail_block };
        self.block_mut(blocks, fail_block).terminator = self.fail_closed_terminator();

        LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(ok_block) }
    }

    fn lower_require_expr(
        &mut self,
        require_expr: &RequireExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&require_expr.condition, current, blocks, vars);
        let Some(active) = lowered_cond.current else {
            return lowered_cond;
        };
        let cond = lowered_cond.operand;

        let ok_block = self.push_block(blocks);
        let fail_block = self.push_block(blocks);
        self.block_mut(blocks, active).terminator = IrTerminator::Branch { cond, then_block: ok_block, else_block: fail_block };
        self.block_mut(blocks, fail_block).terminator = self.fail_closed_terminator();

        LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(ok_block) }
    }

    fn fail_closed_terminator(&self) -> IrTerminator {
        IrTerminator::Abort(CellScriptRuntimeError::AssertionFailed)
    }

    /// Lower `require { expr1, expr2, ... }` — desugar into independent atomic `require` statements.
    fn lower_require_block_expr(
        &mut self,
        require_block: &RequireBlockExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let mut active = current;
        for expr in &require_block.expressions {
            // Each expression in a require block is treated as a require condition:
            // require_block { expr1, expr2 } desugars to require expr1; require expr2;
            let lowered = self.lower_expr(expr, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            let cond = lowered.operand;

            let ok_block = self.push_block(blocks);
            let fail_block = self.push_block(blocks);
            self.block_mut(blocks, next).terminator = IrTerminator::Branch { cond, then_block: ok_block, else_block: fail_block };
            self.block_mut(blocks, fail_block).terminator = self.fail_closed_terminator();
            active = ok_block;
        }
        LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
    }

    /// Lower `preserve output from input { field1, field2, ... }` — desugar into
    /// `require output.field1 == input.field1; require output.field2 == input.field2; ...`
    fn lower_preserve_expr(
        &mut self,
        preserve: &PreserveExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let mut active = current;
        for field_name in &preserve.fields {
            let output_field = Expr::FieldAccess(FieldAccessExpr {
                expr: Box::new(Expr::Identifier(preserve.output_name.clone())),
                field: field_name.clone(),
                span: preserve.span,
            });
            let input_field = Expr::FieldAccess(FieldAccessExpr {
                expr: Box::new(Expr::Identifier(preserve.input_name.clone())),
                field: field_name.clone(),
                span: preserve.span,
            });
            let equality = Expr::Binary(BinaryExpr {
                op: BinaryOp::Eq,
                left: Box::new(output_field),
                right: Box::new(input_field),
                span: preserve.span,
            });
            let require_expr = RequireExpr { condition: Box::new(equality), message: None, span: preserve.span };
            let lowered = self.lower_require_expr(&require_expr, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
        }
        LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
    }

    fn lower_cell_metadata_equality_call(
        &mut self,
        qualified: &str,
        call: &StdlibCallExpr,
        field: CellMetadataField,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        if call.args.len() != 2 {
            self.record_error(format!("{} expects 2 arguments (output, input), got {}", qualified, call.args.len()), call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(current) };
        }

        let left = self.lower_expr(&call.args[0], current, blocks, vars);
        let Some(active) = left.current else {
            return left;
        };
        let right = self.lower_expr(&call.args[1], active, blocks, vars);
        let Some(active) = right.current else {
            return right;
        };

        self.block_mut(blocks, active).instructions.push(IrInstruction::CellMetadataEquality {
            left: left.operand,
            right: right.operand,
            field,
        });
        LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
    }

    fn lower_stdlib_output_create_and_preserve(
        &mut self,
        qualified: &str,
        input_role: &str,
        input: &Expr,
        output: &Expr,
        lock: Option<&Expr>,
        preserve_fields: &[String],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<BlockId> {
        let output_name = match output {
            Expr::Identifier(name) => name.clone(),
            _ => {
                self.record_error(format!("{} output must be a named Cell output binding", qualified), output.span());
                return None;
            }
        };
        let input_name = match input {
            Expr::Identifier(name) => name.clone(),
            _ => {
                self.record_error(format!("{} {} must be a named Cell input binding", qualified, input_role), input.span());
                return None;
            }
        };
        let Some(output_ty) = vars.get(&output_name).and_then(|var| Self::named_type_name_from_ir_type(&var.ty).map(str::to_string))
        else {
            self.record_error(format!("{} output must be a named Cell output binding", qualified), output.span());
            return None;
        };

        let create_fields = preserve_fields
            .iter()
            .map(|field| {
                (
                    field.clone(),
                    Expr::FieldAccess(FieldAccessExpr { expr: Box::new(input.clone()), field: field.clone(), span: input.span() }),
                )
            })
            .collect::<Vec<_>>();
        let create_expr = CreateExpr {
            target: Some(output_name.clone()),
            ty: output_ty,
            fields: create_fields,
            lock: lock.cloned().map(Box::new),
            span: output.span(),
        };
        let lowered = self.lower_create_expr(&create_expr, current, blocks, vars);
        let mut active = lowered.current?;

        if !preserve_fields.is_empty() {
            let preserve = PreserveExpr { output_name, input_name, fields: preserve_fields.to_vec(), span: input.span() };
            let lowered = self.lower_preserve_expr(&preserve, active, blocks, vars);
            active = lowered.current?;
        }

        Some(active)
    }

    /// Lower a stdlib call expression by expanding it into core IR instructions.
    ///
    /// Constraint patterns expand into `require` constraints or canonical
    /// verifier metadata checks.
    ///
    /// Lifecycle patterns expand into `consume` plus explicit output and verifier constraints.
    fn lower_stdlib_call(
        &mut self,
        call: &StdlibCallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let qualified = format!("std::{}::{}", call.namespace, call.name);
        let mut active = current;

        match qualified.as_str() {
            // Constraint patterns — expand to canonical verifier constraints
            "std::cell::same_lock" | "std::cell::preserve_lock" => {
                self.lower_cell_metadata_equality_call(&qualified, call, CellMetadataField::LockHash, active, blocks, vars)
            }
            "std::cell::same_type" | "std::cell::preserve_type" => {
                if call.args.len() != 2 {
                    self.record_error(
                        format!("{} expects 2 arguments (output, input), got {}", qualified, call.args.len()),
                        call.span,
                    );
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) };
                }
                let output = &call.args[0];
                let input = &call.args[1];
                let output_type_hash = Expr::Call(CallExpr {
                    func: Box::new(Expr::FieldAccess(FieldAccessExpr {
                        expr: Box::new(output.clone()),
                        field: "type_hash".to_string(),
                        span: call.span,
                    })),
                    args: vec![],
                    span: call.span,
                });
                let input_type_hash = Expr::Call(CallExpr {
                    func: Box::new(Expr::FieldAccess(FieldAccessExpr {
                        expr: Box::new(input.clone()),
                        field: "type_hash".to_string(),
                        span: call.span,
                    })),
                    args: vec![],
                    span: call.span,
                });
                let equality = Expr::Binary(BinaryExpr {
                    op: BinaryOp::Eq,
                    left: Box::new(output_type_hash),
                    right: Box::new(input_type_hash),
                    span: call.span,
                });
                let require_expr = RequireExpr { condition: Box::new(equality), message: None, span: call.span };
                self.lower_require_expr(&require_expr, active, blocks, vars)
            }
            "std::cell::preserve_capacity" => {
                self.lower_cell_metadata_equality_call(&qualified, call, CellMetadataField::Capacity, active, blocks, vars)
            }
            "std::accounting::conserved" => {
                if call.args.len() != 2 {
                    self.record_error(
                        format!("{} expects 2 arguments (output, input), got {}", qualified, call.args.len()),
                        call.span,
                    );
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) };
                }
                let output = &call.args[0];
                let input = &call.args[1];
                let output_amount = Expr::FieldAccess(FieldAccessExpr {
                    expr: Box::new(output.clone()),
                    field: "amount".to_string(),
                    span: call.span,
                });
                let input_amount =
                    Expr::FieldAccess(FieldAccessExpr { expr: Box::new(input.clone()), field: "amount".to_string(), span: call.span });
                let equality = Expr::Binary(BinaryExpr {
                    op: BinaryOp::Eq,
                    left: Box::new(output_amount),
                    right: Box::new(input_amount),
                    span: call.span,
                });
                let require_expr = RequireExpr { condition: Box::new(equality), message: None, span: call.span };
                self.lower_require_expr(&require_expr, active, blocks, vars)
            }

            // Lifecycle patterns — consume + explicit output and verifier constraints
            "std::lifecycle::transfer" => {
                if call.args.len() != 3 {
                    self.record_error(
                        format!("std::lifecycle::transfer expects 3 arguments (input, output, to), got {}", call.args.len()),
                        call.span,
                    );
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) };
                }
                let input = &call.args[0];
                let output = &call.args[1];
                let lock = &call.args[2];

                // 1. consume input
                let consume_expr = ConsumeExpr { expr: Box::new(input.clone()), span: call.span };
                let lowered = self.lower_consume_expr(&consume_expr, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 2. constrain the proposed output binding and lock target
                let output_name = match output {
                    Expr::Identifier(name) => name.clone(),
                    _ => {
                        self.record_error("std::lifecycle::transfer output must be a named Cell output binding", call.span);
                        return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) };
                    }
                };
                let output_ty = vars
                    .get(&output_name)
                    .and_then(|var| Self::named_type_name_from_ir_type(&var.ty).map(str::to_string))
                    .unwrap_or_else(|| "output".to_string());
                let create_fields = call
                    .preserve_fields
                    .iter()
                    .map(|field| {
                        (
                            field.clone(),
                            Expr::FieldAccess(FieldAccessExpr {
                                expr: Box::new(input.clone()),
                                field: field.clone(),
                                span: call.span,
                            }),
                        )
                    })
                    .collect::<Vec<_>>();
                let create_expr = CreateExpr {
                    target: Some(output_name.clone()),
                    ty: output_ty,
                    fields: create_fields,
                    lock: Some(Box::new(lock.clone())),
                    span: call.span,
                };
                let lowered = self.lower_create_expr(&create_expr, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 3. require output.type_hash == input.type_hash
                let output_type_hash = Expr::Call(CallExpr {
                    func: Box::new(Expr::FieldAccess(FieldAccessExpr {
                        expr: Box::new(output.clone()),
                        field: "type_hash".to_string(),
                        span: call.span,
                    })),
                    args: vec![],
                    span: call.span,
                });
                let input_type_hash = Expr::Call(CallExpr {
                    func: Box::new(Expr::FieldAccess(FieldAccessExpr {
                        expr: Box::new(input.clone()),
                        field: "type_hash".to_string(),
                        span: call.span,
                    })),
                    args: vec![],
                    span: call.span,
                });
                let type_eq = Expr::Binary(BinaryExpr {
                    op: BinaryOp::Eq,
                    left: Box::new(output_type_hash),
                    right: Box::new(input_type_hash),
                    span: call.span,
                });
                let require_type = RequireExpr { condition: Box::new(type_eq), message: None, span: call.span };
                let lowered = self.lower_require_expr(&require_type, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 4. preserve listed fields from input to output
                if !call.preserve_fields.is_empty() {
                    let input_name = match input {
                        Expr::Identifier(name) => name.clone(),
                        _ => "input".to_string(),
                    };
                    let preserve = PreserveExpr { output_name, input_name, fields: call.preserve_fields.clone(), span: call.span };
                    let lowered = self.lower_preserve_expr(&preserve, active, blocks, vars);
                    let Some(next) = lowered.current else {
                        return lowered;
                    };
                    active = next;
                }

                LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
            }
            "std::receipt::claim" => {
                if call.args.len() != 3 {
                    self.record_error(format!("std::receipt::claim expects 3 arguments, got {}", call.args.len()), call.span);
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) };
                }
                let receipt = &call.args[0];

                // 1. consume receipt
                let consume_expr = ConsumeExpr { expr: Box::new(receipt.clone()), span: call.span };
                let lowered = self.lower_consume_expr(&consume_expr, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 2. canonical output construction from receipt fields with an explicit lock target.
                let Some(next) = self.lower_stdlib_output_create_and_preserve(
                    "std::receipt::claim",
                    "receipt",
                    receipt,
                    &call.args[1],
                    call.args.get(2),
                    &call.preserve_fields,
                    active,
                    blocks,
                    vars,
                ) else {
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) };
                };
                active = next;

                LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
            }
            "std::lifecycle::settle" => {
                if call.args.len() != 3 {
                    self.record_error(format!("std::lifecycle::settle expects 3 arguments, got {}", call.args.len()), call.span);
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) };
                }
                let input = &call.args[0];

                // 1. consume input
                let consume_expr = ConsumeExpr { expr: Box::new(input.clone()), span: call.span };
                let lowered = self.lower_consume_expr(&consume_expr, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 2. canonical output construction from settled fields with an explicit lock target.
                let Some(next) = self.lower_stdlib_output_create_and_preserve(
                    "std::lifecycle::settle",
                    "input",
                    input,
                    &call.args[1],
                    call.args.get(2),
                    &call.preserve_fields,
                    active,
                    blocks,
                    vars,
                ) else {
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) };
                };
                active = next;

                LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
            }

            _ => {
                self.record_error(
                    format!(
                        "unknown stdlib pattern '{}' — each stdlib primitive must have a canonical expansion into core CellScript",
                        qualified
                    ),
                    call.span,
                );
                LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
            }
        }
    }

    fn lower_assign_expr(
        &mut self,
        assign: &AssignExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_value = self.lower_expr(&assign.value, current, blocks, vars);
        let Some(active) = lowered_value.current else {
            return lowered_value;
        };

        match assign.target.as_ref() {
            Expr::Identifier(name) => {
                let Some(target_var) = vars.get(name).cloned() else {
                    self.record_error(format!("assignment target '{}' is not bound in IR lowering", name), assign.span);
                    return LoweredExpr { operand: lowered_value.operand, current: Some(active) };
                };
                match assign.op {
                    AssignOp::Assign => {
                        self.block_mut(blocks, active)
                            .instructions
                            .push(IrInstruction::Move { dest: target_var.clone(), src: lowered_value.operand });
                    }
                    AssignOp::AddAssign => {
                        let tmp = self.new_var("assign_tmp", target_var.ty.clone());
                        let block = self.block_mut(blocks, active);
                        block.instructions.push(IrInstruction::Binary {
                            dest: tmp.clone(),
                            op: BinaryOp::Add,
                            left: IrOperand::Var(target_var.clone()),
                            right: lowered_value.operand,
                        });
                        block.instructions.push(IrInstruction::Move { dest: target_var.clone(), src: IrOperand::Var(tmp) });
                    }
                }
                LoweredExpr { operand: IrOperand::Var(target_var), current: Some(active) }
            }
            Expr::FieldAccess(field) => {
                self.lower_field_assign(field, assign.op, &assign.value, lowered_value.operand, active, blocks, vars)
            }
            Expr::Index(index) => self.lower_index_assign(index, assign.op, lowered_value.operand, active, blocks, vars),
            _ => {
                self.record_error("invalid assignment target reached IR lowering", assign.span);
                LoweredExpr { operand: lowered_value.operand, current: Some(active) }
            }
        }
    }

    fn lower_create_expr(
        &mut self,
        create: &CreateExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let dest = if let Some(target) = &create.target {
            vars.get(target).cloned().unwrap_or_else(|| self.new_var(target.clone(), IrType::Named(create.ty.clone())))
        } else {
            self.new_var(format!("create_{}", create.ty), IrType::Named(create.ty.clone()))
        };
        let mut active = current;
        let mut lowered_fields = Vec::with_capacity(create.fields.len());
        let mut field_vars = HashMap::new();

        for (field_name, field_expr) in &create.fields {
            let expected_ty = self.type_fields.get(&create.ty).and_then(|fields| fields.get(field_name)).cloned();
            let lowered = if let Some(state_operand) = self.lower_flow_state_initializer(&create.ty, field_name, field_expr) {
                LoweredExpr { operand: state_operand, current: Some(active) }
            } else if let Some(expected_ty) = expected_ty {
                self.lower_expr_with_expected_type(field_expr, &expected_ty, active, blocks, vars)
            } else {
                self.lower_expr(field_expr, active, blocks, vars)
            };
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            lowered_fields.push((field_name.clone(), lowered.operand.clone()));

            let field_ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("{}_{}", create.ty, field_name), field_ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand });
            field_vars.insert(field_name.clone(), field_var);
        }

        let lowered_lock = if let Some(lock_expr) = &create.lock {
            let lowered = self.lower_expr(lock_expr, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            Some(lowered.operand)
        } else {
            None
        };

        let pattern = CreatePattern {
            operation: if create.target.is_some() { "output".to_string() } else { "create".to_string() },
            ty: create.ty.clone(),
            binding: dest.name.clone(),
            fields: lowered_fields,
            lock: lowered_lock,
            identity: IrIdentityPolicy::None,
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::Create { dest: dest.clone(), pattern });
        self.aggregate_fields.insert(dest.id, field_vars);
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_flow_state_initializer(&self, type_name: &str, field_name: &str, expr: &Expr) -> Option<IrOperand> {
        if self.flow_state_fields.get(type_name).is_none_or(|state_field| state_field != field_name) {
            return None;
        }
        let Expr::Identifier(state_name) = expr else {
            return None;
        };
        let index = self.flow_state_index(type_name, state_name)?;
        Some(IrOperand::Const(IrConst::U64(index as u64)))
    }

    fn lower_consume_expr(
        &mut self,
        consume: &ConsumeExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered = self.lower_expr(&consume.expr, current, blocks, vars);
        let Some(active) = lowered.current else {
            return lowered;
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::Consume { operand: lowered.operand.clone() });
        LoweredExpr { operand: lowered.operand, current: Some(active) }
    }

    fn lower_destroy_expr(
        &mut self,
        destroy: &DestroyExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered = self.lower_expr(&destroy.expr, current, blocks, vars);
        let Some(active) = lowered.current else {
            return lowered;
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::Destroy {
            operand: lowered.operand.clone(),
            policy: Self::lower_destruction_policy(&destroy.policy),
        });
        LoweredExpr { operand: lowered.operand, current: Some(active) }
    }

    fn lower_destruction_policy(policy: &DestructionPolicy) -> IrDestructionPolicy {
        match policy {
            DestructionPolicy::Default => IrDestructionPolicy::Default,
            DestructionPolicy::SingletonType => IrDestructionPolicy::SingletonType,
            DestructionPolicy::Unique { identity } => IrDestructionPolicy::Unique { identity: identity.clone() },
            DestructionPolicy::Instance { identity_field } => IrDestructionPolicy::Instance { identity_field: identity_field.clone() },
            DestructionPolicy::BurnAmount { field } => IrDestructionPolicy::BurnAmount { field: field.clone() },
        }
    }

    fn lower_identity_policy(policy: &IdentityPolicy) -> IrIdentityPolicy {
        match policy {
            IdentityPolicy::None => IrIdentityPolicy::None,
            IdentityPolicy::CkbTypeId => IrIdentityPolicy::CkbTypeId,
            IdentityPolicy::Field(path) => IrIdentityPolicy::Field(path.clone()),
            IdentityPolicy::ScriptArgs => IrIdentityPolicy::ScriptArgs,
            IdentityPolicy::SingletonType => IrIdentityPolicy::SingletonType,
        }
    }

    fn lower_create_unique_expr(
        &mut self,
        cu: &CreateUniqueExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let dest = self.new_var(format!("create_unique_{}", cu.ty), IrType::Named(cu.ty.clone()));
        let mut active = current;
        let mut lowered_fields = Vec::with_capacity(cu.fields.len());
        let mut field_vars = HashMap::new();

        for (field_name, field_expr) in &cu.fields {
            let expected_ty = self.type_fields.get(&cu.ty).and_then(|fields| fields.get(field_name)).cloned();
            let lowered = if let Some(state_operand) = self.lower_flow_state_initializer(&cu.ty, field_name, field_expr) {
                LoweredExpr { operand: state_operand, current: Some(active) }
            } else if let Some(expected_ty) = expected_ty {
                self.lower_expr_with_expected_type(field_expr, &expected_ty, active, blocks, vars)
            } else {
                self.lower_expr(field_expr, active, blocks, vars)
            };
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            lowered_fields.push((field_name.clone(), lowered.operand.clone()));

            let field_ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("{}_{}", cu.ty, field_name), field_ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand });
            field_vars.insert(field_name.clone(), field_var);
        }

        let lowered_lock = if let Some(lock_expr) = &cu.lock {
            let lowered = self.lower_expr(lock_expr, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            Some(lowered.operand)
        } else {
            None
        };

        let identity = Self::lower_identity_policy(&cu.identity);
        let pattern = CreatePattern {
            operation: "create_unique".to_string(),
            ty: cu.ty.clone(),
            binding: dest.name.clone(),
            fields: lowered_fields,
            lock: lowered_lock,
            identity: identity.clone(),
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::CreateUnique { dest: dest.clone(), pattern, identity });
        self.aggregate_fields.insert(dest.id, field_vars);
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_replace_unique_expr(
        &mut self,
        ru: &ReplaceUniqueExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        // Lower the input cell expression
        let lowered_input = self.lower_expr(&ru.expr, current, blocks, vars);
        let Some(active) = lowered_input.current else {
            return lowered_input;
        };

        // Lower the replacement output fields
        let dest_ty = self.operand_type(&lowered_input.operand);
        let dest = self.new_var(format!("replace_unique_{}", ru.ty), dest_ty);
        let mut active = active;
        let mut lowered_fields = Vec::with_capacity(ru.fields.len());
        let mut field_vars = HashMap::new();

        for (field_name, field_expr) in &ru.fields {
            let expected_ty = self.type_fields.get(&ru.ty).and_then(|fields| fields.get(field_name)).cloned();
            let lowered = if let Some(state_operand) = self.lower_flow_state_initializer(&ru.ty, field_name, field_expr) {
                LoweredExpr { operand: state_operand, current: Some(active) }
            } else if let Some(expected_ty) = expected_ty {
                self.lower_expr_with_expected_type(field_expr, &expected_ty, active, blocks, vars)
            } else {
                self.lower_expr(field_expr, active, blocks, vars)
            };
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            lowered_fields.push((field_name.clone(), lowered.operand.clone()));

            let field_ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("{}_{}", ru.ty, field_name), field_ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand });
            field_vars.insert(field_name.clone(), field_var);
        }

        let identity = Self::lower_identity_policy(&ru.identity);
        let pattern = CreatePattern {
            operation: "replace_unique".to_string(),
            ty: ru.ty.clone(),
            binding: dest.name.clone(),
            fields: lowered_fields,
            lock: None,
            identity: identity.clone(),
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::ReplaceUnique {
            dest: dest.clone(),
            operand: lowered_input.operand,
            pattern,
            identity,
        });
        self.aggregate_fields.insert(dest.id, field_vars);
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_transfer_expr(
        &mut self,
        transfer: &TransferExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_expr = self.lower_expr(&transfer.expr, current, blocks, vars);
        let Some(active) = lowered_expr.current else {
            return lowered_expr;
        };
        let lowered_to = self.lower_expr(&transfer.to, active, blocks, vars);
        let Some(active) = lowered_to.current else {
            return lowered_to;
        };

        let dest_ty = self.operand_type(&lowered_expr.operand);
        let dest = self.new_var("transfer_tmp", dest_ty);
        let transfer_output_fields = self.materialize_matching_output_fields(&lowered_expr.operand, &dest.ty, active, blocks);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Transfer {
            dest: dest.clone(),
            operand: lowered_expr.operand,
            to: lowered_to.operand,
        });
        if !transfer_output_fields.is_empty() {
            self.aggregate_fields.insert(dest.id, transfer_output_fields);
        }
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_read_ref_expr(&mut self, read_ref: &ReadRefExpr, current: BlockId, blocks: &mut Vec<IrBlock>) -> LoweredExpr {
        let dest = self.new_var(format!("read_ref_{}", read_ref.ty), IrType::Ref(Box::new(IrType::Named(read_ref.ty.clone()))));
        self.block_mut(blocks, current).instructions.push(IrInstruction::ReadRef { dest: dest.clone(), ty: read_ref.ty.clone() });
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) }
    }

    fn lower_claim_expr(
        &mut self,
        claim: &ClaimExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_receipt = self.lower_expr(&claim.receipt, current, blocks, vars);
        let Some(active) = lowered_receipt.current else {
            return lowered_receipt;
        };
        let dest_ty = self.claim_output_type_for_operand(&lowered_receipt.operand);
        let dest = self.new_var("claim_tmp", dest_ty);
        let claim_output_fields = self.materialize_matching_output_fields(&lowered_receipt.operand, &dest.ty, active, blocks);
        self.block_mut(blocks, active)
            .instructions
            .push(IrInstruction::Claim { dest: dest.clone(), receipt: lowered_receipt.operand });
        if !claim_output_fields.is_empty() {
            self.aggregate_fields.insert(dest.id, claim_output_fields);
        }
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_settle_expr(
        &mut self,
        settle: &SettleExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered = self.lower_expr(&settle.expr, current, blocks, vars);
        let Some(active) = lowered.current else {
            return lowered;
        };
        let dest_ty = self.operand_type(&lowered.operand);
        let dest = self.new_var("settle_tmp", dest_ty);
        let settle_output_fields = self.materialize_matching_output_fields(&lowered.operand, &dest.ty, active, blocks);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Settle { dest: dest.clone(), operand: lowered.operand });
        if !settle_output_fields.is_empty() {
            self.aggregate_fields.insert(dest.id, settle_output_fields);
        }
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_index_expr(
        &mut self,
        index: &IndexExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_arr = self.lower_expr(&index.expr, current, blocks, vars);
        let Some(active) = lowered_arr.current else {
            return lowered_arr;
        };
        let lowered_idx = self.lower_expr(&index.index, active, blocks, vars);
        let Some(active) = lowered_idx.current else {
            return lowered_idx;
        };

        if let IrOperand::Var(arr_var) = &lowered_arr.operand {
            if let Some(elements) = self.aggregate_elements.get(&arr_var.id) {
                let Some(index_value) = const_usize_operand(&lowered_idx.operand) else {
                    self.record_error("local fixed-array indexing requires a compile-time constant index", index.span);
                    return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) };
                };
                let Some(element_var) = elements.get(index_value).cloned() else {
                    self.record_error(
                        format!("array index {} is out of bounds for local fixed array of length {}", index_value, elements.len()),
                        index.span,
                    );
                    return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) };
                };
                return LoweredExpr { operand: IrOperand::Var(element_var), current: Some(active) };
            }
        }

        let Some(result_ty) = self.index_result_type(&lowered_arr.operand) else {
            self.record_error("index expression has no lowered element type", index.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) };
        };

        let dest = self.new_var("index_tmp", result_ty);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Index {
            dest: dest.clone(),
            arr: lowered_arr.operand,
            idx: lowered_idx.operand,
        });
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_array_expr(
        &mut self,
        items: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        if items.is_empty() {
            self.record_error("empty array literal reached IR lowering without a declared array type", Span::default());
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) };
        }

        let mut active = current;
        let mut elements = Vec::with_capacity(items.len());
        let mut element_ty = None;

        for (index, item) in items.iter().enumerate() {
            let lowered = self.lower_expr(item, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;

            let ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            element_ty.get_or_insert_with(|| ty.clone());
            let element_var = self.new_var(format!("array_elem_{}", index), ty);
            self.block_mut(blocks, active)
                .instructions
                .push(IrInstruction::Move { dest: element_var.clone(), src: lowered.operand.clone() });
            self.copy_aggregate_metadata(&lowered.operand, element_var.id);
            elements.push(element_var);
        }

        let Some(element_ty) = element_ty else {
            self.record_error("non-empty array literal did not produce an element type during IR lowering", Span::default());
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) };
        };
        let array_ty = IrType::Array(Box::new(element_ty), items.len());
        let aggregate = self.new_var("array_tmp", array_ty);
        self.block_mut(blocks, active)
            .instructions
            .push(IrInstruction::Move { dest: aggregate.clone(), src: IrOperand::Const(IrConst::U64(0)) });
        self.aggregate_elements.insert(aggregate.id, elements);
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(active) }
    }

    fn lower_expr_with_expected_type(
        &mut self,
        expr: &Expr,
        expected_ty: &IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        match expr {
            Expr::Integer(value) => match Self::integer_const_for_ir_type(*value, expected_ty) {
                Some(value) => LoweredExpr { operand: IrOperand::Const(value), current: Some(current) },
                None => {
                    self.record_error(format!("integer literal {} is out of range for {:?}", value, expected_ty), Span::default());
                    LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
                }
            },
            Expr::ByteString(bytes) => match expected_ty {
                IrType::Array(inner, len) if matches!(inner.as_ref(), IrType::U8) && *len == bytes.len() => LoweredExpr {
                    operand: IrOperand::Const(IrConst::Array(bytes.iter().copied().map(IrConst::U8).collect())),
                    current: Some(current),
                },
                IrType::Array(inner, len) if matches!(inner.as_ref(), IrType::U8) => {
                    self.record_error(
                        format!("byte string literal has length {}, expected fixed byte array length {}", bytes.len(), len),
                        Span::default(),
                    );
                    LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
                }
                _ => {
                    self.record_error("byte string literals require an expected [u8; N] type", Span::default());
                    LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
                }
            },
            Expr::Array(items) if collection_item_ir_type(expected_ty).is_some() => {
                self.lower_vec_literal_expr(items, expected_ty.clone(), current, blocks, vars)
            }
            Expr::Array(items) if items.is_empty() && matches!(expected_ty, IrType::Array(_, 0)) => {
                self.lower_empty_array_expr_with_ir_type(expected_ty.clone(), current, blocks)
            }
            Expr::Block(stmts) => self.lower_tail_block_value(stmts, current, blocks, vars, Some(expected_ty)),
            Expr::If(if_expr) => self.lower_if_expr_with_expected_type(if_expr, expected_ty, current, blocks, vars),
            _ => self.lower_expr(expr, current, blocks, vars),
        }
    }

    fn lower_if_expr_with_expected_type(
        &mut self,
        if_expr: &IfExpr,
        expected_ty: &IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&if_expr.condition, current, blocks, vars);
        let cond = lowered_cond.operand;
        let Some(current) = lowered_cond.current else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };

        let then_block = self.push_block(blocks);
        let else_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Branch { cond, then_block, else_block };

        let mut then_vars = vars.clone();
        let then_lowered = self.lower_expr_with_expected_type(&if_expr.then_branch, expected_ty, then_block, blocks, &mut then_vars);
        let mut else_vars = vars.clone();
        let else_lowered = self.lower_expr_with_expected_type(&if_expr.else_branch, expected_ty, else_block, blocks, &mut else_vars);

        if then_lowered.current.is_none() && else_lowered.current.is_none() {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        }

        let dest = self.new_var("if_tmp", expected_ty.clone());
        let join = self.push_block(blocks);
        let branch_values = [
            then_lowered.current.as_ref().map(|_| &then_lowered.operand),
            else_lowered.current.as_ref().map(|_| &else_lowered.operand),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        self.ensure_join_aggregate_slots(&dest, &branch_values);

        if let Some(exit) = then_lowered.current {
            self.lower_join_value_into_dest(&dest, then_lowered.operand, exit, blocks);
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }

        if let Some(exit) = else_lowered.current {
            self.lower_join_value_into_dest(&dest, else_lowered.operand, exit, blocks);
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }

        LoweredExpr { operand: IrOperand::Var(dest), current: Some(join) }
    }

    fn integer_const_for_ir_type(value: u64, expected_ty: &IrType) -> Option<IrConst> {
        match expected_ty {
            IrType::U8 => u8::try_from(value).ok().map(IrConst::U8),
            IrType::U16 => u16::try_from(value).ok().map(IrConst::U16),
            IrType::U32 => u32::try_from(value).ok().map(IrConst::U32),
            IrType::U64 => Some(IrConst::U64(value)),
            IrType::U128 => Some(IrConst::U128(value as u128)),
            _ => None,
        }
    }

    fn cast_const(value: &IrConst, target_ty: &IrType) -> Option<IrConst> {
        match (value, target_ty) {
            (IrConst::Unit, IrType::Unit) => Some(IrConst::Unit),
            (IrConst::Bool(b), IrType::Bool) => Some(IrConst::Bool(*b)),
            (IrConst::Bool(b), IrType::U8) => Some(IrConst::U8(*b as u8)),
            (IrConst::Bool(b), IrType::U16) => Some(IrConst::U16(*b as u16)),
            (IrConst::Bool(b), IrType::U32) => Some(IrConst::U32(*b as u32)),
            (IrConst::Bool(b), IrType::U64) => Some(IrConst::U64(*b as u64)),
            (IrConst::U8(n), IrType::Bool) => (*n <= 1).then_some(IrConst::Bool(*n == 1)),
            (IrConst::U8(n), IrType::U8) => Some(IrConst::U8(*n)),
            (IrConst::U8(n), IrType::U16) => Some(IrConst::U16(*n as u16)),
            (IrConst::U8(n), IrType::U32) => Some(IrConst::U32(*n as u32)),
            (IrConst::U8(n), IrType::U64) => Some(IrConst::U64(*n as u64)),
            (IrConst::U8(n), IrType::U128) => Some(IrConst::U128(*n as u128)),
            (IrConst::U16(n), IrType::Bool) => (*n <= 1).then_some(IrConst::Bool(*n == 1)),
            (IrConst::U16(n), IrType::U8) => u8::try_from(*n).ok().map(IrConst::U8),
            (IrConst::U16(n), IrType::U16) => Some(IrConst::U16(*n)),
            (IrConst::U16(n), IrType::U32) => Some(IrConst::U32(*n as u32)),
            (IrConst::U16(n), IrType::U64) => Some(IrConst::U64(*n as u64)),
            (IrConst::U16(n), IrType::U128) => Some(IrConst::U128(*n as u128)),
            (IrConst::U32(n), IrType::Bool) => (*n <= 1).then_some(IrConst::Bool(*n == 1)),
            (IrConst::U32(n), IrType::U8) => u8::try_from(*n).ok().map(IrConst::U8),
            (IrConst::U32(n), IrType::U16) => u16::try_from(*n).ok().map(IrConst::U16),
            (IrConst::U32(n), IrType::U32) => Some(IrConst::U32(*n)),
            (IrConst::U32(n), IrType::U64) => Some(IrConst::U64(*n as u64)),
            (IrConst::U32(n), IrType::U128) => Some(IrConst::U128(*n as u128)),
            (IrConst::U64(n), IrType::Bool) => (*n <= 1).then_some(IrConst::Bool(*n == 1)),
            (IrConst::U64(n), IrType::U8) => u8::try_from(*n).ok().map(IrConst::U8),
            (IrConst::U64(n), IrType::U16) => u16::try_from(*n).ok().map(IrConst::U16),
            (IrConst::U64(n), IrType::U32) => u32::try_from(*n).ok().map(IrConst::U32),
            (IrConst::U64(n), IrType::U64) => Some(IrConst::U64(*n)),
            (IrConst::U64(n), IrType::U128) => Some(IrConst::U128(*n as u128)),
            (IrConst::U128(n), IrType::Bool) => (*n <= 1).then_some(IrConst::Bool(*n == 1)),
            (IrConst::U128(n), IrType::U8) => u8::try_from(*n).ok().map(IrConst::U8),
            (IrConst::U128(n), IrType::U16) => u16::try_from(*n).ok().map(IrConst::U16),
            (IrConst::U128(n), IrType::U32) => u32::try_from(*n).ok().map(IrConst::U32),
            (IrConst::U128(n), IrType::U64) => u64::try_from(*n).ok().map(IrConst::U64),
            (IrConst::U128(n), IrType::U128) => Some(IrConst::U128(*n)),
            (IrConst::Address(a), IrType::Address) => Some(IrConst::Address(*a)),
            (IrConst::Hash(h), IrType::Hash) => Some(IrConst::Hash(*h)),
            (IrConst::Array(items), IrType::Array(inner, len)) if matches!(inner.as_ref(), IrType::U8) && items.len() == *len => {
                Some(IrConst::Array(items.clone()))
            }
            (value, IrType::Named(name)) if name == "usize" || name == "isize" => Self::cast_const(value, &IrType::U64),
            _ => None,
        }
    }

    fn lower_vec_literal_expr(
        &mut self,
        items: &[Expr],
        vec_ty: IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let Some(item_ty) = collection_item_ir_type(&vec_ty) else {
            self.record_error("Vec literal requires an expected Vec<T> type", Span::default());
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) };
        };

        let dest = self.new_var("vec_literal_tmp", vec_ty);
        self.block_mut(blocks, current).instructions.push(IrInstruction::CollectionNew {
            dest: dest.clone(),
            ty: "Vec".to_string(),
            capacity: (!items.is_empty()).then_some(IrOperand::Const(IrConst::U64(items.len() as u64))),
        });

        let mut active = current;
        for item in items {
            let lowered = self.lower_expr(item, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;

            let actual_ty = self.operand_type(&lowered.operand);
            if actual_ty != item_ty {
                self.record_error(
                    format!("Vec literal type mismatch: expected {:?}, found {:?}", item_ty, actual_ty),
                    Span::default(),
                );
            }

            self.block_mut(blocks, active)
                .instructions
                .push(IrInstruction::CollectionPush { collection: IrOperand::Var(dest.clone()), value: lowered.operand });
        }

        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_empty_array_expr_with_ir_type(&mut self, ir_ty: IrType, current: BlockId, blocks: &mut Vec<IrBlock>) -> LoweredExpr {
        if !matches!(ir_ty, IrType::Array(_, 0)) {
            self.record_error("empty array literal requires a zero-length declared array type", Span::default());
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) };
        }

        let aggregate = self.new_var("array_tmp", ir_ty);
        self.block_mut(blocks, current)
            .instructions
            .push(IrInstruction::Move { dest: aggregate.clone(), src: IrOperand::Const(IrConst::U64(0)) });
        self.aggregate_elements.insert(aggregate.id, Vec::new());
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(current) }
    }

    fn lower_tuple_expr(
        &mut self,
        items: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let mut active = current;
        let mut fields = HashMap::new();
        let mut types = Vec::with_capacity(items.len());

        for (index, item) in items.iter().enumerate() {
            let lowered = self.lower_expr(item, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;

            let ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("tuple_{}", index), ty.clone());
            self.block_mut(blocks, active)
                .instructions
                .push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand.clone() });
            self.copy_aggregate_metadata(&lowered.operand, field_var.id);
            fields.insert(index.to_string(), field_var);
            types.push(ty);
        }

        let aggregate = self.new_var("tuple_tmp", IrType::Tuple(types));
        let fields_for_instruction =
            (0..items.len()).filter_map(|index| fields.get(&index.to_string()).cloned().map(IrOperand::Var)).collect::<Vec<_>>();
        self.block_mut(blocks, active)
            .instructions
            .push(IrInstruction::Tuple { dest: aggregate.clone(), fields: fields_for_instruction });
        self.aggregate_fields.insert(aggregate.id, fields);
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(active) }
    }

    fn lower_struct_init(
        &mut self,
        init: &StructInitExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let aggregate = self.new_var("struct_tmp", IrType::Named(init.ty.clone()));
        let mut field_map = HashMap::new();
        let mut tuple_operands = Vec::new();
        let mut active = current;

        for (field_name, field_expr) in &init.fields {
            let expected_ty = self.type_fields.get(&init.ty).and_then(|fields| fields.get(field_name)).cloned();
            let lowered = if let Some(expected_ty) = expected_ty {
                self.lower_expr_with_expected_type(field_expr, &expected_ty, active, blocks, vars)
            } else {
                self.lower_expr(field_expr, active, blocks, vars)
            };
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;

            let field_ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("{}_{}", init.ty, field_name), field_ty);
            tuple_operands.push(lowered.operand.clone());
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand });
            field_map.insert(field_name.clone(), field_var);
        }

        self.block_mut(blocks, active).instructions.push(IrInstruction::Tuple { dest: aggregate.clone(), fields: tuple_operands });
        self.aggregate_fields.insert(aggregate.id, field_map);
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(active) }
    }

    fn lower_field_access(
        &mut self,
        field: &FieldAccessExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_base = self.lower_expr(&field.expr, current, blocks, vars);
        let Some(active) = lowered_base.current else {
            return lowered_base;
        };

        if let IrOperand::Var(base_var) = &lowered_base.operand {
            if let Some(fields) = self.aggregate_fields.get(&base_var.id) {
                if let Some(field_var) = fields.get(&field.field) {
                    if !self.schema_field_roots.contains_key(&field_var.id) || self.block_defines_var(blocks, active, field_var.id) {
                        return LoweredExpr { operand: IrOperand::Var(field_var.clone()), current: Some(active) };
                    }
                }
            }

            if let Some(field_var) = self.materialize_schema_field(base_var, &field.field, active, blocks) {
                return LoweredExpr { operand: IrOperand::Var(field_var), current: Some(active) };
            }
        }

        self.record_error(format!("field access '.{}' has no lowered schema-backed representation", field.field), field.span);
        LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) }
    }

    fn block_defines_var(&self, blocks: &[IrBlock], block_id: BlockId, var_id: usize) -> bool {
        blocks.get(block_id.0).is_some_and(|block| {
            block.instructions.iter().any(|instruction| instruction_dest(instruction).is_some_and(|dest| dest.id == var_id))
        })
    }

    fn lower_field_assign(
        &mut self,
        field: &FieldAccessExpr,
        op: AssignOp,
        value_expr: &Expr,
        value: IrOperand,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_base = self.lower_expr(&field.expr, current, blocks, vars);
        let Some(active) = lowered_base.current else {
            return lowered_base;
        };

        let Some(base_var) = (match lowered_base.operand {
            IrOperand::Var(var) => Some(var),
            IrOperand::Const(_) => None,
        }) else {
            return LoweredExpr { operand: value, current: Some(active) };
        };

        let field_var = self
            .aggregate_fields
            .get(&base_var.id)
            .and_then(|fields| fields.get(&field.field))
            .cloned()
            .or_else(|| self.materialize_schema_field(&base_var, &field.field, active, blocks));
        let Some(field_var) = field_var else {
            self.record_error(format!("field assignment '.{}' has no lowered schema-backed representation", field.field), field.span);
            return LoweredExpr { operand: value, current: Some(active) };
        };
        self.mutated_fields.entry(base_var.id).or_default().insert(field.field.clone());
        if let Some(transition) = self.mutate_transition_from_assignment(field, op, value_expr, &value, vars) {
            self.mutated_field_transitions.entry(base_var.id).or_default().insert(field.field.clone(), transition);
        }

        match op {
            AssignOp::Assign => {
                self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: value });
            }
            AssignOp::AddAssign => {
                let tmp = self.new_var("field_assign_tmp", field_var.ty.clone());
                let block = self.block_mut(blocks, active);
                block.instructions.push(IrInstruction::Binary {
                    dest: tmp.clone(),
                    op: BinaryOp::Add,
                    left: IrOperand::Var(field_var.clone()),
                    right: value,
                });
                block.instructions.push(IrInstruction::Move { dest: field_var.clone(), src: IrOperand::Var(tmp) });
            }
        }

        LoweredExpr { operand: IrOperand::Var(field_var), current: Some(active) }
    }

    fn mutate_transition_from_assignment(
        &self,
        target: &FieldAccessExpr,
        assign_op: AssignOp,
        value_expr: &Expr,
        value_operand: &IrOperand,
        vars: &HashMap<String, IrVar>,
    ) -> Option<MutateFieldTransition> {
        let (target_root, target_field) = direct_field_access_root(target)?;
        let (op, operand) = match assign_op {
            AssignOp::AddAssign => (
                MutateTransitionOp::Add,
                self.transition_operand_from_expr(value_expr, vars)
                    .or_else(|| self.transition_expr_is_coverable_u64(value_expr, vars).then_some(value_operand.clone()))?,
            ),
            AssignOp::Assign => match value_expr {
                Expr::Binary(binary) if matches!(binary.op, BinaryOp::Add | BinaryOp::Sub) => {
                    let left_is_old = same_direct_field_access(&binary.left, target_root, target_field);
                    let right_is_old = same_direct_field_access(&binary.right, target_root, target_field);
                    match (binary.op, left_is_old, right_is_old) {
                        (BinaryOp::Add, true, false) => {
                            (MutateTransitionOp::Add, self.transition_operand_from_expr(&binary.right, vars)?)
                        }
                        (BinaryOp::Add, false, true) => {
                            (MutateTransitionOp::Add, self.transition_operand_from_expr(&binary.left, vars)?)
                        }
                        (BinaryOp::Sub, true, false) => {
                            (MutateTransitionOp::Sub, self.transition_operand_from_expr(&binary.right, vars)?)
                        }
                        _ => return None,
                    }
                }
                _ => (MutateTransitionOp::Set, self.transition_operand_from_expr(value_expr, vars)?),
            },
        };
        Some(MutateFieldTransition { field: target_field.to_string(), op, operand })
    }

    fn transition_operand_from_expr(&self, expr: &Expr, vars: &HashMap<String, IrVar>) -> Option<IrOperand> {
        match expr {
            Expr::Identifier(name) => vars
                .get(name)
                .filter(|var| self.transition_param_ids.contains(&var.id) || self.transition_coverable_value_ids.contains(&var.id))
                .cloned()
                .map(IrOperand::Var),
            Expr::Integer(value) => Some(IrOperand::Const(IrConst::U64(*value))),
            Expr::FieldAccess(field) => {
                let (root, field_name) = direct_field_access_root(field)?;
                let root_var = vars.get(root)?;
                if !self.transition_param_ids.contains(&root_var.id) {
                    return None;
                }
                self.aggregate_fields.get(&root_var.id)?.get(field_name).cloned().map(IrOperand::Var)
            }
            Expr::Cast(cast) => self.transition_operand_from_expr(&cast.expr, vars),
            _ => None,
        }
    }

    fn transition_expr_is_coverable_u64(&self, expr: &Expr, vars: &HashMap<String, IrVar>) -> bool {
        match expr {
            Expr::Identifier(name) => vars.get(name).is_some_and(|var| {
                var.ty == IrType::U64
                    && (self.transition_param_ids.contains(&var.id) || self.transition_coverable_value_ids.contains(&var.id))
            }),
            Expr::Integer(_) => true,
            Expr::FieldAccess(field) => {
                let Some((root, field_name)) = direct_field_access_root(field) else {
                    return false;
                };
                let Some(root_var) = vars.get(root) else {
                    return false;
                };
                self.transition_param_ids.contains(&root_var.id)
                    && self
                        .lookup_field_ir_type(&root_var.ty, field_name)
                        .is_some_and(|ty| matches!(ty, IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64))
            }
            Expr::Cast(cast) => self.transition_expr_is_coverable_u64(&cast.expr, vars),
            Expr::Call(call) if call.args.is_empty() => match call.func.as_ref() {
                Expr::FieldAccess(field) if field.field == "len" => match field.expr.as_ref() {
                    Expr::Identifier(name) => vars.get(name).is_some_and(|var| {
                        (matches!(&var.ty, IrType::Named(type_name) if type_name == "String" || type_name.starts_with("Vec<"))
                            || matches!(&var.ty, IrType::Array(_, _)))
                            && (self.transition_param_ids.contains(&var.id) || self.transition_coverable_value_ids.contains(&var.id))
                    }),
                    _ => false,
                },
                _ => false,
            },
            Expr::Binary(binary) if matches!(binary.op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div) => {
                self.transition_expr_is_coverable_u64(&binary.left, vars) && self.transition_expr_is_coverable_u64(&binary.right, vars)
            }
            Expr::Call(call) if call.args.len() == 2 && call_target_is_min(call.func.as_ref()) => {
                call.args.iter().all(|arg| self.transition_expr_is_coverable_u64(arg, vars))
            }
            _ => false,
        }
    }

    fn lower_index_assign(
        &mut self,
        index: &IndexExpr,
        op: AssignOp,
        value: IrOperand,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_arr = self.lower_expr(&index.expr, current, blocks, vars);
        let Some(active) = lowered_arr.current else {
            return lowered_arr;
        };
        let lowered_idx = self.lower_expr(&index.index, active, blocks, vars);
        let Some(active) = lowered_idx.current else {
            return lowered_idx;
        };

        let Some(arr_var) = (match lowered_arr.operand {
            IrOperand::Var(var) => Some(var),
            IrOperand::Const(_) => None,
        }) else {
            self.record_error("index assignment requires a local fixed-array value", index.span);
            return LoweredExpr { operand: value, current: Some(active) };
        };
        let Some(index_value) = const_usize_operand(&lowered_idx.operand) else {
            self.record_error("local fixed-array assignment requires a compile-time constant index", index.span);
            return LoweredExpr { operand: value, current: Some(active) };
        };
        let Some(elements) = self.aggregate_elements.get(&arr_var.id) else {
            self.record_error("index assignment requires a local fixed-array value with lowered element slots", index.span);
            return LoweredExpr { operand: value, current: Some(active) };
        };
        let Some(element_var) = elements.get(index_value).cloned() else {
            self.record_error(
                format!("array index {} is out of bounds for local fixed array of length {}", index_value, elements.len()),
                index.span,
            );
            return LoweredExpr { operand: value, current: Some(active) };
        };

        match op {
            AssignOp::Assign => {
                self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: element_var.clone(), src: value });
            }
            AssignOp::AddAssign => {
                let tmp = self.new_var("index_assign_tmp", element_var.ty.clone());
                let block = self.block_mut(blocks, active);
                block.instructions.push(IrInstruction::Binary {
                    dest: tmp.clone(),
                    op: BinaryOp::Add,
                    left: IrOperand::Var(element_var.clone()),
                    right: value,
                });
                block.instructions.push(IrInstruction::Move { dest: element_var.clone(), src: IrOperand::Var(tmp) });
            }
        }

        LoweredExpr { operand: IrOperand::Var(element_var), current: Some(active) }
    }

    fn lower_if_expr(
        &mut self,
        if_expr: &IfExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&if_expr.condition, current, blocks, vars);
        let cond = lowered_cond.operand;
        let Some(current) = lowered_cond.current else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };

        let then_block = self.push_block(blocks);
        let else_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Branch { cond, then_block, else_block };

        let mut then_vars = vars.clone();
        let then_lowered = self.lower_expr(&if_expr.then_branch, then_block, blocks, &mut then_vars);
        let mut else_vars = vars.clone();
        let else_lowered = self.lower_expr(&if_expr.else_branch, else_block, blocks, &mut else_vars);

        if then_lowered.current.is_none() && else_lowered.current.is_none() {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        }

        let result_ty = match (then_lowered.current.is_some(), else_lowered.current.is_some()) {
            (true, _) => self.operand_type(&then_lowered.operand),
            (false, true) => self.operand_type(&else_lowered.operand),
            (false, false) => return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None },
        };
        let dest = self.new_var("if_tmp", result_ty);
        let join = self.push_block(blocks);
        let branch_values = [
            then_lowered.current.as_ref().map(|_| &then_lowered.operand),
            else_lowered.current.as_ref().map(|_| &else_lowered.operand),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        self.ensure_join_aggregate_slots(&dest, &branch_values);

        if let Some(exit) = then_lowered.current {
            self.lower_join_value_into_dest(&dest, then_lowered.operand, exit, blocks);
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }

        if let Some(exit) = else_lowered.current {
            self.lower_join_value_into_dest(&dest, else_lowered.operand, exit, blocks);
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }

        LoweredExpr { operand: IrOperand::Var(dest), current: Some(join) }
    }

    fn lower_match_expr(
        &mut self,
        match_expr: &MatchExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_scrutinee = self.lower_expr(&match_expr.expr, current, blocks, vars);
        let Some(mut check_block) = lowered_scrutinee.current else {
            return lowered_scrutinee;
        };

        if match_expr.arms.is_empty() {
            self.record_error("match expression reached IR lowering without arms", match_expr.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(check_block) };
        }

        let mut arm_entries = Vec::with_capacity(match_expr.arms.len());
        for _ in &match_expr.arms {
            arm_entries.push(self.push_block(blocks));
        }
        let join = self.push_block(blocks);
        let mut result_dest: Option<IrVar> = None;

        for (index, arm) in match_expr.arms.iter().enumerate() {
            let arm_entry = arm_entries[index];
            if arm.pattern == "_" {
                self.block_mut(blocks, check_block).terminator = IrTerminator::Jump(arm_entry);
                // Wildcard matches everything; subsequent arms are unreachable.
                // Lower this arm's value and skip remaining arms.
                let mut arm_vars = vars.clone();
                let lowered_value = self.lower_expr(&arm.value, arm_entry, blocks, &mut arm_vars);
                let Some(arm_exit) = lowered_value.current else {
                    break;
                };
                if result_dest.is_none() {
                    let ty = self.operand_type(&lowered_value.operand);
                    result_dest = Some(self.new_var("match_tmp", ty));
                }
                let dest = result_dest.as_ref().expect("match result destination must be initialized");
                self.ensure_join_aggregate_slots(dest, &[&lowered_value.operand]);
                self.lower_join_value_into_dest(dest, lowered_value.operand, arm_exit, blocks);
                self.block_mut(blocks, arm_exit).terminator = IrTerminator::Jump(join);
                break;
            } else {
                let Some(pattern_operand) = self.lower_match_pattern_operand(&arm.pattern, arm.span) else {
                    return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(check_block) };
                };

                let cond_var = self.new_var("match_cond", IrType::Bool);
                {
                    let block = self.block_mut(blocks, check_block);
                    block.instructions.push(IrInstruction::Binary {
                        dest: cond_var.clone(),
                        op: BinaryOp::Eq,
                        left: lowered_scrutinee.operand.clone(),
                        right: pattern_operand,
                    });
                }

                let else_block = if index + 1 < match_expr.arms.len() {
                    self.push_block(blocks)
                } else {
                    let fail_block = self.push_block(blocks);
                    self.block_mut(blocks, fail_block).terminator =
                        IrTerminator::Abort(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
                    fail_block
                };
                self.block_mut(blocks, check_block).terminator =
                    IrTerminator::Branch { cond: IrOperand::Var(cond_var), then_block: arm_entry, else_block };
                check_block = else_block;
            }

            let mut arm_vars = vars.clone();
            let lowered_value = self.lower_expr(&arm.value, arm_entry, blocks, &mut arm_vars);
            let Some(arm_exit) = lowered_value.current else {
                continue;
            };

            if result_dest.is_none() {
                let ty = self.operand_type(&lowered_value.operand);
                result_dest = Some(self.new_var("match_tmp", ty));
            }
            let dest = result_dest.as_ref().expect("match result destination must be initialized");
            self.ensure_join_aggregate_slots(dest, &[&lowered_value.operand]);
            self.lower_join_value_into_dest(dest, lowered_value.operand, arm_exit, blocks);
            self.block_mut(blocks, arm_exit).terminator = IrTerminator::Jump(join);
        }

        let Some(dest) = result_dest else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(join) };
        };
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(join) }
    }

    fn operand_type(&self, operand: &IrOperand) -> IrType {
        match operand {
            IrOperand::Var(var) => var.ty.clone(),
            IrOperand::Const(value) => self.const_type(value),
        }
    }

    fn try_lower_builtin_call(
        &mut self,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        match call.func.as_ref() {
            Expr::Identifier(name) => match name.as_str() {
                "Address::zero" if call.args.is_empty() => {
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Address([0; 32])), current: Some(current) })
                }
                "Hash::zero" if call.args.is_empty() => {
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(current) })
                }
                "env::current_timepoint" if call.args.is_empty() => {
                    let dest = self.new_var("current_timepoint", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__env_current_timepoint".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "env::sighash_all" if call.args.len() == 1 => {
                    self.lower_simple_runtime_call("__ckb_sighash_all", "sighash_all", IrType::Hash, &call.args, current, blocks, vars)
                }
                "ckb::header_epoch_number" if call.args.is_empty() => {
                    let dest = self.new_var("ckb_header_epoch_number", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_header_epoch_number".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "ckb::header_epoch_start_block_number" if call.args.is_empty() => {
                    let dest = self.new_var("ckb_header_epoch_start_block_number", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_header_epoch_start_block_number".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "ckb::header_epoch_length" if call.args.is_empty() => {
                    let dest = self.new_var("ckb_header_epoch_length", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_header_epoch_length".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "ckb::input_since" if call.args.is_empty() => {
                    let dest = self.new_var("ckb_input_since", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_input_since".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "source::input" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_input",
                    "source_input",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::output" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_output",
                    "source_output",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::cell_dep" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_cell_dep",
                    "source_cell_dep",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::header_dep" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_header_dep",
                    "source_header_dep",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::group_input" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_group_input",
                    "source_group_input",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::group_output" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_group_output",
                    "source_group_output",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::raw" if call.args.len() == 1 => {
                    self.lower_simple_runtime_call("__ckb_witness_raw", "witness_raw", IrType::Hash, &call.args, current, blocks, vars)
                }
                "witness::lock" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_witness_lock",
                    "witness_lock",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::input_type" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_witness_input_type",
                    "witness_input_type",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::output_type" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_witness_output_type",
                    "witness_output_type",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "spawn" if call.args.len() == 1 => {
                    let dest = self.new_var("spawn_result", IrType::U64);
                    let target = match &call.args[0] {
                        Expr::String(value) => IrOperand::Const(IrConst::U64(stable_u64_tag(value))),
                        Expr::Identifier(name) => match self.constants.get(name) {
                            Some((_, Expr::String(value))) => IrOperand::Const(IrConst::U64(stable_u64_tag(value))),
                            _ => {
                                let lowered = self.lower_expr(&call.args[0], current, blocks, vars);
                                let active = lowered.current?;
                                self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                                    dest: Some(dest.clone()),
                                    func: "__ckb_spawn".to_string(),
                                    args: vec![lowered.operand],
                                });
                                return Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) });
                            }
                        },
                        other => {
                            let lowered = self.lower_expr(other, current, blocks, vars);
                            let active = lowered.current?;
                            self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                                dest: Some(dest.clone()),
                                func: "__ckb_spawn".to_string(),
                                args: vec![lowered.operand],
                            });
                            return Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) });
                        }
                    };
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_spawn".to_string(),
                        args: vec![target],
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "pipe" if call.args.is_empty() => {
                    let dest = self.new_var("pipe_pair", IrType::Tuple(vec![IrType::U64, IrType::U64]));
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_pipe".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "wait" if call.args.is_empty() => {
                    self.lower_simple_runtime_call("__ckb_wait", "wait_result", IrType::U64, &call.args, current, blocks, vars)
                }
                "process_id" if call.args.is_empty() => {
                    self.lower_simple_runtime_call("__ckb_process_id", "process_id", IrType::U64, &call.args, current, blocks, vars)
                }
                "pipe_write" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_pipe_write", &call.args, current, blocks, vars)
                }
                "pipe_read" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_pipe_read",
                    "pipe_read_result",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "inherited_fd" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_inherited_fd",
                    "inherited_fd",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "close" if call.args.len() == 1 => self.lower_void_runtime_call("__ckb_close", &call.args, current, blocks, vars),
                "require_maturity" if call.args.len() == 1 => {
                    self.lower_void_runtime_call("__ckb_require_maturity", &call.args, current, blocks, vars)
                }
                "require_time" if call.args.len() == 1 => {
                    self.lower_void_runtime_call("__ckb_require_time", &call.args, current, blocks, vars)
                }
                "require_epoch_after" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__ckb_require_epoch_after", &call.args, current, blocks, vars)
                }
                "require_epoch_relative" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__ckb_require_epoch_relative", &call.args, current, blocks, vars)
                }
                "occupied_capacity" if call.args.len() == 1 => {
                    let dest = self.new_var("occupied_capacity", IrType::U64);
                    let tag = match &call.args[0] {
                        Expr::String(value) => stable_u64_tag(value),
                        _ => 0,
                    };
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_occupied_capacity".to_string(),
                        args: vec![IrOperand::Const(IrConst::U64(tag))],
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "hash_chain" if call.args.len() == 1 => {
                    self.lower_simple_runtime_call("__ckb_hash_chain", "hash_chain", IrType::Hash, &call.args, current, blocks, vars)
                }
                "hash_blake2b" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_hash_blake2b",
                    "hash_blake2b",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "Vec::new" if call.args.is_empty() => {
                    let dest = self.new_var("vec_new_tmp", IrType::Named("Vec".to_string()));
                    self.block_mut(blocks, current).instructions.push(IrInstruction::CollectionNew {
                        dest: dest.clone(),
                        ty: "Vec".to_string(),
                        capacity: None,
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "Vec::with_capacity" if call.args.len() == 1 => {
                    let lowered_capacity = self.lower_expr(&call.args[0], current, blocks, vars);
                    let active = lowered_capacity.current?;
                    let dest = self.new_var("vec_with_capacity_tmp", IrType::Named("Vec".to_string()));
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionNew {
                        dest: dest.clone(),
                        ty: "Vec".to_string(),
                        capacity: Some(lowered_capacity.operand),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                _ => None,
            },
            Expr::FieldAccess(field) => match field.field.as_str() {
                "len" if call.args.is_empty() => {
                    let lowered = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered.current?;
                    if let IrOperand::Var(var) = &lowered.operand {
                        if let Some(elements) = self.aggregate_elements.get(&var.id) {
                            return Some(LoweredExpr {
                                operand: IrOperand::Const(IrConst::U64(elements.len() as u64)),
                                current: Some(active),
                            });
                        }
                    }
                    let dest = self.new_var("len_tmp", IrType::U64);
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::Length { dest: dest.clone(), operand: lowered.operand });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "is_empty" if call.args.is_empty() => {
                    let lowered = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered.current?;
                    if let IrOperand::Var(var) = &lowered.operand {
                        if let Some(elements) = self.aggregate_elements.get(&var.id) {
                            return Some(LoweredExpr {
                                operand: IrOperand::Const(IrConst::Bool(elements.is_empty())),
                                current: Some(active),
                            });
                        }
                    }
                    let len_dest = self.new_var("is_empty_len_tmp", IrType::U64);
                    let dest = self.new_var("is_empty_tmp", IrType::Bool);
                    let block = self.block_mut(blocks, active);
                    block.instructions.push(IrInstruction::Length { dest: len_dest.clone(), operand: lowered.operand });
                    block.instructions.push(IrInstruction::Binary {
                        dest: dest.clone(),
                        op: BinaryOp::Eq,
                        left: IrOperand::Var(len_dest),
                        right: IrOperand::Const(IrConst::U64(0)),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "capacity" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let dest = self.new_var("capacity_tmp", IrType::U64);
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionCapacity { dest: dest.clone(), collection: lowered_collection.operand });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "first" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let item_ty = collection_item_ir_type(&self.operand_type(&lowered_collection.operand)).unwrap_or(IrType::U64);
                    let dest = self.new_var("first_tmp", item_ty);
                    self.block_mut(blocks, active).instructions.push(IrInstruction::Index {
                        dest: dest.clone(),
                        arr: lowered_collection.operand,
                        idx: IrOperand::Const(IrConst::U64(0)),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "last" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let item_ty = collection_item_ir_type(&self.operand_type(&lowered_collection.operand)).unwrap_or(IrType::U64);
                    let len_dest = self.new_var("last_len_tmp", IrType::U64);
                    let index_dest = self.new_var("last_index_tmp", IrType::U64);
                    let dest = self.new_var("last_tmp", item_ty);
                    let block = self.block_mut(blocks, active);
                    block
                        .instructions
                        .push(IrInstruction::Length { dest: len_dest.clone(), operand: lowered_collection.operand.clone() });
                    block.instructions.push(IrInstruction::Binary {
                        dest: index_dest.clone(),
                        op: BinaryOp::Sub,
                        left: IrOperand::Var(len_dest),
                        right: IrOperand::Const(IrConst::U64(1)),
                    });
                    block.instructions.push(IrInstruction::Index {
                        dest: dest.clone(),
                        arr: lowered_collection.operand,
                        idx: IrOperand::Var(index_dest),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "type_hash" if call.args.is_empty() => {
                    let lowered = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered.current?;
                    let dest = self.new_var("type_hash_tmp", IrType::Hash);
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::TypeHash { dest: dest.clone(), operand: lowered.operand });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "push" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_value = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_value.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                    {
                        if matches!(&collection_var.ty, IrType::Named(name) if name == "Vec") {
                            if let (Some(item_ty), Some(receiver_var)) =
                                (inline_ir_type_repr(&self.operand_type(&lowered_value.operand)), vars.get_mut(receiver_name))
                            {
                                if receiver_var.id == collection_var.id {
                                    receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                                }
                            }
                        }
                    }
                    let block = self.block_mut(blocks, active);
                    block.instructions.push(IrInstruction::CollectionPush {
                        collection: collection_operand.clone(),
                        value: lowered_value.operand.clone(),
                    });
                    if let IrOperand::Var(collection_var) = &collection_operand {
                        if let Some((root_id, field_name)) = self.schema_field_roots.get(&collection_var.id).cloned() {
                            self.mutated_fields.entry(root_id).or_default().insert(field_name.clone());
                            self.mutated_field_transitions.entry(root_id).or_default().insert(
                                field_name.clone(),
                                MutateFieldTransition {
                                    field: field_name,
                                    op: MutateTransitionOp::Append,
                                    operand: lowered_value.operand,
                                },
                            );
                        }
                    }
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "clear" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionClear { collection: lowered_collection.operand });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "reverse" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionReverse { collection: lowered_collection.operand });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "truncate" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_len = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_len.current?;
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionTruncate { collection: lowered_collection.operand, len: lowered_len.operand });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "swap" if call.args.len() == 2 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_left = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_left.current?;
                    let lowered_right = self.lower_expr(&call.args[1], active, blocks, vars);
                    let active = lowered_right.current?;
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionSwap {
                        collection: lowered_collection.operand,
                        left: lowered_left.operand,
                        right: lowered_right.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "contains" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_value = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_value.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                    {
                        if matches!(&collection_var.ty, IrType::Named(name) if name == "Vec") {
                            if let (Some(item_ty), Some(receiver_var)) =
                                (inline_ir_type_repr(&self.operand_type(&lowered_value.operand)), vars.get_mut(receiver_name))
                            {
                                if receiver_var.id == collection_var.id {
                                    receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                                }
                            }
                        }
                    }
                    let dest = self.new_var("contains_tmp", IrType::Bool);
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionContains {
                        dest: dest.clone(),
                        collection: collection_operand,
                        value: lowered_value.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "remove" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_index = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_index.current?;
                    let item_ty = collection_item_ir_type(&self.operand_type(&lowered_collection.operand)).unwrap_or(IrType::U64);
                    let dest = self.new_var("remove_tmp", item_ty);
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionRemove {
                        dest: dest.clone(),
                        collection: lowered_collection.operand,
                        index: lowered_index.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "pop" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let item_ty = collection_item_ir_type(&self.operand_type(&lowered_collection.operand)).unwrap_or(IrType::U64);
                    let dest = self.new_var("pop_tmp", item_ty);
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionPop { dest: dest.clone(), collection: lowered_collection.operand });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "insert" if call.args.len() == 2 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_index = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_index.current?;
                    let lowered_value = self.lower_expr(&call.args[1], active, blocks, vars);
                    let active = lowered_value.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                    {
                        if matches!(&collection_var.ty, IrType::Named(name) if name == "Vec") {
                            if let (Some(item_ty), Some(receiver_var)) =
                                (inline_ir_type_repr(&self.operand_type(&lowered_value.operand)), vars.get_mut(receiver_name))
                            {
                                if receiver_var.id == collection_var.id {
                                    receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                                }
                            }
                        }
                    }
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionInsert {
                        collection: collection_operand,
                        index: lowered_index.operand,
                        value: lowered_value.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "set" if call.args.len() == 2 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_index = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_index.current?;
                    let lowered_value = self.lower_expr(&call.args[1], active, blocks, vars);
                    let active = lowered_value.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                    {
                        if matches!(&collection_var.ty, IrType::Named(name) if name == "Vec") {
                            if let (Some(item_ty), Some(receiver_var)) =
                                (inline_ir_type_repr(&self.operand_type(&lowered_value.operand)), vars.get_mut(receiver_name))
                            {
                                if receiver_var.id == collection_var.id {
                                    receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                                }
                            }
                        }
                    }
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionSet {
                        collection: collection_operand,
                        index: lowered_index.operand,
                        value: lowered_value.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "extend_from_slice" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_slice = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_slice.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                    {
                        if matches!(&collection_var.ty, IrType::Named(name) if name == "Vec") {
                            if let (IrType::Array(inner, _), Some(receiver_var)) =
                                (self.operand_type(&lowered_slice.operand), vars.get_mut(receiver_name))
                            {
                                if receiver_var.id == collection_var.id {
                                    if let Some(item_ty) = inline_ir_type_repr(inner.as_ref()) {
                                        receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                                    }
                                }
                            }
                        }
                    }
                    let block = self.block_mut(blocks, active);
                    block
                        .instructions
                        .push(IrInstruction::CollectionExtend { collection: collection_operand, slice: lowered_slice.operand });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn lower_simple_runtime_call(
        &mut self,
        func: &str,
        dest_name: &str,
        return_ty: IrType,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let mut active = current;
        let mut lowered_args = Vec::with_capacity(args.len());
        for arg in args {
            let lowered = self.lower_expr(arg, active, blocks, vars);
            active = lowered.current?;
            lowered_args.push(lowered.operand);
        }
        let dest = self.new_var(dest_name, return_ty);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: Some(dest.clone()),
            func: func.to_string(),
            args: lowered_args,
        });
        Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
    }

    fn lower_void_runtime_call(
        &mut self,
        func: &str,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let mut active = current;
        let mut lowered_args = Vec::with_capacity(args.len());
        for arg in args {
            let lowered = self.lower_expr(arg, active, blocks, vars);
            active = lowered.current?;
            lowered_args.push(lowered.operand);
        }
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: None,
            func: func.to_string(),
            args: lowered_args,
        });
        Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) })
    }

    fn lower_match_pattern_operand(&mut self, pattern: &str, span: Span) -> Option<IrOperand> {
        if pattern == "_" {
            return Some(IrOperand::Const(IrConst::U64(0)));
        }
        if let Some(variant) = self.lower_enum_variant(pattern) {
            return Some(variant);
        }
        if let Some(constant) = self.lower_constant(pattern, span) {
            return Some(constant);
        }
        self.record_error(format!("match pattern '{}' is not supported by IR lowering", pattern), span);
        None
    }

    fn record_error(&mut self, message: impl Into<String>, span: Span) {
        self.errors.push(CompileError::new(message, span));
    }

    fn lower_constant(&mut self, name: &str, span: Span) -> Option<IrOperand> {
        let (ty, value) = self.constants.get(name)?.clone();
        match value {
            Expr::Integer(n) => {
                let expected_ty = Self::convert_type(&ty);
                match Self::integer_const_for_ir_type(n, &expected_ty) {
                    Some(value) => Some(IrOperand::Const(value)),
                    None => {
                        self.record_error(format!("constant '{}' is out of range for {:?}", name, expected_ty), span);
                        None
                    }
                }
            }
            Expr::Bool(b) => Some(IrOperand::Const(IrConst::Bool(b))),
            Expr::ByteString(bytes) => {
                let items = bytes.into_iter().map(IrConst::U8).collect::<Vec<_>>();
                Some(IrOperand::Const(IrConst::Array(items)))
            }
            _ => {
                self.record_error(format!("constant '{}' uses an expression IR lowering does not support", name), span);
                None
            }
        }
    }

    fn lower_zero_value(&self, name: &str) -> Option<IrOperand> {
        match name {
            "Address::zero" => Some(IrOperand::Const(IrConst::Address([0; 32]))),
            "Hash::zero" => Some(IrOperand::Const(IrConst::Hash([0; 32]))),
            _ => None,
        }
    }

    fn lower_enum_variant(&self, name: &str) -> Option<IrOperand> {
        let (enum_name, variant_name) = name.rsplit_once("::")?;
        let ordinal = self.enum_variants.get(enum_name)?.get(variant_name).copied()?;
        Some(IrOperand::Const(IrConst::U64(ordinal)))
    }

    fn lower_flow_state_name(&self, name: &str) -> Option<IrOperand> {
        let (type_name, _) = name.rsplit_once("::")?;
        let index = self.flow_state_index(type_name, name)?;
        let field_ty = self
            .flow_state_fields
            .get(type_name)
            .and_then(|field_name| self.type_fields.get(type_name).and_then(|fields| fields.get(field_name)));
        let value = if let Some(field_ty) = field_ty {
            Self::integer_const_for_ir_type(index as u64, field_ty)?
        } else {
            IrConst::U64(index as u64)
        };
        Some(IrOperand::Const(value))
    }

    fn flow_state_index(&self, type_name: &str, name: &str) -> Option<usize> {
        let states = self.flow_states.get(type_name)?;
        if let Some((qualified_type, state_name)) = name.rsplit_once("::") {
            if qualified_type != type_name {
                return None;
            }
            states.iter().position(|state| state == state_name)
        } else {
            states.iter().position(|state| state == name)
        }
    }

    fn lookup_field_ir_type(&self, ty: &IrType, field: &str) -> Option<IrType> {
        match ty {
            IrType::Tuple(items) => field.parse::<usize>().ok().and_then(|index| items.get(index)).cloned(),
            IrType::Named(name) => self.type_fields.get(name).and_then(|fields| fields.get(field)).cloned(),
            IrType::Ref(inner) | IrType::MutRef(inner) => self.lookup_field_ir_type(inner, field),
            IrType::Address | IrType::Hash if field == "0" => Some(IrType::Array(Box::new(IrType::U8), 32)),
            _ => None,
        }
    }

    fn index_result_type(&self, operand: &IrOperand) -> Option<IrType> {
        match operand {
            IrOperand::Var(var) => self.index_result_type_from_ir_type(&var.ty),
            IrOperand::Const(IrConst::Array(items)) => items.first().map(|item| self.const_type(item)),
            _ => None,
        }
    }

    fn index_result_type_from_ir_type(&self, ty: &IrType) -> Option<IrType> {
        match ty {
            IrType::Array(inner, _) => Some((**inner).clone()),
            IrType::Ref(inner) | IrType::MutRef(inner) => self.index_result_type_from_ir_type(inner),
            IrType::Named(name) => self.named_vec_item_type(name),
            _ => None,
        }
    }

    fn iter_item_type(&self, operand: &IrOperand) -> Option<IrType> {
        match operand {
            IrOperand::Var(var) => self.index_result_type_from_ir_type(&var.ty),
            IrOperand::Const(IrConst::Array(items)) => items.first().map(|item| self.const_type(item)),
            _ => None,
        }
    }

    fn named_vec_item_type(&self, name: &str) -> Option<IrType> {
        let inner = name.strip_prefix("Vec<")?.strip_suffix('>')?;
        Some(self.parse_inline_ir_type(inner))
    }

    fn parse_inline_ir_type(&self, ty: &str) -> IrType {
        match ty {
            "u8" => IrType::U8,
            "u16" => IrType::U16,
            "u32" => IrType::U32,
            "u64" => IrType::U64,
            "u128" => IrType::U128,
            "bool" => IrType::Bool,
            "Address" => IrType::Address,
            "Hash" => IrType::Hash,
            other => IrType::Named(other.to_string()),
        }
    }

    fn lower_call_target_name(&self, name: &str) -> String {
        if let Some((module, symbol)) = name.rsplit_once("::") {
            if module == self.module.name {
                return symbol.to_string();
            }
        }
        name.to_string()
    }

    fn call_return_type(&self, source_name: &str, lowered_name: &str) -> Option<Option<IrType>> {
        self.function_return_types
            .get(source_name)
            .or_else(|| self.function_return_types.get(lowered_name))
            .or_else(|| self.external_function_return_types.get(source_name))
            .or_else(|| self.external_function_return_types.get(lowered_name))
            .cloned()
    }

    fn materialize_schema_field(
        &mut self,
        base_var: &IrVar,
        field: &str,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
    ) -> Option<IrVar> {
        let field_ty = self.lookup_field_ir_type(&base_var.ty, field)?;
        let field_var = self.new_var(format!("{}_{}", base_var.name, field), field_ty);
        self.block_mut(blocks, current).instructions.push(IrInstruction::FieldAccess {
            dest: field_var.clone(),
            obj: IrOperand::Var(base_var.clone()),
            field: field.to_string(),
        });
        self.aggregate_fields.entry(base_var.id).or_default().insert(field.to_string(), field_var.clone());
        self.schema_field_roots.insert(field_var.id, (base_var.id, field.to_string()));
        Some(field_var)
    }

    fn copy_aggregate_metadata(&mut self, source: &IrOperand, dest_id: usize) {
        let IrOperand::Var(source_var) = source else {
            return;
        };
        if let Some(fields) = self.aggregate_fields.get(&source_var.id).cloned() {
            self.aggregate_fields.insert(dest_id, fields);
        }
        if let Some(root) = self.schema_field_roots.get(&source_var.id).cloned() {
            self.schema_field_roots.insert(dest_id, root);
        }
        if let Some(elements) = self.aggregate_elements.get(&source_var.id).cloned() {
            self.aggregate_elements.insert(dest_id, elements);
        }
    }
}

fn certify_straight_line_lifecycle_only(stmts: &[Stmt]) -> Result<StraightLineLifecycleOnly> {
    let mut bindings = HashMap::new();
    certify_lifecycle_stmts(stmts, false, &mut bindings)?;
    Ok(StraightLineLifecycleOnly)
}

fn certify_lifecycle_stmts(stmts: &[Stmt], branch_local: bool, bindings: &mut HashMap<String, (&'static str, Span)>) -> Result<()> {
    for stmt in stmts {
        match stmt {
            Stmt::Let(let_stmt) => certify_lifecycle_expr(&let_stmt.value, branch_local, bindings)?,
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => certify_lifecycle_expr(expr, branch_local, bindings)?,
            Stmt::Return(None) => {}
            Stmt::If(if_stmt) => {
                certify_lifecycle_expr(&if_stmt.condition, branch_local, bindings)?;
                certify_lifecycle_stmts(&if_stmt.then_branch, true, bindings)?;
                if let Some(else_branch) = &if_stmt.else_branch {
                    certify_lifecycle_stmts(else_branch, true, bindings)?;
                }
            }
            Stmt::For(for_stmt) => {
                certify_lifecycle_expr(&for_stmt.iterable, branch_local, bindings)?;
                certify_lifecycle_stmts(&for_stmt.body, true, bindings)?;
            }
            Stmt::While(while_stmt) => {
                certify_lifecycle_expr(&while_stmt.condition, branch_local, bindings)?;
                certify_lifecycle_stmts(&while_stmt.body, true, bindings)?;
            }
        }
    }
    Ok(())
}

fn certify_lifecycle_expr(expr: &Expr, branch_local: bool, bindings: &mut HashMap<String, (&'static str, Span)>) -> Result<()> {
    for (key, span, label) in lifecycle_effect_keys(expr) {
        if branch_local {
            return Err(CompileError::new(
                format!("branch-local lifecycle operation '{}' cannot be lowered by straight-line IR lifecycle collectors", label),
                span,
            ));
        }
        if let Some((previous_label, previous_span)) = bindings.insert(key.clone(), (label, span)) {
            return Err(CompileError::new(
                format!(
                    "duplicate lifecycle binding '{}' cannot be lowered by straight-line IR lifecycle collectors; previous '{}' binding starts at byte {}",
                    key, previous_label, previous_span.start
                ),
                span,
            ));
        }
    }

    match expr {
        Expr::Assign(assign) => {
            certify_lifecycle_expr(&assign.target, branch_local, bindings)?;
            certify_lifecycle_expr(&assign.value, branch_local, bindings)
        }
        Expr::Binary(binary) => {
            certify_lifecycle_expr(&binary.left, branch_local, bindings)?;
            certify_lifecycle_expr(&binary.right, branch_local, bindings)
        }
        Expr::Unary(unary) => certify_lifecycle_expr(&unary.expr, branch_local, bindings),
        Expr::Call(call) => {
            certify_lifecycle_expr(&call.func, branch_local, bindings)?;
            for arg in &call.args {
                certify_lifecycle_expr(arg, branch_local, bindings)?;
            }
            Ok(())
        }
        Expr::FieldAccess(field) => certify_lifecycle_expr(&field.expr, branch_local, bindings),
        Expr::Index(index) => {
            certify_lifecycle_expr(&index.expr, branch_local, bindings)?;
            certify_lifecycle_expr(&index.index, branch_local, bindings)
        }
        Expr::Create(create) => {
            for (_, value) in &create.fields {
                certify_lifecycle_expr(value, branch_local, bindings)?;
            }
            if let Some(lock) = &create.lock {
                certify_lifecycle_expr(lock, branch_local, bindings)?;
            }
            Ok(())
        }
        Expr::Consume(consume) => certify_lifecycle_expr(&consume.expr, branch_local, bindings),
        Expr::Transfer(transfer) => {
            certify_lifecycle_expr(&transfer.expr, branch_local, bindings)?;
            certify_lifecycle_expr(&transfer.to, branch_local, bindings)
        }
        Expr::Destroy(destroy) => certify_lifecycle_expr(&destroy.expr, branch_local, bindings),
        Expr::Claim(claim) => certify_lifecycle_expr(&claim.receipt, branch_local, bindings),
        Expr::Settle(settle) => certify_lifecycle_expr(&settle.expr, branch_local, bindings),
        Expr::CreateUnique(create) => {
            for (_, value) in &create.fields {
                certify_lifecycle_expr(value, branch_local, bindings)?;
            }
            if let Some(lock) = &create.lock {
                certify_lifecycle_expr(lock, branch_local, bindings)?;
            }
            Ok(())
        }
        Expr::ReplaceUnique(replace) => {
            certify_lifecycle_expr(&replace.expr, branch_local, bindings)?;
            for (_, value) in &replace.fields {
                certify_lifecycle_expr(value, branch_local, bindings)?;
            }
            Ok(())
        }
        Expr::Assert(assert_expr) => {
            certify_lifecycle_expr(&assert_expr.condition, branch_local, bindings)?;
            certify_lifecycle_expr(&assert_expr.message, branch_local, bindings)
        }
        Expr::Require(require_expr) => {
            certify_lifecycle_expr(&require_expr.condition, branch_local, bindings)?;
            if let Some(message) = &require_expr.message {
                certify_lifecycle_expr(message, branch_local, bindings)?;
            }
            Ok(())
        }
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                certify_lifecycle_expr(expr, branch_local, bindings)?;
            }
            Ok(())
        }
        Expr::Preserve(_) => Ok(()),
        Expr::Block(stmts) => certify_lifecycle_stmts(stmts, branch_local, bindings),
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                certify_lifecycle_expr(item, branch_local, bindings)?;
            }
            Ok(())
        }
        Expr::If(if_expr) => {
            certify_lifecycle_expr(&if_expr.condition, branch_local, bindings)?;
            certify_lifecycle_expr(&if_expr.then_branch, true, bindings)?;
            certify_lifecycle_expr(&if_expr.else_branch, true, bindings)
        }
        Expr::Cast(cast) => certify_lifecycle_expr(&cast.expr, branch_local, bindings),
        Expr::Range(range) => {
            certify_lifecycle_expr(&range.start, branch_local, bindings)?;
            certify_lifecycle_expr(&range.end, branch_local, bindings)
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                certify_lifecycle_expr(value, branch_local, bindings)?;
            }
            Ok(())
        }
        Expr::Match(match_expr) => {
            certify_lifecycle_expr(&match_expr.expr, branch_local, bindings)?;
            for arm in &match_expr.arms {
                certify_lifecycle_expr(&arm.value, true, bindings)?;
            }
            Ok(())
        }
        Expr::StdlibCall(call) => {
            for arg in &call.args {
                certify_lifecycle_expr(arg, branch_local, bindings)?;
            }
            Ok(())
        }
        Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) | Expr::ReadRef(_) => Ok(()),
    }
}

fn inline_ir_type_repr(ty: &IrType) -> Option<String> {
    match ty {
        IrType::U8 => Some("u8".to_string()),
        IrType::U16 => Some("u16".to_string()),
        IrType::U32 => Some("u32".to_string()),
        IrType::U64 => Some("u64".to_string()),
        IrType::U128 => Some("u128".to_string()),
        IrType::Bool => Some("bool".to_string()),
        IrType::Address => Some("Address".to_string()),
        IrType::Hash => Some("Hash".to_string()),
        IrType::Named(name) => Some(name.clone()),
        IrType::Unit | IrType::Array(_, _) | IrType::Tuple(_) | IrType::Ref(_) | IrType::MutRef(_) => None,
    }
}

fn collection_item_ir_type(ty: &IrType) -> Option<IrType> {
    let IrType::Named(name) = ty else {
        return None;
    };
    let inner = name.strip_prefix("Vec<")?.strip_suffix('>')?;
    Some(parse_inline_ir_type_repr(inner))
}

fn parse_inline_ir_type_repr(repr: &str) -> IrType {
    match repr.trim() {
        "u8" => IrType::U8,
        "u16" => IrType::U16,
        "u32" => IrType::U32,
        "u64" => IrType::U64,
        "u128" => IrType::U128,
        "bool" => IrType::Bool,
        "Address" => IrType::Address,
        "Hash" => IrType::Hash,
        other => IrType::Named(other.to_string()),
    }
}

pub fn verify_module(module: &IrModule) -> Result<()> {
    let mut callable_returns = HashMap::new();
    let mut callable_params = HashMap::new();

    for item in &module.items {
        match item {
            IrItem::Action(action) => {
                callable_returns.insert(action.name.clone(), action.return_type.clone());
                callable_params.insert(action.name.clone(), action.params.iter().map(|param| param.ty.clone()).collect::<Vec<_>>());
            }
            IrItem::PureFn(function) => {
                callable_returns.insert(function.name.clone(), function.return_type.clone());
                callable_params
                    .insert(function.name.clone(), function.params.iter().map(|param| param.ty.clone()).collect::<Vec<_>>());
            }
            IrItem::Lock(lock) => {
                callable_params.insert(lock.name.clone(), lock.params.iter().map(|param| param.ty.clone()).collect::<Vec<_>>());
            }
            IrItem::TypeDef(_) | IrItem::Invariant(_) => {}
        }
    }
    for abi in &module.external_callable_abis {
        callable_params.insert(abi.name.clone(), abi.params.iter().map(|param| param.ty.clone()).collect::<Vec<_>>());
    }

    for item in &module.items {
        match item {
            IrItem::Action(action) => verify_body(
                &format!("action '{}'", action.name),
                &action.body,
                &action.params,
                None,
                &callable_returns,
                &callable_params,
            )?,
            IrItem::PureFn(function) => verify_body(
                &format!("function '{}'", function.name),
                &function.body,
                &function.params,
                function.return_type.as_ref(),
                &callable_returns,
                &callable_params,
            )?,
            IrItem::Lock(lock) => verify_body(
                &format!("lock '{}'", lock.name),
                &lock.body,
                &lock.params,
                Some(&IrType::Bool),
                &callable_returns,
                &callable_params,
            )?,
            IrItem::TypeDef(_) | IrItem::Invariant(_) => {}
        }
    }

    Ok(())
}

fn verify_body(
    label: &str,
    body: &IrBody,
    params: &[IrParam],
    return_type: Option<&IrType>,
    callable_returns: &HashMap<String, Option<IrType>>,
    callable_params: &HashMap<String, Vec<IrType>>,
) -> Result<()> {
    if body.blocks.is_empty() {
        return Err(ir_verify_error(format!("{label} has no IR blocks")));
    }

    let mut block_ids = HashSet::new();
    for (index, block) in body.blocks.iter().enumerate() {
        if !block_ids.insert(block.id) {
            return Err(ir_verify_error(format!("{label} has duplicate block id {}", block.id.0)));
        }
        if block.id.0 != index {
            return Err(ir_verify_error(format!(
                "{label} block id {} is stored at index {}; IR block ids must remain index-addressable",
                block.id.0, index
            )));
        }
    }

    let mut predecessors: HashMap<BlockId, Vec<BlockId>> = body.blocks.iter().map(|block| (block.id, Vec::new())).collect();
    for block in &body.blocks {
        for target in terminator_targets(&block.terminator) {
            if !block_ids.contains(&target) {
                return Err(ir_verify_error(format!("{label} block {} terminates to missing block {}", block.id.0, target.0)));
            }
            predecessors.entry(target).or_default().push(block.id);
        }
    }

    let entry = body.blocks[0].id;
    let mut reachable = HashSet::new();
    let mut stack = vec![entry];
    while let Some(block_id) = stack.pop() {
        if !reachable.insert(block_id) {
            continue;
        }
        stack.extend(terminator_targets(&body.blocks[block_id.0].terminator));
    }
    for block in &body.blocks {
        if block.id != entry && !reachable.contains(&block.id) {
            return Err(ir_verify_error(format!("{label} contains unreachable block {}", block.id.0)));
        }
    }

    let all_var_types = collect_body_var_types(body, params)?;
    let all_value_kinds = collect_body_value_kinds(body, params)?;
    let all_defs = all_var_types.keys().copied().collect::<HashSet<_>>();
    let param_defs = params.iter().map(|param| param.binding.id).collect::<HashSet<_>>();
    let mut in_defs = body.blocks.iter().map(|block| (block.id, HashSet::new())).collect::<HashMap<_, _>>();
    let mut out_defs = body
        .blocks
        .iter()
        .map(|block| {
            let defs = if block.id == entry { HashSet::new() } else { all_defs.clone() };
            (block.id, defs)
        })
        .collect::<HashMap<_, _>>();
    in_defs.insert(entry, param_defs.clone());

    let mut changed = true;
    while changed {
        changed = false;
        for block in &body.blocks {
            let input = if block.id == entry {
                param_defs.clone()
            } else {
                let preds = predecessors.get(&block.id).cloned().unwrap_or_default();
                let mut iter = preds.into_iter().map(|pred| out_defs.get(&pred).cloned().unwrap_or_default());
                if let Some(mut intersection) = iter.next() {
                    for defs in iter {
                        intersection.retain(|id| defs.contains(id));
                    }
                    intersection
                } else {
                    HashSet::new()
                }
            };
            if in_defs.get(&block.id) != Some(&input) {
                in_defs.insert(block.id, input.clone());
                changed = true;
            }

            let mut output = input;
            for instruction in &block.instructions {
                if let Some(dest) = instruction_dest(instruction) {
                    output.insert(dest.id);
                }
            }
            if out_defs.get(&block.id) != Some(&output) {
                out_defs.insert(block.id, output);
                changed = true;
            }
        }
    }

    for block in &body.blocks {
        let mut defs = in_defs.get(&block.id).cloned().unwrap_or_default();
        for instruction in &block.instructions {
            verify_instruction_operands(label, block.id, instruction, &defs, &all_var_types)?;
            verify_instruction_abi(label, block.id, instruction, callable_returns, callable_params)?;
            verify_instruction_value_kinds(label, block.id, instruction, &all_value_kinds)?;
            if let Some(dest) = instruction_dest(instruction) {
                defs.insert(dest.id);
            }
        }
        verify_terminator(label, block.id, &block.terminator, return_type, &defs, &all_var_types)?;
        verify_terminator_value_kinds(label, block.id, &block.terminator, &all_value_kinds)?;
    }
    verify_no_unconsumed_status_values(label, body, &all_value_kinds)?;

    Ok(())
}

fn collect_body_var_types(body: &IrBody, params: &[IrParam]) -> Result<HashMap<usize, IrType>> {
    let mut types = HashMap::new();
    for param in params {
        insert_var_type(&mut types, &param.binding)?;
    }
    for block in &body.blocks {
        for instruction in &block.instructions {
            for var in instruction_vars(instruction) {
                insert_var_type(&mut types, var)?;
            }
        }
        for var in terminator_vars(&block.terminator) {
            insert_var_type(&mut types, var)?;
        }
    }
    Ok(types)
}

fn insert_var_type(types: &mut HashMap<usize, IrType>, var: &IrVar) -> Result<()> {
    if let Some(existing) = types.get(&var.id) {
        if !ir_type_compatible_with_abi(existing, &var.ty) && !ir_type_compatible_with_abi(&var.ty, existing) {
            return Err(ir_verify_error(format!("IR var{} has inconsistent types {:?} and {:?}", var.id, existing, var.ty)));
        }
    } else {
        types.insert(var.id, var.ty.clone());
    }
    Ok(())
}

fn collect_body_value_kinds(body: &IrBody, params: &[IrParam]) -> Result<HashMap<usize, IrValueKind>> {
    let mut kinds = HashMap::new();
    for param in params {
        insert_value_kind(&mut kinds, &param.binding, param.ty.value_kind())?;
    }
    for block in &body.blocks {
        for instruction in &block.instructions {
            if let Some((var, kind)) = instruction_result_value_kind(instruction, &kinds) {
                insert_value_kind(&mut kinds, var, kind)?;
            }
        }
    }
    Ok(kinds)
}

fn insert_value_kind(kinds: &mut HashMap<usize, IrValueKind>, var: &IrVar, kind: IrValueKind) -> Result<()> {
    if let Some(existing) = kinds.get(&var.id) {
        if *existing != kind {
            return Err(ir_verify_error(format!("IR var{} has inconsistent value kinds {:?} and {:?}", var.id, existing, kind)));
        }
    } else {
        kinds.insert(var.id, kind);
    }
    Ok(())
}

fn instruction_result_value_kind<'a>(
    instruction: &'a IrInstruction,
    known_kinds: &HashMap<usize, IrValueKind>,
) -> Option<(&'a IrVar, IrValueKind)> {
    match instruction {
        IrInstruction::Call { dest: Some(dest), func, .. } => {
            let kind = low_level_value_class_for_raw_symbol(func).map(low_level_value_kind).unwrap_or_else(|| dest.ty.value_kind());
            Some((dest, kind))
        }
        IrInstruction::Move { dest, src } => {
            let kind = operand_value_kind(src, known_kinds).unwrap_or_else(|| dest.ty.value_kind());
            Some((dest, kind))
        }
        IrInstruction::LoadConst { dest, value } => Some((dest, const_ir_type(value).value_kind())),
        _ => instruction_dest(instruction).map(|dest| (dest, dest.ty.value_kind())),
    }
}

fn low_level_value_kind(class: LowLevelValueClass) -> IrValueKind {
    match class {
        LowLevelValueClass::Bool => IrValueKind::Bool,
        LowLevelValueClass::DomainU64 => IrValueKind::Domain,
        LowLevelValueClass::ErrorCode => IrValueKind::ErrorCode,
        LowLevelValueClass::ExitStatus => IrValueKind::ExitStatus,
        LowLevelValueClass::HelperStatus => IrValueKind::HelperStatus,
        LowLevelValueClass::SyscallStatus => IrValueKind::SyscallStatus,
    }
}

fn verify_instruction_operands(
    label: &str,
    block_id: BlockId,
    instruction: &IrInstruction,
    defs: &HashSet<usize>,
    all_var_types: &HashMap<usize, IrType>,
) -> Result<()> {
    for operand in instruction_operands(instruction) {
        verify_operand_defined(label, block_id, operand, defs, all_var_types).map_err(|err| {
            let message = err.message.strip_prefix("IR verifier failed: ").unwrap_or(&err.message);
            ir_verify_error(format!("{message} while checking instruction {instruction:?}"))
        })?;
    }
    Ok(())
}

fn verify_instruction_abi(
    label: &str,
    block_id: BlockId,
    instruction: &IrInstruction,
    callable_returns: &HashMap<String, Option<IrType>>,
    callable_params: &HashMap<String, Vec<IrType>>,
) -> Result<()> {
    let IrInstruction::Call { dest, func, args } = instruction else {
        return Ok(());
    };
    if let Some(raw_class) = low_level_value_class_for_raw_symbol(func) {
        if let (None, LowLevelValueClass::SyscallStatus | LowLevelValueClass::ErrorCode | LowLevelValueClass::ExitStatus) =
            (dest, raw_class)
        {
            return Err(ir_verify_error(format!(
                "{label} block {} call '{}' drops raw {:?} without a typed runtime boundary",
                block_id.0, func, raw_class
            )));
        }
    }
    if let Some(kind) =
        checked_runtime_helper_spec(func).map(|spec| spec.kind).or_else(|| fail_closed_helper_spec(func).map(|spec| spec.kind))
    {
        match (kind, dest) {
            (SyscallKind::Unit, Some(_)) => {
                return Err(ir_verify_error(format!(
                    "{label} block {} call '{}' stores a unit helper status as a DSL value",
                    block_id.0, func
                )));
            }
            (SyscallKind::Value | SyscallKind::MultiReturn, None) => {
                return Err(ir_verify_error(format!(
                    "{label} block {} call '{}' drops a checked runtime helper value",
                    block_id.0, func
                )));
            }
            (SyscallKind::MultiReturn, Some(dest)) if !matches!(&dest.ty, IrType::Tuple(_)) => {
                return Err(ir_verify_error(format!(
                    "{label} block {} call '{}' stores multi-return helper into non-tuple destination {:?}",
                    block_id.0, func, dest.ty
                )));
            }
            _ => {}
        }
    }
    if func.starts_with("__") {
        return Ok(());
    }
    if let Some(params) = callable_params.get(func) {
        if params.len() != args.len() {
            return Err(ir_verify_error(format!(
                "{label} block {} calls '{}' with {} args but ABI expects {}",
                block_id.0,
                func,
                args.len(),
                params.len()
            )));
        }
        for (index, (arg, expected)) in args.iter().zip(params).enumerate() {
            let actual = operand_ir_type(arg);
            if !ir_type_compatible_with_abi(&actual, expected) {
                return Err(ir_verify_error(format!(
                    "{label} block {} call '{}' arg {} has type {:?}, expected {:?}",
                    block_id.0, func, index, actual, expected
                )));
            }
        }
    }
    if let Some(expected_return) = callable_returns.get(func) {
        match (dest, expected_return) {
            (Some(dest), Some(expected)) if &dest.ty != expected => {
                return Err(ir_verify_error(format!(
                    "{label} block {} call '{}' destination has type {:?}, expected {:?}",
                    block_id.0, func, dest.ty, expected
                )));
            }
            (Some(_), None) => {
                return Err(ir_verify_error(format!("{label} block {} call '{}' stores a unit return", block_id.0, func)))
            }
            (None, Some(expected)) => {
                return Err(ir_verify_error(format!(
                    "{label} block {} call '{}' drops non-unit return {:?}",
                    block_id.0, func, expected
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

fn verify_instruction_value_kinds(
    label: &str,
    block_id: BlockId,
    instruction: &IrInstruction,
    value_kinds: &HashMap<usize, IrValueKind>,
) -> Result<()> {
    match instruction {
        IrInstruction::Call { func, args, .. } => {
            for (index, arg) in args.iter().enumerate() {
                if let Some(kind) = operand_value_kind(arg, value_kinds).filter(|kind| kind.is_status_like()) {
                    return Err(status_value_error(label, block_id, format!("passes {:?} as argument {} to '{}'", kind, index, func)));
                }
            }
        }
        IrInstruction::StoreVar { name, src } => {
            if let Some(kind) = operand_value_kind(src, value_kinds).filter(|kind| kind.is_status_like()) {
                return Err(status_value_error(label, block_id, format!("stores {:?} into DSL local '{}'", kind, name)));
            }
        }
        IrInstruction::Move { dest, src } | IrInstruction::Cast { dest, src } => {
            if let Some(kind) = operand_value_kind(src, value_kinds).filter(|kind| kind.is_status_like()) {
                return Err(status_value_error(label, block_id, format!("stores {:?} into DSL value var{}", kind, dest.id)));
            }
        }
        IrInstruction::Tuple { fields, .. } => {
            for (index, field) in fields.iter().enumerate() {
                if let Some(kind) = operand_value_kind(field, value_kinds).filter(|kind| kind.is_status_like()) {
                    return Err(status_value_error(label, block_id, format!("stores {:?} into tuple field {}", kind, index)));
                }
            }
        }
        _ => {
            for operand in instruction_operands(instruction) {
                if let Some(kind) = operand_value_kind(operand, value_kinds).filter(|kind| kind.is_status_like()) {
                    return Err(status_value_error(label, block_id, format!("uses {:?} as an ordinary IR operand", kind)));
                }
            }
        }
    }
    Ok(())
}

fn verify_terminator(
    label: &str,
    block_id: BlockId,
    terminator: &IrTerminator,
    expected_return: Option<&IrType>,
    defs: &HashSet<usize>,
    all_var_types: &HashMap<usize, IrType>,
) -> Result<()> {
    match terminator {
        IrTerminator::Return(Some(operand)) => {
            verify_operand_defined(label, block_id, operand, defs, all_var_types)?;
            if let Some(expected) = expected_return {
                let actual = operand_ir_type(operand);
                if &actual != expected {
                    return Err(ir_verify_error(format!(
                        "{label} block {} returns type {:?}, expected {:?}",
                        block_id.0, actual, expected
                    )));
                }
            }
        }
        IrTerminator::Return(None) => {
            if let Some(expected) = expected_return {
                return Err(ir_verify_error(format!("{label} block {} returns unit, expected {:?}", block_id.0, expected)));
            }
        }
        IrTerminator::Abort(_) => {}
        IrTerminator::Branch { cond, .. } => {
            verify_operand_defined(label, block_id, cond, defs, all_var_types)?;
        }
        IrTerminator::Jump(_) => {}
    }
    Ok(())
}

fn verify_terminator_value_kinds(
    label: &str,
    block_id: BlockId,
    terminator: &IrTerminator,
    value_kinds: &HashMap<usize, IrValueKind>,
) -> Result<()> {
    match terminator {
        IrTerminator::Return(Some(operand)) => {
            if let Some(kind) = operand_value_kind(operand, value_kinds).filter(|kind| kind.is_status_like()) {
                return Err(status_value_error(label, block_id, format!("returns {:?} as an ordinary value", kind)));
            }
        }
        IrTerminator::Branch { cond, .. } => {
            if let Some(kind) = operand_value_kind(cond, value_kinds).filter(|kind| kind.is_status_like()) {
                return Err(status_value_error(label, block_id, format!("branches on {:?} as a predicate", kind)));
            }
        }
        IrTerminator::Return(None) | IrTerminator::Abort(_) | IrTerminator::Jump(_) => {}
    }
    Ok(())
}

fn verify_no_unconsumed_status_values(label: &str, body: &IrBody, value_kinds: &HashMap<usize, IrValueKind>) -> Result<()> {
    for block in &body.blocks {
        for instruction in &block.instructions {
            let Some(dest) = instruction_dest(instruction) else {
                continue;
            };
            let Some(kind) = value_kinds.get(&dest.id).copied().filter(|kind| kind.is_status_like()) else {
                continue;
            };
            return Err(status_value_error(
                label,
                block.id,
                format!("produces {:?} in var{} without a checked fail-closed consumer", kind, dest.id),
            ));
        }
    }
    Ok(())
}

fn verify_operand_defined(
    label: &str,
    block_id: BlockId,
    operand: &IrOperand,
    defs: &HashSet<usize>,
    all_var_types: &HashMap<usize, IrType>,
) -> Result<()> {
    let IrOperand::Var(var) = operand else {
        return Ok(());
    };
    if !all_var_types.contains_key(&var.id) {
        return Err(ir_verify_error(format!("{label} block {} references unknown var{}", block_id.0, var.id)));
    }
    if !defs.contains(&var.id) {
        let mut available = defs.iter().copied().collect::<Vec<_>>();
        available.sort_unstable();
        return Err(ir_verify_error(format!(
            "{label} block {} uses var{} before it is defined on all incoming paths; available vars: {:?}",
            block_id.0, var.id, available
        )));
    }
    Ok(())
}

fn operand_value_kind(operand: &IrOperand, value_kinds: &HashMap<usize, IrValueKind>) -> Option<IrValueKind> {
    match operand {
        IrOperand::Var(var) => Some(value_kinds.get(&var.id).copied().unwrap_or_else(|| var.ty.value_kind())),
        IrOperand::Const(value) => Some(const_ir_type(value).value_kind()),
    }
}

fn status_value_error(label: &str, block_id: BlockId, detail: String) -> CompileError {
    ir_verify_error(format!(
        "{label} block {} leaks a status/error boundary value: {detail}; status-like values must be consumed by a checked fail-closed boundary",
        block_id.0
    ))
}

fn terminator_targets(terminator: &IrTerminator) -> Vec<BlockId> {
    match terminator {
        IrTerminator::Jump(target) => vec![*target],
        IrTerminator::Branch { then_block, else_block, .. } => vec![*then_block, *else_block],
        IrTerminator::Return(_) | IrTerminator::Abort(_) => Vec::new(),
    }
}

fn instruction_dest(instruction: &IrInstruction) -> Option<&IrVar> {
    match instruction {
        IrInstruction::LoadConst { dest, .. }
        | IrInstruction::LoadVar { dest, .. }
        | IrInstruction::Binary { dest, .. }
        | IrInstruction::Unary { dest, .. }
        | IrInstruction::FieldAccess { dest, .. }
        | IrInstruction::Index { dest, .. }
        | IrInstruction::Length { dest, .. }
        | IrInstruction::TypeHash { dest, .. }
        | IrInstruction::CollectionNew { dest, .. }
        | IrInstruction::CollectionCapacity { dest, .. }
        | IrInstruction::CollectionContains { dest, .. }
        | IrInstruction::CollectionRemove { dest, .. }
        | IrInstruction::CollectionPop { dest, .. }
        | IrInstruction::ReadRef { dest, .. }
        | IrInstruction::Move { dest, .. }
        | IrInstruction::Cast { dest, .. }
        | IrInstruction::Tuple { dest, .. }
        | IrInstruction::Create { dest, .. }
        | IrInstruction::Transfer { dest, .. }
        | IrInstruction::Claim { dest, .. }
        | IrInstruction::Settle { dest, .. }
        | IrInstruction::CreateUnique { dest, .. }
        | IrInstruction::ReplaceUnique { dest, .. } => Some(dest),
        IrInstruction::Call { dest, .. } => dest.as_ref(),
        IrInstruction::StoreVar { .. }
        | IrInstruction::CollectionPush { .. }
        | IrInstruction::CollectionExtend { .. }
        | IrInstruction::CollectionClear { .. }
        | IrInstruction::CollectionInsert { .. }
        | IrInstruction::CollectionSet { .. }
        | IrInstruction::CollectionReverse { .. }
        | IrInstruction::CollectionTruncate { .. }
        | IrInstruction::CollectionSwap { .. }
        | IrInstruction::Consume { .. }
        | IrInstruction::Destroy { .. }
        | IrInstruction::CellMetadataEquality { .. } => None,
    }
}

fn instruction_operands(instruction: &IrInstruction) -> Vec<&IrOperand> {
    match instruction {
        IrInstruction::LoadConst { .. } | IrInstruction::LoadVar { .. } | IrInstruction::ReadRef { .. } => Vec::new(),
        IrInstruction::StoreVar { src, .. }
        | IrInstruction::Unary { operand: src, .. }
        | IrInstruction::Length { operand: src, .. }
        | IrInstruction::TypeHash { operand: src, .. }
        | IrInstruction::CollectionCapacity { collection: src, .. }
        | IrInstruction::CollectionClear { collection: src }
        | IrInstruction::CollectionReverse { collection: src }
        | IrInstruction::CollectionPop { collection: src, .. }
        | IrInstruction::Move { src, .. }
        | IrInstruction::Cast { src, .. }
        | IrInstruction::Consume { operand: src }
        | IrInstruction::Destroy { operand: src, .. }
        | IrInstruction::Claim { receipt: src, .. }
        | IrInstruction::Settle { operand: src, .. } => vec![src],
        IrInstruction::Binary { left, right, .. }
        | IrInstruction::Index { arr: left, idx: right, .. }
        | IrInstruction::CollectionExtend { collection: left, slice: right }
        | IrInstruction::CollectionContains { collection: left, value: right, .. }
        | IrInstruction::CollectionRemove { collection: left, index: right, .. }
        | IrInstruction::CollectionTruncate { collection: left, len: right }
        | IrInstruction::Transfer { operand: left, to: right, .. }
        | IrInstruction::CellMetadataEquality { left, right, .. } => vec![left, right],
        IrInstruction::FieldAccess { obj, .. } => vec![obj],
        IrInstruction::CollectionNew { capacity, .. } => capacity.iter().collect(),
        IrInstruction::CollectionPush { collection, value } => vec![collection, value],
        IrInstruction::CollectionInsert { collection, index, value } | IrInstruction::CollectionSet { collection, index, value } => {
            vec![collection, index, value]
        }
        IrInstruction::CollectionSwap { collection, left, right } => vec![collection, left, right],
        IrInstruction::Call { args, .. } | IrInstruction::Tuple { fields: args, .. } => args.iter().collect(),
        IrInstruction::Create { pattern, .. } | IrInstruction::CreateUnique { pattern, .. } => create_pattern_operands(pattern),
        IrInstruction::ReplaceUnique { operand, pattern, .. } => {
            let mut operands = vec![operand];
            operands.extend(create_pattern_operands(pattern));
            operands
        }
    }
}

fn instruction_vars(instruction: &IrInstruction) -> Vec<&IrVar> {
    let mut vars = instruction_operands(instruction)
        .into_iter()
        .filter_map(|operand| match operand {
            IrOperand::Var(var) => Some(var),
            IrOperand::Const(_) => None,
        })
        .collect::<Vec<_>>();
    if let Some(dest) = instruction_dest(instruction) {
        vars.push(dest);
    }
    vars
}

fn terminator_vars(terminator: &IrTerminator) -> Vec<&IrVar> {
    match terminator {
        IrTerminator::Return(Some(IrOperand::Var(var))) | IrTerminator::Branch { cond: IrOperand::Var(var), .. } => vec![var],
        IrTerminator::Return(_) | IrTerminator::Abort(_) | IrTerminator::Branch { .. } | IrTerminator::Jump(_) => Vec::new(),
    }
}

fn create_pattern_operands(pattern: &CreatePattern) -> Vec<&IrOperand> {
    let mut operands = pattern.fields.iter().map(|(_, operand)| operand).collect::<Vec<_>>();
    if let Some(lock) = &pattern.lock {
        operands.push(lock);
    }
    operands
}

fn operand_ir_type(operand: &IrOperand) -> IrType {
    match operand {
        IrOperand::Var(var) => var.ty.clone(),
        IrOperand::Const(value) => const_ir_type(value),
    }
}

fn ir_type_compatible_with_abi(actual: &IrType, expected: &IrType) -> bool {
    if actual == expected {
        return true;
    }
    match expected {
        IrType::Ref(inner) | IrType::MutRef(inner) => actual == inner.as_ref(),
        IrType::Named(expected) if expected == "Vec" => {
            matches!(actual, IrType::Named(actual) if actual == "Vec" || actual.starts_with("Vec<"))
        }
        IrType::Named(expected) if expected.starts_with("Vec<") => matches!(actual, IrType::Named(actual) if actual == "Vec"),
        _ => false,
    }
}

fn const_ir_type(value: &IrConst) -> IrType {
    match value {
        IrConst::Unit => IrType::Unit,
        IrConst::U8(_) => IrType::U8,
        IrConst::U16(_) => IrType::U16,
        IrConst::U32(_) => IrType::U32,
        IrConst::U64(_) => IrType::U64,
        IrConst::U128(_) => IrType::U128,
        IrConst::Bool(_) => IrType::Bool,
        IrConst::Address(_) => IrType::Address,
        IrConst::Hash(_) => IrType::Hash,
        IrConst::Array(items) => IrType::Array(Box::new(items.first().map(const_ir_type).unwrap_or(IrType::U8)), items.len()),
    }
}

fn ir_verify_error(message: impl Into<String>) -> CompileError {
    CompileError::without_span(format!("IR verifier failed: {}", message.into()))
}

pub fn generate(ast: &Module) -> Result<IrModule> {
    let generator = IrGenerator::new(ast.name.clone());
    generator.generate(ast)
}

pub fn generate_with_resolver(ast: &Module, resolver: &ModuleResolver, module_name: &str) -> Result<IrModule> {
    generate_with_resolver_inner(ast, resolver, module_name, true)
}

fn generate_with_resolver_inner(
    ast: &Module,
    resolver: &ModuleResolver,
    module_name: &str,
    include_external_callables: bool,
) -> Result<IrModule> {
    let mut type_fields = HashMap::new();
    let mut type_kinds = HashMap::new();
    let mut receipt_claim_outputs = HashMap::new();
    let mut flow_states = HashMap::new();
    let mut external_type_defs = Vec::new();
    let mut external_type_names = HashSet::new();
    let mut external_callable_abis = Vec::new();
    let mut external_callable_names = HashSet::new();
    let mut external_function_effects = HashMap::new();
    let mut external_function_return_types = HashMap::new();

    for item in &ast.items {
        let Item::Use(use_stmt) = item else {
            continue;
        };

        for import in &use_stmt.imports {
            let local_name = import.alias.clone().unwrap_or_else(|| import.name.clone());
            if let Some(type_def) = resolver.resolve_type(module_name, &local_name) {
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
                    if let Some(ir_type_def) = resolver_type_def_to_ir(&local_name, &type_def) {
                        external_type_defs.push(ir_type_def);
                    }
                }
            }
            if let Some(function) = resolver.resolve_function(module_name, &local_name) {
                external_function_effects.insert(local_name.clone(), function_def_effect_class(&function));
                external_function_return_types.insert(local_name.clone(), function_def_return_type(&function));
                push_external_callable_abi(&mut external_callable_abis, &mut external_callable_names, local_name, &function);
            }
        }
    }
    for call_name in collect_call_names(ast) {
        if let Some(function) = resolver.resolve_function(module_name, &call_name) {
            external_function_effects.insert(call_name.clone(), function_def_effect_class(&function));
            external_function_return_types.insert(call_name.clone(), function_def_return_type(&function));
            push_external_callable_abi(&mut external_callable_abis, &mut external_callable_names, call_name, &function);
        }
    }

    let generator = IrGenerator::with_import_context(
        ast.name.clone(),
        type_fields,
        type_kinds,
        receipt_claim_outputs,
        flow_states,
        external_function_effects,
        external_function_return_types,
    );
    let mut ir = generator.generate(ast)?;
    ir.external_type_defs = external_type_defs;
    ir.external_callable_abis = external_callable_abis;
    if include_external_callables {
        append_external_callable_bodies(&mut ir, ast, resolver, module_name)?;
    }
    Ok(ir)
}

fn append_external_callable_bodies(ir: &mut IrModule, ast: &Module, resolver: &ModuleResolver, module_name: &str) -> Result<()> {
    let mut known_callables = ir_callable_names(ir);
    let mut imported_callables = HashSet::new();
    let mut pending = collect_call_names(ast).into_iter().collect::<Vec<_>>();

    while let Some(call_name) = pending.pop() {
        let symbol = call_name.rsplit("::").next().unwrap_or(&call_name).to_string();
        if known_callables.contains(&symbol) {
            continue;
        }

        let Some((owner_module, function)) = resolver.resolve_function_with_module(module_name, &call_name) else {
            continue;
        };
        let import_key = format!("{}::{}", owner_module, symbol);
        if owner_module == module_name || !imported_callables.insert(import_key) {
            continue;
        }

        let Some(owner_ast) = resolver.module(&owner_module) else {
            continue;
        };
        let external_ir = generate_with_resolver_inner(owner_ast, resolver, &owner_module, false)?;
        merge_external_type_defs(ir, &external_ir);
        merge_external_callable_abis(ir, &external_ir);
        for (name, size) in external_ir.enum_fixed_sizes {
            ir.enum_fixed_sizes.entry(name).or_insert(size);
        }

        let target_symbol = function_def_name(&function);
        if let Some(item) =
            external_ir.items.into_iter().find(|item| ir_item_callable_name(item).is_some_and(|name| name == target_symbol))
        {
            let item = rename_ir_callable_item(item, symbol.clone());
            if known_callables.insert(symbol) {
                collect_ir_item_call_names(&item, &mut pending);
                ir.items.push(item);
            }
        }
    }

    Ok(())
}

fn ir_callable_names(ir: &IrModule) -> HashSet<String> {
    ir.items.iter().filter_map(ir_item_callable_name).map(str::to_string).collect()
}

fn ir_item_callable_name(item: &IrItem) -> Option<&str> {
    match item {
        IrItem::Action(action) => Some(&action.name),
        IrItem::PureFn(function) => Some(&function.name),
        IrItem::Lock(lock) => Some(&lock.name),
        IrItem::TypeDef(_) | IrItem::Invariant(_) => None,
    }
}

fn function_def_name(function: &FunctionDef) -> &str {
    match function {
        FunctionDef::Action(action) => &action.name,
        FunctionDef::Function(function) => &function.name,
        FunctionDef::Lock(lock) => &lock.name,
    }
}

fn rename_ir_callable_item(mut item: IrItem, name: String) -> IrItem {
    match &mut item {
        IrItem::Action(action) => action.name = name,
        IrItem::PureFn(function) => function.name = name,
        IrItem::Lock(lock) => lock.name = name,
        IrItem::TypeDef(_) | IrItem::Invariant(_) => {}
    }
    item
}

fn merge_external_type_defs(ir: &mut IrModule, external_ir: &IrModule) {
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

fn collect_ir_item_call_names(item: &IrItem, pending: &mut Vec<String>) {
    match item {
        IrItem::Action(action) => collect_ir_body_call_names(&action.body, pending),
        IrItem::PureFn(function) => collect_ir_body_call_names(&function.body, pending),
        IrItem::Lock(lock) => collect_ir_body_call_names(&lock.body, pending),
        IrItem::TypeDef(_) | IrItem::Invariant(_) => {}
    }
}

fn collect_ir_body_call_names(body: &IrBody, pending: &mut Vec<String>) {
    for block in &body.blocks {
        for instruction in &block.instructions {
            if let IrInstruction::Call { func, .. } = instruction {
                pending.push(func.clone());
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

fn collect_call_names(ast: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &ast.items {
        match item {
            Item::Action(action) => collect_call_names_from_stmts(&action.body, &mut names),
            Item::Function(function) => collect_call_names_from_stmts(&function.body, &mut names),
            Item::Lock(lock) => collect_call_names_from_stmts(&lock.body, &mut names),
            _ => {}
        }
    }
    names
}

fn collect_call_names_from_stmts(stmts: &[Stmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        collect_call_names_from_stmt(stmt, names);
    }
}

fn collect_call_names_from_stmt(stmt: &Stmt, names: &mut HashSet<String>) {
    match stmt {
        Stmt::Expr(expr) | Stmt::Let(LetStmt { value: expr, .. }) | Stmt::Return(Some(expr)) => {
            collect_call_names_from_expr(expr, names);
        }
        Stmt::If(if_stmt) => {
            collect_call_names_from_expr(&if_stmt.condition, names);
            collect_call_names_from_stmts(&if_stmt.then_branch, names);
            if let Some(else_branch) = &if_stmt.else_branch {
                collect_call_names_from_stmts(else_branch, names);
            }
        }
        Stmt::For(for_stmt) => {
            collect_call_names_from_expr(&for_stmt.iterable, names);
            collect_call_names_from_stmts(&for_stmt.body, names);
        }
        Stmt::While(while_stmt) => {
            collect_call_names_from_expr(&while_stmt.condition, names);
            collect_call_names_from_stmts(&while_stmt.body, names);
        }
        Stmt::Return(None) => {}
    }
}

fn collect_call_names_from_expr(expr: &Expr, names: &mut HashSet<String>) {
    match expr {
        Expr::Call(call) => {
            if let Expr::Identifier(name) = call.func.as_ref() {
                names.insert(name.clone());
            }
            collect_call_names_from_expr(&call.func, names);
            for arg in &call.args {
                collect_call_names_from_expr(arg, names);
            }
        }
        Expr::Assign(assign) => {
            collect_call_names_from_expr(&assign.target, names);
            collect_call_names_from_expr(&assign.value, names);
        }
        Expr::Binary(binary) => {
            collect_call_names_from_expr(&binary.left, names);
            collect_call_names_from_expr(&binary.right, names);
        }
        Expr::Unary(unary) => collect_call_names_from_expr(&unary.expr, names),
        Expr::FieldAccess(field) => collect_call_names_from_expr(&field.expr, names),
        Expr::Index(index) => {
            collect_call_names_from_expr(&index.expr, names);
            collect_call_names_from_expr(&index.index, names);
        }
        Expr::Create(create) => {
            for (_, value) in &create.fields {
                collect_call_names_from_expr(value, names);
            }
            if let Some(lock) = &create.lock {
                collect_call_names_from_expr(lock, names);
            }
        }
        Expr::Consume(consume) => collect_call_names_from_expr(&consume.expr, names),
        Expr::Transfer(transfer) => {
            collect_call_names_from_expr(&transfer.expr, names);
            collect_call_names_from_expr(&transfer.to, names);
        }
        Expr::Destroy(destroy) => collect_call_names_from_expr(&destroy.expr, names),
        Expr::Claim(claim) => collect_call_names_from_expr(&claim.receipt, names),
        Expr::Settle(settle) => collect_call_names_from_expr(&settle.expr, names),
        Expr::CreateUnique(cu) => {
            for (_, value) in &cu.fields {
                collect_call_names_from_expr(value, names);
            }
            if let Some(lock) = &cu.lock {
                collect_call_names_from_expr(lock, names);
            }
        }
        Expr::ReplaceUnique(ru) => {
            collect_call_names_from_expr(&ru.expr, names);
            for (_, value) in &ru.fields {
                collect_call_names_from_expr(value, names);
            }
        }
        Expr::Assert(assert_expr) => {
            collect_call_names_from_expr(&assert_expr.condition, names);
        }
        Expr::Require(require_expr) => {
            collect_call_names_from_expr(&require_expr.condition, names);
            if let Some(message) = &require_expr.message {
                collect_call_names_from_expr(message, names);
            }
        }
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                collect_call_names_from_expr(expr, names);
            }
        }
        Expr::Preserve(_) => {}
        Expr::Block(stmts) => collect_call_names_from_stmts(stmts, names),
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                collect_call_names_from_expr(item, names);
            }
        }
        Expr::If(if_expr) => {
            collect_call_names_from_expr(&if_expr.condition, names);
            collect_call_names_from_expr(&if_expr.then_branch, names);
            collect_call_names_from_expr(&if_expr.else_branch, names);
        }
        Expr::Cast(cast) => collect_call_names_from_expr(&cast.expr, names),
        Expr::Range(range) => {
            collect_call_names_from_expr(&range.start, names);
            collect_call_names_from_expr(&range.end, names);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_call_names_from_expr(value, names);
            }
        }
        Expr::Match(match_expr) => {
            collect_call_names_from_expr(&match_expr.expr, names);
            for arm in &match_expr.arms {
                collect_call_names_from_expr(&arm.value, names);
            }
        }
        Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::ByteString(_)
        | Expr::Identifier(_)
        | Expr::ReadRef(_)
        | Expr::StdlibCall(_) => {}
    }
}

fn function_def_effect_class(function: &FunctionDef) -> EffectClass {
    match function {
        FunctionDef::Action(action) => {
            if action.effect_declared {
                ast_effect_to_ir(action.effect)
            } else {
                infer_action_effect_without_call_graph(action)
            }
        }
        FunctionDef::Function(function) => infer_fn_effect_without_call_graph(function),
        FunctionDef::Lock(_) => EffectClass::ReadOnly,
    }
}

fn function_def_return_type(function: &FunctionDef) -> Option<IrType> {
    match function {
        FunctionDef::Action(action) => action.return_type.as_ref().map(ast_type_to_ir),
        FunctionDef::Function(function) => function.return_type.as_ref().map(ast_type_to_ir),
        FunctionDef::Lock(_) => Some(IrType::Bool),
    }
}

fn ast_type_to_ir(ty: &Type) -> IrType {
    match ty {
        Type::U8 => IrType::U8,
        Type::U16 => IrType::U16,
        Type::U32 => IrType::U32,
        Type::U64 => IrType::U64,
        Type::U128 => IrType::U128,
        Type::Bool => IrType::Bool,
        Type::Unit => IrType::Unit,
        Type::Address => IrType::Address,
        Type::Hash => IrType::Hash,
        Type::Array(elem, size) => IrType::Array(Box::new(ast_type_to_ir(elem)), *size),
        Type::Tuple(types) => IrType::Tuple(types.iter().map(ast_type_to_ir).collect()),
        Type::Named(name) if name == "usize" || name == "isize" => IrType::U64,
        Type::Named(name) => IrType::Named(name.clone()),
        Type::Ref(inner) => IrType::Ref(Box::new(ast_type_to_ir(inner))),
        Type::MutRef(inner) => IrType::MutRef(Box::new(ast_type_to_ir(inner))),
    }
}

fn infer_action_effect_without_call_graph(action: &ActionDef) -> EffectClass {
    if !action_core_input_binding_names(action).is_empty() {
        return EffectClass::Mutating;
    }
    let mut footprint = EffectFootprint::default();
    for stmt in &action.body {
        collect_ast_stmt_effects(stmt, &mut footprint);
    }
    effect_from_footprint(&footprint)
}

fn infer_fn_effect_without_call_graph(function: &FnDef) -> EffectClass {
    let mut footprint = EffectFootprint::default();
    for stmt in &function.body {
        collect_ast_stmt_effects(stmt, &mut footprint);
    }
    effect_from_footprint(&footprint)
}

fn collect_ast_stmt_effects(stmt: &Stmt, footprint: &mut EffectFootprint) {
    match stmt {
        Stmt::Expr(expr) | Stmt::Let(LetStmt { value: expr, .. }) | Stmt::Return(Some(expr)) => {
            collect_ast_expr_effects(expr, footprint);
        }
        Stmt::If(if_stmt) => {
            collect_ast_expr_effects(&if_stmt.condition, footprint);
            for stmt in &if_stmt.then_branch {
                collect_ast_stmt_effects(stmt, footprint);
            }
            if let Some(else_branch) = &if_stmt.else_branch {
                for stmt in else_branch {
                    collect_ast_stmt_effects(stmt, footprint);
                }
            }
        }
        Stmt::For(for_stmt) => {
            collect_ast_expr_effects(&for_stmt.iterable, footprint);
            for stmt in &for_stmt.body {
                collect_ast_stmt_effects(stmt, footprint);
            }
        }
        Stmt::While(while_stmt) => {
            collect_ast_expr_effects(&while_stmt.condition, footprint);
            for stmt in &while_stmt.body {
                collect_ast_stmt_effects(stmt, footprint);
            }
        }
        _ => {}
    }
}

fn collect_ast_expr_effects(expr: &Expr, footprint: &mut EffectFootprint) {
    match expr {
        Expr::Consume(consume) => {
            footprint.has_consume = true;
            collect_ast_expr_effects(&consume.expr, footprint);
        }
        Expr::Create(create) => {
            footprint.has_create = true;
            for (_, value) in &create.fields {
                collect_ast_expr_effects(value, footprint);
            }
            if let Some(lock) = &create.lock {
                collect_ast_expr_effects(lock, footprint);
            }
        }
        Expr::Destroy(destroy) => {
            footprint.has_consume = true;
            collect_ast_expr_effects(&destroy.expr, footprint);
        }
        Expr::ReadRef(_) => footprint.has_read_ref = true,
        Expr::Assert(assert_expr) => {
            collect_ast_expr_effects(&assert_expr.condition, footprint);
        }
        Expr::Require(require_expr) => {
            collect_ast_expr_effects(&require_expr.condition, footprint);
            if let Some(message) = &require_expr.message {
                collect_ast_expr_effects(message, footprint);
            }
        }
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                collect_ast_expr_effects(expr, footprint);
            }
        }
        Expr::Preserve(_) => {}
        Expr::Assign(assign) => {
            collect_ast_expr_effects(&assign.target, footprint);
            collect_ast_expr_effects(&assign.value, footprint);
        }
        Expr::Binary(binary) => {
            collect_ast_expr_effects(&binary.left, footprint);
            collect_ast_expr_effects(&binary.right, footprint);
        }
        Expr::Unary(unary) => collect_ast_expr_effects(&unary.expr, footprint),
        Expr::Call(call) => {
            for arg in &call.args {
                collect_ast_expr_effects(arg, footprint);
            }
        }
        Expr::FieldAccess(field) => collect_ast_expr_effects(&field.expr, footprint),
        Expr::Index(index) => {
            collect_ast_expr_effects(&index.expr, footprint);
            collect_ast_expr_effects(&index.index, footprint);
        }
        Expr::If(if_expr) => {
            collect_ast_expr_effects(&if_expr.condition, footprint);
            collect_ast_expr_effects(&if_expr.then_branch, footprint);
            collect_ast_expr_effects(&if_expr.else_branch, footprint);
        }
        Expr::Cast(cast) => collect_ast_expr_effects(&cast.expr, footprint),
        Expr::Range(range) => {
            collect_ast_expr_effects(&range.start, footprint);
            collect_ast_expr_effects(&range.end, footprint);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_ast_expr_effects(value, footprint);
            }
        }
        Expr::Match(match_expr) => {
            collect_ast_expr_effects(&match_expr.expr, footprint);
            for arm in &match_expr.arms {
                collect_ast_expr_effects(&arm.value, footprint);
            }
        }
        Expr::Block(stmts) => {
            for stmt in stmts {
                collect_ast_stmt_effects(stmt, footprint);
            }
        }
        Expr::StdlibCall(call) => {
            for arg in &call.args {
                collect_ast_expr_effects(arg, footprint);
            }
        }
        Expr::CreateUnique(cu) => {
            footprint.has_create = true;
            for (_, value) in &cu.fields {
                collect_ast_expr_effects(value, footprint);
            }
            if let Some(lock) = &cu.lock {
                collect_ast_expr_effects(lock, footprint);
            }
        }
        Expr::ReplaceUnique(ru) => {
            footprint.has_consume = true;
            footprint.has_create = true;
            collect_ast_expr_effects(&ru.expr, footprint);
            for (_, value) in &ru.fields {
                collect_ast_expr_effects(value, footprint);
            }
        }
        Expr::Transfer(transfer) => {
            footprint.has_consume = true;
            collect_ast_expr_effects(&transfer.expr, footprint);
            collect_ast_expr_effects(&transfer.to, footprint);
        }
        Expr::Claim(claim) => {
            footprint.has_consume = true;
            collect_ast_expr_effects(&claim.receipt, footprint);
        }
        Expr::Settle(settle) => {
            footprint.has_consume = true;
            collect_ast_expr_effects(&settle.expr, footprint);
        }
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                collect_ast_expr_effects(item, footprint);
            }
        }
        _ => {}
    }
}

fn effect_from_footprint(footprint: &EffectFootprint) -> EffectClass {
    match (footprint.has_consume, footprint.has_create, footprint.has_read_ref) {
        (true, true, _) => EffectClass::Mutating,
        (true, false, _) => EffectClass::Destroying,
        (false, true, _) => EffectClass::Creating,
        (false, false, true) => EffectClass::ReadOnly,
        (false, false, false) => EffectClass::Pure,
    }
}

fn ast_effect_to_ir(effect: crate::ast::EffectClass) -> EffectClass {
    match effect {
        crate::ast::EffectClass::Pure => EffectClass::Pure,
        crate::ast::EffectClass::ReadOnly => EffectClass::ReadOnly,
        crate::ast::EffectClass::Mutating => EffectClass::Mutating,
        crate::ast::EffectClass::Creating => EffectClass::Creating,
        crate::ast::EffectClass::Destroying => EffectClass::Destroying,
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

    Some(fields.iter().map(|field| (field.name.clone(), ast_type_to_ir_type(&field.ty))).collect())
}

fn resolver_type_def_to_ir(local_name: &str, type_def: &TypeDef) -> Option<IrTypeDef> {
    match type_def {
        TypeDef::Resource(resource) => Some(IrTypeDef {
            name: local_name.to_string(),
            type_id: resource.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: resource.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: resource.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Resource,
            fields: layout_resolver_fields(&resource.fields),
            capabilities: resource.capabilities.clone(),
            claim_output: None,
            flow_states: None,
            flow_state_field: None,
            flow_rules: Vec::new(),
            identity: lower_identity_policy_ast(&resource.identity),
        }),
        TypeDef::Shared(shared) => Some(IrTypeDef {
            name: local_name.to_string(),
            type_id: shared.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: shared.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: shared.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Shared,
            fields: layout_resolver_fields(&shared.fields),
            capabilities: shared.capabilities.clone(),
            claim_output: None,
            flow_states: None,
            flow_state_field: None,
            flow_rules: Vec::new(),
            identity: lower_identity_policy_ast(&shared.identity),
        }),
        TypeDef::Receipt(receipt) => Some(IrTypeDef {
            name: local_name.to_string(),
            type_id: receipt.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: receipt.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: receipt.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Receipt,
            fields: layout_resolver_fields(&receipt.fields),
            capabilities: receipt.capabilities.clone(),
            claim_output: receipt.claim_output.as_ref().map(ast_type_to_ir_type),
            flow_states: None,
            flow_state_field: None,
            flow_rules: Vec::new(),
            identity: lower_identity_policy_ast(&receipt.identity),
        }),
        TypeDef::Struct(struct_def) => Some(IrTypeDef {
            name: local_name.to_string(),
            type_id: struct_def.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: struct_def.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: struct_def.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Struct,
            fields: layout_resolver_fields(&struct_def.fields),
            capabilities: Vec::new(),
            claim_output: None,
            flow_states: None,
            flow_state_field: None,
            flow_rules: Vec::new(),
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

fn layout_resolver_fields(fields: &[Field]) -> Vec<IrField> {
    let mut next_offset = Some(0usize);
    fields
        .iter()
        .map(|field| {
            let ty = ast_type_to_ir_type(&field.ty);
            let fixed_size = fixed_encoded_size_for_ir_type(&ty);
            let offset = next_offset.unwrap_or(0);
            next_offset = next_offset.and_then(|current| fixed_size.and_then(|size| current.checked_add(size)));
            IrField { name: field.name.clone(), ty, offset, fixed_size }
        })
        .collect()
}

fn fixed_encoded_size_for_ir_type(ty: &IrType) -> Option<usize> {
    match ty {
        IrType::U8 | IrType::Bool => Some(1),
        IrType::U16 => Some(2),
        IrType::U32 => Some(4),
        IrType::U64 => Some(8),
        IrType::U128 => Some(16),
        IrType::Address | IrType::Hash => Some(32),
        IrType::Array(inner, len) => fixed_encoded_size_for_ir_type(inner).and_then(|inner_size| inner_size.checked_mul(*len)),
        IrType::Tuple(items) => {
            items.iter().try_fold(0usize, |acc, item| fixed_encoded_size_for_ir_type(item).and_then(|size| acc.checked_add(size)))
        }
        IrType::Unit => Some(0),
        IrType::Named(_) | IrType::Ref(_) | IrType::MutRef(_) => None,
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
        TypeDef::Receipt(receipt) => Some(receipt.claim_output.as_ref().map(ast_type_to_ir_type)),
        TypeDef::Resource(_) | TypeDef::Shared(_) | TypeDef::Struct(_) | TypeDef::Enum(_) => None,
    }
}

fn resolver_flow_states_to_ir(type_def: &TypeDef) -> Option<Vec<String>> {
    match type_def {
        TypeDef::Resource(_) | TypeDef::Shared(_) | TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        TypeDef::Receipt(_) => None,
    }
}

fn ast_type_to_ir_type(ty: &Type) -> IrType {
    match ty {
        Type::U8 => IrType::U8,
        Type::U16 => IrType::U16,
        Type::U32 => IrType::U32,
        Type::U64 => IrType::U64,
        Type::U128 => IrType::U128,
        Type::Bool => IrType::Bool,
        Type::Unit => IrType::Unit,
        Type::Address => IrType::Address,
        Type::Hash => IrType::Hash,
        Type::Array(inner, size) => IrType::Array(Box::new(ast_type_to_ir_type(inner)), *size),
        Type::Tuple(items) => IrType::Tuple(items.iter().map(ast_type_to_ir_type).collect()),
        Type::Named(name) if name == "usize" || name == "isize" => IrType::U64,
        Type::Named(name) => IrType::Named(name.clone()),
        Type::Ref(inner) => IrType::Ref(Box::new(ast_type_to_ir_type(inner))),
        Type::MutRef(inner) => IrType::MutRef(Box::new(ast_type_to_ir_type(inner))),
    }
}

fn action_core_input_binding_names(action: &ActionDef) -> HashSet<String> {
    action_inferred_lineage_bindings(action).keys().cloned().collect()
}

fn action_inferred_lineage_bindings(action: &ActionDef) -> HashMap<String, String> {
    let mut bindings = HashMap::new();
    let consumed = action_consumed_bindings(action);
    for state_edge in &action.state_edges {
        if consumed.contains(&state_edge.path.base) {
            continue;
        }
        bindings.entry(state_edge.path.base.clone()).or_insert_with(|| state_edge.to_path.base.clone());
    }

    let mut outputs_by_type: HashMap<String, Vec<String>> = HashMap::new();
    for (name, type_name) in action_output_binding_types(action) {
        if bindings.values().any(|bound_output| bound_output == &name) {
            continue;
        }
        outputs_by_type.entry(type_name).or_default().push(name);
    }

    let mut inputs_by_type: HashMap<String, Vec<String>> = HashMap::new();
    for param in &action.params {
        if consumed.contains(&param.name) || bindings.contains_key(&param.name) {
            continue;
        }
        let Some(type_name) = ast_named_cell_type_name(&param.ty) else {
            continue;
        };
        if param.source != ParamSource::Default && param.source != ParamSource::Input {
            continue;
        }
        if param.is_ref || param.is_mut || param.is_read_ref {
            continue;
        }
        inputs_by_type.entry(type_name.to_string()).or_default().push(param.name.clone());
    }

    for (type_name, inputs) in inputs_by_type {
        let Some(outputs) = outputs_by_type.get(&type_name) else {
            continue;
        };
        if inputs.len() == 1 && outputs.len() == 1 {
            bindings.insert(inputs[0].clone(), outputs[0].clone());
        }
    }

    bindings
}

fn action_output_binding_types(action: &ActionDef) -> HashMap<String, String> {
    let mut bindings = HashMap::new();
    for output in &action.outputs {
        if let Some(type_name) = ast_named_cell_type_name(&output.ty) {
            bindings.insert(output.name.clone(), type_name.to_string());
        }
    }
    bindings
}

fn ast_named_cell_type_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Named(name) => Some(name.split('<').next().unwrap_or(name.as_str())),
        _ => None,
    }
}

fn action_consumed_bindings(action: &ActionDef) -> HashSet<String> {
    let mut bindings = HashSet::new();
    collect_consumed_bindings_from_stmts(&action.body, &mut bindings);
    bindings
}

fn collect_consumed_bindings_from_stmts(stmts: &[Stmt], bindings: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Let(let_stmt) => collect_consumed_bindings_from_expr(&let_stmt.value, bindings),
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => collect_consumed_bindings_from_expr(expr, bindings),
            Stmt::Return(None) => {}
            Stmt::If(if_stmt) => {
                collect_consumed_bindings_from_expr(&if_stmt.condition, bindings);
                collect_consumed_bindings_from_stmts(&if_stmt.then_branch, bindings);
                if let Some(else_branch) = &if_stmt.else_branch {
                    collect_consumed_bindings_from_stmts(else_branch, bindings);
                }
            }
            Stmt::For(for_stmt) => {
                collect_consumed_bindings_from_expr(&for_stmt.iterable, bindings);
                collect_consumed_bindings_from_stmts(&for_stmt.body, bindings);
            }
            Stmt::While(while_stmt) => {
                collect_consumed_bindings_from_expr(&while_stmt.condition, bindings);
                collect_consumed_bindings_from_stmts(&while_stmt.body, bindings);
            }
        }
    }
}

fn collect_consumed_bindings_from_expr(expr: &Expr, bindings: &mut HashSet<String>) {
    match expr {
        Expr::Consume(consume) => {
            if let Expr::Identifier(name) = consume.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&consume.expr, bindings);
        }
        Expr::Assign(assign) => {
            collect_consumed_bindings_from_expr(&assign.target, bindings);
            collect_consumed_bindings_from_expr(&assign.value, bindings);
        }
        Expr::Binary(binary) => {
            collect_consumed_bindings_from_expr(&binary.left, bindings);
            collect_consumed_bindings_from_expr(&binary.right, bindings);
        }
        Expr::Unary(unary) => collect_consumed_bindings_from_expr(&unary.expr, bindings),
        Expr::Call(call) => {
            collect_consumed_bindings_from_expr(&call.func, bindings);
            for arg in &call.args {
                collect_consumed_bindings_from_expr(arg, bindings);
            }
        }
        Expr::FieldAccess(field) => collect_consumed_bindings_from_expr(&field.expr, bindings),
        Expr::Index(index) => {
            collect_consumed_bindings_from_expr(&index.expr, bindings);
            collect_consumed_bindings_from_expr(&index.index, bindings);
        }
        Expr::Create(create) => {
            for (_, value) in &create.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
            if let Some(lock) = &create.lock {
                collect_consumed_bindings_from_expr(lock, bindings);
            }
        }
        Expr::Destroy(destroy) => {
            if let Expr::Identifier(name) = destroy.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&destroy.expr, bindings);
        }
        Expr::Transfer(transfer) => {
            if let Expr::Identifier(name) = transfer.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&transfer.expr, bindings);
            collect_consumed_bindings_from_expr(&transfer.to, bindings);
        }
        Expr::Claim(claim) => {
            if let Expr::Identifier(name) = claim.receipt.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&claim.receipt, bindings);
        }
        Expr::Settle(settle) => {
            if let Expr::Identifier(name) = settle.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&settle.expr, bindings);
        }
        Expr::CreateUnique(create) => {
            for (_, value) in &create.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
            if let Some(lock) = &create.lock {
                collect_consumed_bindings_from_expr(lock, bindings);
            }
        }
        Expr::ReplaceUnique(replace) => {
            if let Expr::Identifier(name) = replace.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&replace.expr, bindings);
            for (_, value) in &replace.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
        }
        Expr::ReadRef(_) => {}
        Expr::Assert(assert_expr) => {
            collect_consumed_bindings_from_expr(&assert_expr.condition, bindings);
            collect_consumed_bindings_from_expr(&assert_expr.message, bindings);
        }
        Expr::Require(require_expr) => {
            collect_consumed_bindings_from_expr(&require_expr.condition, bindings);
            if let Some(message) = &require_expr.message {
                collect_consumed_bindings_from_expr(message, bindings);
            }
        }
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                collect_consumed_bindings_from_expr(expr, bindings);
            }
        }
        Expr::Preserve(_) => {}
        Expr::Block(stmts) => collect_consumed_bindings_from_stmts(stmts, bindings),
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                collect_consumed_bindings_from_expr(item, bindings);
            }
        }
        Expr::If(if_expr) => {
            collect_consumed_bindings_from_expr(&if_expr.condition, bindings);
            collect_consumed_bindings_from_expr(&if_expr.then_branch, bindings);
            collect_consumed_bindings_from_expr(&if_expr.else_branch, bindings);
        }
        Expr::Cast(cast) => collect_consumed_bindings_from_expr(&cast.expr, bindings),
        Expr::Range(range) => {
            collect_consumed_bindings_from_expr(&range.start, bindings);
            collect_consumed_bindings_from_expr(&range.end, bindings);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
        }
        Expr::Match(match_expr) => {
            collect_consumed_bindings_from_expr(&match_expr.expr, bindings);
            for arm in &match_expr.arms {
                collect_consumed_bindings_from_expr(&arm.value, bindings);
            }
        }
        Expr::StdlibCall(call) => {
            if let Some(name) = stdlib_lifecycle_consumed_binding(call) {
                bindings.insert(name);
            }
        }
        Expr::Identifier(_) | Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) => {}
    }
}

fn stdlib_lifecycle_consumed_binding(call: &StdlibCallExpr) -> Option<String> {
    let qualified = format!("std::{}::{}", call.namespace, call.name);
    match qualified.as_str() {
        "std::lifecycle::transfer" | "std::receipt::claim" | "std::lifecycle::settle" => match call.args.first() {
            Some(Expr::Identifier(name)) => Some(name.clone()),
            _ => None,
        },
        _ => None,
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
    matches!(ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64 | IrType::Address | IrType::Hash)
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
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn parse_and_lower(source: &str) -> IrModule {
        let tokens = lex(source).unwrap();
        let ast = parse(&tokens).unwrap();
        crate::types::check(&ast).unwrap();
        crate::flow::check(&ast).unwrap();
        let ir = generate(&ast).unwrap();
        verify_module(&ir).unwrap();
        ir
    }

    fn parse_and_generate_without_typecheck(source: &str) -> Result<IrModule> {
        let tokens = lex(source).unwrap();
        let ast = parse(&tokens).unwrap();
        generate(&ast)
    }

    fn first_action_body_mut(ir: &mut IrModule) -> &mut IrBody {
        ir.items
            .iter_mut()
            .find_map(|item| match item {
                IrItem::Action(action) => Some(&mut action.body),
                _ => None,
            })
            .expect("expected action body")
    }

    fn raw_status_var(id: usize) -> IrVar {
        IrVar { id, name: format!("raw_status_{id}"), ty: IrType::U64 }
    }

    fn module_with_instructions(instructions: Vec<IrInstruction>, return_type: Option<IrType>, terminator: IrTerminator) -> IrModule {
        IrModule {
            name: "ir::raw_syscall_boundary".to_string(),
            items: vec![IrItem::PureFn(IrPureFn {
                name: "bad".to_string(),
                params: Vec::new(),
                return_type,
                body: IrBody {
                    consume_set: Vec::new(),
                    read_refs: Vec::new(),
                    create_set: Vec::new(),
                    mutate_set: Vec::new(),
                    write_intents: Vec::new(),
                    blocks: vec![IrBlock { id: BlockId(0), instructions, terminator }],
                },
            })],
            external_type_defs: Vec::new(),
            external_callable_abis: Vec::new(),
            enum_fixed_sizes: HashMap::new(),
        }
    }

    fn raw_status_call(dest: Option<IrVar>) -> IrInstruction {
        IrInstruction::Call { dest, func: "__syscall_current_cycles".to_string(), args: Vec::new() }
    }

    fn raw_unit_helper_call(dest: Option<IrVar>) -> IrInstruction {
        IrInstruction::Call { dest, func: "__ckb_close".to_string(), args: vec![IrOperand::Const(IrConst::U64(0))] }
    }

    #[test]
    fn ir_verifier_rejects_raw_syscall_status_returned_as_domain_u64() {
        let status = raw_status_var(0);
        let ir = module_with_instructions(
            vec![raw_status_call(Some(status.clone()))],
            Some(IrType::U64),
            IrTerminator::Return(Some(IrOperand::Var(status))),
        );

        let err = verify_module(&ir).unwrap_err();

        assert!(err.message.contains("returns SyscallStatus as an ordinary value"), "{}", err.message);
    }

    #[test]
    fn ir_verifier_rejects_raw_syscall_status_stored_as_dsl_local() {
        let status = raw_status_var(0);
        let ir = module_with_instructions(
            vec![
                raw_status_call(Some(status.clone())),
                IrInstruction::StoreVar { name: "cycles".to_string(), src: IrOperand::Var(status) },
            ],
            None,
            IrTerminator::Return(None),
        );

        let err = verify_module(&ir).unwrap_err();

        assert!(err.message.contains("stores SyscallStatus into DSL local 'cycles'"), "{}", err.message);
    }

    #[test]
    fn ir_verifier_rejects_dropped_raw_syscall_status() {
        let ir = module_with_instructions(vec![raw_status_call(None)], None, IrTerminator::Return(None));

        let err = verify_module(&ir).unwrap_err();

        assert!(err.message.contains("drops raw SyscallStatus without a typed runtime boundary"), "{}", err.message);
    }

    #[test]
    fn ir_verifier_rejects_raw_syscall_status_in_tuple_field() {
        let status = raw_status_var(0);
        let tuple = IrVar { id: 1, name: "tuple".to_string(), ty: IrType::Tuple(vec![IrType::U64]) };
        let ir = module_with_instructions(
            vec![raw_status_call(Some(status.clone())), IrInstruction::Tuple { dest: tuple, fields: vec![IrOperand::Var(status)] }],
            None,
            IrTerminator::Return(None),
        );

        let err = verify_module(&ir).unwrap_err();

        assert!(err.message.contains("stores SyscallStatus into tuple field 0"), "{}", err.message);
    }

    #[test]
    fn ir_verifier_rejects_raw_syscall_status_as_domain_call_argument() {
        let status = raw_status_var(0);
        let ir = module_with_instructions(
            vec![
                raw_status_call(Some(status.clone())),
                IrInstruction::Call { dest: None, func: "domain_sink".to_string(), args: vec![IrOperand::Var(status)] },
            ],
            None,
            IrTerminator::Return(None),
        );
        let mut ir = ir;
        ir.external_callable_abis.push(IrCallableAbi {
            name: "domain_sink".to_string(),
            params: vec![IrParam {
                name: "value".to_string(),
                ty: IrType::U64,
                is_mut: false,
                is_ref: false,
                is_read_ref: false,
                source: ParamSource::Default,
                binding: IrVar { id: 99, name: "value".to_string(), ty: IrType::U64 },
            }],
            type_hash_param_indices: BTreeSet::new(),
        });

        let err = verify_module(&ir).unwrap_err();

        assert!(err.message.contains("passes SyscallStatus as argument 0 to 'domain_sink'"), "{}", err.message);
    }

    #[test]
    fn ir_verifier_rejects_raw_syscall_status_produced_without_checked_consumer() {
        let status = raw_status_var(0);
        let ir = module_with_instructions(vec![raw_status_call(Some(status))], None, IrTerminator::Return(None));

        let err = verify_module(&ir).unwrap_err();

        assert!(err.message.contains("produces SyscallStatus in var0 without a checked fail-closed consumer"), "{}", err.message);
    }

    #[test]
    fn ir_verifier_rejects_unit_runtime_helper_status_stored_as_domain_u64() {
        let status = raw_status_var(0);
        let ir = module_with_instructions(vec![raw_unit_helper_call(Some(status))], None, IrTerminator::Return(None));

        let err = verify_module(&ir).unwrap_err();

        assert!(err.message.contains("call '__ckb_close' stores a unit helper status as a DSL value"), "{}", err.message);
    }

    #[test]
    fn ir_verifier_allows_unit_runtime_helper_when_status_is_checked_by_codegen_boundary() {
        let ir = module_with_instructions(vec![raw_unit_helper_call(None)], None, IrTerminator::Return(None));

        verify_module(&ir).expect("unit helper statement should be checked by codegen boundary");
    }

    #[test]
    fn ir_verifier_allows_domain_u64_return_tuple_and_call_argument() {
        let value = IrVar { id: 0, name: "value".to_string(), ty: IrType::U64 };
        let tuple = IrVar { id: 1, name: "tuple".to_string(), ty: IrType::Tuple(vec![IrType::U64]) };
        let ir = module_with_instructions(
            vec![
                IrInstruction::LoadConst { dest: value.clone(), value: IrConst::U64(42) },
                IrInstruction::Tuple { dest: tuple, fields: vec![IrOperand::Var(value.clone())] },
                IrInstruction::Call { dest: None, func: "domain_sink".to_string(), args: vec![IrOperand::Var(value.clone())] },
            ],
            Some(IrType::U64),
            IrTerminator::Return(Some(IrOperand::Var(value))),
        );
        let mut ir = ir;
        ir.external_callable_abis.push(IrCallableAbi {
            name: "domain_sink".to_string(),
            params: vec![IrParam {
                name: "value".to_string(),
                ty: IrType::U64,
                is_mut: false,
                is_ref: false,
                is_read_ref: false,
                source: ParamSource::Default,
                binding: IrVar { id: 99, name: "value".to_string(), ty: IrType::U64 },
            }],
            type_hash_param_indices: BTreeSet::new(),
        });

        verify_module(&ir).unwrap();
    }

    #[test]
    fn ir_straight_line_lifecycle_certificate_rejects_branch_local_create_without_typecheck() {
        let err = parse_and_generate_without_typecheck(
            r#"
module ir::branch_create_certificate

resource Token has store {
    amount: u64
}

action mint(flag: bool) -> Token
where
    if flag {
        return create Token { amount: 1 }
    } else {
        return create Token { amount: 2 }
    }
"#,
        )
        .unwrap_err();

        assert!(err.message.contains("straight-line IR lifecycle collectors"), "{}", err.message);
    }

    #[test]
    fn ir_straight_line_lifecycle_certificate_rejects_duplicate_consume_without_typecheck() {
        let err = parse_and_generate_without_typecheck(
            r#"
module ir::duplicate_consume_certificate

resource Token has store {
    amount: u64
}

action burn(token: Token) -> bool
where
    consume token
    consume token
    return true
"#,
        )
        .unwrap_err();

        assert!(err.message.contains("duplicate lifecycle binding 'input:token'"), "{}", err.message);
        assert!(err.message.contains("straight-line IR lifecycle collectors"), "{}", err.message);
    }

    #[test]
    fn strict_audit_ir_verifier_rejects_missing_terminator_target() {
        let mut ir = parse_and_lower(
            r#"
module ir::bad_target

action run(x: u64) -> u64
where
    return x
"#,
        );
        first_action_body_mut(&mut ir).blocks[0].terminator = IrTerminator::Jump(BlockId(999));

        let err = verify_module(&ir).unwrap_err();
        assert!(err.message.contains("terminates to missing block 999"), "unexpected verifier error: {}", err.message);
    }

    #[test]
    fn strict_audit_ir_verifier_rejects_use_not_defined_on_all_paths() {
        let cond = IrVar { id: 0, name: "cond".to_string(), ty: IrType::Bool };
        let branch_value = IrVar { id: 1, name: "branch_value".to_string(), ty: IrType::U64 };
        let body = IrBody {
            consume_set: Vec::new(),
            read_refs: Vec::new(),
            create_set: Vec::new(),
            mutate_set: Vec::new(),
            write_intents: Vec::new(),
            blocks: vec![
                IrBlock {
                    id: BlockId(0),
                    instructions: Vec::new(),
                    terminator: IrTerminator::Branch {
                        cond: IrOperand::Var(cond.clone()),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
                },
                IrBlock {
                    id: BlockId(1),
                    instructions: vec![IrInstruction::LoadConst { dest: branch_value.clone(), value: IrConst::U64(7) }],
                    terminator: IrTerminator::Jump(BlockId(3)),
                },
                IrBlock { id: BlockId(2), instructions: Vec::new(), terminator: IrTerminator::Jump(BlockId(3)) },
                IrBlock {
                    id: BlockId(3),
                    instructions: Vec::new(),
                    terminator: IrTerminator::Return(Some(IrOperand::Var(branch_value))),
                },
            ],
        };
        let module = IrModule {
            name: "ir::audit".to_string(),
            items: vec![IrItem::Action(IrAction {
                name: "run".to_string(),
                params: vec![IrParam {
                    name: "cond".to_string(),
                    ty: IrType::Bool,
                    is_mut: false,
                    is_ref: false,
                    is_read_ref: false,
                    source: ParamSource::Default,
                    binding: cond,
                }],
                return_type: Some(IrType::U64),
                state_transition_edges: Vec::new(),
                body,
                effect_class: EffectClass::Pure,
                scheduler_hints: SchedulerHints::default(),
            })],
            external_type_defs: Vec::new(),
            external_callable_abis: Vec::new(),
            enum_fixed_sizes: HashMap::new(),
        };

        let err = verify_module(&module).unwrap_err();
        assert!(
            err.message.contains("uses var1 before it is defined on all incoming paths"),
            "unexpected verifier error: {}",
            err.message
        );
    }

    #[test]
    fn strict_audit_schema_field_accesses_are_rematerialized_per_cfg_path() {
        let ir = parse_and_lower(
            r#"
module ir::schema_field_paths

struct Grant {
    amount: u64
}

action claim(grant: Grant, flag: bool) -> u64
where
    let vested = if flag { grant.amount } else { grant.amount * 2 }
    return vested
"#,
        );

        let field_access_count = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(action) if action.name == "claim" => Some(
                    action
                        .body
                        .blocks
                        .iter()
                        .flat_map(|block| &block.instructions)
                        .filter(|instruction| matches!(instruction, IrInstruction::FieldAccess { field, .. } if field == "amount"))
                        .count(),
                ),
                _ => None,
            })
            .expect("expected claim action");
        assert!(field_access_count >= 2, "schema field must be materialized independently in both if-expression paths");
    }

    #[test]
    fn binary_arithmetic_result_type_preserves_left_operand_width() {
        let ir = parse_and_lower(
            r#"
module ir::narrow_arithmetic

action check(x: u8, y: u8) -> u8
where
    let z: u8 = x + y
    return z
"#,
        );

        let binary_dest = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(action) => action.body.blocks.iter().flat_map(|block| &block.instructions).find_map(|instruction| {
                    if let IrInstruction::Binary { dest, op: BinaryOp::Add, .. } = instruction {
                        Some(dest)
                    } else {
                        None
                    }
                }),
                _ => None,
            })
            .expect("expected lowered add instruction");

        assert_eq!(binary_dest.ty, IrType::U8);
    }

    #[test]
    fn contextual_integer_binary_operands_lower_to_peer_width() {
        let ir = parse_and_lower(
            r#"
module ir::contextual_integer_binary

action check(x: u8) -> u8
where
    return x + 1
"#,
        );

        let right = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(action) => action.body.blocks.iter().flat_map(|block| &block.instructions).find_map(|instruction| {
                    if let IrInstruction::Binary { op: BinaryOp::Add, right, .. } = instruction {
                        Some(right)
                    } else {
                        None
                    }
                }),
                _ => None,
            })
            .expect("expected add right operand");

        assert!(matches!(right, IrOperand::Const(IrConst::U8(1))));
    }

    #[test]
    fn mixed_width_expression_local_widening_lowers_as_explicit_casts() {
        let ir = parse_and_lower(
            r#"
module ir::mixed_width_widening

action check(a: u64, b: u16) -> bool
where
    let product: u64 = a * b
    let ordered: bool = b < a
    return b == a
"#,
        );

        let instructions = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(action) => {
                    Some(action.body.blocks.iter().flat_map(|block| block.instructions.iter()).collect::<Vec<_>>())
                }
                _ => None,
            })
            .expect("expected lowered action");

        let cast_count = instructions
            .iter()
            .filter(|instruction| matches!(instruction, IrInstruction::Cast { dest, src: IrOperand::Var(src) } if dest.ty == IrType::U64 && src.ty == IrType::U16))
            .count();
        assert!(cast_count >= 3, "expected arithmetic, ordering, and equality widening casts, got {cast_count}: {instructions:#?}");

        assert!(instructions.iter().any(|instruction| {
            matches!(
                instruction,
                IrInstruction::Binary {
                    dest,
                    op: BinaryOp::Mul,
                    left,
                    right,
                } if dest.ty == IrType::U64
                    && matches!(left, IrOperand::Var(var) if var.ty == IrType::U64)
                    && matches!(right, IrOperand::Var(var) if var.ty == IrType::U64)
            )
        }));
        assert!(instructions.iter().any(|instruction| {
            matches!(
                instruction,
                IrInstruction::Binary {
                    dest,
                    op: BinaryOp::Lt,
                    left,
                    right,
                } if dest.ty == IrType::Bool
                    && matches!(left, IrOperand::Var(var) if var.ty == IrType::U64)
                    && matches!(right, IrOperand::Var(var) if var.ty == IrType::U64)
            )
        }));
        assert!(instructions.iter().any(|instruction| {
            matches!(
                instruction,
                IrInstruction::Binary {
                    dest,
                    op: BinaryOp::Eq,
                    left,
                    right,
                } if dest.ty == IrType::Bool
                    && matches!(left, IrOperand::Var(var) if var.ty == IrType::U64)
                    && matches!(right, IrOperand::Var(var) if var.ty == IrType::U64)
            )
        }));
    }

    #[test]
    fn runtime_narrowing_cast_lowers_as_cast_instruction() {
        let ir = parse_and_lower(
            r#"
module ir::runtime_cast

action check(x: u64) -> u8
where
    return x as u8
"#,
        );

        let cast_dest = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(action) => action.body.blocks.iter().flat_map(|block| &block.instructions).find_map(|instruction| {
                    if let IrInstruction::Cast { dest, .. } = instruction {
                        Some(dest)
                    } else {
                        None
                    }
                }),
                _ => None,
            })
            .expect("expected cast instruction");

        assert_eq!(cast_dest.ty, IrType::U8);
    }

    #[test]
    fn constant_cast_rejects_out_of_range_u128_narrowing() {
        assert!(matches!(IrGenerator::cast_const(&IrConst::U128(u128::from(u64::MAX)), &IrType::U64), Some(IrConst::U64(u64::MAX))));
        assert!(IrGenerator::cast_const(&IrConst::U128(u128::from(u64::MAX) + 1), &IrType::U64).is_none());
        assert!(matches!(IrGenerator::cast_const(&IrConst::U128(255), &IrType::U8), Some(IrConst::U8(255))));
        assert!(IrGenerator::cast_const(&IrConst::U128(256), &IrType::U8).is_none());
        assert!(matches!(IrGenerator::cast_const(&IrConst::U128(1), &IrType::Bool), Some(IrConst::Bool(true))));
        assert!(IrGenerator::cast_const(&IrConst::U128(2), &IrType::Bool).is_none());
    }

    #[test]
    fn preserve_sugar_populates_preserved_fields() {
        let source = r#"
module test

resource Offer has store {
    seller: u64
    price: u64
    payment_symbol: u64
    state: u8
}

flow Offer.state {
    Live -> Filled;
}

action fill(input: Offer) -> (output: Offer)
    transition input.state: Live -> output.state: Filled
where
    preserve output from input {
        seller
        price
    }
    require output.payment_symbol == input.payment_symbol
"#;
        let ir = parse_and_lower(source);
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(a) if a.name == "fill" => Some(a.clone()),
                _ => None,
            })
            .expect("expected fill action");
        // Find the mutate for the output binding
        let mutate = action.body.mutate_set.iter().find(|m| m.binding == "output");
        if let Some(mutate) = mutate {
            assert!(
                mutate.preserved_fields.contains(&"seller".to_string()),
                "preserved_fields should contain 'seller', got {:?}",
                mutate.preserved_fields
            );
            assert!(
                mutate.preserved_fields.contains(&"price".to_string()),
                "preserved_fields should contain 'price', got {:?}",
                mutate.preserved_fields
            );
        }
    }

    #[test]
    fn require_block_lowers_to_atomic_requires() {
        let source = r#"
module test

action check(x: u64, y: u64) -> u64
where
    require {
        x > 0
        y > 0
    }
    return x + y
"#;
        let ir = parse_and_lower(source);
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(a) if a.name == "check" => Some(a.clone()),
                _ => None,
            })
            .expect("expected check action");
        // The require block should produce multiple basic blocks due to conditional branching.
        // Each require produces a Branch terminator (cond ? ok : fail) in the IR CFG.
        // With 2 require expressions + return, we expect at least 5 blocks
        // (entry, require1_ok, require2_ok, return_block, fail_block).
        assert!(
            action.body.blocks.len() >= 3,
            "expected at least 3 basic blocks from 2 require expressions, found {} blocks",
            action.body.blocks.len()
        );
        // Count the Branch terminators which represent require conditionals
        let branch_count = action.body.blocks.iter().filter(|b| matches!(b.terminator, IrTerminator::Branch { .. })).count();
        assert!(
            branch_count >= 2,
            "expected at least 2 branch terminators from require block, found {} out of {} blocks",
            branch_count,
            action.body.blocks.len()
        );
    }

    #[test]
    fn assert_in_pure_function_lowers_failure_to_abort_terminator() {
        let ir = parse_and_lower(
            r#"
module test

fn checked(value: u64) -> u64 {
    assert_invariant(value > 0, "positive")
    return value
}

action run(value: u64) -> u64
where
    checked(value)
"#,
        );
        let function = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::PureFn(function) if function.name == "checked" => Some(function),
                _ => None,
            })
            .expect("expected checked function");

        assert!(
            function
                .body
                .blocks
                .iter()
                .any(|block| matches!(block.terminator, IrTerminator::Abort(CellScriptRuntimeError::AssertionFailed))),
            "assert failure must lower to Abort, blocks: {:?}",
            function.body.blocks
        );
        assert!(
            !function.body.blocks.iter().any(|block| {
                matches!(
                    block.terminator,
                    IrTerminator::Return(Some(IrOperand::Const(IrConst::U64(code))))
                        if code == CellScriptRuntimeError::AssertionFailed.code()
                )
            }),
            "assert failure must not lower to an ordinary u64 return: {:?}",
            function.body.blocks
        );
    }

    #[test]
    fn exhaustive_enum_match_unmatched_path_lowers_to_abort_terminator() {
        let ir = parse_and_lower(
            r#"
module test

enum Flag {
    On,
    Off,
}

action select(flag: Flag) -> u64
where
    return match flag {
        Flag::On => { 1 }
        Flag::Off => { 2 }
    }
"#,
        );
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(action) if action.name == "select" => Some(action),
                _ => None,
            })
            .expect("expected select action");

        assert!(
            action.body.blocks.iter().any(|block| {
                matches!(block.terminator, IrTerminator::Abort(CellScriptRuntimeError::NumericOrDiscriminantInvalid))
            }),
            "unmatched match path must lower to Abort, blocks: {:?}",
            action.body.blocks
        );
        assert!(
            !action.body.blocks.iter().any(|block| {
                matches!(
                    block.terminator,
                    IrTerminator::Return(Some(IrOperand::Const(IrConst::U64(code))))
                        if code == CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()
                )
            }),
            "unmatched match path must not lower to an ordinary u64 return: {:?}",
            action.body.blocks
        );
    }

    #[test]
    fn stdlib_transfer_lowers_to_single_consumed_input_and_locked_output() {
        let source = r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

action transfer_only(coin: Coin, to: Address) -> next_coin: Coin
where
    std::lifecycle::transfer(coin, next_coin, to) { amount }
"#;
        let ir = parse_and_lower(source);
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(a) if a.name == "transfer_only" => Some(a),
                _ => None,
            })
            .expect("expected transfer_only action");

        assert_eq!(action.body.consume_set.len(), 1, "stdlib transfer should not also infer an input lineage consume");
        assert_eq!(action.body.consume_set[0].operation, "consume");
        assert_eq!(action.body.consume_set[0].binding, "coin");

        assert_eq!(action.body.create_set.len(), 1);
        let output = &action.body.create_set[0];
        assert_eq!(output.operation, "output");
        assert_eq!(output.binding, "next_coin");
        assert!(output.lock.is_some(), "stdlib transfer must bind the output lock target");
        assert!(output.fields.iter().any(|(field, _)| field == "amount"));
    }

    #[test]
    fn stdlib_claim_lowers_to_consumed_receipt_and_locked_declared_output() {
        let source = r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

receipt Voucher -> Coin has destroy {
    amount: u64
    holder: Address
}

action claim_only(voucher: Voucher) -> coin: Coin
where
    std::receipt::claim(voucher, coin, voucher.holder) { amount }
"#;
        let ir = parse_and_lower(source);
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(a) if a.name == "claim_only" => Some(a),
                _ => None,
            })
            .expect("expected claim_only action");

        assert_eq!(action.body.consume_set.len(), 1, "stdlib claim should consume exactly the receipt");
        assert_eq!(action.body.consume_set[0].operation, "consume");
        assert_eq!(action.body.consume_set[0].binding, "voucher");

        assert_eq!(action.body.create_set.len(), 1);
        let output = &action.body.create_set[0];
        assert_eq!(output.operation, "output");
        assert_eq!(output.binding, "coin");
        assert!(output.lock.is_some(), "stdlib claim must bind the explicit output lock target");
        assert!(output.fields.iter().any(|(field, _)| field == "amount"));
    }

    #[test]
    fn stdlib_settle_lowers_to_consumed_input_and_locked_output() {
        let source = r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
    owner: Address
}

action settle_coin(coin: Coin) -> next_coin: Coin
where
    std::lifecycle::settle(coin, next_coin, coin.owner) {
        amount
        owner
    }
"#;
        let ir = parse_and_lower(source);
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Action(a) if a.name == "settle_coin" => Some(a),
                _ => None,
            })
            .expect("expected settle_coin action");

        assert_eq!(action.body.consume_set.len(), 1, "stdlib settle should consume exactly the input");
        assert_eq!(action.body.consume_set[0].operation, "consume");
        assert_eq!(action.body.consume_set[0].binding, "coin");

        assert_eq!(action.body.create_set.len(), 1);
        let output = &action.body.create_set[0];
        assert_eq!(output.operation, "output");
        assert_eq!(output.binding, "next_coin");
        assert!(output.lock.is_some(), "stdlib settle must bind the explicit output lock target");
        assert!(output.fields.iter().any(|(field, _)| field == "amount"));
        assert!(output.fields.iter().any(|(field, _)| field == "owner"));
    }
}
