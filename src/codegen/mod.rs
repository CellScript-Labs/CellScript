use crate::ast::{BinaryOp, ParamSource, UnaryOp};
use crate::error::{CompileError, Result};
use crate::flow::FLOW_STATE_FIELD_NAME;
use crate::ir::*;
use crate::runtime_errors::CellScriptRuntimeError;
use crate::{ArtifactFormat, TargetProfile};
use std::collections::{BTreeSet, HashMap};

const CKB_LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER: u64 = 2082;
const CKB_LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER: u64 = 2083;
const CKB_LOAD_WITNESS_SYSCALL_NUMBER: u64 = 2074;
const CKB_LOAD_SCRIPT_SYSCALL_NUMBER: u64 = 2052;
const CKB_LOAD_CELL_BY_FIELD_SYSCALL_NUMBER: u64 = 2081;
const CKB_LOAD_CELL_DATA_SYSCALL_NUMBER: u64 = 2092;
const CKB_HEADER_FIELD_EPOCH_NUMBER: u64 = 0;
const CKB_HEADER_FIELD_EPOCH_START_BLOCK_NUMBER: u64 = 1;
const CKB_HEADER_FIELD_EPOCH_LENGTH: u64 = 2;
const CKB_INPUT_FIELD_SINCE: u64 = 1;
const CKB_SOURCE_INPUT: u64 = 0x01;
const CKB_SOURCE_OUTPUT: u64 = 0x02;
const CKB_SOURCE_CELL_DEP: u64 = 0x03;
const CKB_SOURCE_HEADER_DEP: u64 = 0x04;
const CKB_SOURCE_GROUP_FLAG: u64 = 0x0100_0000_0000_0000;
const CKB_SOURCE_GROUP_INPUT: u64 = CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT;
const CKB_CELL_FIELD_CAPACITY: u64 = 0;
const CKB_CELL_FIELD_LOCK_HASH: u64 = 3;
const CKB_CELL_FIELD_TYPE_HASH: u64 = 5;
const CKB_INDEX_OUT_OF_BOUND: u64 = 1;
const CKB_ITEM_MISSING: u64 = 2;
const RUNTIME_SCRATCH_BUFFER_SIZE: usize = 512;
const RUNTIME_SCRATCH_SLOT_SIZE: usize = 8 + RUNTIME_SCRATCH_BUFFER_SIZE;
const RUNTIME_SCRATCH_SIZE: usize = RUNTIME_SCRATCH_SLOT_SIZE * 2;
const RUNTIME_EXPR_TEMP_SLOTS: usize = 16;
const RUNTIME_EXPR_TEMP_SIZE: usize = RUNTIME_EXPR_TEMP_SLOTS * 8;
const RUNTIME_CELL_BUFFER_SIZE: usize = 512;
const RUNTIME_CELL_SLOT_SIZE: usize = 8 + RUNTIME_CELL_BUFFER_SIZE;
const RUNTIME_COLLECTION_BUFFER_SIZE: usize = 256;

#[derive(Debug, Clone)]
struct SchemaFieldLayout {
    index: usize,
    offset: usize,
    ty: IrType,
    fixed_size: Option<usize>,
    fixed_enum_size: Option<usize>,
}

#[derive(Debug, Clone)]
struct SchemaFieldValueSource {
    obj_var_id: usize,
    type_name: String,
    field: String,
    layout: SchemaFieldLayout,
}

#[derive(Debug, Clone)]
struct AggregatePointerSource {
    ty: IrType,
}

#[derive(Debug, Clone)]
enum ExpectedFixedByteSource {
    SchemaField(SchemaFieldValueSource),
    Const(Vec<u8>),
    StackSlot { var_id: usize, width: usize },
    PointerBytes { var_id: usize, width: usize },
    ParamBytes { var_id: usize, size_offset: usize, width: usize },
    LoadedBytes { var_id: usize, size_offset: usize, width: usize },
}

#[derive(Debug, Clone, Copy)]
enum SourcePointer {
    LoadedStackPointer { var_id: usize, offset: usize },
    StackAddress { offset: usize },
}

fn fixed_scalar_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    match (ty, fixed_size) {
        (IrType::Bool | IrType::U8, Some(1)) => Some(1),
        (IrType::U16, Some(2)) => Some(2),
        (IrType::U32, Some(4)) => Some(4),
        (IrType::U64, Some(8)) => Some(8),
        _ => None,
    }
}

fn identity_policy_label(identity: &IrIdentityPolicy) -> String {
    match identity {
        IrIdentityPolicy::None => "none".to_string(),
        IrIdentityPolicy::CkbTypeId => "ckb_type_id".to_string(),
        IrIdentityPolicy::Field(path) => format!("field({})", path),
        IrIdentityPolicy::ScriptArgs => "script_args".to_string(),
        IrIdentityPolicy::SingletonType => "singleton_type".to_string(),
    }
}

fn destruction_policy_label(policy: &IrDestructionPolicy) -> String {
    match policy {
        IrDestructionPolicy::Default => "default".to_string(),
        IrDestructionPolicy::SingletonType => "singleton_type".to_string(),
        IrDestructionPolicy::Unique { identity } => format!("unique({})", identity),
        IrDestructionPolicy::Instance { identity_field } => format!("instance({})", identity_field),
        IrDestructionPolicy::BurnAmount { field } => format!("burn_amount({})", field),
    }
}

fn destroy_policy_uses_output_absence_scan(policy: &IrDestructionPolicy) -> bool {
    match policy {
        IrDestructionPolicy::Default | IrDestructionPolicy::SingletonType => true,
        IrDestructionPolicy::Unique { identity } => matches!(identity.as_str(), "type_id" | "ckb_type_id"),
        IrDestructionPolicy::Instance { .. } | IrDestructionPolicy::BurnAmount { .. } => false,
    }
}

/// Fixed-width types that fit in a single RISC-V 64-bit register (≤8 bytes).
/// Used by transition formula verification which needs scalar add/sub.
pub(crate) fn fixed_register_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    let w = fixed_scalar_width(ty, fixed_size)?;
    (w <= 8).then_some(w)
}

fn fixed_byte_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    if let Some(width) = fixed_scalar_width(ty, fixed_size) {
        return Some(width);
    }
    match (ty, fixed_size) {
        (IrType::Address | IrType::Hash, Some(32)) => Some(32),
        (IrType::U128, Some(16)) => Some(16),
        (IrType::Array(inner, len), Some(size)) if matches!(inner.as_ref(), IrType::U8) && *len == size => Some(size),
        (IrType::Ref(inner) | IrType::MutRef(inner), _) => fixed_byte_width(inner, type_static_length(inner)),
        _ => None,
    }
}

fn molecule_vector_element_fixed_width(
    ty: &IrType,
    type_fixed_sizes: &HashMap<String, usize>,
    enum_fixed_sizes: &HashMap<String, usize>,
) -> Option<usize> {
    let IrType::Named(name) = ty else {
        return None;
    };
    if name == "String" {
        return Some(1);
    }
    let inner = name.strip_prefix("Vec<")?.strip_suffix('>')?;
    molecule_inline_type_fixed_width(inner, type_fixed_sizes, enum_fixed_sizes)
}

fn molecule_inline_type_fixed_width(
    ty: &str,
    type_fixed_sizes: &HashMap<String, usize>,
    enum_fixed_sizes: &HashMap<String, usize>,
) -> Option<usize> {
    match ty.trim() {
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" => Some(4),
        "u64" => Some(8),
        "u128" => Some(16),
        "Address" | "Hash" => Some(32),
        other => type_fixed_sizes.get(other).copied().or_else(|| enum_fixed_sizes.get(other).copied()),
    }
}

fn layout_fixed_scalar_width(layout: &SchemaFieldLayout) -> Option<usize> {
    fixed_scalar_width(&layout.ty, layout.fixed_size).or(layout.fixed_enum_size)
}

fn layout_fixed_byte_width(layout: &SchemaFieldLayout) -> Option<usize> {
    fixed_byte_width(&layout.ty, layout.fixed_size).or(layout.fixed_enum_size)
}

pub(crate) fn type_static_length(ty: &IrType) -> Option<usize> {
    match ty {
        IrType::Bool | IrType::U8 => Some(1),
        IrType::U16 => Some(2),
        IrType::U32 => Some(4),
        IrType::U64 => Some(8),
        IrType::U128 => Some(16),
        IrType::Address | IrType::Hash => Some(32),
        IrType::Array(inner, len) => type_static_length(inner).map(|inner_len| inner_len * len),
        IrType::Tuple(items) => items.iter().try_fold(0usize, |acc, item| type_static_length(item).map(|len| acc + len)),
        IrType::Unit => Some(0),
        IrType::Ref(inner) | IrType::MutRef(inner) => type_static_length(inner),
        IrType::Named(_) => None,
    }
}

fn operand_fixed_byte_width(operand: &IrOperand) -> Option<usize> {
    let ty = match operand {
        IrOperand::Const(IrConst::Address(_)) | IrOperand::Const(IrConst::Hash(_)) => return Some(32),
        IrOperand::Const(IrConst::Array(values)) => return Some(values.len()),
        IrOperand::Const(IrConst::U128(_)) => return Some(16),
        IrOperand::Var(var) => &var.ty,
        _ => return None,
    };
    match ty {
        IrType::Address | IrType::Hash => Some(32),
        IrType::U128 => Some(16),
        IrType::Array(inner, len) if matches!(inner.as_ref(), IrType::U8) => Some(*len),
        _ => None,
    }
}

fn constructed_byte_vector_part_width(operand: &IrOperand) -> Option<usize> {
    operand_fixed_byte_width(operand).or_else(|| match operand {
        IrOperand::Var(var) => fixed_scalar_width(&var.ty, type_static_length(&var.ty)),
        IrOperand::Const(IrConst::Bool(_)) | IrOperand::Const(IrConst::U8(_)) => Some(1),
        IrOperand::Const(IrConst::U16(_)) => Some(2),
        IrOperand::Const(IrConst::U32(_)) => Some(4),
        IrOperand::Const(IrConst::U64(_)) => Some(8),
        _ => None,
    })
}

fn fixed_scalar_operand_width(operand: &IrOperand) -> Option<usize> {
    match operand {
        IrOperand::Var(var) => fixed_scalar_width(&var.ty, type_static_length(&var.ty)),
        IrOperand::Const(IrConst::Bool(_)) | IrOperand::Const(IrConst::U8(_)) => Some(1),
        IrOperand::Const(IrConst::U16(_)) => Some(2),
        IrOperand::Const(IrConst::U32(_)) => Some(4),
        IrOperand::Const(IrConst::U64(_)) => Some(8),
        _ => None,
    }
}

fn collect_pure_const_returns(ir: &IrModule) -> HashMap<String, IrConst> {
    ir.items
        .iter()
        .filter_map(|item| {
            let IrItem::PureFn(function) = item else {
                return None;
            };
            pure_const_return(&function.body).map(|value| (function.name.clone(), value))
        })
        .collect()
}

fn pure_const_return(body: &IrBody) -> Option<IrConst> {
    let [block] = body.blocks.as_slice() else {
        return None;
    };
    match (&block.instructions[..], &block.terminator) {
        ([], IrTerminator::Return(Some(IrOperand::Const(value)))) => Some(value.clone()),
        ([IrInstruction::LoadConst { dest, value }], IrTerminator::Return(Some(IrOperand::Var(var)))) if dest.id == var.id => {
            Some(value.clone())
        }
        _ => None,
    }
}

pub(crate) fn fixed_byte_pointer_param_width(ty: &IrType) -> Option<usize> {
    fixed_byte_width(ty, type_static_length(ty)).filter(|width| *width > 8)
}

pub(crate) fn fixed_aggregate_pointer_param_width(ty: &IrType) -> Option<usize> {
    match ty {
        IrType::Array(_, _) | IrType::Tuple(_) => type_static_length(ty).filter(|width| *width > 8),
        _ => None,
    }
}

fn fixed_byte_const_bytes(value: &IrConst) -> Option<Vec<u8>> {
    match value {
        IrConst::Address(bytes) | IrConst::Hash(bytes) => Some(bytes.to_vec()),
        IrConst::U128(value) => Some(value.to_le_bytes().to_vec()),
        IrConst::Array(values) => values
            .iter()
            .map(|value| match value {
                IrConst::U8(byte) => Some(*byte),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

fn fixed_scalar_const_value(value: &IrConst) -> Option<u64> {
    match value {
        IrConst::Bool(value) => Some(u64::from(*value)),
        IrConst::U8(value) => Some((*value).into()),
        IrConst::U16(value) => Some((*value).into()),
        IrConst::U32(value) => Some((*value).into()),
        IrConst::U64(value) => Some(*value),
        _ => None,
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
        IrConst::Array(values) => IrType::Array(Box::new(values.first().map(const_ir_type).unwrap_or(IrType::U8)), values.len()),
    }
}

fn const_usize_operand(operand: &IrOperand) -> Option<usize> {
    match operand {
        IrOperand::Const(IrConst::U8(value)) => Some((*value).into()),
        IrOperand::Const(IrConst::U16(value)) => Some((*value).into()),
        IrOperand::Const(IrConst::U32(value)) => Some(*value as usize),
        IrOperand::Const(IrConst::U64(value)) => usize::try_from(*value).ok(),
        _ => None,
    }
}

fn aggregate_type_label(ty: &IrType) -> String {
    match ty {
        IrType::Tuple(_) => "tuple".to_string(),
        IrType::Array(_, len) => format!("array{}", len),
        IrType::Address => "Address".to_string(),
        IrType::Hash => "Hash".to_string(),
        other => format!("{:?}", other),
    }
}

fn aggregate_field_layout(ty: &IrType, field: &str) -> Option<SchemaFieldLayout> {
    match ty {
        IrType::Tuple(items) => {
            let index = field.parse::<usize>().ok()?;
            let field_ty = items.get(index)?.clone();
            let offset = items.iter().take(index).try_fold(0usize, |acc, item| type_static_length(item).map(|size| acc + size))?;
            let fixed_size = type_static_length(&field_ty);
            Some(SchemaFieldLayout { index, offset, ty: field_ty, fixed_size, fixed_enum_size: None })
        }
        IrType::Address | IrType::Hash if field == "0" => Some(SchemaFieldLayout {
            index: 0,
            offset: 0,
            ty: IrType::Array(Box::new(IrType::U8), 32),
            fixed_size: Some(32),
            fixed_enum_size: None,
        }),
        _ => None,
    }
}

fn tuple_return_field_type(ty: &IrType, field: &str) -> Option<IrType> {
    let IrType::Tuple(items) = ty else {
        return None;
    };
    let index = field.parse::<usize>().ok()?;
    (index < 8).then(|| items.get(index).cloned()).flatten()
}


#[derive(Debug, Clone)]
enum PreludeU64OperandSource {
    Const(u64),
    ParamVar(usize),
    StackVar(usize),
    Field(SchemaFieldValueSource),
    Expr(Box<PreludeU64ValueSource>),
}

#[derive(Debug, Clone)]
enum PreludeU64ValueSource {
    Const(u64),
    ParamVar(usize),
    StackVar(usize),
    Field(SchemaFieldValueSource),
    Binary { op: BinaryOp, left: Box<PreludeU64ValueSource>, right: PreludeU64OperandSource },
    Min { left: Box<PreludeU64ValueSource>, right: PreludeU64OperandSource },
}


#[derive(Debug, Clone)]
pub struct CodegenOptions {
    pub opt_level: u8,
    pub debug: bool,
    /// Artifact target profile. CKB selects the CKB syscall/source ABI.
    pub target_profile: TargetProfile,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self { opt_level: 0, debug: false, target_profile: TargetProfile::Ckb }
    }
}

pub struct CodeGenerator {
    options: CodegenOptions,
    assembly: Vec<String>,
    current_function: Option<String>,
    frame_size: usize,
    next_virtual_output: usize,
    /// Stack-frame start offset for runtime collection buffers.
    collection_region_start: usize,
    /// Runtime collection buffer allocator for the current function.
    next_collection_slot: usize,
    /// Named schema field layouts, keyed by type name then field name.
    type_layouts: HashMap<String, HashMap<String, SchemaFieldLayout>>,
    /// Fieldless enum storage widths, keyed by enum name.
    enum_fixed_sizes: HashMap<String, usize>,
    /// Fixed encoded size of named schemas when all fields have fixed-width layouts.
    type_fixed_sizes: HashMap<String, usize>,
    /// Named types declared as receipts.
    receipt_type_names: BTreeSet<String>,
    /// Named types that are transaction cell-backed values.
    cell_type_names: BTreeSet<String>,
    /// State names for schemas that declared flow policy.
    flow_states: HashMap<String, Vec<String>>,
    /// Flow field name keyed by schema type.
    flow_state_fields: HashMap<String, String>,
    /// Declared flow/flow transition graph keyed by schema type.
    flow_rules: HashMap<String, Vec<IrFlowRule>>,
    /// Action-specific state edges for the function currently being emitted.
    current_state_transition_edges: Vec<IrStateTransitionEdge>,
    /// ABI summaries for locally emitted actions/functions/locks.
    callable_abis: HashMap<String, CallableAbi>,
    /// Function parameters whose slot contains a pointer to encoded schema bytes.
    schema_pointer_vars: BTreeSet<usize>,
    /// Function parameter slots available before the prelude summaries run.
    param_vars: BTreeSet<usize>,
    /// Schema pointer slots backed by a VM-loaded cell buffer size word.
    schema_pointer_size_offsets: HashMap<usize, usize>,
    /// Fixed-byte parameter pointer slots backed by a separate ABI length word.
    fixed_byte_param_size_offsets: HashMap<usize, usize>,
    /// Fixed-width aggregate pointer slots backed by ABI bytes, keyed by IR variable id.
    aggregate_pointer_sources: HashMap<usize, AggregatePointerSource>,
    /// Tuple-valued call results that can be projected from RISC-V return registers.
    tuple_call_return_vars: HashMap<usize, IrType>,
    /// Stack slots populated from tuple call return registers, keyed by `(tuple_var_id, field)`.
    tuple_call_return_field_slots: HashMap<(usize, String), usize>,
    /// Tuple aggregate fields produced in the current function body, keyed by tuple var id.
    tuple_aggregate_fields: HashMap<usize, Vec<IrOperand>>,
    /// Fixed scalar temporaries that are aliases for schema-backed field loads.
    schema_field_value_sources: HashMap<usize, SchemaFieldValueSource>,
    /// U64 temporaries that can be recomputed in the CKB-runtime prelude.
    prelude_u64_value_sources: HashMap<usize, PreludeU64ValueSource>,
    /// Fixed scalar temporaries that can be recomputed as immediates in the CKB-runtime prelude.
    prelude_scalar_immediates: HashMap<usize, u64>,
    /// Fixed-byte constant temporaries that can be recomputed byte-by-byte in the CKB-runtime prelude.
    prelude_fixed_byte_constants: HashMap<usize, Vec<u8>>,
    /// Function-local fixed-byte storage for wide scalar temporaries such as u128.
    fixed_byte_local_offsets: HashMap<usize, usize>,
    /// Named IR variable slots used by StoreVar/LoadVar instructions.
    named_var_offsets: HashMap<String, usize>,
    /// Deduplicated immutable byte constants emitted into .rodata.
    const_data_labels: HashMap<Vec<u8>, String>,
    const_data_entries: Vec<(String, Vec<u8>)>,
    /// Local pure functions proven to return one constant on every path.
    pure_const_returns: HashMap<String, IrConst>,
    /// Per-CKB-runtime cell data buffers keyed by IR variable id.
    cell_buffer_offsets: HashMap<usize, usize>,
    /// Per-CKB-runtime cell size words keyed by IR variable id.
    cell_buffer_size_offsets: HashMap<usize, usize>,
    /// Byte-size slots for dynamic Molecule values projected from schema table fields.
    dynamic_value_size_offsets: HashMap<usize, usize>,
    /// Empty collection temporaries that can be verified as empty Molecule vectors.
    empty_molecule_vector_vars: BTreeSet<usize>,
    /// Stack-backed local collection variables whose length word and buffer are emitted in this frame.
    stack_collection_vars: BTreeSet<usize>,
    /// Locally constructed `Vec<u8>` bytes keyed by collection variable id.
    constructed_byte_vectors: HashMap<usize, Vec<IrOperand>>,
    /// Root `CollectionNew` variable for aliases of locally constructed vectors.
    constructed_byte_vector_roots: HashMap<usize, usize>,
    /// Collection variable ids whose full construction is covered by create-output vector verification.
    verified_collection_construction_vectors: BTreeSet<usize>,
    /// `type_hash()` temporaries that can be loaded from a created Output cell's TypeHash field.
    output_type_hash_sources: HashMap<usize, usize>,
    /// Schema parameter TypeHash pointer slots, keyed by source parameter variable id.
    param_type_hash_pointer_offsets: HashMap<usize, usize>,
    /// Schema parameter TypeHash length slots, keyed by source parameter variable id.
    param_type_hash_size_offsets: HashMap<usize, usize>,
    /// `type_hash()` temporaries backed by trusted parameter TypeHash ABI bytes.
    param_type_hash_sources: HashMap<usize, usize>,
    /// Consumed IR operand variable ids in source lowering order.
    consume_order: Vec<usize>,
    /// Consumed Input index keyed by IR operand variable id.
    consume_indices: HashMap<usize, usize>,
    /// Consumed named schema type keyed by IR operand variable id.
    consume_type_names: HashMap<usize, String>,
    /// Consumed IR operand variable id keyed by source binding name.
    consume_binding_ids: HashMap<String, usize>,
    /// Read-ref IR destination variable ids in source lowering order.
    read_ref_order: Vec<usize>,
    /// Read-ref CellDep index keyed by IR destination variable id.
    read_ref_indices: HashMap<usize, usize>,
    /// Read-only schema parameter variable ids keyed by source binding name.
    read_ref_param_ids: HashMap<String, usize>,
    /// CKB Input index for read-only schema parameters keyed by IR variable id.
    read_ref_param_input_indices: HashMap<usize, usize>,
    /// CKB CellDep index for read_ref schema parameters keyed by IR variable id.
    read_ref_param_dep_indices: HashMap<usize, usize>,
    /// Proposed transaction Output parameter variable ids keyed by source binding name.
    output_param_ids: HashMap<String, usize>,
    /// Whether the current entry function should bind read-only schema params from Inputs.
    bind_readonly_schema_params: bool,
    /// Whether the current function is a CKB lock predicate entry.
    current_lock_entry: bool,
    /// Mutable schema parameter variable ids keyed by source binding name.
    mutate_param_ids: HashMap<String, usize>,
    /// Output index for source-level operations that materialize transaction Outputs.
    operation_output_indices: HashMap<usize, usize>,
    /// Operation destination ids whose transaction Output relation is fully verifier-covered.
    verified_operation_outputs: BTreeSet<usize>,
    /// Collection push value ids whose effect is covered by a mutate append verifier.
    verified_collection_push_values: BTreeSet<usize>,
    /// Function-local cold fail handlers keyed by returned verifier error code.
    fail_handler_codes: BTreeSet<CellScriptRuntimeError>,
    /// Unique label counter for runtime checks.
    next_runtime_label: usize,
}

impl CodeGenerator {
    fn fixed_named_type_width(&self, ty: &IrType) -> Option<usize> {
        match ty {
            IrType::Named(name) => self.type_fixed_sizes.get(name).copied().or_else(|| self.enum_fixed_sizes.get(name).copied()),
            IrType::Ref(inner) | IrType::MutRef(inner) => self.fixed_named_type_width(inner),
            _ => None,
        }
    }

    fn fixed_byte_like_width(&self, ty: &IrType) -> Option<usize> {
        fixed_byte_width(ty, type_static_length(ty)).or_else(|| self.fixed_named_type_width(ty))
    }

    fn const_data_label_for_bytes(&mut self, bytes: Vec<u8>) -> String {
        if let Some(label) = self.const_data_labels.get(&bytes) {
            return label.clone();
        }
        let label = format!("__cellscript_const_data_{}", self.const_data_entries.len());
        self.const_data_labels.insert(bytes.clone(), label.clone());
        self.const_data_entries.push((label.clone(), bytes));
        label
    }

    fn emit_const_data_pool(&mut self) {
        if self.const_data_entries.is_empty() {
            return;
        }
        self.emit_section(".rodata");
        let entries = std::mem::take(&mut self.const_data_entries);
        for (label, bytes) in &entries {
            self.emit_label(label);
            for byte in bytes {
                self.emit(format!(".byte {}", byte));
            }
            self.emit(".align 3");
        }
        self.const_data_entries = entries;
    }

    fn constructed_byte_vector_part_width(&self, operand: &IrOperand) -> Option<usize> {
        constructed_byte_vector_part_width(operand).or_else(|| match operand {
            IrOperand::Var(var) => self.fixed_named_type_width(&var.ty),
            _ => None,
        })
    }

    fn param_is_runtime_bound(&self, param: &IrParam) -> bool {
        param.source == ParamSource::LockArgs
            || param.is_ref
            || named_type_name(&param.ty).is_some_and(|name| self.cell_type_names.contains(name))
    }

    pub fn new(options: CodegenOptions) -> Self {
        Self {
            options,
            assembly: Vec::new(),
            current_function: None,
            frame_size: 16,
            next_virtual_output: 0,
            collection_region_start: 0,
            next_collection_slot: 0,
            type_layouts: HashMap::new(),
            enum_fixed_sizes: HashMap::new(),
            type_fixed_sizes: HashMap::new(),
            receipt_type_names: BTreeSet::new(),
            cell_type_names: BTreeSet::new(),
            flow_states: HashMap::new(),
            flow_state_fields: HashMap::new(),
            flow_rules: HashMap::new(),
            current_state_transition_edges: Vec::new(),
            callable_abis: HashMap::new(),
            schema_pointer_vars: BTreeSet::new(),
            param_vars: BTreeSet::new(),
            schema_pointer_size_offsets: HashMap::new(),
            fixed_byte_param_size_offsets: HashMap::new(),
            aggregate_pointer_sources: HashMap::new(),
            tuple_call_return_vars: HashMap::new(),
            tuple_call_return_field_slots: HashMap::new(),
            tuple_aggregate_fields: HashMap::new(),
            schema_field_value_sources: HashMap::new(),
            prelude_u64_value_sources: HashMap::new(),
            prelude_scalar_immediates: HashMap::new(),
            prelude_fixed_byte_constants: HashMap::new(),
            fixed_byte_local_offsets: HashMap::new(),
            named_var_offsets: HashMap::new(),
            const_data_labels: HashMap::new(),
            const_data_entries: Vec::new(),
            pure_const_returns: HashMap::new(),
            cell_buffer_offsets: HashMap::new(),
            cell_buffer_size_offsets: HashMap::new(),
            dynamic_value_size_offsets: HashMap::new(),
            empty_molecule_vector_vars: BTreeSet::new(),
            stack_collection_vars: BTreeSet::new(),
            constructed_byte_vectors: HashMap::new(),
            constructed_byte_vector_roots: HashMap::new(),
            verified_collection_construction_vectors: BTreeSet::new(),
            output_type_hash_sources: HashMap::new(),
            param_type_hash_pointer_offsets: HashMap::new(),
            param_type_hash_size_offsets: HashMap::new(),
            param_type_hash_sources: HashMap::new(),
            consume_order: Vec::new(),
            consume_indices: HashMap::new(),
            consume_type_names: HashMap::new(),
            consume_binding_ids: HashMap::new(),
            read_ref_order: Vec::new(),
            read_ref_indices: HashMap::new(),
            read_ref_param_ids: HashMap::new(),
            read_ref_param_input_indices: HashMap::new(),
            read_ref_param_dep_indices: HashMap::new(),
            output_param_ids: HashMap::new(),
            bind_readonly_schema_params: false,
            current_lock_entry: false,
            mutate_param_ids: HashMap::new(),
            operation_output_indices: HashMap::new(),
            verified_operation_outputs: BTreeSet::new(),
            verified_collection_push_values: BTreeSet::new(),
            fail_handler_codes: BTreeSet::new(),
            next_runtime_label: 0,
        }
    }

    fn runtime_abi(&self) -> RuntimeSyscallAbi {
        runtime_syscall_abi(self.options.target_profile)
    }

    pub fn generate(mut self, ir: &IrModule, format: ArtifactFormat) -> Result<Vec<u8>> {
        let has_entrypoint = ir.items.iter().any(|item| matches!(item, IrItem::Action(_) | IrItem::Lock(_)));
        self.enum_fixed_sizes = ir.enum_fixed_sizes.clone();
        self.pure_const_returns = collect_pure_const_returns(ir);
        for item in &ir.items {
            if let IrItem::TypeDef(type_def) = item {
                self.register_type_def(type_def);
            }
        }
        for type_def in &ir.external_type_defs {
            self.register_type_def(type_def);
        }
        self.register_callable_abis(ir);

        self.emit_header();

        for item in &ir.items {
            if let IrItem::TypeDef(type_def) = item {
                self.generate_type_def(type_def)?;
            }
        }

        self.emit_section(".text");
        if let Some((entry_name, entry_params)) = first_entrypoint(ir) {
            if entry_params.is_empty() {
                self.emit_entry_direct_wrapper(entry_name);
            } else {
                self.emit_entry_witness_wrapper(entry_name, entry_params)?;
            }
        }

        for item in &ir.items {
            if let IrItem::Action(action) = item {
                self.generate_action(action)?;
            }
        }
        for item in &ir.items {
            if let IrItem::Lock(lock) = item {
                self.generate_lock(lock)?;
            }
        }
        if has_entrypoint {
            for item in &ir.items {
                if let IrItem::PureFn(function) = item {
                    self.generate_pure_fn(function)?;
                }
            }
        }

        self.generate_runtime_support(ir);
        self.emit_const_data_pool();

        self.assemble(format)
    }

    fn emit_header(&mut self) {
        self.assembly.push("# CellScript Generated Assembly".to_string());
        self.assembly.push(format!("# opt_level={}, debug={}", self.options.opt_level, self.options.debug));
        self.assembly.push(".option arch, +rv64imac".to_string());
        self.assembly.push("".to_string());
    }

    fn emit_section(&mut self, section: &str) {
        self.assembly.push(format!(".section {}", section));
    }

    fn emit_global(&mut self, name: &str) {
        self.assembly.push(format!(".global {}", name));
        self.assembly.push(format!(".type {}, @function", name));
    }

    fn emit_label(&mut self, name: &str) {
        self.assembly.push(format!("{}:", name));
    }

    fn emit(&mut self, instruction: impl Into<String>) {
        let instruction = instruction.into();
        if self.emit_large_immediate_access_if_needed(&instruction) {
            return;
        }
        self.assembly.push(format!("    {}", instruction));
    }

    fn emit_large_immediate_access_if_needed(&mut self, instruction: &str) -> bool {
        let Some(clean) = strip_comment(instruction) else {
            return false;
        };
        if clean.is_empty() || clean.starts_with('.') || clean.ends_with(':') {
            return false;
        }

        let mut parts = clean.splitn(2, char::is_whitespace);
        let opcode = parts.next().unwrap_or_default();
        let args = parts.next().unwrap_or("").trim();
        let args = if args.is_empty() { Vec::new() } else { args.split(',').map(str::trim).collect::<Vec<_>>() };

        match opcode {
            "ld" | "lbu" if args.len() == 2 => {
                let Some((offset, base)) = memory_operand_offset_and_base(args[1]) else {
                    return false;
                };
                if parse_register(args[0]).is_err() || parse_register(base).is_err() {
                    return false;
                }
                if small_signed_immediate(offset) {
                    return false;
                }
                let scratch = scratch_register_avoiding(&[args[0], base]);
                self.assembly.push(format!("    li {}, {}", scratch, offset));
                self.assembly.push(format!("    add {}, {}, {}", scratch, base, scratch));
                self.assembly.push(format!("    {} {}, 0({})", opcode, args[0], scratch));
                true
            }
            "sb" | "sh" | "sw" | "sd" if args.len() == 2 => {
                let Some((offset, base)) = memory_operand_offset_and_base(args[1]) else {
                    return false;
                };
                if parse_register(args[0]).is_err() || parse_register(base).is_err() {
                    return false;
                }
                if small_signed_immediate(offset) {
                    return false;
                }
                let scratch = scratch_register_avoiding(&[args[0], base]);
                self.assembly.push(format!("    li {}, {}", scratch, offset));
                self.assembly.push(format!("    add {}, {}, {}", scratch, base, scratch));
                self.assembly.push(format!("    {} {}, 0({})", opcode, args[0], scratch));
                true
            }
            "addi" if args.len() == 3 => {
                let Ok(offset) = parse_immediate(args[2]) else {
                    return false;
                };
                if parse_register(args[0]).is_err() || parse_register(args[1]).is_err() {
                    return false;
                }
                if small_signed_immediate(offset) {
                    return false;
                }
                let scratch = scratch_register_avoiding(&[args[0], args[1]]);
                self.assembly.push(format!("    li {}, {}", scratch, offset));
                self.assembly.push(format!("    add {}, {}, {}", args[0], args[1], scratch));
                true
            }
            _ => false,
        }
    }

    fn generate_type_def(&mut self, type_def: &IrTypeDef) -> Result<()> {
        self.emit_section(".rodata");
        self.emit_label(&format!("__type_desc_{}", type_def.name));

        self.emit(format!(".word {}", type_def.fields.len()));

        for field in &type_def.fields {
            self.emit(format!(".byte {}", field.name.len()));
            self.emit(format!(".ascii \"{}\"", field.name));
            self.emit(".align 3");
            self.emit(format!(".word {}", self.type_id(&field.ty)));
        }

        Ok(())
    }

    fn register_type_def(&mut self, type_def: &IrTypeDef) {
        if let Some(fixed_size) = type_def.fields.iter().try_fold(0usize, |acc, field| field.fixed_size.map(|size| acc + size)) {
            self.type_fixed_sizes.insert(type_def.name.clone(), fixed_size);
        }
        if let Some(states) = &type_def.flow_states {
            self.flow_states.insert(type_def.name.clone(), states.clone());
        }
        if let Some(field) = &type_def.flow_state_field {
            self.flow_state_fields.insert(type_def.name.clone(), field.clone());
        }
        if !type_def.flow_rules.is_empty() {
            self.flow_rules.insert(type_def.name.clone(), type_def.flow_rules.clone());
        }
        if matches!(type_def.kind, IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt) {
            self.cell_type_names.insert(type_def.name.clone());
            if type_def.kind == IrTypeKind::Receipt {
                self.receipt_type_names.insert(type_def.name.clone());
            }
        }
        let fields = type_def
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let fixed_enum_size = match &field.ty {
                    IrType::Named(name) => self.enum_fixed_sizes.get(name).copied(),
                    _ => None,
                };
                (
                    field.name.clone(),
                    SchemaFieldLayout {
                        index,
                        offset: field.offset,
                        ty: field.ty.clone(),
                        fixed_size: field.fixed_size,
                        fixed_enum_size,
                    },
                )
            })
            .collect();
        self.type_layouts.insert(type_def.name.clone(), fields);
    }


    fn type_id(&self, ty: &IrType) -> u32 {
        match ty {
            IrType::U8 => 1,
            IrType::U16 => 2,
            IrType::U32 => 3,
            IrType::U64 => 4,
            IrType::U128 => 5,
            IrType::Bool => 6,
            IrType::Address => 7,
            IrType::Hash => 8,
            IrType::Array(_, _) => 9,
            IrType::Tuple(_) => 10,
            IrType::Named(_) => 11,
            IrType::Ref(_) => 12,
            IrType::MutRef(_) => 13,
            IrType::Unit => 14,
        }
    }

    fn generate_action(&mut self, action: &IrAction) -> Result<()> {
        self.current_function = Some(action.name.clone());
        self.current_state_transition_edges = action.state_transition_edges.clone();
        self.bind_readonly_schema_params = true;
        self.fail_handler_codes.clear();
        self.prepare_function_layout(&action.body, &action.params)?;
        self.next_virtual_output = 0;
        self.set_schema_pointer_params(&action.params);
        self.set_consumed_schema_pointers(&action.body);
        self.set_read_ref_schema_pointers(&action.body);
        self.set_pointer_aliases(&action.body);
        self.set_schema_field_value_sources(&action.body);
        self.set_verified_operation_outputs(&action.body);
        self.set_constructed_byte_vectors(&action.body);
        self.set_verified_collection_push_values(&action.body);

        if !action.params.is_empty() {
            self.emit_entry_abi_marker(&action.name);
        }
        self.emit_global(&action.name);
        self.emit_label(&action.name);

        self.emit_prologue();
        self.emit_param_spills(&action.params)?;

        self.generate_body(&action.body)?;
        self.emit_shared_epilogue();

        self.current_function = None;
        self.current_state_transition_edges.clear();
        self.bind_readonly_schema_params = false;
        self.schema_pointer_vars.clear();
        self.schema_pointer_size_offsets.clear();
        self.fixed_byte_param_size_offsets.clear();
        self.schema_field_value_sources.clear();
        self.aggregate_pointer_sources.clear();
        self.tuple_call_return_vars.clear();
        self.tuple_call_return_field_slots.clear();
        self.tuple_aggregate_fields.clear();
        self.output_type_hash_sources.clear();
        self.param_type_hash_pointer_offsets.clear();
        self.param_type_hash_size_offsets.clear();
        self.param_type_hash_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.prelude_fixed_byte_constants.clear();
        self.operation_output_indices.clear();
        self.verified_operation_outputs.clear();
        self.output_param_ids.clear();
        self.verified_collection_push_values.clear();
        self.stack_collection_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        self.param_vars.clear();
        Ok(())
    }

    fn generate_pure_fn(&mut self, function: &IrPureFn) -> Result<()> {
        self.current_function = Some(function.name.clone());
        self.bind_readonly_schema_params = false;
        self.fail_handler_codes.clear();
        self.prepare_function_layout(&function.body, &function.params)?;
        self.next_virtual_output = 0;
        self.set_schema_pointer_params(&function.params);
        self.set_consumed_schema_pointers(&function.body);
        self.set_read_ref_schema_pointers(&function.body);
        self.set_pointer_aliases(&function.body);
        self.set_schema_field_value_sources(&function.body);
        self.set_verified_operation_outputs(&function.body);
        self.set_constructed_byte_vectors(&function.body);
        self.set_verified_collection_push_values(&function.body);

        self.emit_global(&function.name);
        self.emit_label(&function.name);

        self.emit_prologue();
        self.emit_param_spills(&function.params)?;
        self.generate_body(&function.body)?;
        self.emit_shared_epilogue();

        self.current_function = None;
        self.schema_pointer_vars.clear();
        self.schema_pointer_size_offsets.clear();
        self.fixed_byte_param_size_offsets.clear();
        self.schema_field_value_sources.clear();
        self.aggregate_pointer_sources.clear();
        self.tuple_call_return_vars.clear();
        self.tuple_call_return_field_slots.clear();
        self.tuple_aggregate_fields.clear();
        self.output_type_hash_sources.clear();
        self.param_type_hash_pointer_offsets.clear();
        self.param_type_hash_size_offsets.clear();
        self.param_type_hash_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.prelude_fixed_byte_constants.clear();
        self.operation_output_indices.clear();
        self.verified_operation_outputs.clear();
        self.output_param_ids.clear();
        self.verified_collection_push_values.clear();
        self.stack_collection_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        self.param_vars.clear();
        Ok(())
    }

    fn generate_lock(&mut self, lock: &IrLock) -> Result<()> {
        self.current_function = Some(lock.name.clone());
        self.bind_readonly_schema_params = true;
        self.current_lock_entry = true;
        self.fail_handler_codes.clear();
        self.prepare_function_layout(&lock.body, &lock.params)?;
        self.next_virtual_output = 0;
        self.set_schema_pointer_params(&lock.params);
        self.set_consumed_schema_pointers(&lock.body);
        self.set_read_ref_schema_pointers(&lock.body);
        self.set_pointer_aliases(&lock.body);
        self.set_schema_field_value_sources(&lock.body);
        self.set_verified_operation_outputs(&lock.body);
        self.set_constructed_byte_vectors(&lock.body);
        self.set_verified_collection_push_values(&lock.body);

        if !lock.params.is_empty() {
            self.emit_entry_abi_marker(&lock.name);
        }
        self.emit_global(&lock.name);
        self.emit_label(&lock.name);

        self.emit_prologue();
        self.emit_param_spills(&lock.params)?;

        self.generate_body(&lock.body)?;
        self.emit_shared_epilogue();

        self.current_function = None;
        self.bind_readonly_schema_params = false;
        self.current_lock_entry = false;
        self.schema_pointer_vars.clear();
        self.schema_pointer_size_offsets.clear();
        self.fixed_byte_param_size_offsets.clear();
        self.schema_field_value_sources.clear();
        self.aggregate_pointer_sources.clear();
        self.tuple_call_return_vars.clear();
        self.tuple_call_return_field_slots.clear();
        self.tuple_aggregate_fields.clear();
        self.output_type_hash_sources.clear();
        self.param_type_hash_pointer_offsets.clear();
        self.param_type_hash_size_offsets.clear();
        self.param_type_hash_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.prelude_fixed_byte_constants.clear();
        self.operation_output_indices.clear();
        self.verified_operation_outputs.clear();
        self.output_param_ids.clear();
        self.verified_collection_push_values.clear();
        self.stack_collection_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        self.param_vars.clear();
        Ok(())
    }

    fn set_schema_pointer_params(&mut self, params: &[IrParam]) {
        self.schema_pointer_vars.clear();
        self.param_vars.clear();
        self.aggregate_pointer_sources.clear();
        for param in params {
            self.param_vars.insert(param.binding.id);
            if named_type_name(&param.ty).is_some() {
                self.schema_pointer_vars.insert(param.binding.id);
            } else if fixed_byte_pointer_param_width(&param.ty).is_some() || fixed_aggregate_pointer_param_width(&param.ty).is_some() {
                self.aggregate_pointer_sources.insert(param.binding.id, AggregatePointerSource { ty: param.ty.clone() });
            }
        }
    }

    fn set_read_ref_schema_pointers(&mut self, body: &IrBody) {
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::ReadRef { dest, .. } = instruction {
                    self.schema_pointer_vars.insert(dest.id);
                    if let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() {
                        self.schema_pointer_size_offsets.insert(dest.id, size_offset);
                    }
                }
            }
        }
    }

    fn set_consumed_schema_pointers(&mut self, body: &IrBody) {
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let Some(var) = consumed_operand_var(instruction) {
                    self.schema_pointer_vars.insert(var.id);
                    if let Some(size_offset) = self.cell_buffer_size_offsets.get(&var.id).copied() {
                        self.schema_pointer_size_offsets.insert(var.id, size_offset);
                    }
                }
            }
        }
    }

    fn set_pointer_aliases(&mut self, body: &IrBody) {
        let mut changed = true;
        while changed {
            changed = false;
            for block in &body.blocks {
                for instruction in &block.instructions {
                    let alias = match instruction {
                        IrInstruction::Unary { dest, op: UnaryOp::Ref | UnaryOp::Deref, operand: IrOperand::Var(src) }
                        | IrInstruction::Move { dest, src: IrOperand::Var(src) }
                            if dest.ty == src.ty =>
                        {
                            Some((dest, src))
                        }
                        _ => None,
                    };
                    let Some((dest, src)) = alias else {
                        continue;
                    };
                    if self.schema_pointer_vars.contains(&src.id) && self.schema_pointer_vars.insert(dest.id) {
                        changed = true;
                    }
                    if let Some(size_offset) = self.schema_pointer_size_offsets.get(&src.id).copied() {
                        if self.schema_pointer_size_offsets.insert(dest.id, size_offset) != Some(size_offset) {
                            changed = true;
                        }
                    }
                    if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&src.id).copied() {
                        if self.fixed_byte_param_size_offsets.insert(dest.id, size_offset) != Some(size_offset) {
                            changed = true;
                        }
                    }
                    if let Some(size_offset) = self.dynamic_value_size_offsets.get(&src.id).copied() {
                        if self.dynamic_value_size_offsets.insert(dest.id, size_offset) != Some(size_offset) {
                            changed = true;
                        }
                    }
                    if self.empty_molecule_vector_vars.contains(&src.id) && self.empty_molecule_vector_vars.insert(dest.id) {
                        changed = true;
                    }
                    if let Some(source) = self.aggregate_pointer_sources.get(&src.id).cloned() {
                        if self.aggregate_pointer_sources.insert(dest.id, source).is_none() {
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    fn set_schema_field_value_sources(&mut self, body: &IrBody) {
        self.schema_field_value_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.prelude_fixed_byte_constants.clear();
        self.tuple_call_return_vars.clear();
        self.tuple_call_return_field_slots.clear();
        self.tuple_aggregate_fields.clear();
        let mut named_stack_collections = HashMap::<String, usize>::new();
        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    IrInstruction::StoreVar { name, src: IrOperand::Var(src) } => {
                        if self.stack_collection_vars.contains(&src.id) {
                            named_stack_collections.insert(name.clone(), src.id);
                        }
                    }
                    IrInstruction::LoadVar { dest, name } => {
                        if named_stack_collections.contains_key(name) {
                            self.stack_collection_vars.insert(dest.id);
                        }
                    }
                    IrInstruction::Tuple { dest, fields } => {
                        self.tuple_aggregate_fields.insert(dest.id, fields.clone());
                    }
                    IrInstruction::Call { dest: Some(dest), .. } if matches!(dest.ty, IrType::Tuple(_)) => {
                        self.tuple_call_return_vars.insert(dest.id, dest.ty.clone());
                    }
                    IrInstruction::Call { dest: Some(dest), func, .. } if self.pure_const_returns.contains_key(func) => {
                        let value = self.pure_const_returns.get(func).cloned().expect("guarded pure const return");
                        if let Some(value) = fixed_scalar_const_value(&value) {
                            self.prelude_scalar_immediates.insert(dest.id, value);
                            if dest.ty == IrType::U64 {
                                self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Const(value));
                            }
                        }
                        if let Some(bytes) = fixed_byte_const_bytes(&value) {
                            self.prelude_fixed_byte_constants.insert(dest.id, bytes);
                        }
                    }
                    IrInstruction::LoadConst { dest, value } => {
                        if let Some(value) = fixed_scalar_const_value(value) {
                            self.prelude_scalar_immediates.insert(dest.id, value);
                            if dest.ty == IrType::U64 {
                                self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Const(value));
                            }
                        }
                        if let Some(bytes) = fixed_byte_const_bytes(value) {
                            self.prelude_fixed_byte_constants.insert(dest.id, bytes);
                        }
                    }
                    IrInstruction::FieldAccess { dest, obj: IrOperand::Var(obj), field } => {
                        if self
                            .tuple_call_return_vars
                            .get(&obj.id)
                            .and_then(|ty| tuple_return_field_type(ty, field))
                            .is_some_and(|field_ty| field_ty == dest.ty)
                        {
                            self.tuple_call_return_field_slots.insert((obj.id, field.clone()), dest.id);
                            continue;
                        }
                        let source = if self.schema_pointer_vars.contains(&obj.id) {
                            let Some(type_name) = named_type_name(&obj.ty) else {
                                continue;
                            };
                            let Some(layout) = self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
                                continue;
                            };
                            Some(SchemaFieldValueSource {
                                obj_var_id: obj.id,
                                type_name: type_name.to_string(),
                                field: field.clone(),
                                layout,
                            })
                        } else {
                            self.aggregate_pointer_sources.get(&obj.id).and_then(|source| {
                                aggregate_field_layout(&source.ty, field).map(|layout| SchemaFieldValueSource {
                                    obj_var_id: obj.id,
                                    type_name: aggregate_type_label(&source.ty),
                                    field: field.clone(),
                                    layout,
                                })
                            })
                        };
                        let Some(source) = source else {
                            continue;
                        };
                        let layout = source.layout.clone();
                        if layout_fixed_byte_width(&layout).is_some() && layout.ty == dest.ty {
                            self.schema_field_value_sources.insert(dest.id, source.clone());
                            if layout_fixed_scalar_width(&layout).is_some() {
                                self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Field(source));
                            }
                        }
                    }
                    IrInstruction::Index { dest, arr: IrOperand::Var(arr), idx } => {
                        if self.aggregate_pointer_sources.contains_key(&arr.id) {
                            if let (IrType::Array(inner, len), Some(index)) = (&arr.ty, const_usize_operand(idx)) {
                                let element_ty = inner.as_ref();
                                if index < *len && type_static_length(element_ty).is_some() {
                                    if fixed_scalar_width(element_ty, type_static_length(element_ty)).is_some()
                                        && element_ty == &dest.ty
                                    {
                                        if dest.ty == IrType::U64 {
                                            self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                                        }
                                    } else {
                                        self.aggregate_pointer_sources
                                            .insert(dest.id, AggregatePointerSource { ty: element_ty.clone() });
                                    }
                                }
                            }
                        } else if self.stack_collection_vars.contains(&arr.id)
                            && molecule_vector_element_fixed_width(&arr.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
                                .is_some_and(|element_width| {
                                    self.fixed_byte_like_width(&dest.ty)
                                        .is_some_and(|dest_width| dest_width == element_width && dest_width > 8)
                                })
                        {
                            self.aggregate_pointer_sources.insert(dest.id, AggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                    IrInstruction::Binary { dest, op, left, right }
                        if dest.ty == IrType::U64 && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div) =>
                    {
                        let Some(left) = self.prelude_u64_value_source(left) else {
                            continue;
                        };
                        let Some(right) = self.prelude_u64_operand_source(right) else {
                            continue;
                        };
                        self.prelude_u64_value_sources
                            .insert(dest.id, PreludeU64ValueSource::Binary { op: *op, left: Box::new(left), right });
                    }
                    IrInstruction::Call { dest: Some(dest), func, args }
                        if dest.ty == IrType::U64 && is_min_call(func) && args.len() == 2 =>
                    {
                        let Some(left) = self.prelude_u64_value_source(&args[0]) else {
                            continue;
                        };
                        let Some(right) = self.prelude_u64_operand_source(&args[1]) else {
                            continue;
                        };
                        self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Min { left: Box::new(left), right });
                    }
                    IrInstruction::Call { dest: Some(dest), func, args }
                        if dest.ty == IrType::U64 && is_runtime_header_u64_call(func) && args.is_empty() =>
                    {
                        self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                    }
                    IrInstruction::Length { dest, operand }
                        if dest.ty == IrType::U64
                            && (self.static_length(operand).is_some()
                                || self.dynamic_length_from_size_offset(operand).is_some()
                                || matches!(
                                    operand,
                                    IrOperand::Var(var)
                                        if self.dynamic_value_size_offsets.contains_key(&var.id)
                                            || self.schema_pointer_size_offsets.contains_key(&var.id)
                                )) =>
                    {
                        self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                    }
                    IrInstruction::CollectionCapacity { dest, collection: IrOperand::Var(collection) }
                        if dest.ty == IrType::U64
                            && self.stack_collection_vars.contains(&collection.id)
                            && molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
                                .is_some_and(|width| width != 0) =>
                    {
                        self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                    }
                    IrInstruction::CollectionRemove { dest, collection: IrOperand::Var(collection), .. }
                    | IrInstruction::CollectionPop { dest, collection: IrOperand::Var(collection) }
                        if self.stack_collection_vars.contains(&collection.id)
                            && molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
                                .is_some_and(|element_width| {
                                    self.fixed_byte_like_width(&dest.ty)
                                        .is_some_and(|dest_width| dest_width == element_width && dest_width > 8)
                                }) =>
                    {
                        self.aggregate_pointer_sources.insert(dest.id, AggregatePointerSource { ty: dest.ty.clone() });
                    }
                    IrInstruction::Move { dest, src } | IrInstruction::Cast { dest, src } if dest.ty == IrType::U64 => {
                        if self.prelude_u64_value_source(src).is_some() {
                            self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::StackVar(dest.id));
                        }
                    }
                    IrInstruction::Move { dest, src } | IrInstruction::Cast { dest, src }
                        if matches!(dest.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32) =>
                    {
                        if let Some(value) = self.prelude_scalar_immediate(src) {
                            self.prelude_scalar_immediates.insert(dest.id, value);
                        }
                    }
                    IrInstruction::Move { dest, src } | IrInstruction::Cast { dest, src }
                        if fixed_byte_width(&dest.ty, type_static_length(&dest.ty)).is_some() =>
                    {
                        if let Some(bytes) = self.prelude_fixed_byte_constant(src) {
                            self.prelude_fixed_byte_constants.insert(dest.id, bytes);
                        }
                    }
                    IrInstruction::Move { dest, src: IrOperand::Var(src) }
                    | IrInstruction::Cast { dest, src: IrOperand::Var(src) }
                    | IrInstruction::Unary { dest, op: UnaryOp::Ref | UnaryOp::Deref, operand: IrOperand::Var(src) } => {
                        if self.stack_collection_vars.contains(&src.id) && dest.ty == src.ty {
                            self.stack_collection_vars.insert(dest.id);
                        }
                        if let Some(source) = self.schema_field_value_sources.get(&src.id).cloned() {
                            self.schema_field_value_sources.insert(dest.id, source);
                        }
                    }
                    IrInstruction::CollectionNew { dest, .. } => {
                        self.stack_collection_vars.insert(dest.id);
                    }
                    _ => {}
                }
            }
        }
    }

    fn set_verified_operation_outputs(&mut self, body: &IrBody) {
        self.operation_output_indices.clear();
        self.verified_operation_outputs.clear();

        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    IrInstruction::Create { dest, pattern }
                    | IrInstruction::CreateUnique { dest, pattern, .. }
                    | IrInstruction::ReplaceUnique { dest, pattern, .. } => {
                        if pattern.operation != "create" {
                            if let Some(output_index) =
                                Self::create_output_index(body, &pattern.operation, &pattern.binding, &pattern.ty)
                            {
                                self.operation_output_indices.insert(dest.id, output_index);
                            }
                        }
                    }
                    IrInstruction::Transfer { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "transfer", dest) {
                            self.record_verified_operation_output(body, output_index, dest, "transfer");
                        }
                    }
                    IrInstruction::Claim { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "claim", dest) {
                            self.record_verified_operation_output(body, output_index, dest, "claim");
                        }
                    }
                    IrInstruction::Settle { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "settle", dest) {
                            self.record_verified_operation_output(body, output_index, dest, "settle");
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn create_output_index(body: &IrBody, operation: &str, binding: &str, ty: &str) -> Option<usize> {
        body.create_set.iter().position(|pattern| pattern.operation == operation && pattern.binding == binding && pattern.ty == ty)
    }

    fn create_output_index_for_dest(body: &IrBody, operation: &str, dest: &IrVar) -> Option<usize> {
        let ty = named_type_name(&dest.ty)?;
        Self::create_output_index(body, operation, &dest.name, ty)
    }

    fn record_verified_operation_output(&mut self, body: &IrBody, output_index: usize, dest: &IrVar, operation: &str) {
        self.operation_output_indices.insert(dest.id, output_index);
        if body
            .create_set
            .get(output_index)
            .is_some_and(|pattern| self.operation_output_pattern_is_verified(pattern, operation, &dest.ty))
        {
            self.verified_operation_outputs.insert(dest.id);
        }
    }

    fn operation_output_pattern_is_verified(&self, pattern: &CreatePattern, operation: &str, dest_ty: &IrType) -> bool {
        pattern.operation == operation
            && named_type_name(dest_ty).is_some_and(|type_name| type_name == pattern.ty.as_str())
            && self.can_verify_create_output_fields(pattern)
            && self.can_verify_output_lock(pattern)
    }

    fn set_verified_collection_push_values(&mut self, body: &IrBody) {
        self.verified_collection_push_values.clear();
        for pattern in &body.mutate_set {
            for transition in &pattern.transitions {
                if transition.op != MutateTransitionOp::Append {
                    continue;
                }
                let IrOperand::Var(var) = &transition.operand else {
                    continue;
                };
                let Some(layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field)) else {
                    continue;
                };
                let Some(element_width) =
                    molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
                else {
                    continue;
                };
                if self.fixed_append_fields(&transition.operand, element_width).is_some() {
                    self.verified_collection_push_values.insert(var.id);
                }
            }
        }
    }

    fn set_constructed_byte_vectors(&mut self, body: &IrBody) {
        self.stack_collection_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        let mut named_vectors = HashMap::<String, usize>::new();
        let mut named_stack_collections = HashMap::<String, usize>::new();
        let mut loaded_vector_names = HashMap::<usize, String>::new();
        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    IrInstruction::StoreVar { name, src: IrOperand::Var(src) } => {
                        if self.stack_collection_vars.contains(&src.id) {
                            named_stack_collections.insert(name.clone(), src.id);
                        }
                        if self.constructed_byte_vectors.contains_key(&src.id) {
                            named_vectors.insert(name.clone(), src.id);
                        }
                    }
                    IrInstruction::LoadVar { dest, name } => {
                        if let Some(source_id) = named_stack_collections.get(name).copied() {
                            self.stack_collection_vars.insert(dest.id);
                            named_stack_collections.insert(name.clone(), dest.id);
                            if let Some(bytes) = self.constructed_byte_vectors.get(&source_id).cloned() {
                                self.constructed_byte_vectors.insert(dest.id, bytes);
                                if let Some(root_id) = self.constructed_byte_vector_roots.get(&source_id).copied() {
                                    self.constructed_byte_vector_roots.insert(dest.id, root_id);
                                }
                                loaded_vector_names.insert(dest.id, name.clone());
                            }
                            continue;
                        }
                        if let Some(source_id) = named_vectors.get(name).copied() {
                            if let Some(bytes) = self.constructed_byte_vectors.get(&source_id).cloned() {
                                self.constructed_byte_vectors.insert(dest.id, bytes);
                                if let Some(root_id) = self.constructed_byte_vector_roots.get(&source_id).copied() {
                                    self.constructed_byte_vector_roots.insert(dest.id, root_id);
                                }
                                loaded_vector_names.insert(dest.id, name.clone());
                            }
                        }
                    }
                    IrInstruction::CollectionNew { dest, .. } => {
                        self.stack_collection_vars.insert(dest.id);
                        self.constructed_byte_vectors.insert(dest.id, Vec::new());
                        self.constructed_byte_vector_roots.insert(dest.id, dest.id);
                    }
                    IrInstruction::CollectionPush { collection: IrOperand::Var(collection), value } => {
                        let width = self.constructed_byte_vector_part_width(value);
                        let source_available = width.is_some_and(|width| self.expected_fixed_byte_source(value, width).is_some());
                        if let Some(bytes) = self.constructed_byte_vectors.get_mut(&collection.id) {
                            if source_available {
                                bytes.push(value.clone());
                                if let Some(name) = loaded_vector_names.get(&collection.id).cloned() {
                                    named_vectors.insert(name, collection.id);
                                }
                            } else {
                                self.constructed_byte_vectors.remove(&collection.id);
                            }
                        }
                    }
                    IrInstruction::CollectionExtend { collection: IrOperand::Var(collection), slice } => {
                        let Some(width) = operand_fixed_byte_width(slice) else {
                            self.constructed_byte_vectors.remove(&collection.id);
                            continue;
                        };
                        let source_available = self.expected_fixed_byte_source(slice, width).is_some();
                        if let Some(bytes) = self.constructed_byte_vectors.get_mut(&collection.id) {
                            if source_available {
                                bytes.push(slice.clone());
                                if let Some(name) = loaded_vector_names.get(&collection.id).cloned() {
                                    named_vectors.insert(name, collection.id);
                                }
                            } else {
                                self.constructed_byte_vectors.remove(&collection.id);
                            }
                        }
                    }
                    IrInstruction::CollectionClear { collection: IrOperand::Var(collection) } => {
                        if let Some(bytes) = self.constructed_byte_vectors.get_mut(&collection.id) {
                            bytes.clear();
                            if let Some(name) = loaded_vector_names.get(&collection.id).cloned() {
                                named_vectors.insert(name, collection.id);
                            }
                        }
                    }
                    IrInstruction::CollectionReverse { collection: IrOperand::Var(collection) } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionTruncate { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionSwap { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionInsert { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionSet { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::CollectionPop { collection: IrOperand::Var(collection), .. } => {
                        self.constructed_byte_vectors.remove(&collection.id);
                    }
                    IrInstruction::Move { dest, src: IrOperand::Var(src) }
                    | IrInstruction::Unary { dest, op: UnaryOp::Ref | UnaryOp::Deref, operand: IrOperand::Var(src) } => {
                        if self.stack_collection_vars.contains(&src.id) {
                            self.stack_collection_vars.insert(dest.id);
                        }
                        if let Some(bytes) = self.constructed_byte_vectors.get(&src.id).cloned() {
                            self.constructed_byte_vectors.insert(dest.id, bytes);
                            if let Some(root_id) = self.constructed_byte_vector_roots.get(&src.id).copied() {
                                self.constructed_byte_vector_roots.insert(dest.id, root_id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut verified_roots = BTreeSet::new();
        for pattern in &body.create_set {
            let Some(layouts) = self.type_layouts.get(&pattern.ty) else {
                continue;
            };
            for (field, value) in &pattern.fields {
                let Some(layout) = layouts.get(field) else {
                    continue;
                };
                if molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).is_none() {
                    continue;
                }
                let IrOperand::Var(var) = value else {
                    continue;
                };
                if self.constructed_byte_vectors.contains_key(&var.id) {
                    verified_roots.insert(self.constructed_byte_vector_roots.get(&var.id).copied().unwrap_or(var.id));
                }
            }
        }
        for (var_id, root_id) in &self.constructed_byte_vector_roots {
            if verified_roots.contains(root_id) {
                self.verified_collection_construction_vectors.insert(*var_id);
            }
        }
    }

    fn prelude_scalar_immediate(&self, operand: &IrOperand) -> Option<u64> {
        match operand {
            IrOperand::Const(value) => fixed_scalar_const_value(value),
            IrOperand::Var(var) => self.prelude_scalar_immediates.get(&var.id).copied(),
        }
    }

    fn prelude_fixed_byte_constant(&self, operand: &IrOperand) -> Option<Vec<u8>> {
        match operand {
            IrOperand::Const(value) => fixed_byte_const_bytes(value),
            IrOperand::Var(var) => self.prelude_fixed_byte_constants.get(&var.id).cloned(),
        }
    }

    fn prelude_u64_value_source(&self, operand: &IrOperand) -> Option<PreludeU64ValueSource> {
        match operand {
            IrOperand::Const(IrConst::U64(n)) => Some(PreludeU64ValueSource::Const(*n)),
            IrOperand::Var(var) if var.ty == IrType::U64 && self.param_vars.contains(&var.id) => {
                Some(PreludeU64ValueSource::ParamVar(var.id))
            }
            IrOperand::Var(var) => self.prelude_u64_value_sources.get(&var.id).cloned(),
            _ => None,
        }
    }

    fn prelude_u64_operand_source(&self, operand: &IrOperand) -> Option<PreludeU64OperandSource> {
        match operand {
            IrOperand::Const(IrConst::U64(n)) => Some(PreludeU64OperandSource::Const(*n)),
            IrOperand::Var(var) if var.ty == IrType::U64 && self.param_vars.contains(&var.id) => {
                Some(PreludeU64OperandSource::ParamVar(var.id))
            }
            IrOperand::Var(var) => match self.prelude_u64_value_sources.get(&var.id)? {
                PreludeU64ValueSource::Const(n) => Some(PreludeU64OperandSource::Const(*n)),
                PreludeU64ValueSource::ParamVar(var_id) => Some(PreludeU64OperandSource::ParamVar(*var_id)),
                PreludeU64ValueSource::StackVar(var_id) => Some(PreludeU64OperandSource::StackVar(*var_id)),
                PreludeU64ValueSource::Field(source) => Some(PreludeU64OperandSource::Field(source.clone())),
                PreludeU64ValueSource::Binary { .. } | PreludeU64ValueSource::Min { .. } => {
                    Some(PreludeU64OperandSource::Expr(Box::new(self.prelude_u64_value_sources.get(&var.id)?.clone())))
                }
            },
            _ => None,
        }
    }

    fn generate_body(&mut self, body: &IrBody) -> Result<()> {
        self.emit_read_ref_parameter_bindings();

        for (index, pattern) in body.consume_set.iter().enumerate() {
            self.generate_consume(pattern, index)?;
        }

        let mut read_ref_index = 0usize;
        for pattern in &body.read_refs {
            if self.read_ref_param_ids.contains_key(&pattern.binding) {
                continue;
            }
            let index = read_ref_index;
            read_ref_index += 1;
            self.generate_read_ref(pattern, index)?;
        }

        // Signature-bound outputs are loaded in the entry prelude so `where`
        // constraints can read them. Explicit `create name = ...` field
        // checks must stay in body order because their expected expressions may
        // depend on earlier `let`/index computations.
        let explicit_output_create_bindings = body
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .filter_map(|instruction| match instruction {
                IrInstruction::Create { pattern, .. } => Some(pattern.binding.as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        for (index, pattern) in body.create_set.iter().enumerate() {
            if !matches!(pattern.operation.as_str(), "create" | "create_unique" | "replace_unique") {
                let explicit_output_create = explicit_output_create_bindings.contains(pattern.binding.as_str());
                self.generate_create(pattern, index, !explicit_output_create, explicit_output_create)?;
            }
        }

        for pattern in &body.mutate_set {
            self.generate_mutate_replacement(pattern)?;
        }

        for block in &body.blocks {
            self.generate_block(block)?;
        }

        Ok(())
    }

    fn emit_read_ref_parameter_bindings(&mut self) {
        let mut input_bindings = self
            .read_ref_param_ids
            .iter()
            .filter_map(|(binding, var_id)| {
                self.read_ref_param_input_indices.get(var_id).copied().map(|input_index| (input_index, binding.clone(), *var_id))
            })
            .collect::<Vec<_>>();
        input_bindings.sort_by_key(|(input_index, _, _)| *input_index);
        for (input_index, binding, var_id) in input_bindings {
            let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
                continue;
            };
            let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
                continue;
            };
            self.emit(format!("# cellscript abi: bind read-only param {} to Input#{} cell data", binding, input_index));
            self.emit_load_cell_data_syscall_to_offsets(
                "read_ref_param_input",
                CKB_SOURCE_INPUT,
                input_index,
                size_offset,
                buffer_offset,
                RUNTIME_CELL_BUFFER_SIZE,
            );
            self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
            self.emit_sp_addi("t0", buffer_offset);
            self.emit_stack_store("t0", var_id * 8);
        }

        let mut dep_bindings = self
            .read_ref_param_ids
            .iter()
            .filter_map(|(binding, var_id)| {
                self.read_ref_param_dep_indices.get(var_id).copied().map(|dep_index| (dep_index, binding.clone(), *var_id))
            })
            .collect::<Vec<_>>();
        dep_bindings.sort_by_key(|(dep_index, _, _)| *dep_index);
        for (dep_index, binding, var_id) in dep_bindings {
            let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
                continue;
            };
            let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
                continue;
            };
            self.emit(format!("# cellscript abi: bind read-only param {} to CellDep#{} cell data", binding, dep_index));
            self.emit_load_cell_data_syscall_to_offsets(
                "read_ref_param_dep",
                CKB_SOURCE_CELL_DEP,
                dep_index,
                size_offset,
                buffer_offset,
                RUNTIME_CELL_BUFFER_SIZE,
            );
            self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
            self.emit_sp_addi("t0", buffer_offset);
            self.emit_stack_store("t0", var_id * 8);
        }
    }

    fn generate_consume(&mut self, pattern: &CellPattern, index: usize) -> Result<()> {
        self.emit(format!("# {} input {}", pattern.operation, pattern.binding));
        if let Some(var_id) =
            self.consume_binding_ids.get(&pattern.binding).copied().or_else(|| self.consume_order.get(index).copied())
        {
            if let (Some(size_offset), Some(buffer_offset)) =
                (self.cell_buffer_size_offsets.get(&var_id).copied(), self.cell_buffer_offsets.get(&var_id).copied())
            {
                let input_index = self.consume_indices.get(&var_id).copied().unwrap_or(index);
                self.emit_load_cell_data_syscall_to_offsets(
                    &pattern.operation,
                    CKB_SOURCE_INPUT,
                    input_index,
                    size_offset,
                    buffer_offset,
                    RUNTIME_CELL_BUFFER_SIZE,
                );
                self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
                self.emit_sp_addi("t0", buffer_offset);
                self.emit_stack_store("t0", var_id * 8);
                self.emit_destroy_policy_scan(pattern, input_index);
                return Ok(());
            }
        }

        self.emit_load_cell_data_syscall(&pattern.operation, CKB_SOURCE_INPUT, index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_destroy_policy_scan(pattern, index);
        Ok(())
    }

    fn generate_read_ref(&mut self, pattern: &CellPattern, index: usize) -> Result<()> {
        self.emit(format!("# read_ref {}", pattern.binding));
        if let Some(var_id) = self.read_ref_order.get(index).copied() {
            if let (Some(size_offset), Some(buffer_offset)) =
                (self.cell_buffer_size_offsets.get(&var_id).copied(), self.cell_buffer_offsets.get(&var_id).copied())
            {
                let dep_index = self.read_ref_indices.get(&var_id).copied().unwrap_or(index);
                self.emit_load_cell_data_syscall_to_offsets(
                    "read_ref",
                    CKB_SOURCE_CELL_DEP,
                    dep_index,
                    size_offset,
                    buffer_offset,
                    RUNTIME_CELL_BUFFER_SIZE,
                );
                self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
                self.emit_sp_addi("t0", buffer_offset);
                self.emit_stack_store("t0", var_id * 8);
                return Ok(());
            }
        }

        self.emit_load_cell_data_syscall("read_ref", CKB_SOURCE_CELL_DEP, index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        Ok(())
    }

    fn generate_create(
        &mut self,
        pattern: &CreatePattern,
        index: usize,
        defer_unverifiable_output_fields: bool,
        defer_all_output_fields: bool,
    ) -> Result<()> {
        // The verifier cannot create cells inside CKB-VM; it can only verify the
        // transaction output selected by the lowering metadata.
        self.emit(format!("# {} output {}", pattern.operation, pattern.ty));
        if pattern.operation == "output" {
            if let Some(var_id) = self.output_param_ids.get(&pattern.binding).copied() {
                let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                };
                let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                };
                self.emit_load_cell_data_syscall_to_offsets(
                    "output_param",
                    CKB_SOURCE_OUTPUT,
                    index,
                    size_offset,
                    buffer_offset,
                    RUNTIME_CELL_BUFFER_SIZE,
                );
                self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
                self.emit_sp_addi("t0", buffer_offset);
                self.emit_stack_store("t0", var_id * 8);
                self.operation_output_indices.insert(var_id, index);
                if defer_all_output_fields {
                    self.emit("# cellscript abi: output field verification deferred to ordered create constraint");
                } else if pattern.fields.is_empty() {
                    self.emit_state_transition_check(pattern, size_offset, buffer_offset);
                } else if self.can_verify_create_output_fields(pattern) {
                    self.emit_create_output_checks_at(pattern, size_offset, buffer_offset);
                } else if defer_unverifiable_output_fields && self.create_output_fields_cover_type(pattern) {
                    self.emit("# cellscript abi: output field verification deferred to explicit where constraints");
                } else {
                    self.emit("# cellscript abi: output field verification incomplete for this named output");
                    self.emit("# cellscript abi: fail closed because the output state is not fully verified");
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                }
                if let Some(lock) = &pattern.lock {
                    if !(self.can_verify_output_lock(pattern) && self.emit_output_lock_hash_check(index, lock)) {
                        self.emit("# cellscript abi: output lock verification incomplete for this named output");
                        self.emit("# cellscript abi: fail closed because the output lock is not fully verified");
                        self.emit_fail(CellScriptRuntimeError::EntryWitnessMagicMismatch);
                        return Ok(());
                    }
                }
                self.next_virtual_output = self.next_virtual_output.max(index + 1);
                return Ok(());
            }
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return Ok(());
        }
        self.emit_load_cell_data_syscall(&pattern.operation, CKB_SOURCE_OUTPUT, index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);

        if pattern.lock.is_some() {
            self.emit("# set lock script");
        }

        if self.can_verify_create_output_fields(pattern) {
            self.emit_create_output_checks(pattern);
        } else {
            self.emit("# cellscript abi: output field verification incomplete for this create pattern");
            self.emit("# cellscript abi: fail closed because the output state is not fully verified");
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return Ok(());
        }

        if let Some(lock) = &pattern.lock {
            if self.can_verify_output_lock(pattern) && self.emit_output_lock_hash_check(index, lock) {
                return Ok(());
            }
            self.emit("# cellscript abi: output lock verification incomplete for this create pattern");
            self.emit("# cellscript abi: fail closed because the output lock is not fully verified");
            self.emit_fail(CellScriptRuntimeError::EntryWitnessMagicMismatch);
        }

        Ok(())
    }

    fn generate_mutate_replacement(&mut self, pattern: &MutatePattern) -> Result<()> {
        self.emit(format!(
            "# mutate output {} {} Input#{} -> Output#{}",
            pattern.binding, pattern.ty, pattern.input_index, pattern.output_index
        ));
        self.emit_mutate_parameter_binding(pattern);
        if pattern.preserve_type_hash {
            self.emit_mutate_replacement_field_hash_check(
                pattern,
                CKB_CELL_FIELD_TYPE_HASH,
                "type_hash",
                CellScriptRuntimeError::TypeHashPreservationMismatch,
            );
        }
        if pattern.preserve_lock_hash {
            self.emit_mutate_replacement_field_hash_check(
                pattern,
                CKB_CELL_FIELD_LOCK_HASH,
                "lock_hash",
                CellScriptRuntimeError::LockHashPreservationMismatch,
            );
        }
        self.emit_mutate_replacement_preserved_field_checks(pattern);
        self.emit_mutate_replacement_transition_checks(pattern);
        self.emit_mutate_replacement_set_transition_checks(pattern);
        self.emit_mutate_replacement_u128_transition_checks(pattern);
        Ok(())
    }

    fn emit_mutate_parameter_binding(&mut self, pattern: &MutatePattern) {
        let Some(var_id) = self.mutate_param_ids.get(&pattern.binding).copied() else {
            return;
        };
        let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
            return;
        };
        let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
            return;
        };
        self.emit(format!("# cellscript abi: bind mutable param {} to Input#{} cell data", pattern.binding, pattern.input_index));
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_param_input",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            size_offset,
            buffer_offset,
            RUNTIME_CELL_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", var_id * 8);
    }

    fn generate_block(&mut self, block: &IrBlock) -> Result<()> {
        self.emit_label(&format!(".L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), block.id.0));

        for instruction in &block.instructions {
            self.generate_instruction(instruction)?;
        }

        self.generate_terminator(&block.terminator)?;

        Ok(())
    }

    fn generate_instruction(&mut self, instruction: &IrInstruction) -> Result<()> {
        match instruction {
            IrInstruction::LoadConst { dest, value } => {
                self.emit_load_const(dest, value)?;
            }
            IrInstruction::LoadVar { dest, name } => {
                self.emit_load_var(dest, name)?;
            }
            IrInstruction::StoreVar { name, src } => {
                self.emit_store_var(name, src)?;
            }
            IrInstruction::Binary { dest, op, left, right } => {
                self.emit_binary(dest, *op, left, right)?;
            }
            IrInstruction::Unary { dest, op, operand } => {
                self.emit_unary(dest, *op, operand)?;
            }
            IrInstruction::FieldAccess { dest, obj, field } => {
                self.emit_field_access(dest, obj, field)?;
            }
            IrInstruction::Index { dest, arr, idx } => {
                self.emit_index(dest, arr, idx)?;
            }
            IrInstruction::Length { dest, operand } => {
                self.emit_length(dest, operand)?;
            }
            IrInstruction::TypeHash { dest, operand } => {
                self.emit_type_hash(dest, operand)?;
            }
            IrInstruction::CollectionNew { dest, ty, capacity } => {
                self.emit_collection_new(dest, ty, capacity.as_ref())?;
            }
            IrInstruction::CollectionCapacity { dest, collection } => {
                self.emit_collection_capacity(dest, collection)?;
            }
            IrInstruction::CollectionPush { collection, value } => {
                self.emit_collection_push(collection, value)?;
            }
            IrInstruction::CollectionExtend { collection, slice } => {
                self.emit_collection_extend(collection, slice)?;
            }
            IrInstruction::CollectionClear { collection } => {
                self.emit_collection_clear(collection)?;
            }
            IrInstruction::CollectionReverse { collection } => {
                self.emit_collection_reverse(collection)?;
            }
            IrInstruction::CollectionTruncate { collection, len } => {
                self.emit_collection_truncate(collection, len)?;
            }
            IrInstruction::CollectionSwap { collection, left, right } => {
                self.emit_collection_swap(collection, left, right)?;
            }
            IrInstruction::CollectionContains { dest, collection, value } => {
                self.emit_collection_contains(dest, collection, value)?;
            }
            IrInstruction::CollectionRemove { dest, collection, index } => {
                self.emit_collection_remove(dest, collection, index)?;
            }
            IrInstruction::CollectionInsert { collection, index, value } => {
                self.emit_collection_insert(collection, index, value)?;
            }
            IrInstruction::CollectionSet { collection, index, value } => {
                self.emit_collection_set(collection, index, value)?;
            }
            IrInstruction::CollectionPop { dest, collection } => {
                self.emit_collection_pop(dest, collection)?;
            }
            IrInstruction::Call { dest, func, args } => {
                self.emit_call(dest.as_ref(), func, args)?;
            }
            IrInstruction::ReadRef { dest, ty } => {
                self.emit_read_ref(dest, ty)?;
            }
            IrInstruction::Move { dest, src } => {
                self.emit_move(dest, src)?;
            }
            IrInstruction::Cast { dest, src } => {
                self.emit_cast(dest, src)?;
            }
            IrInstruction::Tuple { dest, fields } => {
                self.emit_tuple(dest, fields)?;
            }
            IrInstruction::Consume { operand } => {
                self.emit_consume(operand)?;
            }
            IrInstruction::Create { dest, pattern } => {
                self.emit_create(dest, pattern)?;
            }
            IrInstruction::Transfer { dest, operand, to } => {
                self.emit_transfer(dest, operand, to)?;
            }
            IrInstruction::Destroy { operand, policy } => {
                self.emit_destroy(operand, policy)?;
            }
            IrInstruction::Claim { dest, receipt } => {
                self.emit_claim(dest, receipt)?;
            }
            IrInstruction::Settle { dest, operand } => {
                self.emit_settle(dest, operand)?;
            }
            IrInstruction::CellMetadataEquality { left, right, field } => {
                self.emit_cell_metadata_equality(left, right, *field)?;
            }
            IrInstruction::CreateUnique { dest, pattern, identity } => {
                self.emit_create_unique(dest, pattern, identity)?;
            }
            IrInstruction::ReplaceUnique { dest, operand, pattern, identity } => {
                self.emit_replace_unique(dest, operand, pattern, identity)?;
            }
        }
        Ok(())
    }

    fn generate_terminator(&mut self, terminator: &IrTerminator) -> Result<()> {
        match terminator {
            IrTerminator::Return(None) => {
                self.emit("li a0, 0");
                self.emit_epilogue();
            }
            IrTerminator::Return(Some(operand)) => {
                if let IrOperand::Var(v) = operand {
                    if let Some(fields) = self.tuple_aggregate_fields.get(&v.id).cloned() {
                        self.emit(format!("# cellscript abi: return tuple aggregate var{} fields={}", v.id, fields.len()));
                        if fields.is_empty() {
                            self.emit("li a0, 0");
                        }
                        for (index, field) in fields.iter().take(8).enumerate() {
                            self.emit(format!("# cellscript abi: return tuple field .{} via a{}", index, index));
                            self.emit_operand_to_register(&format!("a{}", index), field);
                        }
                        self.emit_epilogue();
                        return Ok(());
                    }
                }
                self.emit_operand_to_register("a0", operand);
                if self.current_lock_entry {
                    let ok_label = self.fresh_label("lock_predicate_true");
                    self.emit(format!("bnez a0, {}", ok_label));
                    self.emit_runtime_error_comment(CellScriptRuntimeError::AssertionFailed);
                    self.emit(format!("li a0, {}", CellScriptRuntimeError::AssertionFailed.code()));
                    self.emit_epilogue();
                    self.emit_label(&ok_label);
                    self.emit("li a0, 0");
                    self.emit_epilogue();
                    return Ok(());
                }
                self.emit_epilogue();
            }
            IrTerminator::Jump(block_id) => {
                self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), block_id.0));
            }
            IrTerminator::Branch { cond, then_block, else_block } => match cond {
                IrOperand::Const(IrConst::Bool(b)) => {
                    if *b {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), then_block.0));
                    } else {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), else_block.0));
                    }
                }
                IrOperand::Const(IrConst::U64(n)) => {
                    if *n != 0 {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), then_block.0));
                    } else {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), else_block.0));
                    }
                }
                IrOperand::Const(IrConst::U8(n)) => {
                    if *n != 0 {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), then_block.0));
                    } else {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), else_block.0));
                    }
                }
                IrOperand::Const(IrConst::U16(n)) => {
                    if *n != 0 {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), then_block.0));
                    } else {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), else_block.0));
                    }
                }
                IrOperand::Const(IrConst::U32(n)) => {
                    if *n != 0 {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), then_block.0));
                    } else {
                        self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), else_block.0));
                    }
                }
                IrOperand::Var(v) if v.ty == IrType::U128 => {
                    self.emit("# cellscript abi: fail closed because u128 cannot be used as a branch condition");
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                }
                IrOperand::Var(v) => {
                    self.emit_stack_load("t0", v.id * 8);
                    self.emit(format!("beqz t0, .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), else_block.0));
                    self.emit(format!("j .L{}_block_{}", self.current_function.as_deref().unwrap_or("fn"), then_block.0));
                }
                _ => {
                    self.emit("# cellscript abi: fail closed because branch condition is not a boolean or integer");
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                }
            },
        }
        Ok(())
    }

    fn emit_prologue(&mut self) {
        self.emit_large_addi("sp", "sp", -(self.frame_size as i64));
        self.emit_stack_store("ra", self.frame_size - 8);
        self.emit_stack_store("fp", self.frame_size - 16);
        self.emit_sp_addi("fp", self.frame_size);
    }

    fn emit_epilogue(&mut self) {
        if let Some(function) = &self.current_function {
            self.emit(format!("j .L{}_epilogue", function));
            return;
        }
        self.emit_epilogue_body();
    }

    fn emit_fail(&mut self, error: CellScriptRuntimeError) {
        if let Some(function) = &self.current_function {
            self.fail_handler_codes.insert(error);
            self.emit(format!("j .L{}_fail_{}", function, error.code()));
            return;
        }
        self.emit_runtime_error_comment(error);
        self.emit(format!("li a0, {}", error.code()));
        self.emit_epilogue_body();
    }

    fn emit_shared_epilogue(&mut self) {
        let Some(function) = self.current_function.clone() else {
            return;
        };
        let fail_codes = self.fail_handler_codes.iter().copied().collect::<Vec<_>>();
        for error in fail_codes {
            self.emit_label(&format!(".L{}_fail_{}", function, error.code()));
            self.emit_runtime_error_comment(error);
            self.emit(format!("li a0, {}", error.code()));
            self.emit(format!("j .L{}_epilogue", function));
        }
        self.emit_label(&format!(".L{}_epilogue", function));
        self.emit_epilogue_body();
    }

    fn emit_runtime_error_comment(&mut self, error: CellScriptRuntimeError) {
        self.emit(format!("# cellscript runtime error {} {}", error.code(), error.name()));
    }

    fn emit_epilogue_body(&mut self) {
        self.emit_stack_load("ra", self.frame_size - 8);
        self.emit_stack_load("fp", self.frame_size - 16);
        self.emit_large_addi("sp", "sp", self.frame_size as i64);
        self.emit("ret");
    }

    /// Emit `addi rd, rs1, imm` handling immediates that don't fit in 12 bits.
    fn emit_large_addi(&mut self, rd: &str, rs1: &str, imm: i64) {
        if (-2048..=2047).contains(&imm) {
            self.emit(format!("addi {}, {}, {}", rd, rs1, imm));
        } else {
            let scratch = scratch_register_avoiding(&[rs1]);
            self.emit(format!("li {}, {}", scratch, imm));
            self.emit(format!("add {}, {}, {}", rd, rs1, scratch));
        }
    }

    fn emit_memory_load_with_avoid(&mut self, opcode: &str, dst: &str, base: &str, offset: usize, avoid: &[&str]) {
        let offset = i64::try_from(offset).expect("memory offset should fit in i64");
        if small_signed_immediate(offset) {
            self.emit(format!("{} {}, {}({})", opcode, dst, offset, base));
        } else {
            let mut registers = Vec::with_capacity(2 + avoid.len());
            registers.push(dst);
            registers.push(base);
            registers.extend_from_slice(avoid);
            let scratch = scratch_register_avoiding(&registers);
            self.emit(format!("li {}, {}", scratch, offset));
            self.emit(format!("add {}, {}, {}", scratch, base, scratch));
            self.emit(format!("{} {}, 0({})", opcode, dst, scratch));
        }
    }

    /// Emit `ld rd, offset(sp)` through the centralized stack-offset gate.
    fn emit_stack_load(&mut self, rd: &str, offset: usize) {
        self.emit_stack_access("ld", rd, offset);
    }

    /// Emit `lbu rd, offset(sp)` through the centralized stack-offset gate.
    fn emit_stack_load_byte(&mut self, rd: &str, offset: usize) {
        self.emit_stack_access("lbu", rd, offset);
    }

    /// Emit `sd rs2, offset(sp)` through the centralized stack-offset gate.
    fn emit_stack_store(&mut self, rs2: &str, offset: usize) {
        self.emit_stack_access("sd", rs2, offset);
    }

    /// Emit `sb rs2, offset(sp)` through the centralized stack-offset gate.
    fn emit_stack_store_byte(&mut self, rs2: &str, offset: usize) {
        self.emit_stack_access("sb", rs2, offset);
    }

    fn emit_stack_access(&mut self, opcode: &str, register: &str, offset: usize) {
        let offset = i64::try_from(offset).expect("stack offset should fit in i64");
        if small_signed_immediate(offset) {
            self.emit(format!("{} {}, {}(sp)", opcode, register, offset));
        } else {
            let scratch = scratch_register_avoiding(&[register]);
            self.emit(format!("li {}, {}", scratch, offset));
            self.emit(format!("add {}, sp, {}", scratch, scratch));
            self.emit(format!("{} {}, 0({})", opcode, register, scratch));
        }
    }

    /// Emit `addi rd, sp, offset` handling offsets that don't fit in 12 bits.
    fn emit_sp_addi(&mut self, rd: &str, offset: usize) {
        if offset <= 2047 {
            self.emit(format!("addi {}, sp, {}", rd, offset));
        } else if rd == "sp" {
            self.emit_large_addi("sp", "sp", offset as i64);
        } else {
            self.emit(format!("li {}, {}", rd, offset));
            self.emit(format!("add {}, sp, {}", rd, rd));
        }
    }

    fn prepare_function_layout(&mut self, body: &IrBody, params: &[IrParam]) -> Result<()> {
        let mut max_var_id = None;
        let mut fixed_byte_locals = HashMap::<usize, usize>::new();
        let mut named_vars = BTreeSet::<String>::new();
        for param in params {
            self.record_var(&param.binding, &mut max_var_id);
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                self.record_instruction_var(instruction, &mut max_var_id);
                self.record_instruction_fixed_byte_local(instruction, &mut fixed_byte_locals);
                if let IrInstruction::StoreVar { name, .. } = instruction {
                    named_vars.insert(name.clone());
                }
            }
            self.record_terminator_var(&block.terminator, &mut max_var_id);
        }

        let locals_size = max_var_id.map(|id| (id + 1) * 8).unwrap_or(0);
        self.fixed_byte_local_offsets.clear();
        self.named_var_offsets.clear();
        self.cell_buffer_offsets.clear();
        self.cell_buffer_size_offsets.clear();
        self.dynamic_value_size_offsets.clear();
        self.empty_molecule_vector_vars.clear();
        self.constructed_byte_vectors.clear();
        self.constructed_byte_vector_roots.clear();
        self.verified_collection_construction_vectors.clear();
        self.output_type_hash_sources.clear();
        self.consume_order.clear();
        self.consume_indices.clear();
        self.consume_type_names.clear();
        self.consume_binding_ids.clear();
        self.read_ref_order.clear();
        self.read_ref_indices.clear();
        self.read_ref_param_ids.clear();
        self.read_ref_param_input_indices.clear();
        self.read_ref_param_dep_indices.clear();
        self.output_param_ids.clear();
        self.mutate_param_ids.clear();
        self.schema_pointer_size_offsets.clear();
        self.fixed_byte_param_size_offsets.clear();
        self.param_type_hash_pointer_offsets.clear();
        self.param_type_hash_size_offsets.clear();
        self.param_type_hash_sources.clear();
        self.collection_region_start = 0;
        self.next_collection_slot = 0;

        let schema_param_ids =
            params.iter().filter(|param| named_type_name(&param.ty).is_some()).map(|param| param.binding.id).collect::<BTreeSet<_>>();
        let mut param_type_hash_ids = BTreeSet::new();
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::TypeHash { dest, operand: IrOperand::Var(var) } = instruction {
                    if schema_param_ids.contains(&var.id) {
                        param_type_hash_ids.insert(var.id);
                        self.param_type_hash_sources.insert(dest.id, var.id);
                    }
                }
            }
        }

        let mut next_cell_slot = locals_size;
        let mut fixed_byte_locals = fixed_byte_locals.into_iter().collect::<Vec<_>>();
        fixed_byte_locals.sort_unstable_by_key(|(var_id, _)| *var_id);
        for (var_id, width) in fixed_byte_locals {
            next_cell_slot = align_up(next_cell_slot, 8);
            self.fixed_byte_local_offsets.insert(var_id, next_cell_slot);
            next_cell_slot += align_up(width, 8);
        }
        for name in named_vars {
            next_cell_slot = align_up(next_cell_slot, 8);
            self.named_var_offsets.insert(name, next_cell_slot);
            next_cell_slot += 8;
        }
        for param in params {
            if param.source == ParamSource::Output {
                self.output_param_ids.insert(param.name.clone(), param.binding.id);
                self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
                self.cell_buffer_size_offsets.insert(param.binding.id, next_cell_slot);
                self.cell_buffer_offsets.insert(param.binding.id, next_cell_slot + 8);
                next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                continue;
            }
            if named_type_name(&param.ty).is_some() {
                self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
                next_cell_slot += 8;
            } else if fixed_byte_pointer_param_width(&param.ty).is_some() || fixed_aggregate_pointer_param_width(&param.ty).is_some() {
                self.fixed_byte_param_size_offsets.insert(param.binding.id, next_cell_slot);
                next_cell_slot += 8;
            }
        }
        for param in params {
            if param_type_hash_ids.contains(&param.binding.id) {
                self.param_type_hash_pointer_offsets.insert(param.binding.id, next_cell_slot);
                next_cell_slot += 8;
                self.param_type_hash_size_offsets.insert(param.binding.id, next_cell_slot);
                next_cell_slot += 8;
            }
        }

        if self.bind_readonly_schema_params {
            let consumed_param_names = body.consume_set.iter().map(|pattern| pattern.binding.as_str()).collect::<BTreeSet<_>>();
            let mutate_param_names = body.mutate_set.iter().map(|pattern| pattern.binding.as_str()).collect::<BTreeSet<_>>();
            let read_ref_indices_by_binding =
                body.read_refs.iter().enumerate().map(|(index, pattern)| (pattern.binding.as_str(), index)).collect::<HashMap<_, _>>();
            let mut read_ref_param_index = 0usize;
            for param in params {
                if param.source == ParamSource::Output {
                    continue;
                }
                if !self.param_is_runtime_bound(param) {
                    continue;
                }
                if mutate_param_names.contains(param.name.as_str()) || consumed_param_names.contains(param.name.as_str()) {
                    continue;
                }
                self.read_ref_param_ids.insert(param.name.clone(), param.binding.id);
                if let Some(dep_index) = read_ref_indices_by_binding.get(param.name.as_str()).copied() {
                    self.read_ref_param_dep_indices.insert(param.binding.id, dep_index);
                } else {
                    let input_index = body.consume_set.len() + body.mutate_set.len() + read_ref_param_index;
                    self.read_ref_param_input_indices.insert(param.binding.id, input_index);
                    read_ref_param_index += 1;
                }
                self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
                self.cell_buffer_size_offsets.insert(param.binding.id, next_cell_slot);
                self.cell_buffer_offsets.insert(param.binding.id, next_cell_slot + 8);
                next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
            }
        }

        for pattern in &body.mutate_set {
            let Some(param) = params.iter().find(|param| param.name == pattern.binding) else {
                continue;
            };
            self.mutate_param_ids.insert(pattern.binding.clone(), param.binding.id);
            self.consume_type_names.insert(param.binding.id, pattern.ty.clone());
            self.consume_binding_ids.insert(pattern.binding.clone(), param.binding.id);
            self.consume_indices.insert(param.binding.id, pattern.input_index);
            self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
            self.cell_buffer_size_offsets.insert(param.binding.id, next_cell_slot);
            self.cell_buffer_offsets.insert(param.binding.id, next_cell_slot + 8);
            next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
        }

        let consume_pattern_indices =
            body.consume_set.iter().enumerate().map(|(index, pattern)| (pattern.binding.as_str(), index)).collect::<HashMap<_, _>>();
        for pattern in &body.consume_set {
            let Some(param) = params.iter().find(|param| param.name == pattern.binding) else {
                continue;
            };
            if self.consume_binding_ids.contains_key(&pattern.binding) {
                continue;
            }
            if let Some(type_name) = named_type_name(&param.ty) {
                self.consume_type_names.insert(param.binding.id, type_name.to_string());
            }
            self.consume_binding_ids.insert(pattern.binding.clone(), param.binding.id);
            self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
            self.cell_buffer_size_offsets.insert(param.binding.id, next_cell_slot);
            self.cell_buffer_offsets.insert(param.binding.id, next_cell_slot + 8);
            self.consume_order.push(param.binding.id);
            self.consume_indices.insert(param.binding.id, consume_pattern_indices.get(pattern.binding.as_str()).copied().unwrap_or(0));
            next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let Some(var) = consumed_operand_var(instruction) {
                    if self.consume_binding_ids.contains_key(&var.name) {
                        continue;
                    }
                    if let Some(type_name) = named_type_name(&var.ty) {
                        self.consume_type_names.insert(var.id, type_name.to_string());
                    }
                    self.consume_binding_ids.insert(var.name.clone(), var.id);
                    self.schema_pointer_size_offsets.insert(var.id, next_cell_slot);
                    self.cell_buffer_size_offsets.insert(var.id, next_cell_slot);
                    self.cell_buffer_offsets.insert(var.id, next_cell_slot + 8);
                    self.consume_order.push(var.id);
                    self.consume_indices.insert(
                        var.id,
                        consume_pattern_indices.get(var.name.as_str()).copied().unwrap_or(self.consume_order.len() - 1),
                    );
                    next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                }
            }
        }

        let mut read_ref_index = 0usize;
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::ReadRef { dest, .. } = instruction {
                    self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                    self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                    self.read_ref_order.push(dest.id);
                    self.read_ref_indices.insert(dest.id, read_ref_index);
                    next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                    read_ref_index += 1;
                }
            }
        }

        let mut create_dest_outputs = HashMap::new();
        let mut next_create_output_index =
            body.create_set.iter().position(|pattern| pattern.operation == "create").unwrap_or(body.create_set.len());
        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    IrInstruction::FieldAccess { dest, obj: IrOperand::Var(obj), field } => {
                        if named_type_name(&dest.ty).is_some()
                            && named_type_name(&obj.ty)
                                .and_then(|type_name| self.type_layouts.get(type_name))
                                .and_then(|fields| fields.get(field))
                                .is_some_and(|layout| {
                                    layout_fixed_byte_width(layout).is_none()
                                        && molecule_vector_element_fixed_width(
                                            &layout.ty,
                                            &self.type_fixed_sizes,
                                            &self.enum_fixed_sizes,
                                        )
                                        .is_some()
                                })
                        {
                            self.dynamic_value_size_offsets.insert(dest.id, next_cell_slot);
                            next_cell_slot += 8;
                        }
                    }
                    IrInstruction::Create { dest, pattern } => {
                        let output_index = if pattern.operation == "create" {
                            let output_index = next_create_output_index;
                            next_create_output_index += 1;
                            Some(output_index)
                        } else {
                            Self::create_output_index(body, &pattern.operation, &pattern.binding, &pattern.ty)
                        };
                        if let Some(output_index) = output_index {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::CreateUnique { dest, pattern, .. } | IrInstruction::ReplaceUnique { dest, pattern, .. } => {
                        if let Some(output_index) = Self::create_output_index(body, &pattern.operation, &pattern.binding, &pattern.ty)
                        {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::Transfer { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "transfer", dest) {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::Claim { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "claim", dest) {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::Settle { dest, .. } => {
                        if let Some(output_index) = Self::create_output_index_for_dest(body, "settle", dest) {
                            create_dest_outputs.insert(dest.id, output_index);
                        }
                    }
                    IrInstruction::TypeHash { dest, operand: IrOperand::Var(var) } => {
                        if let Some(output_index) = create_dest_outputs.get(&var.id).copied() {
                            self.output_type_hash_sources.insert(dest.id, output_index);
                            self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                            self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                            next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                        } else if self.consume_indices.contains_key(&var.id)
                            || self.read_ref_indices.contains_key(&var.id)
                            || self.read_ref_param_input_indices.contains_key(&var.id)
                        {
                            self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                            self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                            next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                        }
                    }
                    _ => {}
                }
            }
        }

        let collection_slot_size = 8 + RUNTIME_COLLECTION_BUFFER_SIZE;
        let collection_count = body
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .filter(|instruction| matches!(instruction, IrInstruction::CollectionNew { .. }))
            .count();
        self.collection_region_start = next_cell_slot;
        next_cell_slot += collection_count * collection_slot_size;

        let raw_frame_size = next_cell_slot
            .checked_add(RUNTIME_EXPR_TEMP_SIZE)
            .and_then(|s| s.checked_add(RUNTIME_SCRATCH_SIZE))
            .and_then(|s| s.checked_add(16))
            .ok_or_else(|| {
                CompileError::new("stack frame size overflow: too many local variables or cell buffers", crate::error::Span::default())
            })?;
        self.frame_size = align_frame(raw_frame_size);
        Ok(())
    }

    fn runtime_expr_temp_offset(&self, depth: usize) -> Option<usize> {
        (depth < RUNTIME_EXPR_TEMP_SLOTS).then(|| self.runtime_scratch_size_offset() - RUNTIME_EXPR_TEMP_SIZE + depth * 8)
    }

    fn runtime_scratch_size_offset(&self) -> usize {
        self.frame_size - 16 - RUNTIME_SCRATCH_SIZE
    }

    fn runtime_scratch_buffer_offset(&self) -> usize {
        self.runtime_scratch_size_offset() + 8
    }

    fn runtime_scratch2_size_offset(&self) -> usize {
        self.runtime_scratch_size_offset() + RUNTIME_SCRATCH_SLOT_SIZE
    }

    fn runtime_scratch2_buffer_offset(&self) -> usize {
        self.runtime_scratch2_size_offset() + 8
    }

    fn emit_store_data_args_at(&mut self, max_bytes: usize, size_offset: usize, buffer_offset: usize) {
        self.emit(format!("li t0, {}", max_bytes));
        self.emit_stack_store("t0", size_offset);
        self.emit_sp_addi("a0", buffer_offset);
        self.emit_sp_addi("a1", size_offset);
        self.emit("li a2, 0");
    }

    fn emit_load_cell_data_syscall(&mut self, reason: &str, source: u64, index: usize) {
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(reason, source, index, size_offset, buffer_offset, RUNTIME_SCRATCH_BUFFER_SIZE);
    }

    fn emit_load_cell_data_syscall_to_offsets(
        &mut self,
        reason: &str,
        source: u64,
        index: usize,
        size_offset: usize,
        buffer_offset: usize,
        max_bytes: usize,
    ) {
        self.emit(format!("# cellscript abi: LOAD_CELL_DATA reason={} source={} index={}", reason, ckb_source_name(source), index));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("li a3, {}", index));
        self.emit(format!("li a4, {}", source));
        self.emit(format!("li a7, {}", self.runtime_abi().load_cell_data));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_load_witness_syscall_to_offsets(
        &mut self,
        reason: &str,
        source: u64,
        index: usize,
        size_offset: usize,
        buffer_offset: usize,
        max_bytes: usize,
    ) {
        self.emit(format!("# cellscript abi: LOAD_WITNESS reason={} source={} index={}", reason, ckb_source_name(source), index));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("li a3, {}", index));
        self.emit(format!("li a4, {}", source));
        self.emit(format!("li a7, {}", self.runtime_abi().load_witness));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    #[allow(dead_code)]
    fn emit_load_script_syscall_to_offsets(&mut self, reason: &str, size_offset: usize, buffer_offset: usize, max_bytes: usize) {
        self.emit(format!("# cellscript abi: LOAD_SCRIPT reason={}", reason));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("li a7, {}", self.runtime_abi().load_script));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_load_cell_by_field_syscall_to_offsets(
        &mut self,
        reason: &str,
        source: u64,
        index: usize,
        field: u64,
        size_offset: usize,
        buffer_offset: usize,
        max_bytes: usize,
    ) {
        self.emit(format!(
            "# cellscript abi: LOAD_CELL_BY_FIELD reason={} source={} index={} field={}",
            reason,
            ckb_source_name(source),
            index,
            field
        ));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("li a3, {}", index));
        self.emit(format!("li a4, {}", source));
        self.emit(format!("li a5, {}", field));
        self.emit(format!("li a7, {}", self.runtime_abi().load_cell_by_field));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_load_cell_by_field_syscall_to_offsets_dynamic_index(
        &mut self,
        reason: &str,
        source: u64,
        index_reg: &str,
        field: u64,
        size_offset: usize,
        buffer_offset: usize,
        max_bytes: usize,
    ) {
        self.emit(format!(
            "# cellscript abi: LOAD_CELL_BY_FIELD reason={} source={} index={} field={}",
            reason,
            ckb_source_name(source),
            index_reg,
            field
        ));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("addi a3, {}, 0", index_reg));
        self.emit(format!("li a4, {}", source));
        self.emit(format!("li a5, {}", field));
        self.emit(format!("li a7, {}", self.runtime_abi().load_cell_by_field));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_return_on_syscall_error(&mut self, error: CellScriptRuntimeError) {
        let ok_label = self.fresh_label("ckb_syscall_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_fail(error);
        self.emit_label(&ok_label);
    }

    fn emit_loaded_schema_bounds_check(&mut self, size_offset: usize, required_size: usize, context: &str) {
        self.emit(format!("# cellscript abi: bounds check {} required={}", context, required_size));
        let ok_label = self.fresh_label("schema_bounds_ok");
        self.emit_stack_load("a0", size_offset);
        self.emit(format!("li a1, {}", required_size));
        self.emit("call __cellscript_require_min_size");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&ok_label);
    }

    fn emit_loaded_schema_exact_size_check(&mut self, size_offset: usize, expected_size: usize, context: &str) {
        self.emit(format!("# cellscript abi: exact size check {} expected={}", context, expected_size));
        let ok_label = self.fresh_label("schema_size_ok");
        self.emit_stack_load("a0", size_offset);
        self.emit(format!("li a1, {}", expected_size));
        self.emit("call __cellscript_require_exact_size");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::ExactSizeMismatch);
        self.emit_label(&ok_label);
    }

    fn emit_molecule_table_field_bounds_to_t5(
        &mut self,
        base_reg: &str,
        size_offset: usize,
        field_index: usize,
        field_width: usize,
        context: &str,
    ) {
        self.emit(format!("# cellscript abi: molecule table field {} index={} min_width={}", context, field_index, field_width));
        let field_count = field_index + 1;
        let header_size = 4 + 4 * field_count;
        self.emit_loaded_schema_bounds_check(size_offset, header_size, context);

        self.emit_stack_load("a0", size_offset);
        let total_ok = self.fresh_label("molecule_table_total_ok");
        self.emit_unaligned_scalar_load(base_reg, "t0", "t2", 0, 4);
        self.emit("sub t2, t0, a0");
        self.emit(format!("beqz t2, {}", total_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&total_ok);

        self.emit_unaligned_scalar_load(base_reg, "t5", "t2", 4 + 4 * field_index, 4);
        self.emit(format!("li t1, {}", header_size));
        self.emit("sltu t2, t5, t1");
        let start_ok = self.fresh_label("molecule_table_start_ok");
        self.emit(format!("beqz t2, {}", start_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&start_ok);

        if field_width > 0 {
            self.emit(format!("li t1, {}", field_width));
            self.emit("add t3, t5, t1");
            self.emit("sltu t2, t3, t5");
            let overflow_ok = self.fresh_label("molecule_table_field_overflow_ok");
            self.emit(format!("beqz t2, {}", overflow_ok));
            self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
            self.emit_label(&overflow_ok);
            self.emit("sltu t2, a0, t3");
            let end_ok = self.fresh_label("molecule_table_end_ok");
            self.emit(format!("beqz t2, {}", end_ok));
            self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
            self.emit_label(&end_ok);
        }
    }

    fn emit_molecule_table_field_span_to_t5_t6(
        &mut self,
        base_reg: &str,
        size_offset: usize,
        field_index: usize,
        field_count: usize,
        context: &str,
    ) {
        self.emit(format!(
            "# cellscript abi: molecule table dynamic field {} index={} field_count={}",
            context, field_index, field_count
        ));
        let header_size = 4 + 4 * field_count;
        self.emit_loaded_schema_bounds_check(size_offset, header_size, context);

        self.emit_stack_load("a0", size_offset);
        let total_ok = self.fresh_label("molecule_table_total_ok");
        self.emit_unaligned_scalar_load(base_reg, "t0", "t2", 0, 4);
        self.emit("sub t2, t0, a0");
        self.emit(format!("beqz t2, {}", total_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&total_ok);

        self.emit_unaligned_scalar_load(base_reg, "t5", "t2", 4 + 4 * field_index, 4);
        if field_index + 1 < field_count {
            self.emit_unaligned_scalar_load(base_reg, "t6", "t2", 4 + 4 * (field_index + 1), 4);
        } else {
            self.emit("add t6, a0, zero");
        }

        self.emit(format!("li t1, {}", header_size));
        self.emit("sltu t2, t5, t1");
        let start_ok = self.fresh_label("molecule_table_start_ok");
        self.emit(format!("beqz t2, {}", start_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&start_ok);

        self.emit("sltu t2, t6, t5");
        let order_ok = self.fresh_label("molecule_table_order_ok");
        self.emit(format!("beqz t2, {}", order_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&order_ok);

        self.emit("sltu t2, a0, t6");
        let end_ok = self.fresh_label("molecule_table_end_ok");
        self.emit(format!("beqz t2, {}", end_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&end_ok);
    }

    fn emit_mutate_replacement_field_hash_check(
        &mut self,
        pattern: &MutatePattern,
        cell_field: u64,
        field_name: &str,
        error: CellScriptRuntimeError,
    ) {
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();

        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("mutate_input_{}", field_name),
            CKB_SOURCE_INPUT,
            pattern.input_index,
            cell_field,
            input_size_offset,
            input_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("mutate_output_{}", field_name),
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            cell_field,
            output_size_offset,
            output_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(input_size_offset, 32, &format!("mutate input {}", field_name));
        self.emit_loaded_schema_exact_size_check(output_size_offset, 32, &format!("mutate output {}", field_name));
        self.emit(format!(
            "# cellscript abi: verify mutate output {} {} Input#{} == Output#{} size=32",
            pattern.ty, field_name, pattern.input_index, pattern.output_index
        ));
        self.emit_sp_addi("t4", input_buffer_offset);
        self.emit_sp_addi("t5", output_buffer_offset);
        for byte_index in 0..32 {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("mutate_identity_byte_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_runtime_error_comment(error);
            self.emit(format!("li a0, {}", error.code()));
            self.emit_epilogue();
            self.emit_label(&ok_label);
        }
    }

    fn emit_cell_metadata_equality(&mut self, left: &IrOperand, right: &IrOperand, field: CellMetadataField) -> Result<()> {
        let Some((left_source, left_index)) = self.operand_cell_location(left) else {
            self.emit("# cellscript abi: fail closed because left cell metadata source cannot be determined");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(());
        };
        let Some((right_source, right_index)) = self.operand_cell_location(right) else {
            self.emit("# cellscript abi: fail closed because right cell metadata source cannot be determined");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(());
        };
        let (cell_field, field_name, width, mismatch_error) = match field {
            CellMetadataField::LockHash => {
                (CKB_CELL_FIELD_LOCK_HASH, "lock_hash", 32usize, CellScriptRuntimeError::LockHashPreservationMismatch)
            }
            CellMetadataField::Capacity => {
                (CKB_CELL_FIELD_CAPACITY, "capacity", 8usize, CellScriptRuntimeError::CapacityPreservationMismatch)
            }
        };

        let left_size_offset = self.runtime_scratch_size_offset();
        let left_buffer_offset = self.runtime_scratch_buffer_offset();
        let right_size_offset = self.runtime_scratch2_size_offset();
        let right_buffer_offset = self.runtime_scratch2_buffer_offset();

        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("cell_metadata_left_{}", field_name),
            left_source,
            left_index,
            cell_field,
            left_size_offset,
            left_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_by_field_syscall_to_offsets(
            &format!("cell_metadata_right_{}", field_name),
            right_source,
            right_index,
            cell_field,
            right_size_offset,
            right_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(left_size_offset, width, &format!("cell metadata left {}", field_name));
        self.emit_loaded_schema_exact_size_check(right_size_offset, width, &format!("cell metadata right {}", field_name));
        self.emit(format!(
            "# cellscript abi: verify cell metadata {} equality {}#{} == {}#{} size={}",
            field_name,
            ckb_source_name(left_source),
            left_index,
            ckb_source_name(right_source),
            right_index,
            width
        ));
        self.emit_sp_addi("t4", left_buffer_offset);
        self.emit_sp_addi("t5", right_buffer_offset);
        for byte_index in 0..width {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("cell_metadata_byte_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_runtime_error_comment(mismatch_error);
            self.emit(format!("li a0, {}", mismatch_error.code()));
            self.emit_epilogue();
            self.emit_label(&ok_label);
        }
        Ok(())
    }

    fn emit_cell_field_hash_equality(
        &mut self,
        left_reason: &str,
        left_source: u64,
        left_index: usize,
        right_reason: &str,
        right_source: u64,
        right_index: usize,
        cell_field: u64,
        field_name: &str,
        detail: &str,
        error: CellScriptRuntimeError,
    ) {
        let left_size_offset = self.runtime_scratch_size_offset();
        let left_buffer_offset = self.runtime_scratch_buffer_offset();
        let right_size_offset = self.runtime_scratch2_size_offset();
        let right_buffer_offset = self.runtime_scratch2_buffer_offset();

        self.emit_load_cell_by_field_syscall_to_offsets(
            left_reason,
            left_source,
            left_index,
            cell_field,
            left_size_offset,
            left_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(error);
        self.emit_load_cell_by_field_syscall_to_offsets(
            right_reason,
            right_source,
            right_index,
            cell_field,
            right_size_offset,
            right_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(error);
        self.emit_loaded_schema_exact_size_check(left_size_offset, 32, &format!("{} {}", left_reason, field_name));
        self.emit_loaded_schema_exact_size_check(right_size_offset, 32, &format!("{} {}", right_reason, field_name));
        self.emit(format!(
            "# cellscript abi: verify {} {} {}#{} == {}#{} size=32",
            detail,
            field_name,
            ckb_source_name(left_source),
            left_index,
            ckb_source_name(right_source),
            right_index
        ));
        self.emit_sp_addi("a0", left_buffer_offset);
        self.emit_sp_addi("a1", right_buffer_offset);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        let ok_label = self.fresh_label("identity_hash_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_runtime_error_comment(error);
        self.emit(format!("li a0, {}", error.code()));
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn emit_output_type_hash_present_check(&mut self, output_index: usize, context: &str) {
        let size_offset = self.runtime_scratch2_size_offset();
        let buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_by_field_syscall_to_offsets(
            context,
            CKB_SOURCE_OUTPUT,
            output_index,
            CKB_CELL_FIELD_TYPE_HASH,
            size_offset,
            buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::TypeHashMismatch);
        self.emit_loaded_schema_exact_size_check(size_offset, 32, context);
        self.emit(format!("# cellscript abi: verify {} Output#{} TypeHash is present size=32", context, output_index));
    }

    fn emit_loaded_fixed_field_pointer_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        width: usize,
        context: &str,
        pointer_stack_offset: usize,
    ) {
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        self.emit_sp_addi("t5", buffer_offset + layout.offset);
        self.emit_stack_store("t5", pointer_stack_offset);
    }

    fn emit_dynamic_fixed_field_pointer_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        field_count: usize,
        width: usize,
        context: &str,
        pointer_stack_offset: usize,
        len_stack_offset: usize,
    ) {
        self.emit_dynamic_table_field_span_to_stack(
            size_offset,
            buffer_offset,
            layout.index,
            field_count,
            context,
            pointer_stack_offset,
            len_stack_offset,
        );
        self.emit_stack_load("t0", len_stack_offset);
        self.emit(format!("li t1, {}", width));
        self.emit("sub t2, t0, t1");
        let ok_label = self.fresh_label("identity_field_len_ok");
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit_runtime_error_comment(CellScriptRuntimeError::DynamicFieldValueMismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DynamicFieldValueMismatch.code()));
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn emit_fixed_pointer_equality(
        &mut self,
        left_pointer_stack_offset: usize,
        right_pointer_stack_offset: usize,
        width: usize,
        context: &str,
        error: CellScriptRuntimeError,
    ) {
        self.emit(format!("# cellscript abi: verify {} size={}", context, width));
        self.emit_stack_load("a0", left_pointer_stack_offset);
        self.emit_stack_load("a1", right_pointer_stack_offset);
        self.emit(format!("li a2, {}", width));
        self.emit("call __cellscript_memcmp_fixed");
        let ok_label = self.fresh_label("identity_field_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit_runtime_error_comment(error);
        self.emit(format!("li a0, {}", error.code()));
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn operand_cell_location(&self, operand: &IrOperand) -> Option<(u64, usize)> {
        let IrOperand::Var(var) = operand else {
            return None;
        };
        if let Some(input_index) = self.consume_indices.get(&var.id).copied() {
            Some((CKB_SOURCE_INPUT, input_index))
        } else if let Some(output_index) = self.operation_output_indices.get(&var.id).copied() {
            Some((CKB_SOURCE_OUTPUT, output_index))
        } else if let Some(dep_index) = self.read_ref_indices.get(&var.id).copied() {
            Some((CKB_SOURCE_CELL_DEP, dep_index))
        } else if let Some(input_index) = self.read_ref_param_input_indices.get(&var.id).copied() {
            Some((CKB_SOURCE_INPUT, input_index))
        } else {
            self.read_ref_param_dep_indices.get(&var.id).copied().map(|dep_index| (CKB_SOURCE_CELL_DEP, dep_index))
        }
    }

    fn emit_destroy_policy_scan(&mut self, pattern: &CellPattern, input_index: usize) {
        if pattern.operation != "destroy" {
            return;
        }
        let policy = pattern.destruction_policy.as_ref().unwrap_or(&IrDestructionPolicy::Default);
        self.emit(format!("# cellscript abi: destroy policy {} for {}", destruction_policy_label(policy), pattern.binding));
        if destroy_policy_uses_output_absence_scan(policy) {
            self.emit_destroy_group_output_absence_scan(pattern, input_index);
            return;
        }
        match policy {
            IrDestructionPolicy::Instance { identity_field } => {
                self.emit(format!(
                    "# cellscript abi: destroy_instance {}.{} is metadata-visible and runtime-required; no same-TypeHash absence scan emitted",
                    pattern.binding, identity_field
                ));
            }
            IrDestructionPolicy::BurnAmount { field } => {
                self.emit(format!(
                    "# cellscript abi: burn_amount {}.{} is metadata-visible and runtime-required; no same-TypeHash absence scan emitted",
                    pattern.binding, field
                ));
            }
            IrDestructionPolicy::Unique { identity } => {
                self.emit(format!("# cellscript abi: fail closed because destroy_unique identity '{}' is not executable", identity));
                self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            }
            IrDestructionPolicy::Default | IrDestructionPolicy::SingletonType => {}
        }
    }

    fn emit_destroy_group_output_absence_scan(&mut self, pattern: &CellPattern, input_index: usize) {
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        let loop_label = self.fresh_label("destroy_output_scan");
        let type_hash_label = self.fresh_label("destroy_output_type_hash");
        let next_label = self.fresh_label("destroy_output_next");
        let done_label = self.fresh_label("destroy_output_done");

        self.emit(format!("# cellscript abi: destroy output type-hash absence scan binding={} size=32", pattern.binding));
        self.emit_load_cell_by_field_syscall_to_offsets(
            "destroy_input_type_hash",
            CKB_SOURCE_INPUT,
            input_index,
            CKB_CELL_FIELD_TYPE_HASH,
            input_size_offset,
            input_buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(input_size_offset, 32, "destroy input type hash");
        self.emit("li t6, 0");
        self.emit_label(&loop_label);
        self.emit_load_cell_by_field_syscall_to_offsets_dynamic_index(
            "destroy_output_type_hash",
            CKB_SOURCE_OUTPUT,
            "t6",
            CKB_CELL_FIELD_TYPE_HASH,
            output_size_offset,
            output_buffer_offset,
            32,
        );
        self.emit(format!("beqz a0, {}", type_hash_label));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", done_label));
        self.emit(format!("li t0, {}", CKB_ITEM_MISSING));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", next_label));
        self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);

        self.emit_label(&type_hash_label);
        self.emit_loaded_schema_exact_size_check(output_size_offset, 32, "destroy output type hash");
        self.emit(format!("# cellscript abi: reject destroy successor when Output#t6 TypeHash matches consumed {}", pattern.binding));
        self.emit_sp_addi("t4", output_buffer_offset);
        self.emit_sp_addi("t5", input_buffer_offset);
        for byte_index in 0..32 {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", next_label));
        }
        self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);

        self.emit_label(&next_label);
        self.emit("addi t6, t6, 1");
        self.emit(format!("j {}", loop_label));
        self.emit_label(&done_label);
        self.emit("li a0, 0");
    }

    fn mutate_preserved_field_layouts(&self, pattern: &MutatePattern) -> Vec<(String, SchemaFieldLayout, usize)> {
        let Some(type_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return Vec::new();
        };
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return Vec::new();
        }
        pattern
            .preserved_fields
            .iter()
            .filter_map(|field| {
                let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(field)).cloned()?;
                let width = layout_fixed_byte_width(&layout)?;
                (layout.offset + width <= RUNTIME_SCRATCH_BUFFER_SIZE).then(|| (field.clone(), layout, width))
            })
            .collect()
    }

    fn mutate_transition_exclusion_ranges(&self, pattern: &MutatePattern) -> Option<Vec<(usize, usize)>> {
        if pattern.transitions.len() != pattern.fields.len() {
            return None;
        }
        let type_size = self.type_fixed_sizes.get(&pattern.ty).copied()?;
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return None;
        }
        let mut ranges = Vec::new();
        for transition in &pattern.transitions {
            let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field))?;
            let width = layout_fixed_byte_width(layout)?;
            if layout.offset + width > RUNTIME_SCRATCH_BUFFER_SIZE {
                return None;
            }
            ranges.push((layout.offset, layout.offset + width));
        }
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in ranges {
            if start >= end {
                continue;
            }
            if let Some(last) = merged.last_mut() {
                if start <= last.1 {
                    last.1 = last.1.max(end);
                    continue;
                }
            }
            merged.push((start, end));
        }
        Some(merged)
    }

    fn emit_mutate_replacement_preserved_field_checks(&mut self, pattern: &MutatePattern) {
        let preserved_fields = self.mutate_preserved_field_layouts(pattern);
        if !pattern.preserved_fields.is_empty() && preserved_fields.len() != pattern.preserved_fields.len() {
            if self.emit_mutate_replacement_dynamic_table_preserved_field_checks(pattern) {
                return;
            }
            if self.emit_mutate_replacement_data_except_transition_checks(pattern) {
                return;
            }
            self.emit("# cellscript abi: fail closed because not all preserved fields are verifier-addressable");
            self.emit_fail(CellScriptRuntimeError::FieldPreservationMismatch);
            return;
        }
        if preserved_fields.is_empty() {
            return;
        }
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_data",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_data",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(input_size_offset, expected_size, &format!("{} mutate input", pattern.ty));
            self.emit_loaded_schema_exact_size_check(output_size_offset, expected_size, &format!("{} mutate output", pattern.ty));
        }
        self.emit(format!(
            "# cellscript abi: verify mutate preserved fields {} Input#{} == Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        self.emit_sp_addi("t4", input_buffer_offset);
        self.emit_sp_addi("t5", output_buffer_offset);
        for (field, layout, width) in preserved_fields {
            self.emit_loaded_schema_bounds_check(input_size_offset, layout.offset + width, &format!("{} input.{}", pattern.ty, field));
            self.emit_loaded_schema_bounds_check(
                output_size_offset,
                layout.offset + width,
                &format!("{} output.{}", pattern.ty, field),
            );
            self.emit(format!(
                "# cellscript abi: verify mutate preserved field {}.{} Input#{} == Output#{} offset={} size={}",
                pattern.ty, field, pattern.input_index, pattern.output_index, layout.offset, width
            ));
            let mismatch_label = self.fresh_label("mutate_preserved_byte_mismatch");
            for byte_index in 0..width {
                self.emit(format!("lbu t0, {}(t4)", layout.offset + byte_index));
                self.emit(format!("lbu t1, {}(t5)", layout.offset + byte_index));
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch_label));
            }
            self.emit_fixed_byte_mismatch_fail(&mismatch_label, CellScriptRuntimeError::FieldPreservationMismatch);
        }
    }

    fn emit_mutate_replacement_dynamic_table_preserved_field_checks(&mut self, pattern: &MutatePattern) -> bool {
        if self.type_fixed_sizes.contains_key(&pattern.ty) || pattern.preserved_fields.is_empty() {
            return false;
        }
        let Some(layouts) = self.type_layouts.get(&pattern.ty).cloned() else {
            return false;
        };
        let field_count = layouts.len();
        if field_count == 0 || !pattern.preserved_fields.iter().all(|field| layouts.contains_key(field)) {
            return false;
        }

        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_table_preserved",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_table_preserved",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit(format!(
            "# cellscript abi: verify mutate preserved Molecule table fields {} Input#{} == Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        for field in &pattern.preserved_fields {
            let Some(layout) = layouts.get(field).cloned() else {
                return false;
            };
            self.emit_dynamic_table_field_equality_check(
                &pattern.ty,
                field,
                &layout,
                field_count,
                input_size_offset,
                input_buffer_offset,
                output_size_offset,
                output_buffer_offset,
                CellScriptRuntimeError::FieldPreservationMismatch,
            );
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_dynamic_table_field_equality_check(
        &mut self,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
        field_count: usize,
        input_size_offset: usize,
        input_buffer_offset: usize,
        output_size_offset: usize,
        output_buffer_offset: usize,
        fail_code: CellScriptRuntimeError,
    ) {
        let start_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        let output_start_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
        if let Some(width) = layout_fixed_byte_width(layout) {
            self.emit_dynamic_table_fixed_field_pointer_to_stack(
                input_size_offset,
                input_buffer_offset,
                layout,
                width,
                &format!("{} input.{}", type_name, field),
                start_offset,
            );
            self.emit_dynamic_table_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                layout,
                width,
                &format!("{} output.{}", type_name, field),
                output_start_offset,
            );
            self.emit(format!("li t0, {}", width));
            self.emit_stack_store("t0", len_offset);
        } else {
            self.emit_dynamic_table_field_span_to_stack(
                input_size_offset,
                input_buffer_offset,
                layout.index,
                field_count,
                &format!("{} input.{}", type_name, field),
                start_offset,
                len_offset,
            );
            self.emit_dynamic_table_field_span_to_stack(
                output_size_offset,
                output_buffer_offset,
                layout.index,
                field_count,
                &format!("{} output.{}", type_name, field),
                output_start_offset,
                self.runtime_expr_temp_offset(3).expect("runtime temp slot 3"),
            );
            self.emit_stack_load("t0", len_offset);
            self.emit_stack_load("t1", self.runtime_expr_temp_offset(3).expect("runtime temp slot 3"));
            self.emit("sub t2, t0, t1");
            let len_ok = self.fresh_label("mutate_table_field_len_ok");
            self.emit(format!("beqz t2, {}", len_ok));
            self.emit_fail(fail_code);
            self.emit_label(&len_ok);
        }

        self.emit(format!(
            "# cellscript abi: verify mutate preserved Molecule table field {}.{} Input#{} == Output#{}",
            type_name, field, 0, 1
        ));
        let mismatch_label = self.fresh_label("mutate_table_field_mismatch");
        self.emit_stack_load("a0", start_offset);
        self.emit_stack_load("a1", output_start_offset);
        self.emit_stack_load("a2", len_offset);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch_label));
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, fail_code);
    }

    fn emit_dynamic_table_field_span_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        field_index: usize,
        field_count: usize,
        context: &str,
        start_stack_offset: usize,
        len_stack_offset: usize,
    ) {
        self.emit_sp_addi("t4", buffer_offset);
        self.emit_molecule_table_field_span_to_t5_t6("t4", size_offset, field_index, field_count, context);
        self.emit_sp_addi("t4", buffer_offset);
        self.emit("add t5, t4, t5");
        self.emit("add t6, t4, t6");
        self.emit("sub t0, t6, t5");
        self.emit_stack_store("t5", start_stack_offset);
        self.emit_stack_store("t0", len_stack_offset);
    }

    fn emit_dynamic_table_fixed_field_pointer_to_stack(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        width: usize,
        context: &str,
        start_stack_offset: usize,
    ) {
        self.emit_sp_addi("t4", buffer_offset);
        self.emit_molecule_table_field_bounds_to_t5("t4", size_offset, layout.index, width, context);
        self.emit_sp_addi("t4", buffer_offset);
        self.emit("add t5, t4, t5");
        self.emit_stack_store("t5", start_stack_offset);
    }

    fn emit_mutate_replacement_dynamic_table_append_checks(&mut self, pattern: &MutatePattern) -> bool {
        if self.type_fixed_sizes.contains_key(&pattern.ty) || pattern.transitions.is_empty() {
            return false;
        }
        let Some(layouts) = self.type_layouts.get(&pattern.ty).cloned() else {
            return false;
        };
        let field_count = layouts.len();
        let appends = pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                if transition.op != MutateTransitionOp::Append {
                    return None;
                }
                let layout = layouts.get(&transition.field).cloned()?;
                let element_width = molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)?;
                self.fixed_append_fields(&transition.operand, element_width)
                    .map(|fields| (transition.clone(), layout, element_width, fields))
            })
            .collect::<Vec<_>>();
        if appends.len() != pattern.transitions.len() {
            return false;
        }

        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_table_append",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_table_append",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit(format!(
            "# cellscript abi: verify mutate Molecule table append fields {} Input#{} -> Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        for (transition, layout, element_width, fields) in appends {
            self.emit_dynamic_table_vector_append_check(
                &pattern.ty,
                &transition.field,
                &layout,
                field_count,
                element_width,
                &fields,
                input_size_offset,
                input_buffer_offset,
                output_size_offset,
                output_buffer_offset,
            );
        }
        true
    }

    fn fixed_append_fields(&self, operand: &IrOperand, expected_width: usize) -> Option<Vec<(IrOperand, SchemaFieldLayout, usize)>> {
        if self.expected_fixed_byte_source(operand, expected_width).is_some() {
            let ty = match operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(IrConst::Address(_)) => IrType::Address,
                IrOperand::Const(IrConst::Hash(_)) => IrType::Hash,
                IrOperand::Const(IrConst::Array(items)) => IrType::Array(Box::new(IrType::U8), items.len()),
                IrOperand::Const(_) => return None,
            };
            return Some(vec![(
                operand.clone(),
                SchemaFieldLayout { index: 0, offset: 0, ty, fixed_size: Some(expected_width), fixed_enum_size: None },
                expected_width,
            )]);
        }
        let IrOperand::Var(var) = operand else {
            return None;
        };
        let fields = self.tuple_aggregate_fields.get(&var.id)?;
        let type_name = named_type_name(&var.ty)?;
        let mut layouts = self.type_layouts.get(type_name)?.values().cloned().collect::<Vec<_>>();
        layouts.sort_by_key(|layout| layout.offset);
        if layouts.len() != fields.len() {
            return None;
        }
        let total_width = self.type_fixed_sizes.get(type_name).copied()?;
        if total_width != expected_width {
            return None;
        }
        fields
            .iter()
            .cloned()
            .zip(layouts)
            .map(|(field_operand, layout)| {
                let width = layout_fixed_byte_width(&layout)?;
                self.expected_fixed_byte_source(&field_operand, width)?;
                Some((field_operand, layout, width))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_dynamic_table_vector_append_check(
        &mut self,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
        field_count: usize,
        element_width: usize,
        fields: &[(IrOperand, SchemaFieldLayout, usize)],
        input_size_offset: usize,
        input_buffer_offset: usize,
        output_size_offset: usize,
        output_buffer_offset: usize,
    ) {
        let input_start_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let input_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        let output_start_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
        let output_len_offset = self.runtime_expr_temp_offset(3).expect("runtime temp slot 3");
        self.emit_dynamic_table_field_span_to_stack(
            input_size_offset,
            input_buffer_offset,
            layout.index,
            field_count,
            &format!("{} input.{}", type_name, field),
            input_start_offset,
            input_len_offset,
        );
        self.emit_dynamic_table_field_span_to_stack(
            output_size_offset,
            output_buffer_offset,
            layout.index,
            field_count,
            &format!("{} output.{}", type_name, field),
            output_start_offset,
            output_len_offset,
        );
        self.emit(format!(
            "# cellscript abi: verify mutate Molecule vector append {}.{} element_size={}",
            type_name, field, element_width
        ));
        self.emit_loaded_schema_bounds_check(input_len_offset, 4, &format!("{} input.{} vector", type_name, field));
        self.emit_loaded_schema_bounds_check(output_len_offset, 4 + element_width, &format!("{} output.{} vector", type_name, field));

        self.emit_stack_load("t4", input_start_offset);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, 4);
        self.emit_stack_load("t1", input_len_offset);
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t0, t2");
        self.emit("addi t3, t3, 4");
        self.emit("sub t2, t1, t3");
        let input_size_ok = self.fresh_label("molecule_append_input_size_ok");
        self.emit(format!("beqz t2, {}", input_size_ok));
        self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
        self.emit_label(&input_size_ok);

        self.emit_stack_load("t4", output_start_offset);
        self.emit_unaligned_scalar_load("t4", "t1", "t2", 0, 4);
        self.emit("addi t0, t0, 1");
        self.emit("sub t2, t1, t0");
        let count_ok = self.fresh_label("molecule_append_count_ok");
        self.emit(format!("beqz t2, {}", count_ok));
        self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
        self.emit_label(&count_ok);

        self.emit_stack_load("t0", input_len_offset);
        self.emit(format!("li t1, {}", element_width));
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t1", output_len_offset);
        self.emit("sub t2, t1, t0");
        let len_ok = self.fresh_label("molecule_append_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
        self.emit_label(&len_ok);

        let prefix_ok = self.fresh_label("molecule_append_prefix_ok");
        self.emit_stack_load("a0", input_start_offset);
        self.emit("addi a0, a0, 4");
        self.emit_stack_load("a1", output_start_offset);
        self.emit("addi a1, a1, 4");
        self.emit_stack_load("a2", input_len_offset);
        self.emit("addi a2, a2, -4");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("beqz a0, {}", prefix_ok));
        self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
        self.emit_label(&prefix_ok);

        self.emit_stack_load("t0", output_start_offset);
        self.emit_stack_load("t1", input_len_offset);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", output_start_offset);
        for (operand, field_layout, width) in fields {
            let Some(source) = self.expected_fixed_byte_source(operand, *width) else {
                self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
                continue;
            };
            self.emit_prepare_fixed_byte_source(&source, *width, &format!("append {}.{}", type_name, field));
            self.emit_pointer_fixed_bytes_against_source(
                output_start_offset,
                field_layout.offset,
                &source,
                *width,
                CellScriptRuntimeError::MutateTransitionMismatch,
            );
        }
    }

    fn emit_pointer_fixed_bytes_against_source(
        &mut self,
        output_pointer_stack_offset: usize,
        output_field_offset: usize,
        source: &ExpectedFixedByteSource,
        width: usize,
        fail_code: CellScriptRuntimeError,
    ) {
        let mismatch_label = self.fresh_label("fixed_byte_mismatch");
        match source {
            ExpectedFixedByteSource::Const(bytes) => {
                self.emit_stack_load("t4", output_pointer_stack_offset);
                for (byte_index, byte) in bytes.iter().take(width).enumerate() {
                    self.emit(format!("lbu t0, {}(t4)", output_field_offset + byte_index));
                    self.emit(format!("li t1, {}", byte));
                    self.emit("sub t2, t0, t1");
                    self.emit(format!("bnez t2, {}", mismatch_label));
                }
            }
            ExpectedFixedByteSource::SchemaField(source) => {
                if self.emit_schema_field_source_pointer_to("a1", source, width) {
                    self.emit_stack_load("a0", output_pointer_stack_offset);
                    if output_field_offset != 0 {
                        self.emit_large_addi("a0", "a0", output_field_offset as i64);
                    }
                    self.emit(format!("li a2, {}", width));
                    self.emit("call __cellscript_memcmp_fixed");
                    self.emit(format!("bnez a0, {}", mismatch_label));
                } else {
                    self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
                }
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_stack_load("a0", output_pointer_stack_offset);
                if output_field_offset != 0 {
                    self.emit_large_addi("a0", "a0", output_field_offset as i64);
                }
                self.emit_sp_addi("a1", var_id * 8);
                self.emit(format!("li a2, {}", width));
                self.emit("call __cellscript_memcmp_fixed");
                self.emit(format!("bnez a0, {}", mismatch_label));
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_stack_load("a0", output_pointer_stack_offset);
                if output_field_offset != 0 {
                    self.emit_large_addi("a0", "a0", output_field_offset as i64);
                }
                self.emit_stack_load("a1", var_id * 8);
                self.emit(format!("li a2, {}", width));
                self.emit("call __cellscript_memcmp_fixed");
                self.emit(format!("bnez a0, {}", mismatch_label));
            }
        }
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, fail_code);
    }

    fn emit_mutate_replacement_data_except_transition_checks(&mut self, pattern: &MutatePattern) -> bool {
        let Some(exclusion_ranges) = self.mutate_transition_exclusion_ranges(pattern) else {
            return false;
        };
        if exclusion_ranges.is_empty() {
            return false;
        }
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_preserved_data",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_preserved_data",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(
                input_size_offset,
                expected_size,
                &format!("{} preserved-data input", pattern.ty),
            );
            self.emit_loaded_schema_exact_size_check(
                output_size_offset,
                expected_size,
                &format!("{} preserved-data output", pattern.ty),
            );
        }
        let size_ok_label = self.fresh_label("mutate_preserved_data_size_ok");
        self.emit_stack_load("t0", input_size_offset);
        self.emit_stack_load("t1", output_size_offset);
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", size_ok_label));
        self.emit_fail(CellScriptRuntimeError::FieldPreservationMismatch);
        self.emit_label(&size_ok_label);

        self.emit(format!(
            "# cellscript abi: verify mutate preserved data {} Input#{} == Output#{} except transition ranges {:?}",
            pattern.ty, pattern.input_index, pattern.output_index, exclusion_ranges
        ));
        let loop_label = self.fresh_label("mutate_preserved_data_loop");
        let compare_label = self.fresh_label("mutate_preserved_data_compare");
        let skip_label = self.fresh_label("mutate_preserved_data_skip");
        let done_label = self.fresh_label("mutate_preserved_data_done");
        let mismatch_label = self.fresh_label("mutate_preserved_data_mismatch");
        self.emit_sp_addi("a3", input_buffer_offset);
        self.emit_sp_addi("a4", output_buffer_offset);
        self.emit("li t6, 0");
        self.emit_label(&loop_label);
        self.emit("sltu t2, t6, t0");
        self.emit(format!("beqz t2, {}", done_label));
        for (range_index, (start, end)) in exclusion_ranges.iter().enumerate() {
            let next_range_label = self.fresh_label(&format!("mutate_preserved_data_next_range_{}", range_index));
            self.emit(format!("li t3, {}", start));
            self.emit("sltu t2, t6, t3");
            self.emit(format!("bnez t2, {}", compare_label));
            self.emit(format!("li t3, {}", end));
            self.emit("sltu t2, t6, t3");
            self.emit(format!("beqz t2, {}", next_range_label));
            self.emit(format!("j {}", skip_label));
            self.emit_label(&next_range_label);
        }
        self.emit_label(&compare_label);
        self.emit("add t3, a3, t6");
        self.emit("lbu t4, 0(t3)");
        self.emit("add t3, a4, t6");
        self.emit("lbu t5, 0(t3)");
        self.emit("sub t2, t4, t5");
        self.emit(format!("bnez t2, {}", mismatch_label));
        self.emit_label(&skip_label);
        self.emit("addi t6, t6, 1");
        self.emit(format!("j {}", loop_label));
        self.emit_label(&mismatch_label);
        self.emit_fail(CellScriptRuntimeError::FieldPreservationMismatch);
        self.emit_label(&done_label);
        true
    }

    fn mutate_u128_transition_layouts(&self, pattern: &MutatePattern) -> Vec<(MutateFieldTransition, SchemaFieldLayout)> {
        let Some(type_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return Vec::new();
        };
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return Vec::new();
        }
        pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                if transition.op == MutateTransitionOp::Set {
                    return None;
                }
                let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field)).cloned()?;
                // Only u128 fields (16 bytes) that don't fit in a single register.
                if layout.ty != IrType::U128 || layout.fixed_size != Some(16) {
                    return None;
                }
                if layout.offset + 16 > RUNTIME_SCRATCH_BUFFER_SIZE {
                    return None;
                }
                // u128 transition: the operand must be a u64 value (delta always fits in 64 bits).
                self.prelude_u64_operand_source(&transition.operand)?;
                Some((transition.clone(), layout))
            })
            .collect()
    }

    fn mutate_transition_layouts(&self, pattern: &MutatePattern) -> Vec<(MutateFieldTransition, SchemaFieldLayout, usize)> {
        let Some(type_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return Vec::new();
        };
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return Vec::new();
        }
        pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                if transition.op == MutateTransitionOp::Set {
                    return None;
                }
                let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field)).cloned()?;
                let width = fixed_register_width(&layout.ty, layout.fixed_size)?;
                if layout.offset + width > RUNTIME_SCRATCH_BUFFER_SIZE {
                    return None;
                }
                self.prelude_u64_operand_source(&transition.operand)?;
                Some((transition.clone(), layout, width))
            })
            .collect()
    }

    fn mutate_set_transition_layouts(&self, pattern: &MutatePattern) -> Vec<(MutateFieldTransition, SchemaFieldLayout, usize)> {
        let Some(type_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return Vec::new();
        };
        if type_size > RUNTIME_SCRATCH_BUFFER_SIZE {
            return Vec::new();
        }
        pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                if transition.op != MutateTransitionOp::Set {
                    return None;
                }
                let layout = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&transition.field)).cloned()?;
                let width = layout_fixed_byte_width(&layout)?;
                if layout.offset + width > RUNTIME_SCRATCH_BUFFER_SIZE {
                    return None;
                }
                if layout_fixed_scalar_width(&layout).is_none()
                    && self.expected_fixed_byte_source(&transition.operand, width).is_none()
                {
                    return None;
                }
                Some((transition.clone(), layout, width))
            })
            .collect()
    }

    fn emit_mutate_replacement_transition_checks(&mut self, pattern: &MutatePattern) {
        if self.emit_mutate_replacement_dynamic_table_append_checks(pattern) {
            return;
        }
        if self.emit_mutate_replacement_dynamic_table_transition_checks(pattern) {
            return;
        }
        let transitions = self.mutate_transition_layouts(pattern);
        if transitions.is_empty() {
            return;
        }
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_transition",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_transition",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(
                input_size_offset,
                expected_size,
                &format!("{} mutate transition input", pattern.ty),
            );
            self.emit_loaded_schema_exact_size_check(
                output_size_offset,
                expected_size,
                &format!("{} mutate transition output", pattern.ty),
            );
        }
        self.emit(format!(
            "# cellscript abi: verify mutate transition fields {} Input#{} -> Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        for (transition, layout, width) in transitions {
            let Some(delta) = self.prelude_u64_operand_source(&transition.operand) else {
                continue;
            };
            self.emit_loaded_schema_bounds_check(
                input_size_offset,
                layout.offset + width,
                &format!("{} input.{}", pattern.ty, transition.field),
            );
            self.emit_loaded_schema_bounds_check(
                output_size_offset,
                layout.offset + width,
                &format!("{} output.{}", pattern.ty, transition.field),
            );
            self.emit(format!(
                "# cellscript abi: verify mutate transition field {}.{} {:?} Input#{} -> Output#{} offset={} size={}",
                pattern.ty, transition.field, transition.op, pattern.input_index, pattern.output_index, layout.offset, width
            ));
            self.emit_sp_addi("t4", input_buffer_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            let input_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 2).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve mutate input scalar before transition expression");
            self.emit_stack_store("t0", input_value_offset);
            self.emit_prelude_u64_operand_source_to_t1(&delta);
            self.emit_stack_load("t0", input_value_offset);
            match transition.op {
                MutateTransitionOp::Add => self.emit("add t1, t0, t1"),
                MutateTransitionOp::Sub => self.emit("sub t1, t0, t1"),
                MutateTransitionOp::Set => {
                    unreachable!("set transitions are verified by emit_mutate_replacement_set_transition_checks")
                }
                MutateTransitionOp::Append => {
                    unreachable!("append transitions are verified by emit_mutate_replacement_dynamic_table_append_checks")
                }
            }
            let expected_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 1).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve mutate expected scalar across output field load");
            self.emit_stack_store("t1", expected_value_offset);
            self.emit_sp_addi("t4", output_buffer_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            self.emit_stack_load("t1", expected_value_offset);
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("mutate_transition_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
            self.emit_label(&ok_label);
        }
    }

    fn emit_mutate_replacement_dynamic_table_transition_checks(&mut self, pattern: &MutatePattern) -> bool {
        if self.type_fixed_sizes.contains_key(&pattern.ty) || pattern.transitions.is_empty() {
            return false;
        }
        let Some(layouts) = self.type_layouts.get(&pattern.ty).cloned() else {
            return false;
        };
        let field_count = layouts.len();
        let transitions = pattern
            .transitions
            .iter()
            .filter_map(|transition| {
                let layout = layouts.get(&transition.field).cloned()?;
                let width = layout_fixed_scalar_width(&layout)?;
                (width <= 8 && self.prelude_u64_operand_source(&transition.operand).is_some())
                    .then(|| (transition.clone(), layout, width))
            })
            .collect::<Vec<_>>();
        if transitions.len() != pattern.transitions.len() {
            return false;
        }

        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_table_transition",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_table_transition",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit(format!(
            "# cellscript abi: verify mutate Molecule table transition fields {} Input#{} -> Output#{}",
            pattern.ty, pattern.input_index, pattern.output_index
        ));
        for (transition, layout, width) in transitions {
            let Some(delta) = self.prelude_u64_operand_source(&transition.operand) else {
                continue;
            };
            self.emit_sp_addi("t4", input_buffer_offset);
            self.emit_molecule_table_field_bounds_to_t5(
                "t4",
                input_size_offset,
                layout.index,
                width,
                &format!("{} input.{}", pattern.ty, transition.field),
            );
            self.emit("add t4, t4, t5");
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
            let input_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 2).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve mutate table input scalar before transition expression");
            self.emit_stack_store("t0", input_value_offset);
            self.emit_prelude_u64_operand_source_to_t1(&delta);
            self.emit_stack_load("t0", input_value_offset);
            match transition.op {
                MutateTransitionOp::Add => self.emit("add t1, t0, t1"),
                MutateTransitionOp::Sub => self.emit("sub t1, t0, t1"),
                MutateTransitionOp::Set => {}
                MutateTransitionOp::Append => {}
            }
            let expected_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 1).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve mutate table expected scalar across output field load");
            self.emit_stack_store("t1", expected_value_offset);
            self.emit_sp_addi("t4", output_buffer_offset);
            self.emit_molecule_table_field_bounds_to_t5(
                "t4",
                output_size_offset,
                layout.index,
                width,
                &format!("{} output.{}", pattern.ty, transition.field),
            );
            self.emit("add t4, t4, t5");
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
            self.emit_stack_load("t1", expected_value_offset);
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("mutate_table_transition_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
            self.emit_label(&ok_label);
        }
        let _ = field_count;
        true
    }

    fn emit_mutate_replacement_set_transition_checks(&mut self, pattern: &MutatePattern) {
        let transitions = self.mutate_set_transition_layouts(pattern);
        if transitions.is_empty() {
            return;
        }
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_set_transition",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(
                output_size_offset,
                expected_size,
                &format!("{} mutate set transition output", pattern.ty),
            );
        }
        self.emit(format!("# cellscript abi: verify mutate set transition fields {} Output#{}", pattern.ty, pattern.output_index));
        for (transition, layout, width) in transitions {
            self.emit(format!(
                "# cellscript abi: verify mutate set transition field {}.{} Output#{} offset={} size={}",
                pattern.ty, transition.field, pattern.output_index, layout.offset, width
            ));
            if !self.emit_loaded_field_bytes_equals_expected(
                output_size_offset,
                output_buffer_offset,
                &layout,
                &transition.operand,
                &format!("{} set.{}", pattern.ty, transition.field),
            ) {
                self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
            }
        }
    }

    /// u128 transition verification using 128-bit add/sub with carry.
    /// Layout: field is 16 bytes (low 8 + high 8, little-endian).
    /// Delta is always u64 (fits in a single register).
    /// Verification: output == input +/- delta, with carry propagation.
    fn emit_mutate_replacement_u128_transition_checks(&mut self, pattern: &MutatePattern) {
        let transitions = self.mutate_u128_transition_layouts(pattern);
        if transitions.is_empty() {
            return;
        }
        let input_size_offset = self.runtime_scratch_size_offset();
        let input_buffer_offset = self.runtime_scratch_buffer_offset();
        let output_size_offset = self.runtime_scratch2_size_offset();
        let output_buffer_offset = self.runtime_scratch2_buffer_offset();
        // Load Input and Output cell data (already done by the caller for
        // preserved field checks, but we need it for transition checks too).
        // If the scratch buffers were already loaded by the preserved-field
        // path, the syscall results are cached in the buffer; we only need
        // to reload if this function is called independently.
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_input_u128_transition",
            CKB_SOURCE_INPUT,
            pattern.input_index,
            input_size_offset,
            input_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_load_cell_data_syscall_to_offsets(
            "mutate_output_u128_transition",
            CKB_SOURCE_OUTPUT,
            pattern.output_index,
            output_size_offset,
            output_buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(
                input_size_offset,
                expected_size,
                &format!("{} mutate u128 transition input", pattern.ty),
            );
            self.emit_loaded_schema_exact_size_check(
                output_size_offset,
                expected_size,
                &format!("{} mutate u128 transition output", pattern.ty),
            );
        }
        for (transition, layout) in transitions {
            let Some(delta) = self.prelude_u64_operand_source(&transition.operand) else {
                continue;
            };
            self.emit_loaded_schema_bounds_check(
                input_size_offset,
                layout.offset + 16,
                &format!("{} input.{}", pattern.ty, transition.field),
            );
            self.emit_loaded_schema_bounds_check(
                output_size_offset,
                layout.offset + 16,
                &format!("{} output.{}", pattern.ty, transition.field),
            );
            self.emit(format!(
                "# cellscript abi: verify mutate u128 transition field {}.{} {:?} Input#{} -> Output#{} offset={} size=16",
                pattern.ty, transition.field, transition.op, pattern.input_index, pattern.output_index, layout.offset
            ));

            // Load input low 64 bits (little-endian bytes 0..8) into t0
            // Load input high 64 bits (little-endian bytes 8..16) into t3
            self.emit_sp_addi("t4", input_buffer_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, 8);
            self.emit_unaligned_scalar_load("t4", "t3", "t2", layout.offset + 8, 8);

            // Load delta into t1
            self.emit_prelude_u64_operand_source_to_t1(&delta);

            // Compute expected output = input +/- delta with carry
            match transition.op {
                MutateTransitionOp::Add => {
                    // expected_lo = input_lo + delta
                    // expected_hi = input_hi + carry
                    // where carry = (input_lo + delta < input_lo) ? 1 : 0
                    self.emit("add t5, t0, t1"); // expected_lo = input_lo + delta
                    self.emit("sltu t2, t5, t0"); // carry = 1 if addition overflowed
                    self.emit("add t6, t3, t2"); // expected_hi = input_hi + carry
                }
                MutateTransitionOp::Sub => {
                    // expected_lo = input_lo - delta
                    // expected_hi = input_hi - borrow
                    // where borrow = (input_lo < delta) ? 1 : 0
                    self.emit("sub t5, t0, t1"); // expected_lo = input_lo - delta
                    self.emit("sltu t2, t0, t1"); // borrow = 1 if subtraction underflowed
                    self.emit("sub t6, t3, t2"); // expected_hi = input_hi - borrow
                }
                MutateTransitionOp::Set => {
                    unreachable!("set transitions are verified by emit_mutate_replacement_set_transition_checks")
                }
                MutateTransitionOp::Append => {
                    unreachable!("append transitions are verified by emit_mutate_replacement_dynamic_table_append_checks")
                }
            }

            // Load actual output low 64 bits into t0, high 64 bits into t3
            self.emit_sp_addi("t4", output_buffer_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, 8);
            self.emit_unaligned_scalar_load("t4", "t3", "t2", layout.offset + 8, 8);

            // Compare: expected (t5, t6) == actual (t0, t3)
            let ok_label = self.fresh_label("mutate_u128_transition_ok");
            self.emit("sub t2, t0, t5"); // diff_lo = actual_lo - expected_lo
            self.emit("sub t1, t3, t6"); // diff_hi = actual_hi - expected_hi
            self.emit("or t2, t2, t1"); // combined diff = diff_lo | diff_hi
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_fail(CellScriptRuntimeError::MutateTransitionMismatch);
            self.emit_label(&ok_label);
        }
    }

    fn emit_loaded_field_equals_expected(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        expected: &IrOperand,
        context: &str,
    ) {
        let Some(width) = layout_fixed_scalar_width(layout) else {
            return;
        };
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        self.emit(format!("# cellscript abi: verify output field {} offset={} size={}", context, layout.offset, width));
        self.emit_sp_addi("t4", buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
        let actual_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 1).expect("runtime temp slot");
        self.emit("# cellscript abi: preserve output scalar before expected expression");
        self.emit_stack_store("t0", actual_value_offset);
        self.emit_expected_operand_to_t1(expected);
        self.emit_stack_load("t0", actual_value_offset);
        self.emit("sub t2, t0, t1");
        let ok_label = self.fresh_label("output_field_ok");
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&ok_label);
    }

    fn emit_loaded_fixed_bytes_against_source(
        &mut self,
        output_buffer_offset: usize,
        output_field_offset: usize,
        source: &ExpectedFixedByteSource,
        width: usize,
        fail_code: CellScriptRuntimeError,
    ) {
        let mismatch_label = self.fresh_label("fixed_byte_mismatch");
        self.emit_sp_addi("t4", output_buffer_offset);
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                if self.emit_schema_field_source_pointer_to("a1", source, width) {
                    self.emit_sp_addi("a0", output_buffer_offset + output_field_offset);
                    self.emit(format!("li a2, {}", width));
                    self.emit("call __cellscript_memcmp_fixed");
                    self.emit(format!("bnez a0, {}", mismatch_label));
                } else {
                    self.emit("# cellscript abi: fail closed because schema field byte source is not addressable");
                    self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
                }
            }
            ExpectedFixedByteSource::Const(bytes) => {
                if width >= 8 && bytes.iter().take(width).all(|byte| *byte == 0) {
                    self.emit_sp_addi("a0", output_buffer_offset + output_field_offset);
                    self.emit(format!("li a1, {}", width));
                    self.emit("call __cellscript_memzero_fixed");
                    self.emit(format!("bnez a0, {}", mismatch_label));
                } else {
                    for (byte_index, byte) in bytes.iter().take(width).enumerate() {
                        self.emit(format!("lbu t0, {}(t4)", output_field_offset + byte_index));
                        self.emit(format!("li t1, {}", byte));
                        self.emit("sub t2, t0, t1");
                        self.emit(format!("bnez t2, {}", mismatch_label));
                    }
                }
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_loaded_fixed_bytes_helper_call(
                    output_buffer_offset,
                    output_field_offset,
                    SourcePointer::StackAddress { offset: var_id * 8 },
                    width,
                    &mismatch_label,
                );
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_loaded_fixed_bytes_helper_call(
                    output_buffer_offset,
                    output_field_offset,
                    SourcePointer::LoadedStackPointer { var_id: *var_id, offset: 0 },
                    width,
                    &mismatch_label,
                );
            }
        }
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, fail_code);
    }

    fn emit_loaded_fixed_bytes_helper_call(
        &mut self,
        output_buffer_offset: usize,
        output_field_offset: usize,
        source: SourcePointer,
        width: usize,
        mismatch_label: &str,
    ) {
        self.emit_sp_addi("a0", output_buffer_offset + output_field_offset);
        match source {
            SourcePointer::LoadedStackPointer { var_id, offset } => {
                self.emit_stack_load("a1", var_id * 8);
                if offset != 0 {
                    self.emit_large_addi("a1", "a1", offset as i64);
                }
            }
            SourcePointer::StackAddress { offset } => {
                self.emit_sp_addi("a1", offset);
            }
        }
        self.emit(format!("li a2, {}", width));
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch_label));
    }

    fn emit_loaded_field_bytes_equals_expected(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        expected: &IrOperand,
        context: &str,
    ) -> bool {
        if layout_fixed_scalar_width(layout).is_some() {
            self.emit_loaded_field_equals_expected(size_offset, buffer_offset, layout, expected, context);
            return true;
        }
        let Some(width) = layout_fixed_byte_width(layout) else {
            return false;
        };
        let Some(source) = self.expected_fixed_byte_source(expected, width) else {
            return false;
        };
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                if let Some(source_size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
                    if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                        self.emit_loaded_schema_exact_size_check(source_size_offset, expected_size, &source.type_name);
                    }
                    self.emit_loaded_schema_bounds_check(
                        source_size_offset,
                        source.layout.offset + width,
                        &format!("{}.{}", source.type_name, source.field),
                    );
                }
                self.emit(format!("# cellscript abi: verify output bytes field {} offset={} size={}", context, layout.offset, width));
                self.emit(format!(
                    "# cellscript abi: expected bytes field {}.{} offset={} size={}",
                    source.type_name, source.field, source.layout.offset, width
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::SchemaField(source),
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::Const(bytes) => {
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against const",
                    context, layout.offset, width
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::Const(bytes),
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::StackSlot { var_id, width } => {
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against stack slot var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::StackSlot { var_id, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::PointerBytes { var_id, width } => {
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against pointer var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::PointerBytes { var_id, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::ParamBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(size_offset, width, &format!("param var{}", var_id));
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against fixed-byte param var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::ParamBytes { var_id, size_offset, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
            ExpectedFixedByteSource::LoadedBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(size_offset, width, &format!("loaded bytes var{}", var_id));
                self.emit(format!(
                    "# cellscript abi: verify output bytes field {} offset={} size={} against loaded bytes var{}",
                    context, layout.offset, width, var_id
                ));
                self.emit_loaded_fixed_bytes_against_source(
                    buffer_offset,
                    layout.offset,
                    &ExpectedFixedByteSource::LoadedBytes { var_id, size_offset, width },
                    width,
                    CellScriptRuntimeError::CellLoadFailed,
                );
            }
        }
        true
    }

    fn emit_prepare_fixed_byte_source(&mut self, source: &ExpectedFixedByteSource, width: usize, context: &str) {
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                self.emit_prepare_schema_field_source(source, width);
            }
            ExpectedFixedByteSource::ParamBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(*size_offset, *width, &format!("{} param var{}", context, var_id));
            }
            ExpectedFixedByteSource::LoadedBytes { var_id, size_offset, width } => {
                self.emit_loaded_schema_exact_size_check(*size_offset, *width, &format!("{} loaded bytes var{}", context, var_id));
            }
            ExpectedFixedByteSource::Const(_)
            | ExpectedFixedByteSource::StackSlot { .. }
            | ExpectedFixedByteSource::PointerBytes { .. } => {}
        }
    }

    fn emit_fixed_byte_source_byte_to(&mut self, dest_reg: &str, base_reg: &str, source: &ExpectedFixedByteSource, byte_index: usize) {
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                if self.emit_schema_field_source_pointer_to(base_reg, source, byte_index + 1) {
                    self.emit(format!("lbu {}, {}({})", dest_reg, byte_index, base_reg));
                } else {
                    self.emit("# cellscript abi: fail closed because schema field byte source is not addressable");
                    self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
                }
            }
            ExpectedFixedByteSource::Const(bytes) => {
                self.emit(format!("li {}, {}", dest_reg, bytes[byte_index]));
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_sp_addi(base_reg, var_id * 8);
                self.emit(format!("lbu {}, {}({})", dest_reg, byte_index, base_reg));
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_stack_load(base_reg, var_id * 8);
                self.emit(format!("lbu {}, {}({})", dest_reg, byte_index, base_reg));
            }
        }
    }

    fn emit_fixed_byte_source_pointer_to(&mut self, dest_reg: &str, source: &ExpectedFixedByteSource) -> bool {
        match source {
            ExpectedFixedByteSource::SchemaField(source) => {
                let Some(width) = layout_fixed_byte_width(&source.layout) else {
                    return false;
                };
                self.emit_schema_field_source_pointer_to(dest_reg, source, width)
            }
            ExpectedFixedByteSource::StackSlot { var_id, .. } => {
                self.emit_sp_addi(dest_reg, var_id * 8);
                true
            }
            ExpectedFixedByteSource::PointerBytes { var_id, .. }
            | ExpectedFixedByteSource::ParamBytes { var_id, .. }
            | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => {
                self.emit_stack_load(dest_reg, var_id * 8);
                true
            }
            ExpectedFixedByteSource::Const(_) => false,
        }
    }

    fn emit_fixed_byte_source_pointer_or_const_to(&mut self, dest_reg: &str, source: &ExpectedFixedByteSource) -> bool {
        if let ExpectedFixedByteSource::Const(bytes) = source {
            let label = self.const_data_label_for_bytes(bytes.clone());
            self.emit(format!("la {}, {}", dest_reg, label));
            true
        } else {
            self.emit_fixed_byte_source_pointer_to(dest_reg, source)
        }
    }

    fn emit_fixed_byte_mismatch_fail(&mut self, mismatch_label: &str, fail_code: CellScriptRuntimeError) {
        let done_label = self.fresh_label("fixed_byte_verify_done");
        self.emit(format!("j {}", done_label));
        self.emit_label(mismatch_label);
        self.emit_fail(fail_code);
        self.emit_label(&done_label);
    }

    fn emit_fixed_byte_comparison(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        let Some(width) = operand_fixed_byte_width(left) else {
            return false;
        };
        if operand_fixed_byte_width(right) != Some(width) {
            return false;
        }
        let Some(left_source) = self.expected_fixed_byte_source(left, width) else {
            return false;
        };
        let Some(right_source) = self.expected_fixed_byte_source(right, width) else {
            return false;
        };
        self.emit(format!("# cellscript abi: fixed-byte {:?} comparison size={}", op, width));
        self.emit_prepare_fixed_byte_source(&left_source, width, "left fixed-byte comparison");
        self.emit_prepare_fixed_byte_source(&right_source, width, "right fixed-byte comparison");
        if width >= 8 && self.emit_fixed_byte_comparison_helper(dest, op, &left_source, &right_source, width) {
            return true;
        }
        let mismatch_label = self.fresh_label("fixed_byte_mismatch");
        let done_label = self.fresh_label("fixed_byte_done");
        for byte_index in 0..width {
            self.emit_fixed_byte_source_byte_to("t0", "t4", &left_source, byte_index);
            self.emit_fixed_byte_source_byte_to("t1", "t5", &right_source, byte_index);
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", mismatch_label));
        }
        let equal_value = if matches!(op, BinaryOp::Eq) { 1 } else { 0 };
        let mismatch_value = if matches!(op, BinaryOp::Eq) { 0 } else { 1 };
        self.emit(format!("li t3, {}", equal_value));
        self.emit(format!("j {}", done_label));
        self.emit_label(&mismatch_label);
        self.emit(format!("li t3, {}", mismatch_value));
        self.emit_label(&done_label);
        self.emit_stack_store("t3", dest.id * 8);
        true
    }

    fn emit_fixed_byte_comparison_helper(
        &mut self,
        dest: &IrVar,
        op: BinaryOp,
        left_source: &ExpectedFixedByteSource,
        right_source: &ExpectedFixedByteSource,
        width: usize,
    ) -> bool {
        match (left_source, right_source) {
            (ExpectedFixedByteSource::Const(bytes), source) if bytes.iter().take(width).all(|byte| *byte == 0) => {
                if !self.emit_fixed_byte_source_pointer_to("a0", source) {
                    return false;
                }
                self.emit(format!("li a1, {}", width));
                self.emit("call __cellscript_memzero_fixed");
            }
            (source, ExpectedFixedByteSource::Const(bytes)) if bytes.iter().take(width).all(|byte| *byte == 0) => {
                if !self.emit_fixed_byte_source_pointer_to("a0", source) {
                    return false;
                }
                self.emit(format!("li a1, {}", width));
                self.emit("call __cellscript_memzero_fixed");
            }
            (ExpectedFixedByteSource::Const(_), _) | (_, ExpectedFixedByteSource::Const(_)) => return false,
            _ => {
                if !self.emit_fixed_byte_source_pointer_to("a0", left_source) {
                    return false;
                }
                let Some(left_pointer_offset) = self.runtime_expr_temp_offset(0) else {
                    return false;
                };
                self.emit_stack_store("a0", left_pointer_offset);
                if !self.emit_fixed_byte_source_pointer_to("a1", right_source) {
                    return false;
                }
                self.emit_stack_load("a0", left_pointer_offset);
                self.emit(format!("li a2, {}", width));
                self.emit("call __cellscript_memcmp_fixed");
            }
        }
        if matches!(op, BinaryOp::Eq) {
            self.emit("seqz t3, a0");
        } else {
            self.emit("snez t3, a0");
        }
        self.emit_stack_store("t3", dest.id * 8);
        true
    }

    fn expected_fixed_byte_source(&self, operand: &IrOperand, expected_width: usize) -> Option<ExpectedFixedByteSource> {
        match operand {
            IrOperand::Const(value) => {
                let bytes = fixed_byte_const_bytes(value)?;
                (bytes.len() == expected_width).then_some(ExpectedFixedByteSource::Const(bytes))
            }
            IrOperand::Var(var) if self.fixed_byte_like_width(&var.ty).is_some() => {
                let var_width = self.fixed_byte_like_width(&var.ty)?;
                if let Some(source) = self.schema_field_value_sources.get(&var.id).cloned() {
                    let source_width = layout_fixed_byte_width(&source.layout)?;
                    if source_width == expected_width {
                        return Some(ExpectedFixedByteSource::SchemaField(source));
                    }
                }
                if let Some(bytes) = self.prelude_fixed_byte_constants.get(&var.id).cloned() {
                    if bytes.len() == expected_width {
                        return Some(ExpectedFixedByteSource::Const(bytes));
                    }
                }
                if self.fixed_byte_local_offsets.contains_key(&var.id) && var_width == expected_width {
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if expected_width <= 8
                    && (fixed_scalar_width(&var.ty, type_static_length(&var.ty)).is_some()
                        || (var_width == expected_width && fixed_byte_width(&var.ty, type_static_length(&var.ty)).is_some()))
                    && expected_width <= var_width
                {
                    return Some(ExpectedFixedByteSource::StackSlot { var_id: var.id, width: expected_width });
                }
                if self.aggregate_pointer_sources.contains_key(&var.id) && var_width == expected_width {
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if self.schema_pointer_vars.contains(&var.id) && var_width == expected_width {
                    if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
                        return Some(ExpectedFixedByteSource::LoadedBytes { var_id: var.id, size_offset, width: expected_width });
                    }
                    return Some(ExpectedFixedByteSource::PointerBytes { var_id: var.id, width: expected_width });
                }
                if self.param_vars.contains(&var.id) && var_width == expected_width {
                    if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&var.id).copied() {
                        return Some(ExpectedFixedByteSource::ParamBytes { var_id: var.id, size_offset, width: expected_width });
                    }
                }
                if let Some(size_offset) = self.cell_buffer_size_offsets.get(&var.id).copied() {
                    if var_width == expected_width {
                        return Some(ExpectedFixedByteSource::LoadedBytes { var_id: var.id, size_offset, width: expected_width });
                    }
                }
                if let Some(param_id) = self.param_type_hash_sources.get(&var.id).copied() {
                    if var_width == expected_width {
                        if let Some(size_offset) = self.param_type_hash_size_offsets.get(&param_id).copied() {
                            return Some(ExpectedFixedByteSource::LoadedBytes { var_id: var.id, size_offset, width: expected_width });
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Generic fixed-byte comparison: when `emit_fixed_byte_comparison` can't determine
    /// the source of bytes, this method loads pointers from stack slots and performs
    /// a byte-by-byte comparison. Works for Var operands whose stack slots contain
    /// pointers to the fixed-byte data.
    fn emit_generic_fixed_byte_comparison(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        let left_width = operand_fixed_byte_width(left);
        let right_width = operand_fixed_byte_width(right);

        // Need at least one Var operand with known width for this to work
        let width = match (left_width, right_width) {
            (Some(w), Some(r)) if w == r => w,
            (Some(w), None) | (None, Some(w)) => w,
            _ => return false,
        };

        if width == 0 {
            return false;
        }

        // We need at least one Var operand
        let left_var = match left {
            IrOperand::Var(v) => Some(v),
            _ => None,
        };
        let right_var = match right {
            IrOperand::Var(v) => Some(v),
            _ => None,
        };
        if left_var.is_none() && right_var.is_none() {
            return false;
        }

        self.emit(format!("# cellscript abi: generic fixed-byte {:?} comparison size={}", op, width));

        // Load left pointer to t4
        if let Some(v) = left_var {
            self.emit_stack_load("t4", v.id * 8);
        } else {
            // Left is a constant – store it to scratch buffer and point t4 there
            let size_offset = self.runtime_scratch_size_offset();
            let buffer_offset = self.runtime_scratch_buffer_offset();
            self.emit_store_fixed_byte_const_to_scratch(left, size_offset, buffer_offset, width);
            self.emit_sp_addi("t4", buffer_offset);
        }

        // Load right pointer to t5
        if let Some(v) = right_var {
            self.emit_stack_load("t5", v.id * 8);
        } else {
            let size_offset = self.runtime_scratch2_size_offset();
            let buffer_offset = self.runtime_scratch2_buffer_offset();
            self.emit_store_fixed_byte_const_to_scratch(right, size_offset, buffer_offset, width);
            self.emit_sp_addi("t5", buffer_offset);
        }

        let mismatch_label = self.fresh_label("gen_fb_mismatch");
        let done_label = self.fresh_label("gen_fb_done");
        for byte_index in 0..width {
            self.emit(format!("lbu t0, {}(t4)", byte_index));
            self.emit(format!("lbu t1, {}(t5)", byte_index));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", mismatch_label));
        }
        let equal_value = if matches!(op, BinaryOp::Eq) { 1 } else { 0 };
        let mismatch_value = if matches!(op, BinaryOp::Eq) { 0 } else { 1 };
        self.emit(format!("li t3, {}", equal_value));
        self.emit(format!("j {}", done_label));
        self.emit_label(&mismatch_label);
        self.emit(format!("li t3, {}", mismatch_value));
        self.emit_label(&done_label);
        self.emit_stack_store("t3", dest.id * 8);
        true
    }

    /// Store fixed-byte constant value to scratch buffer area.
    fn emit_store_fixed_byte_const_to_scratch(&mut self, operand: &IrOperand, size_offset: usize, buffer_offset: usize, width: usize) {
        match operand {
            IrOperand::Const(IrConst::Address(bytes)) | IrOperand::Const(IrConst::Hash(bytes)) => {
                self.emit(format!("# cellscript abi: store fixed-byte const size={}", width));
                self.emit(format!("li t0, {}", width));
                self.emit_stack_store("t0", size_offset);
                for (i, byte) in bytes.iter().enumerate() {
                    self.emit(format!("li t0, {}", byte));
                    if buffer_offset + i <= 2047 {
                        self.emit_stack_store_byte("t0", buffer_offset + i);
                    } else {
                        self.emit(format!("li t6, {}", buffer_offset + i));
                        self.emit("add t6, sp, t6");
                        self.emit("sb t0, 0(t6)");
                    }
                }
            }
            IrOperand::Const(IrConst::U128(value)) => {
                self.emit(format!("# cellscript abi: store u128 const size={}", width));
                self.emit(format!("li t0, {}", width));
                self.emit_stack_store("t0", size_offset);
                for (i, byte) in value.to_le_bytes().iter().enumerate() {
                    self.emit(format!("li t0, {}", byte));
                    self.emit_stack_store_byte("t0", buffer_offset + i);
                }
            }
            IrOperand::Const(IrConst::Array(values)) => {
                self.emit(format!("# cellscript abi: store fixed-byte array const size={}", width));
                self.emit(format!("li t0, {}", width));
                self.emit_stack_store("t0", size_offset);
                for (i, value) in values.iter().enumerate() {
                    if let IrConst::U8(byte) = value {
                        self.emit(format!("li t0, {}", byte));
                        if buffer_offset + i <= 2047 {
                            self.emit_stack_store_byte("t0", buffer_offset + i);
                        } else {
                            self.emit(format!("li t6, {}", buffer_offset + i));
                            self.emit("add t6, sp, t6");
                            self.emit("sb t0, 0(t6)");
                        }
                    }
                }
            }
            _ => {
                self.emit("# cellscript abi: fail closed because unknown const type cannot be stored to scratch");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            }
        }
    }

    fn emit_fixed_byte_source_scalar_to(
        &mut self,
        dest_reg: &str,
        scratch_reg: &str,
        base_reg: &str,
        source: &ExpectedFixedByteSource,
        start: usize,
        width: usize,
    ) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit_fixed_byte_source_byte_to(scratch_reg, base_reg, source, start + byte_index);
            if byte_index != 0 {
                self.emit(format!("slli {}, {}, {}", scratch_reg, scratch_reg, byte_index * 8));
            }
            self.emit(format!("or {}, {}, {}", dest_reg, dest_reg, scratch_reg));
        }
    }

    fn operand_is_u128(&self, operand: &IrOperand) -> bool {
        match operand {
            IrOperand::Const(IrConst::U128(_)) => true,
            IrOperand::Var(var) => var.ty == IrType::U128,
            _ => false,
        }
    }

    fn emit_u128_add_sub_with_u64(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        if dest.ty != IrType::U128 || !matches!(op, BinaryOp::Add | BinaryOp::Sub) {
            return false;
        }
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            return false;
        };

        let (wide_operand, delta_operand) = match op {
            BinaryOp::Add if self.operand_is_u128(left) => (left, right),
            BinaryOp::Add if self.operand_is_u128(right) => (right, left),
            BinaryOp::Sub if self.operand_is_u128(left) => (left, right),
            _ => return false,
        };
        let Some(source) = self.expected_fixed_byte_source(wide_operand, 16) else {
            return false;
        };
        let Some(delta) = self.prelude_u64_operand_source(delta_operand) else {
            return false;
        };

        self.emit(format!("# cellscript abi: u128 {:?} with u64 delta", op));
        self.emit_fixed_byte_source_scalar_to("t0", "t2", "t4", &source, 0, 8);
        self.emit_fixed_byte_source_scalar_to("t3", "t2", "t4", &source, 8, 8);
        self.emit_prelude_u64_operand_source_to_t1(&delta);
        match op {
            BinaryOp::Add => {
                self.emit("add t5, t0, t1");
                self.emit("sltu t2, t5, t0");
                self.emit("add t6, t3, t2");
            }
            BinaryOp::Sub => {
                self.emit("sub t5, t0, t1");
                self.emit("sltu t2, t0, t1");
                self.emit("sub t6, t3, t2");
            }
            _ => unreachable!("guarded u128 binary op"),
        }
        self.emit_stack_store("t5", dest_offset);
        self.emit_stack_store("t6", dest_offset + 8);
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_expected_operand_to_t1(&mut self, operand: &IrOperand) {
        match operand {
            IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("li t1, {}", if *b { 1 } else { 0 })),
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Var(var) => {
                if let Some(source) = self.schema_field_value_sources.get(&var.id).cloned() {
                    self.emit_schema_field_source_to_t1(&source);
                } else if let Some(source) = self.prelude_u64_value_sources.get(&var.id).cloned() {
                    self.emit_prelude_u64_value_source_to_t1(&source);
                } else if matches!(var.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64) {
                    self.emit_stack_load("t1", var.id * 8);
                } else if let Some(value) = self.prelude_scalar_immediates.get(&var.id).copied() {
                    self.emit(format!("li t1, {}", value));
                } else {
                    self.emit_stack_load("t1", var.id * 8);
                }
            }
            operand => self.emit_operand_to_register("t1", operand),
        }
    }

    fn emit_prelude_u64_value_source_to_t1(&mut self, source: &PreludeU64ValueSource) {
        self.emit_prelude_u64_value_source_to_t1_at_depth(source, 0);
    }

    fn emit_prelude_u64_value_source_to_t1_at_depth(&mut self, source: &PreludeU64ValueSource, _depth: usize) {
        match source {
            PreludeU64ValueSource::Const(n) => self.emit(format!("li t1, {}", n)),
            PreludeU64ValueSource::ParamVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64ValueSource::StackVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64ValueSource::Field(source) => self.emit_schema_field_source_to_t1(source),
            PreludeU64ValueSource::Binary { op, left, right } => {
                self.emit(format!("# cellscript abi: expected expression u64 {:?}", op));
                let Some(temp_offset) = self.runtime_expr_temp_offset(_depth) else {
                    self.emit("# cellscript abi: fail closed because expression verifier temp stack is exhausted");
                    self.emit_fail(CellScriptRuntimeError::DataPreservationMismatch);
                    return;
                };
                self.emit_prelude_u64_value_source_to_t1_at_depth(left, _depth + 1);
                self.emit_stack_store("t1", temp_offset);
                self.emit_prelude_u64_operand_source_to_t1_at_depth(right, _depth + 1);
                self.emit_stack_load("t3", temp_offset);
                match op {
                    BinaryOp::Add => self.emit("add t1, t3, t1"),
                    BinaryOp::Sub => self.emit("sub t1, t3, t1"),
                    BinaryOp::Mul => self.emit("mul t1, t3, t1"),
                    BinaryOp::Div => {
                        self.emit_divisor_nonzero_guard("t1");
                        self.emit("divu t1, t3, t1");
                    }
                    _ => unreachable!("prelude u64 binary source only supports add/sub/mul/div"),
                }
            }
            PreludeU64ValueSource::Min { left, right } => {
                self.emit("# cellscript abi: expected expression u64 min");
                let Some(temp_offset) = self.runtime_expr_temp_offset(_depth) else {
                    self.emit("# cellscript abi: fail closed because expression verifier temp stack is exhausted");
                    self.emit_fail(CellScriptRuntimeError::DataPreservationMismatch);
                    return;
                };
                self.emit_prelude_u64_value_source_to_t1_at_depth(left, _depth + 1);
                self.emit_stack_store("t1", temp_offset);
                self.emit_prelude_u64_operand_source_to_t1_at_depth(right, _depth + 1);
                self.emit_stack_load("t3", temp_offset);
                self.emit("sltu t2, t3, t1");
                let right_ok_label = self.fresh_label("prelude_min_right_ok");
                self.emit(format!("beqz t2, {}", right_ok_label));
                self.emit("add t1, t3, zero");
                self.emit_label(&right_ok_label);
            }
        }
    }

    fn emit_prelude_u64_operand_source_to_t1(&mut self, source: &PreludeU64OperandSource) {
        self.emit_prelude_u64_operand_source_to_t1_at_depth(source, 0);
    }

    fn emit_prelude_u64_operand_source_to_t1_at_depth(&mut self, source: &PreludeU64OperandSource, _depth: usize) {
        match source {
            PreludeU64OperandSource::Const(n) => self.emit(format!("li t1, {}", n)),
            PreludeU64OperandSource::ParamVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64OperandSource::StackVar(var_id) => self.emit_stack_load("t1", var_id * 8),
            PreludeU64OperandSource::Field(source) => self.emit_schema_field_source_to_t1(source),
            PreludeU64OperandSource::Expr(source) => self.emit_prelude_u64_value_source_to_t1_at_depth(source, _depth),
        }
    }

    fn emit_schema_field_source_to_t1(&mut self, source: &SchemaFieldValueSource) {
        let context = format!("{}.{}", source.type_name, source.field);
        let Some(width) = layout_fixed_scalar_width(&source.layout) else {
            self.emit("li t1, 0");
            return;
        };
        if !self.type_fixed_sizes.contains_key(&source.type_name) {
            if self.emit_schema_field_source_pointer_to("t4", source, width) {
                self.emit(format!("# cellscript abi: expected table field {} index={} size={}", context, source.layout.index, width));
                self.emit_unaligned_scalar_load("t4", "t1", "t2", 0, width);
            } else {
                self.emit("li t1, 0");
            }
            return;
        }
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
            }
            self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
        }
        self.emit(format!("# cellscript abi: expected field {} offset={} size={}", context, source.layout.offset, width));
        self.emit_stack_load("t4", source.obj_var_id * 8);
        self.emit_unaligned_scalar_load("t4", "t1", "t2", source.layout.offset, width);
    }

    fn emit_prepare_schema_field_source(&mut self, source: &SchemaFieldValueSource, width: usize) {
        let context = format!("{}.{}", source.type_name, source.field);
        let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() else {
            return;
        };
        if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
            self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
        } else {
            self.emit_stack_load("t4", source.obj_var_id * 8);
            self.emit_molecule_table_field_bounds_to_t5("t4", size_offset, source.layout.index, width, &context);
        }
    }

    fn emit_schema_field_source_pointer_to(&mut self, dest_reg: &str, source: &SchemaFieldValueSource, width: usize) -> bool {
        let context = format!("{}.{}", source.type_name, source.field);
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
                self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
                self.emit_stack_load(dest_reg, source.obj_var_id * 8);
                if source.layout.offset != 0 {
                    self.emit_large_addi(dest_reg, dest_reg, source.layout.offset as i64);
                }
            } else {
                self.emit_stack_load("t4", source.obj_var_id * 8);
                self.emit_molecule_table_field_bounds_to_t5("t4", size_offset, source.layout.index, width, &context);
                self.emit(format!("add {}, t4, t5", dest_reg));
            }
            true
        } else if self.aggregate_pointer_sources.contains_key(&source.obj_var_id)
            || self.type_fixed_sizes.contains_key(&source.type_name)
        {
            self.emit_stack_load(dest_reg, source.obj_var_id * 8);
            if source.layout.offset != 0 {
                self.emit_large_addi(dest_reg, dest_reg, source.layout.offset as i64);
            }
            true
        } else {
            false
        }
    }

    fn can_verify_create_output_fields(&self, pattern: &CreatePattern) -> bool {
        if pattern.fields.is_empty() {
            return false;
        }
        if !self.create_output_fields_cover_type(pattern) {
            return false;
        }
        pattern.fields.iter().all(|(field, value)| {
            self.type_layouts.get(&pattern.ty).and_then(|layouts| layouts.get(field)).is_some_and(|layout| {
                if let Some(width) = layout_fixed_byte_width(layout) {
                    self.is_prelude_available_fixed_value(value, width)
                } else {
                    self.can_verify_dynamic_create_output_field_value(value, layout)
                }
            })
        })
    }

    fn create_output_fields_cover_type(&self, pattern: &CreatePattern) -> bool {
        let Some(layouts) = self.type_layouts.get(&pattern.ty) else {
            return false;
        };
        let covered_fields = pattern.fields.iter().map(|(field, _)| field.as_str()).collect::<BTreeSet<_>>();
        layouts.keys().all(|field| covered_fields.contains(field.as_str()))
    }

    fn can_verify_dynamic_create_output_field_value(&self, value: &IrOperand, layout: &SchemaFieldLayout) -> bool {
        let IrOperand::Var(var) = value else {
            return false;
        };
        (self.schema_pointer_vars.contains(&var.id) && self.schema_pointer_size_offsets.contains_key(&var.id))
            || self.constructed_byte_vectors.contains_key(&var.id)
            || (self.empty_molecule_vector_vars.contains(&var.id)
                && molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).is_some())
    }

    fn can_verify_output_lock(&self, pattern: &CreatePattern) -> bool {
        match &pattern.lock {
            Some(lock) => self.expected_fixed_byte_source(lock, 32).is_some(),
            None => true,
        }
    }

    fn emit_create_output_checks(&mut self, pattern: &CreatePattern) {
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_create_output_checks_at(pattern, size_offset, buffer_offset);
    }

    fn emit_create_output_checks_at(&mut self, pattern: &CreatePattern, size_offset: usize, buffer_offset: usize) {
        let is_fixed_type = self.type_fixed_sizes.contains_key(&pattern.ty);
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &pattern.ty);
        }
        for (field, value) in &pattern.fields {
            let Some(layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(field)).cloned() else {
                self.emit(format!("# cellscript abi: fail closed because create output field {}.{} has no layout", pattern.ty, field));
                self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                continue;
            };
            if layout_fixed_byte_width(&layout).is_some() {
                if is_fixed_type {
                    self.emit_loaded_field_bytes_equals_expected(
                        size_offset,
                        buffer_offset,
                        &layout,
                        value,
                        &format!("{}.{}", pattern.ty, field),
                    );
                } else {
                    let Some(field_count) = self.type_layouts.get(&pattern.ty).map(|fields| fields.len()) else {
                        self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                        continue;
                    };
                    if !self.emit_dynamic_create_output_fixed_field_equals_expected(
                        size_offset,
                        buffer_offset,
                        &pattern.ty,
                        field,
                        &layout,
                        field_count,
                        value,
                    ) {
                        self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    }
                }
            } else {
                let Some(field_count) = self.type_layouts.get(&pattern.ty).map(|fields| fields.len()) else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    continue;
                };
                if !self.emit_dynamic_create_output_field_equals_expected(
                    size_offset,
                    buffer_offset,
                    &pattern.ty,
                    field,
                    &layout,
                    field_count,
                    value,
                ) {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                }
            }
        }
        if pattern.operation == "settle" {
            self.emit_settle_final_state_check(pattern, size_offset, buffer_offset);
        } else {
            self.emit_state_transition_check(pattern, size_offset, buffer_offset);
        }
    }

    fn emit_dynamic_create_output_fixed_field_equals_expected(
        &mut self,
        output_size_offset: usize,
        output_buffer_offset: usize,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
        field_count: usize,
        expected: &IrOperand,
    ) -> bool {
        let Some(width) = layout_fixed_byte_width(layout) else {
            return false;
        };
        let output_start_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let output_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        self.emit_dynamic_table_field_span_to_stack(
            output_size_offset,
            output_buffer_offset,
            layout.index,
            field_count,
            &format!("{}.{}", type_name, field),
            output_start_offset,
            output_len_offset,
        );
        self.emit_stack_load("t0", output_len_offset);
        self.emit(format!("li t1, {}", width));
        self.emit("sub t2, t0, t1");
        let len_ok = self.fresh_label("create_fixed_table_field_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&len_ok);

        if layout_fixed_scalar_width(layout).is_some() {
            self.emit(format!(
                "# cellscript abi: verify output Molecule table scalar field {}.{} index={} size={}",
                type_name, field, layout.index, width
            ));
            self.emit_stack_load("t4", output_start_offset);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
            let actual_value_offset = self.runtime_expr_temp_offset(RUNTIME_EXPR_TEMP_SLOTS - 1).expect("runtime temp slot");
            self.emit("# cellscript abi: preserve output table scalar before expected expression");
            self.emit_stack_store("t0", actual_value_offset);
            self.emit_expected_operand_to_t1(expected);
            self.emit_stack_load("t0", actual_value_offset);
            self.emit("sub t2, t0, t1");
            let ok_label = self.fresh_label("output_table_field_ok");
            self.emit(format!("beqz t2, {}", ok_label));
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            self.emit_label(&ok_label);
            return true;
        }

        let Some(source) = self.expected_fixed_byte_source(expected, width) else {
            return false;
        };
        self.emit(format!(
            "# cellscript abi: verify output Molecule table bytes field {}.{} index={} size={}",
            type_name, field, layout.index, width
        ));
        self.emit_prepare_fixed_byte_source(&source, width, &format!("{}.{}", type_name, field));
        self.emit_pointer_fixed_bytes_against_source(
            output_start_offset,
            0,
            &source,
            width,
            CellScriptRuntimeError::DynamicFieldValueMismatch,
        );
        true
    }

    fn emit_dynamic_create_output_field_equals_expected(
        &mut self,
        output_size_offset: usize,
        output_buffer_offset: usize,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
        field_count: usize,
        expected: &IrOperand,
    ) -> bool {
        let IrOperand::Var(var) = expected else {
            return false;
        };
        let output_start_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let output_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        self.emit_dynamic_table_field_span_to_stack(
            output_size_offset,
            output_buffer_offset,
            layout.index,
            field_count,
            &format!("{}.{}", type_name, field),
            output_start_offset,
            output_len_offset,
        );
        if let Some(parts) = self.constructed_byte_vectors.get(&var.id).cloned() {
            if let Some(element_width) =
                molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
            {
                if parts.is_empty() && element_width != 1 {
                    self.emit_empty_molecule_vector_field_check(type_name, field, output_start_offset, output_len_offset);
                    return true;
                }
                self.emit_constructed_molecule_vector_field_check(
                    type_name,
                    field,
                    output_start_offset,
                    output_len_offset,
                    &parts,
                    element_width,
                );
                return true;
            }
        }
        if self.empty_molecule_vector_vars.contains(&var.id)
            && molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).is_some()
        {
            self.emit_empty_molecule_vector_field_check(type_name, field, output_start_offset, output_len_offset);
            return true;
        }
        if !self.schema_pointer_vars.contains(&var.id) {
            return false;
        }
        let Some(expected_size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() else {
            return false;
        };
        self.emit_stack_load("t0", output_len_offset);
        self.emit_stack_load("t1", expected_size_offset);
        self.emit("sub t2, t0, t1");
        let len_ok = self.fresh_label("create_dynamic_field_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&len_ok);

        self.emit(format!("# cellscript abi: verify output dynamic field {}.{} as Molecule bytes", type_name, field));
        let mismatch_label = self.fresh_label("create_dynamic_field_mismatch");
        self.emit_stack_load("a0", output_start_offset);
        self.emit_stack_load("a1", var.id * 8);
        self.emit_stack_load("a2", output_len_offset);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch_label));
        self.emit_fixed_byte_mismatch_fail(&mismatch_label, CellScriptRuntimeError::CellLoadFailed);
        true
    }

    fn emit_empty_molecule_vector_field_check(
        &mut self,
        type_name: &str,
        field: &str,
        output_start_offset: usize,
        output_len_offset: usize,
    ) {
        self.emit(format!("# cellscript abi: verify output dynamic field {}.{} as empty Molecule vector", type_name, field));
        self.emit_stack_load("t0", output_len_offset);
        self.emit("li t1, 4");
        self.emit("sub t2, t0, t1");
        let len_ok = self.fresh_label("create_empty_vector_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&len_ok);
        self.emit_stack_load("t0", output_start_offset);
        for offset in 0..4 {
            self.emit(format!("lbu t1, {}(t0)", offset));
            let byte_ok = self.fresh_label("create_empty_vector_byte_ok");
            self.emit(format!("beqz t1, {}", byte_ok));
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            self.emit_label(&byte_ok);
        }
    }

    fn emit_constructed_molecule_vector_field_check(
        &mut self,
        type_name: &str,
        field: &str,
        output_start_offset: usize,
        output_len_offset: usize,
        parts: &[IrOperand],
        element_width: usize,
    ) {
        let Some(expected_bytes) =
            parts.iter().try_fold(0usize, |acc, part| self.constructed_byte_vector_part_width(part).map(|width| acc + width))
        else {
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            return;
        };
        if element_width == 0 || expected_bytes % element_width != 0 {
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            return;
        }
        let expected_elements = expected_bytes / element_width;
        let expected_len = 4 + expected_bytes;
        if element_width == 1 {
            self.emit(format!(
                "# cellscript abi: verify output dynamic field {}.{} as constructed Molecule byte vector len={}",
                type_name, field, expected_bytes
            ));
        } else {
            self.emit(format!(
                "# cellscript abi: verify output dynamic field {}.{} as constructed Molecule vector elements={} bytes={} element_size={}",
                type_name, field, expected_elements, expected_bytes, element_width
            ));
        }
        self.emit_stack_load("t0", output_len_offset);
        self.emit(format!("li t1, {}", expected_len));
        self.emit("sub t2, t0, t1");
        let len_ok = self.fresh_label("create_constructed_vector_len_ok");
        self.emit(format!("beqz t2, {}", len_ok));
        self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&len_ok);

        self.emit_stack_load("t4", output_start_offset);
        for (offset, byte) in (expected_elements as u32).to_le_bytes().iter().enumerate() {
            self.emit(format!("lbu t0, {}(t4)", offset));
            self.emit(format!("li t1, {}", byte));
            self.emit("sub t2, t0, t1");
            let byte_ok = self.fresh_label("create_constructed_vector_count_ok");
            self.emit(format!("beqz t2, {}", byte_ok));
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            self.emit_label(&byte_ok);
        }

        let mut cursor = 4usize;
        for part in parts {
            let Some(width) = self.constructed_byte_vector_part_width(part) else {
                self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
                continue;
            };
            let Some(source) = self.expected_fixed_byte_source(part, width) else {
                self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
                continue;
            };
            self.emit_prepare_fixed_byte_source(&source, width, &format!("constructed {}.{}", type_name, field));
            self.emit_pointer_fixed_bytes_against_source(
                output_start_offset,
                cursor,
                &source,
                width,
                CellScriptRuntimeError::CellLoadFailed,
            );
            cursor += width;
        }
    }

    fn emit_output_lock_hash_check(&mut self, output_index: usize, expected: &IrOperand) -> bool {
        if self.expected_fixed_byte_source(expected, 32).is_none() {
            return false;
        }
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_load_cell_by_field_syscall_to_offsets(
            "output_lock_hash",
            CKB_SOURCE_OUTPUT,
            output_index,
            CKB_CELL_FIELD_LOCK_HASH,
            size_offset,
            buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(size_offset, 32, "output lock hash");
        self.emit("# cellscript abi: verify output lock hash offset=0 size=32");
        let layout = SchemaFieldLayout { index: 0, offset: 0, ty: IrType::Hash, fixed_size: Some(32), fixed_enum_size: None };
        self.emit_loaded_field_bytes_equals_expected(size_offset, buffer_offset, &layout, expected, "output lock hash")
    }

    fn emit_state_transition_check(&mut self, pattern: &CreatePattern, output_size_offset: usize, output_buffer_offset: usize) {
        let Some(states) = self.flow_states.get(&pattern.ty) else {
            return;
        };
        let state_count = states.len();
        let action_edges = self.state_transition_edges_for_pattern(pattern);
        let Some(consumed_var_id) = self.consumed_var_for_state_transition(&pattern.ty, &action_edges) else {
            if !action_edges.is_empty() {
                self.emit_fail(CellScriptRuntimeError::FlowTransitionMismatch);
            }
            return;
        };
        let Some(input_size_offset) = self.cell_buffer_size_offsets.get(&consumed_var_id).copied() else {
            self.emit(format!(
                "# cellscript abi: fail closed because state transition input size offset for {} is unavailable",
                pattern.ty
            ));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(input_buffer_offset) = self.cell_buffer_offsets.get(&consumed_var_id).copied() else {
            self.emit(format!(
                "# cellscript abi: fail closed because state transition input buffer offset for {} is unavailable",
                pattern.ty
            ));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let state_field = self.flow_state_fields.get(&pattern.ty).cloned().unwrap_or_else(|| FLOW_STATE_FIELD_NAME.to_string());
        let Some(state_layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&state_field)).cloned() else {
            self.emit(format!(
                "# cellscript abi: fail closed because state transition field {}.{} has no layout",
                pattern.ty, state_field
            ));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(width) = layout_fixed_scalar_width(&state_layout) else {
            self.emit(format!(
                "# cellscript abi: fail closed because state transition field {}.{} is not a fixed-width scalar",
                pattern.ty, state_field
            ));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            self.emit(format!("# cellscript abi: fail closed because state transition type {} has no fixed size", pattern.ty));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };

        self.emit(format!("# cellscript abi: state transition {}.{} state_count={}", pattern.ty, state_field, state_count));
        self.emit_loaded_schema_exact_size_check(input_size_offset, expected_size, &format!("{} input", pattern.ty));
        self.emit_loaded_schema_bounds_check(
            input_size_offset,
            state_layout.offset + width,
            &format!("{} input.{}", pattern.ty, state_field),
        );
        self.emit_loaded_schema_bounds_check(
            output_size_offset,
            state_layout.offset + width,
            &format!("{} output.{}", pattern.ty, state_field),
        );
        self.emit_sp_addi("t4", input_buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", state_layout.offset, width);
        let old_range_ok_label = self.fresh_label("flow_old_state_range_ok");
        self.emit(format!("li t3, {}", state_count));
        self.emit("sltu t2, t0, t3");
        self.emit(format!("bnez t2, {}", old_range_ok_label));
        self.emit_fail(CellScriptRuntimeError::FlowOldStateInvalid);
        self.emit_label(&old_range_ok_label);

        self.emit_sp_addi("t4", output_buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t1", "t2", state_layout.offset, width);
        let ok_label = self.fresh_label("flow_transition_ok");
        let rules = self.state_transition_rules_for_pattern(pattern, &action_edges);
        if rules.is_empty() {
            self.emit("addi t0, t0, 1");
            self.emit("sub t2, t1, t0");
            self.emit(format!("beqz t2, {}", ok_label));
        } else {
            for rule in rules {
                let next_rule_label = self.fresh_label("flow_transition_next_rule");
                self.emit(format!("li t3, {}", rule.from_index));
                self.emit("sub t2, t0, t3");
                self.emit(format!("bnez t2, {}", next_rule_label));
                self.emit(format!("li t3, {}", rule.to_index));
                self.emit("sub t2, t1, t3");
                self.emit(format!("beqz t2, {}", ok_label));
                self.emit_label(&next_rule_label);
            }
        }
        self.emit_fail(CellScriptRuntimeError::FlowTransitionMismatch);
        self.emit_label(&ok_label);

        let range_ok_label = self.fresh_label("flow_state_range_ok");
        self.emit(format!("li t3, {}", state_count));
        self.emit("sltu t2, t1, t3");
        self.emit(format!("bnez t2, {}", range_ok_label));
        self.emit_fail(CellScriptRuntimeError::FlowNewStateInvalid);
        self.emit_label(&range_ok_label);
    }

    fn state_transition_edges_for_pattern(&self, pattern: &CreatePattern) -> Vec<IrStateTransitionEdge> {
        self.current_state_transition_edges
            .iter()
            .filter(|state_edge| {
                state_edge.type_name == pattern.ty
                    && state_edge.output_binding.as_ref().is_none_or(|binding| binding == &pattern.binding)
            })
            .cloned()
            .collect()
    }

    fn state_transition_rules_for_pattern(&self, pattern: &CreatePattern, action_edges: &[IrStateTransitionEdge]) -> Vec<IrFlowRule> {
        if !action_edges.is_empty() {
            return action_edges
                .iter()
                .map(|state_edge| IrFlowRule {
                    from: state_edge.from.clone(),
                    to: state_edge.to.clone(),
                    from_index: state_edge.from_index,
                    to_index: state_edge.to_index,
                })
                .collect();
        }
        self.flow_rules.get(&pattern.ty).cloned().unwrap_or_default()
    }

    fn consumed_var_for_state_transition(&self, type_name: &str, action_edges: &[IrStateTransitionEdge]) -> Option<usize> {
        if let Some(binding) = action_edges.iter().filter_map(|state_edge| state_edge.input_binding.as_ref()).next() {
            let var_id = self.consume_binding_ids.get(binding).copied()?;
            if self.consume_type_names.get(&var_id).is_some_and(|consumed_type| consumed_type == type_name) {
                return Some(var_id);
            }
            return None;
        }
        self.consumed_var_for_type(type_name)
    }

    fn emit_settle_final_state_check(&mut self, pattern: &CreatePattern, output_size_offset: usize, output_buffer_offset: usize) {
        let Some(states) = self.flow_states.get(&pattern.ty) else {
            return;
        };
        if states.len() < 2 {
            return;
        }
        let final_state = states.len() - 1;
        let Some(consumed_var_id) = self.consumed_var_for_type(&pattern.ty) else {
            self.emit(format!("# cellscript abi: fail closed because settle consumed var for {} is unavailable", pattern.ty));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(input_size_offset) = self.cell_buffer_size_offsets.get(&consumed_var_id).copied() else {
            self.emit(format!("# cellscript abi: fail closed because settle input size offset for {} is unavailable", pattern.ty));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(input_buffer_offset) = self.cell_buffer_offsets.get(&consumed_var_id).copied() else {
            self.emit(format!("# cellscript abi: fail closed because settle input buffer offset for {} is unavailable", pattern.ty));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let state_field = self.flow_state_fields.get(&pattern.ty).cloned().unwrap_or_else(|| FLOW_STATE_FIELD_NAME.to_string());
        let Some(state_layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(&state_field)).cloned() else {
            self.emit(format!("# cellscript abi: fail closed because settle field {}.{} has no layout", pattern.ty, state_field));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(width) = layout_fixed_scalar_width(&state_layout) else {
            self.emit(format!(
                "# cellscript abi: fail closed because settle field {}.{} is not a fixed-width scalar",
                pattern.ty, state_field
            ));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            self.emit(format!("# cellscript abi: fail closed because settle type {} has no fixed size", pattern.ty));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };

        self.emit(format!(
            "# cellscript abi: settle final-state {}.{} final_state={} state_count={}",
            pattern.ty,
            state_field,
            final_state,
            states.len()
        ));
        self.emit_loaded_schema_exact_size_check(input_size_offset, expected_size, &format!("{} input", pattern.ty));
        self.emit_loaded_schema_bounds_check(
            input_size_offset,
            state_layout.offset + width,
            &format!("{} input.{}", pattern.ty, state_field),
        );
        self.emit_loaded_schema_bounds_check(
            output_size_offset,
            state_layout.offset + width,
            &format!("{} output.{}", pattern.ty, state_field),
        );

        self.emit_sp_addi("t4", input_buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", state_layout.offset, width);
        self.emit(format!("li t3, {}", final_state));
        self.emit("sub t2, t0, t3");
        let input_ok_label = self.fresh_label("settle_input_final_state_ok");
        self.emit(format!("beqz t2, {}", input_ok_label));
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        self.emit_label(&input_ok_label);

        self.emit_sp_addi("t4", output_buffer_offset);
        self.emit_unaligned_scalar_load("t4", "t1", "t2", state_layout.offset, width);
        self.emit("sub t2, t1, t3");
        let output_ok_label = self.fresh_label("settle_output_final_state_ok");
        self.emit(format!("beqz t2, {}", output_ok_label));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&output_ok_label);
    }

    fn consumed_var_for_type(&self, type_name: &str) -> Option<usize> {
        self.consume_order
            .iter()
            .copied()
            .find(|var_id| self.consume_type_names.get(var_id).is_some_and(|consumed_type| consumed_type == type_name))
    }

    fn is_prelude_available_scalar(&self, operand: &IrOperand) -> bool {
        match operand {
            IrOperand::Const(IrConst::Bool(_) | IrConst::U8(_) | IrConst::U16(_) | IrConst::U32(_) | IrConst::U64(_)) => true,
            IrOperand::Var(var) => matches!(var.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64),
            _ => false,
        }
    }

    fn is_prelude_available_fixed_value(&self, operand: &IrOperand, expected_width: usize) -> bool {
        if self.is_prelude_available_scalar(operand) {
            return true;
        }
        self.expected_fixed_byte_source(operand, expected_width).is_some()
    }

    fn emit_unaligned_scalar_load(&mut self, base_reg: &str, dest_reg: &str, scratch_reg: &str, offset: usize, width: usize) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit_memory_load_with_avoid("lbu", scratch_reg, base_reg, offset + byte_index, &[dest_reg, scratch_reg, base_reg]);
            if byte_index != 0 {
                self.emit(format!("slli {}, {}, {}", scratch_reg, scratch_reg, byte_index * 8));
            }
            self.emit(format!("or {}, {}, {}", dest_reg, dest_reg, scratch_reg));
        }
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let label = format!(".L{}_{}", prefix, self.next_runtime_label);
        self.next_runtime_label += 1;
        label
    }

    fn emit_param_spills(&mut self, params: &[IrParam]) -> Result<()> {
        let mut abi_index = 0usize;
        for param in params {
            if named_type_name(&param.ty).is_some() {
                self.emit(format!(
                    "# cellscript abi: schema param {} pointer={} length={}",
                    param.name,
                    abi_arg_label(abi_index),
                    abi_arg_label(abi_index + 1)
                ));
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                if let Some(size_offset) = self.schema_pointer_size_offsets.get(&param.binding.id).copied() {
                    self.emit_spill_abi_arg(abi_index + 1, size_offset);
                }
                abi_index += 2;
                if let (Some(pointer_offset), Some(size_offset)) = (
                    self.param_type_hash_pointer_offsets.get(&param.binding.id).copied(),
                    self.param_type_hash_size_offsets.get(&param.binding.id).copied(),
                ) {
                    self.emit(format!(
                        "# cellscript abi: schema param {} type_hash pointer={} length={} size=32",
                        param.name,
                        abi_arg_label(abi_index),
                        abi_arg_label(abi_index + 1)
                    ));
                    self.emit_spill_abi_arg(abi_index, pointer_offset);
                    self.emit_spill_abi_arg(abi_index + 1, size_offset);
                    abi_index += 2;
                }
            } else if let Some(width) = fixed_byte_pointer_param_width(&param.ty) {
                self.emit(format!(
                    "# cellscript abi: fixed-byte param {} pointer={} length={} size={}",
                    param.name,
                    abi_arg_label(abi_index),
                    abi_arg_label(abi_index + 1),
                    width
                ));
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&param.binding.id).copied() {
                    self.emit_spill_abi_arg(abi_index + 1, size_offset);
                }
                abi_index += 2;
            } else if let Some(width) = fixed_aggregate_pointer_param_width(&param.ty) {
                self.emit(format!(
                    "# cellscript abi: fixed-aggregate param {} pointer={} length={} size={}",
                    param.name,
                    abi_arg_label(abi_index),
                    abi_arg_label(abi_index + 1),
                    width
                ));
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&param.binding.id).copied() {
                    self.emit_spill_abi_arg(abi_index + 1, size_offset);
                }
                abi_index += 2;
            } else {
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                abi_index += 1;
            }
        }

        Ok(())
    }

    fn emit_spill_abi_arg(&mut self, abi_index: usize, stack_offset: usize) {
        if abi_index < 8 {
            self.emit_stack_store(&format!("a{}", abi_index), stack_offset);
        } else {
            let caller_stack_offset = (abi_index - 8) * 8;
            self.emit(format!("# cellscript abi: arg{} loaded from caller stack +{}", abi_index, caller_stack_offset));
            self.emit(format!("ld t0, {}(fp)", caller_stack_offset));
            self.emit_stack_store("t0", stack_offset);
        }
    }

    fn record_instruction_var(&self, instruction: &IrInstruction, max_var_id: &mut Option<usize>) {
        match instruction {
            IrInstruction::LoadConst { dest, .. }
            | IrInstruction::LoadVar { dest, .. }
            | IrInstruction::Unary { dest, .. }
            | IrInstruction::FieldAccess { dest, .. }
            | IrInstruction::Index { dest, .. }
            | IrInstruction::Length { dest, .. }
            | IrInstruction::TypeHash { dest, .. }
            | IrInstruction::Create { dest, .. }
            | IrInstruction::CreateUnique { dest, .. }
            | IrInstruction::ReadRef { dest, .. } => self.record_var(dest, max_var_id),
            IrInstruction::CollectionNew { dest, capacity, .. } => {
                self.record_var(dest, max_var_id);
                if let Some(capacity) = capacity {
                    self.record_operand(capacity, max_var_id);
                }
            }
            IrInstruction::Move { dest, src } | IrInstruction::Cast { dest, src } => {
                self.record_var(dest, max_var_id);
                self.record_operand(src, max_var_id);
            }
            IrInstruction::Tuple { dest, fields } => {
                self.record_var(dest, max_var_id);
                for field in fields {
                    self.record_operand(field, max_var_id);
                }
            }
            IrInstruction::Binary { dest, left, right, .. } => {
                self.record_var(dest, max_var_id);
                self.record_operand(left, max_var_id);
                self.record_operand(right, max_var_id);
            }
            IrInstruction::StoreVar { src, .. } => self.record_operand(src, max_var_id),
            IrInstruction::Call { dest, args, .. } => {
                if let Some(dest) = dest {
                    self.record_var(dest, max_var_id);
                }
                for arg in args {
                    self.record_operand(arg, max_var_id);
                }
            }
            IrInstruction::Consume { operand } | IrInstruction::Destroy { operand, policy: _ } => {
                self.record_operand(operand, max_var_id)
            }
            IrInstruction::Transfer { dest, operand, to } => {
                self.record_var(dest, max_var_id);
                self.record_operand(operand, max_var_id);
                self.record_operand(to, max_var_id);
            }
            IrInstruction::Claim { dest, receipt } => {
                self.record_var(dest, max_var_id);
                self.record_operand(receipt, max_var_id);
            }
            IrInstruction::Settle { dest, operand } => {
                self.record_var(dest, max_var_id);
                self.record_operand(operand, max_var_id)
            }
            IrInstruction::ReplaceUnique { dest, operand, .. } => {
                self.record_var(dest, max_var_id);
                self.record_operand(operand, max_var_id)
            }
            IrInstruction::CellMetadataEquality { left, right, .. } => {
                self.record_operand(left, max_var_id);
                self.record_operand(right, max_var_id);
            }
            IrInstruction::CollectionPush { collection, value } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(value, max_var_id);
            }
            IrInstruction::CollectionCapacity { dest, collection } => {
                self.record_var(dest, max_var_id);
                self.record_operand(collection, max_var_id);
            }
            IrInstruction::CollectionExtend { collection, slice } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(slice, max_var_id);
            }
            IrInstruction::CollectionClear { collection } => {
                self.record_operand(collection, max_var_id);
            }
            IrInstruction::CollectionReverse { collection } => {
                self.record_operand(collection, max_var_id);
            }
            IrInstruction::CollectionTruncate { collection, len } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(len, max_var_id);
            }
            IrInstruction::CollectionSwap { collection, left, right } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(left, max_var_id);
                self.record_operand(right, max_var_id);
            }
            IrInstruction::CollectionContains { dest, collection, value } => {
                self.record_var(dest, max_var_id);
                self.record_operand(collection, max_var_id);
                self.record_operand(value, max_var_id);
            }
            IrInstruction::CollectionRemove { dest, collection, index } => {
                self.record_var(dest, max_var_id);
                self.record_operand(collection, max_var_id);
                self.record_operand(index, max_var_id);
            }
            IrInstruction::CollectionInsert { collection, index, value } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(index, max_var_id);
                self.record_operand(value, max_var_id);
            }
            IrInstruction::CollectionSet { collection, index, value } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(index, max_var_id);
                self.record_operand(value, max_var_id);
            }
            IrInstruction::CollectionPop { dest, collection } => {
                self.record_var(dest, max_var_id);
                self.record_operand(collection, max_var_id);
            }
        }
    }

    fn record_instruction_fixed_byte_local(&self, instruction: &IrInstruction, offsets: &mut HashMap<usize, usize>) {
        let record = |offsets: &mut HashMap<usize, usize>, var: &IrVar| {
            if var.ty == IrType::U128 {
                offsets.insert(var.id, 16);
            }
        };

        match instruction {
            IrInstruction::LoadConst { dest, .. }
            | IrInstruction::LoadVar { dest, .. }
            | IrInstruction::Unary { dest, .. }
            | IrInstruction::FieldAccess { dest, .. }
            | IrInstruction::Index { dest, .. }
            | IrInstruction::Length { dest, .. }
            | IrInstruction::TypeHash { dest, .. }
            | IrInstruction::Create { dest, .. }
            | IrInstruction::CreateUnique { dest, .. }
            | IrInstruction::ReplaceUnique { dest, .. }
            | IrInstruction::Transfer { dest, .. }
            | IrInstruction::Claim { dest, .. }
            | IrInstruction::Settle { dest, .. }
            | IrInstruction::ReadRef { dest, .. }
            | IrInstruction::CollectionCapacity { dest, .. }
            | IrInstruction::CollectionContains { dest, .. }
            | IrInstruction::CollectionRemove { dest, .. }
            | IrInstruction::CollectionPop { dest, .. }
            | IrInstruction::CollectionNew { dest, .. }
            | IrInstruction::Move { dest, .. }
            | IrInstruction::Cast { dest, .. }
            | IrInstruction::Tuple { dest, .. }
            | IrInstruction::Binary { dest, .. } => record(offsets, dest),
            IrInstruction::Call { dest, func, .. } => {
                if let Some(dest) = dest {
                    if is_ckb_fixed_hash_helper(func) && dest.ty == IrType::Hash {
                        offsets.insert(dest.id, 32);
                    }
                    record(offsets, dest);
                }
            }
            IrInstruction::StoreVar { .. }
            | IrInstruction::Consume { .. }
            | IrInstruction::Destroy { .. }
            | IrInstruction::CellMetadataEquality { .. }
            | IrInstruction::CollectionPush { .. }
            | IrInstruction::CollectionExtend { .. }
            | IrInstruction::CollectionClear { .. }
            | IrInstruction::CollectionReverse { .. }
            | IrInstruction::CollectionTruncate { .. }
            | IrInstruction::CollectionSwap { .. }
            | IrInstruction::CollectionInsert { .. }
            | IrInstruction::CollectionSet { .. } => {}
        }
    }

    fn record_terminator_var(&self, terminator: &IrTerminator, max_var_id: &mut Option<usize>) {
        match terminator {
            IrTerminator::Return(Some(operand)) | IrTerminator::Branch { cond: operand, .. } => {
                self.record_operand(operand, max_var_id)
            }
            IrTerminator::Return(None) | IrTerminator::Jump(_) => {}
        }
    }

    fn record_operand(&self, operand: &IrOperand, max_var_id: &mut Option<usize>) {
        if let IrOperand::Var(var) = operand {
            self.record_var(var, max_var_id);
        }
    }

    fn record_var(&self, var: &IrVar, max_var_id: &mut Option<usize>) {
        *max_var_id = Some(max_var_id.map(|current| current.max(var.id)).unwrap_or(var.id));
    }

    fn emit_store_const_bytes_to_stack(&mut self, bytes: &[u8], offset: usize) {
        for (index, byte) in bytes.iter().enumerate() {
            self.emit(format!("li t0, {}", byte));
            self.emit_stack_store_byte("t0", offset + index);
        }
    }

    fn emit_load_const(&mut self, dest: &IrVar, value: &IrConst) -> Result<()> {
        match value {
            IrConst::Unit => self.emit("li t0, 0"),
            IrConst::U8(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U16(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U32(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U64(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U128(value) => {
                if let Some(offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() {
                    self.emit_store_const_bytes_to_stack(&value.to_le_bytes(), offset);
                    self.emit_sp_addi("t0", offset);
                    self.emit_stack_store("t0", dest.id * 8);
                    return Ok(());
                }
                let label = self.const_data_label_for_bytes(value.to_le_bytes().to_vec());
                self.emit(format!("la t0, {}", label));
            }
            IrConst::Bool(b) => self.emit(format!("li t0, {}", if *b { 1 } else { 0 })),
            IrConst::Address(_) | IrConst::Hash(_) | IrConst::Array(_) => {
                let Some(bytes) = fixed_byte_const_bytes(value) else {
                    self.emit("# cellscript abi: fail closed because fixed-byte constant bytes are not materializable");
                    self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                    self.emit("li t0, 0");
                    self.emit_stack_store("t0", dest.id * 8);
                    return Ok(());
                };
                let label = self.const_data_label_for_bytes(bytes);
                self.emit(format!("la t0, {}", label));
            }
        }
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_load_var(&mut self, dest: &IrVar, name: &str) -> Result<()> {
        self.emit(format!("# load var {}", name));
        let Some(offset) = self.named_var_offsets.get(name).copied() else {
            self.emit("# cellscript abi: fail closed because named variable slot was not allocated");
            self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
            return Ok(());
        };
        self.emit_stack_load("t0", offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_store_var(&mut self, name: &str, src: &IrOperand) -> Result<()> {
        self.emit(format!("# store var {}", name));
        let Some(offset) = self.named_var_offsets.get(name).copied() else {
            self.emit("# cellscript abi: fail closed because named variable slot was not allocated");
            self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
            return Ok(());
        };
        self.emit_operand_to_register("t0", src);
        self.emit_stack_store("t0", offset);
        Ok(())
    }

    fn emit_truncate_register_to_type(&mut self, register: &str, ty: &IrType) {
        match ty {
            IrType::U8 => self.emit(format!("andi {}, {}, 255", register, register)),
            IrType::U16 => self.emit_truncate_register_to_width(register, 16),
            IrType::U32 => self.emit_truncate_register_to_width(register, 32),
            _ => {}
        }
    }

    fn emit_truncate_register_to_width(&mut self, register: &str, width: u32) {
        if width >= 64 {
            return;
        }
        let shift = 64 - width;
        self.emit(format!("slli {}, {}, {}", register, register, shift));
        self.emit(format!("srli {}, {}, {}", register, register, shift));
    }

    fn emit_checked_scalar_fits(&mut self, register: &str, width: u32) {
        if width >= 64 {
            return;
        }
        let ok_label = self.fresh_label("cast_fit_ok");
        self.emit(format!("srli t2, {}, {}", register, width));
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        self.emit_label(&ok_label);
    }

    fn emit_bool_canonical_check(&mut self, register: &str) {
        let ok_label = self.fresh_label("bool_canonical_ok");
        self.emit(format!("beqz {}, {}", register, ok_label));
        self.emit("li t2, 1");
        self.emit(format!("beq {}, t2, {}", register, ok_label));
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        self.emit_label(&ok_label);
    }

    fn emit_divisor_nonzero_guard(&mut self, register: &str) {
        let ok_label = self.fresh_label("divisor_nonzero");
        self.emit(format!("bnez {}, {}", register, ok_label));
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        self.emit_label(&ok_label);
    }

    fn emit_binary(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> Result<()> {
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) && self.emit_dynamic_byte_comparison(dest, op, left, right) {
            return Ok(());
        }
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
            && (operand_fixed_byte_width(left).is_some() || operand_fixed_byte_width(right).is_some())
        {
            if self.emit_fixed_byte_comparison(dest, op, left, right) {
                return Ok(());
            }
            if self.emit_generic_fixed_byte_comparison(dest, op, left, right) {
                return Ok(());
            }
            // Final fallback: emit a fail-closed trap with specific error code
            self.emit(format!("# binary {:?} over fixed-byte operands (unresolved)", op));
            self.emit("# cellscript abi: fail closed because fixed-byte operand sources are not available");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(());
        }

        if self.emit_u128_add_sub_with_u64(dest, op, left, right) {
            return Ok(());
        }
        if dest.ty == IrType::U128 || self.operand_is_u128(left) || self.operand_is_u128(right) {
            self.emit(format!("# binary {:?} over unsupported u128 operand shape", op));
            self.emit("# cellscript abi: fail closed because generic u128 arithmetic/comparison shape is not lowered");
            self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
            return Ok(());
        }

        self.emit_operand_to_register("t0", left);
        self.emit_operand_to_register("t1", right);

        match op {
            BinaryOp::Add => self.emit("add t0, t0, t1"),
            BinaryOp::Sub => self.emit("sub t0, t0, t1"),
            BinaryOp::Mul => self.emit("mul t0, t0, t1"),
            BinaryOp::Div => {
                self.emit_divisor_nonzero_guard("t1");
                self.emit("divu t0, t0, t1");
            }
            BinaryOp::Mod => {
                self.emit_divisor_nonzero_guard("t1");
                self.emit("remu t0, t0, t1");
            }
            BinaryOp::Eq => {
                self.emit("sub t0, t0, t1");
                self.emit("seqz t0, t0");
            }
            BinaryOp::Ne => {
                self.emit("sub t0, t0, t1");
                self.emit("snez t0, t0");
            }
            BinaryOp::Lt => self.emit("sltu t0, t0, t1"),
            BinaryOp::Le => {
                self.emit("sgtu t0, t0, t1");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::Gt => self.emit("sgtu t0, t0, t1"),
            BinaryOp::Ge => {
                self.emit("sltu t0, t0, t1");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::And => {
                self.emit_bool_canonical_check("t0");
                self.emit_bool_canonical_check("t1");
                self.emit("and t0, t0, t1");
            }
            BinaryOp::Or => {
                self.emit_bool_canonical_check("t0");
                self.emit_bool_canonical_check("t1");
                self.emit("or t0, t0, t1");
            }
        }

        if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) {
            self.emit_truncate_register_to_type("t0", &dest.ty);
        }
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_dynamic_byte_comparison(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        let (IrOperand::Var(left_var), IrOperand::Var(right_var)) = (left, right) else {
            return false;
        };
        let Some(left_len_offset) = self.dynamic_value_size_offsets.get(&left_var.id).copied() else {
            return false;
        };
        let Some(right_len_offset) = self.dynamic_value_size_offsets.get(&right_var.id).copied() else {
            return false;
        };

        let equal_value = if matches!(op, BinaryOp::Eq) { 1 } else { 0 };
        let mismatch_value = if matches!(op, BinaryOp::Eq) { 0 } else { 1 };
        let len_equal_label = self.fresh_label("dynamic_bytes_len_equal");
        let bytes_equal_label = self.fresh_label("dynamic_bytes_equal");
        let done_label = self.fresh_label("dynamic_bytes_cmp_done");

        self.emit(format!("# binary {:?} over dynamic byte operands", op));
        self.emit_stack_load("t0", left_len_offset);
        self.emit_stack_load("t1", right_len_offset);
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", len_equal_label));
        self.emit(format!("li t0, {}", mismatch_value));
        self.emit_stack_store("t0", dest.id * 8);
        self.emit(format!("j {}", done_label));

        self.emit_label(&len_equal_label);
        self.emit_stack_load("a0", left_var.id * 8);
        self.emit_stack_load("a1", right_var.id * 8);
        self.emit_stack_load("a2", left_len_offset);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("beqz a0, {}", bytes_equal_label));
        self.emit(format!("li t0, {}", mismatch_value));
        self.emit_stack_store("t0", dest.id * 8);
        self.emit(format!("j {}", done_label));

        self.emit_label(&bytes_equal_label);
        self.emit(format!("li t0, {}", equal_value));
        self.emit_stack_store("t0", dest.id * 8);
        self.emit_label(&done_label);
        true
    }

    fn emit_unary(&mut self, dest: &IrVar, op: UnaryOp, operand: &IrOperand) -> Result<()> {
        self.emit_operand_to_register("t0", operand);

        match op {
            UnaryOp::Neg => {
                return Err(CompileError::new("unary negation is not supported for unsigned integers", crate::error::Span::default()));
            }
            UnaryOp::Not => {
                self.emit_bool_canonical_check("t0");
                self.emit("xori t0, t0, 1");
            }
            UnaryOp::Ref | UnaryOp::Deref => self.emit("# reference conversion (no-op in asm backend)"),
        }

        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> Result<()> {
        if self.emit_schema_field_access(dest, obj, field) {
            return Ok(());
        }
        if self.emit_aggregate_field_access(dest, obj, field) {
            return Ok(());
        }
        if self.emit_tuple_call_return_field_access(dest, obj, field) {
            return Ok(());
        }
        if self.emit_generic_field_access(dest, obj, field) {
            return Ok(());
        }

        self.emit(format!("# field access .{} (unresolved)", field));
        self.emit("# cellscript abi: fail closed because field offset is not computable from available type layout");
        self.emit_fail(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
        Ok(())
    }

    fn emit_schema_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        if !self.schema_pointer_vars.contains(&var.id) {
            return false;
        }
        let Some(type_name) = named_type_name(&var.ty) else {
            return false;
        };
        let Some(layout) = self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
            return false;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            return self.emit_dynamic_schema_field_access(dest, var, type_name, field, &layout);
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: schema field {}.{} offset={} size={}", type_name, field, layout.offset, width));
        self.emit_stack_load("t4", var.id * 8);
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, type_name);
                self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
                if layout_fixed_scalar_width(&layout).is_some() {
                    self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
                    if dest.ty == IrType::Bool {
                        self.emit_bool_canonical_check("t0");
                    }
                } else {
                    self.emit(format!("addi t0, t4, {}", layout.offset));
                }
            } else {
                self.emit_molecule_table_field_bounds_to_t5(
                    "t4",
                    size_offset,
                    layout.index,
                    width,
                    &format!("{}.{}", type_name, field),
                );
                self.emit("add t4, t4, t5");
                if layout_fixed_scalar_width(&layout).is_some() {
                    self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
                    if dest.ty == IrType::Bool {
                        self.emit_bool_canonical_check("t0");
                    }
                } else {
                    self.emit("addi t0, t4, 0");
                }
            }
        } else {
            if !self.type_fixed_sizes.contains_key(type_name) {
                return false;
            }
            if layout_fixed_scalar_width(&layout).is_some() {
                self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
                if dest.ty == IrType::Bool {
                    self.emit_bool_canonical_check("t0");
                }
            } else {
                self.emit(format!("addi t0, t4, {}", layout.offset));
            }
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_dynamic_schema_field_access(
        &mut self,
        dest: &IrVar,
        obj: &IrVar,
        type_name: &str,
        field: &str,
        layout: &SchemaFieldLayout,
    ) -> bool {
        if molecule_vector_element_fixed_width(&layout.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).is_none() {
            return false;
        }
        let Some(size_offset) = self.schema_pointer_size_offsets.get(&obj.id).copied() else {
            return false;
        };
        let Some(dest_size_offset) = self.dynamic_value_size_offsets.get(&dest.id).copied() else {
            return false;
        };
        let Some(field_count) = self.type_layouts.get(type_name).map(|fields| fields.len()) else {
            return false;
        };

        let context = format!("{}.{}", type_name, field);
        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: dynamic schema field {} index={} as Molecule vector bytes", context, layout.index));
        self.emit_stack_load("t4", obj.id * 8);
        self.emit_molecule_table_field_span_to_t5_t6("t4", size_offset, layout.index, field_count, &context);
        self.emit("add t0, t4, t5");
        self.emit("sub t1, t6, t5");
        self.emit_stack_store("t0", dest.id * 8);
        self.emit_stack_store("t1", dest_size_offset);
        true
    }

    fn emit_aggregate_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let Some(source) = self.aggregate_pointer_sources.get(&var.id) else {
            return false;
        };
        let source_ty = source.ty.clone();
        let Some(layout) = aggregate_field_layout(&source_ty, field) else {
            return false;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            return false;
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!(
            "# cellscript abi: fixed aggregate field {}.{} offset={} size={}",
            aggregate_type_label(&source_ty),
            field,
            layout.offset,
            width
        ));
        self.emit_stack_load("t4", var.id * 8);
        if layout_fixed_scalar_width(&layout).is_some() {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            if dest.ty == IrType::Bool {
                self.emit_bool_canonical_check("t0");
            }
        } else {
            self.emit(format!("addi t0, t4, {}", layout.offset));
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_tuple_call_return_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let Some(slot_var_id) = self.tuple_call_return_field_slots.get(&(var.id, field.to_string())).copied() else {
            return false;
        };
        if slot_var_id != dest.id {
            return false;
        }
        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: tuple call return field .{} projected from return register", field));
        true
    }

    /// Generic field access: when specialized paths don't match, try to compute the
    /// field offset from type_layouts and emit an unaligned load from the pointer
    /// stored in the object's stack slot. This works for any named-type variable
    /// whose type has a registered layout, even if it wasn't classified as a
    /// schema_pointer_var or aggregate_pointer_source.
    fn emit_generic_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        let Some(type_name) = named_type_name(&var.ty) else {
            return false;
        };
        if !self.type_fixed_sizes.contains_key(type_name) {
            return false;
        }
        let Some(layout) = self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
            return false;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            return false;
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: generic field {}.{} offset={} size={}", type_name, field, layout.offset, width));

        // Bounds check: if the object has a known size offset, verify the data
        // is large enough to contain this field.
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
            self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
        } else if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&var.id).copied() {
            self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
        }

        // Load the object pointer from the stack slot
        self.emit_stack_load("t4", var.id * 8);
        if layout_fixed_scalar_width(&layout).is_some() {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
            if dest.ty == IrType::Bool {
                self.emit_bool_canonical_check("t0");
            }
        } else {
            self.emit(format!("addi t0, t4, {}", layout.offset));
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> Result<()> {
        if self.emit_fixed_aggregate_index(dest, arr, idx) {
            return Ok(());
        }
        if self.emit_dynamic_molecule_vector_index(dest, arr, idx) {
            return Ok(());
        }
        if self.emit_stack_collection_index(dest, arr, idx) {
            return Ok(());
        }
        if self.emit_dynamic_index_access(dest, arr, idx) {
            return Ok(());
        }

        self.emit("# index access (unresolved)");
        self.emit("# cellscript abi: fail closed because element layout is not statically computable");
        self.emit_fail(CellScriptRuntimeError::TypeHashMismatch);
        Ok(())
    }

    fn emit_fixed_aggregate_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> bool {
        let (IrOperand::Var(arr_var), Some(index)) = (arr, const_usize_operand(idx)) else {
            return false;
        };
        if !self.aggregate_pointer_sources.contains_key(&arr_var.id) {
            return false;
        }
        let IrType::Array(inner, len) = &arr_var.ty else {
            return false;
        };
        if index >= *len {
            return false;
        }
        let Some(element_width) = type_static_length(inner) else {
            return false;
        };
        let Some(total_width) = type_static_length(&arr_var.ty) else {
            return false;
        };
        let offset = index * element_width;
        self.emit(format!("# index access [{}]", index));
        self.emit(format!("# cellscript abi: fixed aggregate index element_offset={} element_size={}", offset, element_width));
        if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&arr_var.id).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, total_width, "fixed aggregate param");
            self.emit_loaded_schema_bounds_check(size_offset, offset + element_width, "fixed aggregate index");
        }
        self.emit_stack_load("t4", arr_var.id * 8);
        if let Some(width) = fixed_scalar_width(inner, Some(element_width)) {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", offset, width);
        } else {
            self.emit(format!("addi t0, t4, {}", offset));
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_dynamic_molecule_vector_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> bool {
        let IrOperand::Var(arr_var) = arr else {
            return false;
        };
        let Some(size_offset) = self
            .dynamic_value_size_offsets
            .get(&arr_var.id)
            .copied()
            .or_else(|| self.schema_pointer_size_offsets.get(&arr_var.id).copied())
        else {
            return false;
        };
        let Some(element_width) = molecule_vector_element_fixed_width(&arr_var.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };

        self.emit("# index access");
        self.emit(format!(
            "# cellscript abi: dynamic Molecule vector index element_size={} size_offset={}",
            element_width, size_offset
        ));
        self.emit_loaded_schema_bounds_check(size_offset, 4, "dynamic Molecule vector index");
        self.emit_stack_load("t4", arr_var.id * 8);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, 4);

        self.emit_stack_load("t3", size_offset);
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t5, t0, t2");
        self.emit("addi t5, t5, 4");
        self.emit("sub t2, t3, t5");
        let size_ok = self.fresh_label("molecule_vector_index_size_ok");
        self.emit(format!("beqz t2, {}", size_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&size_ok);

        match idx {
            IrOperand::Var(v) => self.emit_stack_load("t1", v.id * 8),
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li t1, {}", n)),
            operand => self.emit_operand_to_register("t1", operand),
        }

        let bounds_ok = self.fresh_label("molecule_vector_index_bounds_ok");
        self.emit("sltu t2, t1, t0");
        self.emit(format!("bnez t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t1, t1, t2");
        self.emit("addi t1, t1, 4");
        self.emit("add t4, t4, t1");
        if fixed_scalar_width(&dest.ty, Some(element_width)).is_some() {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, element_width.min(8));
        } else {
            self.emit("addi t0, t4, 0");
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_stack_collection_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> bool {
        let IrOperand::Var(arr_var) = arr else {
            return false;
        };
        if !self.stack_collection_vars.contains(&arr_var.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&arr_var.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        let dest_scalar = fixed_scalar_width(&dest.ty, Some(element_width)).is_some();
        let dest_fixed_bytes = self.fixed_byte_like_width(&dest.ty).is_some_and(|width| width == element_width);
        if !dest_scalar && !dest_fixed_bytes {
            return false;
        }

        self.emit("# index access");
        self.emit(format!("# cellscript abi: stack collection index element_size={}", element_width));
        self.emit_stack_load("t4", arr_var.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", idx);

        let bounds_ok = self.fresh_label("stack_collection_index_bounds_ok");
        self.emit("sltu t2, t1, t0");
        self.emit(format!("bnez t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t1, t1, t2");
        self.emit("add t4, t4, t1");
        if dest_scalar {
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, element_width);
        } else {
            self.emit("addi t0, t4, 0");
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    /// Dynamic index access: compute element offset from array type layout.
    /// Handles cases where the index is not a constant or the array is not in
    /// aggregate_pointer_sources, but the element size is still statically known.
    fn emit_dynamic_index_access(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> bool {
        let IrOperand::Var(arr_var) = arr else {
            return false;
        };
        let IrType::Array(inner, len) = &arr_var.ty else {
            return false;
        };
        let Some(element_width) = type_static_length(inner) else {
            return false;
        };
        let Some(total_width) = type_static_length(&arr_var.ty) else {
            return false;
        };

        self.emit("# index access");
        self.emit(format!("# cellscript abi: dynamic index element_size={}", element_width));

        // Bounds check: if we have a size offset, verify total data is large enough
        if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&arr_var.id).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, total_width, "dynamic index aggregate");
        }

        // Load array base pointer
        self.emit_stack_load("t4", arr_var.id * 8);

        // Load index value into t1
        match idx {
            IrOperand::Var(v) => self.emit_stack_load("t1", v.id * 8),
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li t1, {}", n)),
            operand => self.emit_operand_to_register("t1", operand),
        }

        // Bounds check: index < len
        let bounds_ok = self.fresh_label("idx_bounds_ok");
        self.emit(format!("li t2, {}", len));
        self.emit("sltu t3, t1, t2");
        self.emit(format!("bnez t3, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&bounds_ok);

        // Compute offset = index * element_width
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t1, t1, t2");

        if fixed_scalar_width(inner, Some(element_width)).is_some() {
            // Scalar element: load from base + offset
            self.emit("add t4, t4, t1");
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, element_width.min(8));
        } else {
            // Pointer-sized element: compute base + offset
            self.emit("add t0, t4, t1");
        }
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_length(&mut self, dest: &IrVar, operand: &IrOperand) -> Result<()> {
        self.emit("# length");
        if let Some(static_len) = self.static_length(operand) {
            self.emit(format!("li t0, {}", static_len));
        } else if self.emit_stack_collection_length(operand) || self.emit_dynamic_molecule_vector_length(operand) {
        } else if let Some(size_offset) = self.dynamic_length_from_size_offset(operand) {
            // For schema-backed or fixed-byte params, the actual size word is already
            // stored at the size offset; load it directly.
            self.emit(format!("# cellscript abi: dynamic length from size word at offset={}", size_offset));
            self.emit_stack_load("t0", size_offset);
        } else {
            self.emit("# cellscript abi: fail closed because dynamic length is not available");
            self.emit_fail(CellScriptRuntimeError::CollectionRuntimeUnsupported);
            return Ok(());
        }
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_stack_collection_length(&mut self, operand: &IrOperand) -> bool {
        let IrOperand::Var(var) = operand else {
            return false;
        };
        if !self.stack_collection_vars.contains(&var.id) {
            return false;
        }
        self.emit("# cellscript abi: stack collection length");
        self.emit_stack_load("t4", var.id * 8);
        self.emit("ld t0, -8(t4)");
        true
    }

    fn emit_dynamic_molecule_vector_length(&mut self, operand: &IrOperand) -> bool {
        let IrOperand::Var(var) = operand else {
            return false;
        };
        let Some(size_offset) =
            self.dynamic_value_size_offsets.get(&var.id).copied().or_else(|| self.schema_pointer_size_offsets.get(&var.id).copied())
        else {
            return false;
        };
        let Some(element_width) = molecule_vector_element_fixed_width(&var.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes) else {
            return false;
        };

        self.emit(format!(
            "# cellscript abi: dynamic Molecule vector length element_size={} size_offset={}",
            element_width, size_offset
        ));
        self.emit_loaded_schema_bounds_check(size_offset, 4, "dynamic Molecule vector length");
        self.emit_stack_load("t4", var.id * 8);
        self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, 4);

        self.emit_stack_load("t1", size_offset);
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t0, t2");
        self.emit("addi t3, t3, 4");
        self.emit("sub t2, t1, t3");
        let size_ok = self.fresh_label("molecule_vector_size_ok");
        self.emit(format!("beqz t2, {}", size_ok));
        self.emit_fail(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit_label(&size_ok);
        true
    }

    /// Try to obtain the size offset for a dynamically-sized operand.
    fn dynamic_length_from_size_offset(&self, operand: &IrOperand) -> Option<usize> {
        let IrOperand::Var(var) = operand else {
            return None;
        };
        // Check schema pointer size offsets (named-type params, consumed inputs, read_refs)
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
            return Some(size_offset);
        }
        // Check fixed-byte param size offsets
        if let Some(size_offset) = self.fixed_byte_param_size_offsets.get(&var.id).copied() {
            return Some(size_offset);
        }
        // Check cell buffer size offsets (consumed inputs, read_refs, type_hash)
        if let Some(size_offset) = self.cell_buffer_size_offsets.get(&var.id).copied() {
            return Some(size_offset);
        }
        None
    }

    fn emit_type_hash(&mut self, dest: &IrVar, operand: &IrOperand) -> Result<()> {
        if let Some(output_index) = self.output_type_hash_sources.get(&dest.id).copied() {
            let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() else {
                return Ok(());
            };
            let Some(buffer_offset) = self.cell_buffer_offsets.get(&dest.id).copied() else {
                return Ok(());
            };
            self.emit("# type_hash");
            self.emit_operand_comment("type_hash source", operand);
            self.emit_load_cell_by_field_syscall_to_offsets(
                "output_type_hash",
                CKB_SOURCE_OUTPUT,
                output_index,
                CKB_CELL_FIELD_TYPE_HASH,
                size_offset,
                buffer_offset,
                32,
            );
            self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
            self.emit_loaded_schema_exact_size_check(size_offset, 32, "output type hash");
            self.emit_sp_addi("t0", buffer_offset);
            self.emit_stack_store("t0", dest.id * 8);
            return Ok(());
        }
        if self.emit_runtime_type_hash(dest, operand) {
            return Ok(());
        }
        if let Some(param_id) = self.param_type_hash_sources.get(&dest.id).copied() {
            let Some(pointer_offset) = self.param_type_hash_pointer_offsets.get(&param_id).copied() else {
                return Ok(());
            };
            let Some(size_offset) = self.param_type_hash_size_offsets.get(&param_id).copied() else {
                return Ok(());
            };
            self.emit("# type_hash");
            self.emit_operand_comment("type_hash source", operand);
            self.emit_loaded_schema_exact_size_check(size_offset, 32, "param type hash");
            self.emit_stack_load("t0", pointer_offset);
            self.emit_stack_store("t0", dest.id * 8);
            return Ok(());
        }

        self.emit("# type_hash (unresolved)");
        self.emit("# cellscript abi: fail closed because type_hash source cell cannot be determined");
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        Ok(())
    }

    /// Runtime type_hash: try to load the type hash from a cell identified by the operand's
    /// association with a consumed input, created output, or read_ref cell dep.
    fn emit_runtime_type_hash(&mut self, dest: &IrVar, operand: &IrOperand) -> bool {
        let IrOperand::Var(var) = operand else {
            return false;
        };

        // Try to find which cell this var is associated with
        let (source, index) = if let Some(input_index) = self.consume_indices.get(&var.id).copied() {
            (CKB_SOURCE_INPUT, input_index)
        } else if let Some(output_index) = self.operation_output_indices.get(&var.id).copied() {
            (CKB_SOURCE_OUTPUT, output_index)
        } else if let Some(dep_index) = self.read_ref_indices.get(&var.id).copied() {
            (CKB_SOURCE_CELL_DEP, dep_index)
        } else {
            return false;
        };

        let size_offset = self.cell_buffer_size_offsets.get(&dest.id).copied().unwrap_or_else(|| self.runtime_scratch_size_offset());
        let buffer_offset = self.cell_buffer_offsets.get(&dest.id).copied().unwrap_or_else(|| self.runtime_scratch_buffer_offset());

        self.emit("# type_hash");
        self.emit_operand_comment("type_hash source", operand);
        self.emit_load_cell_by_field_syscall_to_offsets(
            "runtime_type_hash",
            source,
            index,
            CKB_CELL_FIELD_TYPE_HASH,
            size_offset,
            buffer_offset,
            32,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(size_offset, 32, "runtime type hash");
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_collection_new(&mut self, dest: &IrVar, ty: &str, capacity: Option<&IrOperand>) -> Result<()> {
        // Stack-allocated collection: the stack slot stores a pointer to the
        // collection buffer area, with the length word immediately before the buffer.
        // Layout: [length: u64][buffer: RUNTIME_COLLECTION_BUFFER_SIZE bytes]
        // We allocate space in the stack frame and initialize length to 0.
        let collection_slot_size = 8 + RUNTIME_COLLECTION_BUFFER_SIZE;
        let length_offset = self.collection_region_start + collection_slot_size * self.next_collection_slot;
        let buffer_offset = length_offset + 8;

        self.emit(format!("# collection new {}", ty));
        self.emit(format!(
            "# cellscript abi: stack collection buffer_offset={} max_size={}",
            buffer_offset, RUNTIME_COLLECTION_BUFFER_SIZE
        ));
        if let Some(capacity) = capacity {
            self.emit("# cellscript abi: stack collection with_capacity uses fixed backing buffer");
            self.emit_operand_comment("capacity", capacity);
        }

        // Initialize length to 0
        self.emit_stack_store("zero", length_offset);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);
        self.empty_molecule_vector_vars.insert(dest.id);
        self.stack_collection_vars.insert(dest.id);
        self.next_collection_slot += 1;
        Ok(())
    }

    fn emit_collection_capacity(&mut self, dest: &IrVar, collection: &IrOperand) -> Result<()> {
        self.emit("# collection capacity");
        self.emit_operand_comment("collection", collection);
        if self.emit_stack_collection_capacity(dest, collection) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection capacity is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_capacity(&mut self, dest: &IrVar, collection: &IrOperand) -> bool {
        if dest.ty != IrType::U64 {
            return false;
        }
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width == 0 {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection capacity element_size={}", element_width));
        self.emit(format!("li t0, {}", RUNTIME_COLLECTION_BUFFER_SIZE / element_width));
        self.emit_stack_store("t0", dest.id * 8);
        true
    }

    fn emit_collection_push(&mut self, collection: &IrOperand, value: &IrOperand) -> Result<()> {
        self.emit("# collection push");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("value", value);
        if matches!(value, IrOperand::Var(var) if self.verified_collection_push_values.contains(&var.id)) {
            self.emit("# cellscript abi: collection push is covered by mutate append verifier");
            return Ok(());
        }
        if matches!(collection, IrOperand::Var(var) if self.verified_collection_construction_vectors.contains(&var.id)) {
            self.emit("# cellscript abi: collection push is covered by create-output vector verifier");
            return Ok(());
        }
        if self.emit_stack_collection_push(collection, value) {
            return Ok(());
        }
        // In the verifier context, collection push is used for building output data.
        // The verifier doesn't need to actually build the data; it needs to verify
        // that the output cell data matches expectations. The collection operations
        // in the verifier body are vestigial from the source-level specification.
        // For now, emit a fail-closed trap because runtime collection mutation is not
        // needed in the verifier path – the prelude already verified the output.
        self.emit("# cellscript abi: collection push is not needed for verifier execution");
        self.emit("# cellscript abi: if this path is reached, the source program uses dynamic collections");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_push(&mut self, collection: &IrOperand, value: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(width) = self.constructed_byte_vector_part_width(value) else {
            return false;
        };
        if width > RUNTIME_COLLECTION_BUFFER_SIZE {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection push element_size={}", width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit(format!("li t1, {}", width));
        self.emit("mul t2, t0, t1");
        self.emit(format!("li t3, {}", RUNTIME_COLLECTION_BUFFER_SIZE));
        self.emit("sub t5, t3, t2");
        self.emit("sltu t5, t5, t1");
        let capacity_ok = self.fresh_label("stack_collection_push_capacity_ok");
        self.emit(format!("beqz t5, {}", capacity_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&capacity_ok);

        self.emit("add t5, t4, t2");
        if width <= 8 && fixed_scalar_operand_width(value).is_some() {
            self.emit_operand_to_register("t1", value);
            match width {
                1 => self.emit("sb t1, 0(t5)"),
                2 => self.emit("sh t1, 0(t5)"),
                4 => self.emit("sw t1, 0(t5)"),
                8 => self.emit("sd t1, 0(t5)"),
                _ => return false,
            }
        } else {
            let Some(source) = self.expected_fixed_byte_source(value, width) else {
                return false;
            };
            self.emit_prepare_fixed_byte_source(&source, width, "stack collection push");
            self.emit(format!("# cellscript abi: stack collection copy fixed bytes size={}", width));
            for byte_index in 0..width {
                self.emit_fixed_byte_source_byte_to("t1", "t6", &source, byte_index);
                self.emit_stack_load("t4", collection.id * 8);
                self.emit("ld t0, -8(t4)");
                self.emit(format!("li t2, {}", width));
                self.emit("mul t2, t0, t2");
                self.emit("add t4, t4, t2");
                if byte_index <= 2047 {
                    self.emit(format!("sb t1, {}(t4)", byte_index));
                } else {
                    self.emit_large_addi("t0", "t4", byte_index as i64);
                    self.emit("sb t1, 0(t0)");
                }
            }
        }
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit("addi t0, t0, 1");
        self.emit("sd t0, -8(t4)");
        true
    }

    fn emit_collection_extend(&mut self, collection: &IrOperand, slice: &IrOperand) -> Result<()> {
        self.emit("# collection extend_from_slice");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("slice", slice);
        if matches!(collection, IrOperand::Var(var) if self.verified_collection_construction_vectors.contains(&var.id)) {
            self.emit("# cellscript abi: collection extend is covered by create-output vector verifier");
            return Ok(());
        }
        if self.emit_stack_collection_extend(collection, slice) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection extend is not needed for verifier execution");
        self.emit("# cellscript abi: if this path is reached, the source program uses dynamic collections");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_extend(&mut self, collection: &IrOperand, slice: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(width) = operand_fixed_byte_width(slice) else {
            return false;
        };
        let element_width =
            molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).unwrap_or(1);
        if element_width == 0 || width % element_width != 0 {
            return false;
        }
        let element_count = width / element_width;
        if width > RUNTIME_COLLECTION_BUFFER_SIZE {
            return false;
        }
        let Some(source) = self.expected_fixed_byte_source(slice, width) else {
            return false;
        };

        self.emit(format!(
            "# cellscript abi: stack collection extend bytes={} elements={} element_size={}",
            width, element_count, element_width
        ));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit(format!("li t1, {}", element_width));
        self.emit("mul t2, t0, t1");
        self.emit(format!("li t3, {}", RUNTIME_COLLECTION_BUFFER_SIZE));
        self.emit("sub t5, t3, t2");
        self.emit(format!("li t1, {}", width));
        self.emit("sltu t5, t5, t1");
        let capacity_ok = self.fresh_label("stack_collection_extend_capacity_ok");
        self.emit(format!("beqz t5, {}", capacity_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&capacity_ok);

        self.emit_prepare_fixed_byte_source(&source, width, "stack collection extend");
        self.emit(format!("# cellscript abi: stack collection extend copy fixed bytes size={}", width));
        for byte_index in 0..width {
            self.emit_fixed_byte_source_byte_to("t1", "t6", &source, byte_index);
            self.emit_stack_load("t4", collection.id * 8);
            self.emit("ld t0, -8(t4)");
            self.emit(format!("li t2, {}", element_width));
            self.emit("mul t2, t0, t2");
            self.emit("add t4, t4, t2");
            if byte_index <= 2047 {
                self.emit(format!("sb t1, {}(t4)", byte_index));
            } else {
                self.emit_large_addi("t0", "t4", byte_index as i64);
                self.emit("sb t1, 0(t0)");
            }
        }
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit(format!("addi t0, t0, {}", element_count));
        self.emit("sd t0, -8(t4)");
        true
    }

    fn emit_collection_clear(&mut self, collection: &IrOperand) -> Result<()> {
        self.emit("# collection clear");
        self.emit_operand_comment("collection", collection);
        if matches!(collection, IrOperand::Var(var) if self.verified_collection_construction_vectors.contains(&var.id)) {
            self.emit("# cellscript abi: collection clear is covered by create-output vector verifier");
            return Ok(());
        }
        if self.emit_stack_collection_clear(collection) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection clear is not needed for verifier execution");
        self.emit("# cellscript abi: if this path is reached, the source program uses dynamic collections");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_clear(&mut self, collection: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        self.emit("# cellscript abi: stack collection clear");
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("sd zero, -8(t4)");
        true
    }

    fn emit_collection_reverse(&mut self, collection: &IrOperand) -> Result<()> {
        self.emit("# collection reverse");
        self.emit_operand_comment("collection", collection);
        if self.emit_stack_collection_reverse(collection) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection reverse is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_reverse(&mut self, collection: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width == 0 || element_width > RUNTIME_COLLECTION_BUFFER_SIZE {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection reverse element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        let done_label = self.fresh_label("stack_collection_reverse_done");
        self.emit("li t1, 2");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", done_label));

        let left_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let right_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        self.emit_stack_store("zero", left_offset);
        self.emit("addi t0, t0, -1");
        self.emit_stack_store("t0", right_offset);

        let loop_label = self.fresh_label("stack_collection_reverse_loop");
        self.emit_label(&loop_label);
        self.emit_stack_load("t0", left_offset);
        self.emit_stack_load("t1", right_offset);
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", done_label));

        self.emit_stack_load("t4", collection.id * 8);
        self.emit(format!("li t3, {}", element_width));
        self.emit("mul t5, t0, t3");
        self.emit("add t5, t4, t5");
        self.emit("mul t6, t1, t3");
        self.emit("add t6, t4, t6");
        self.emit(format!("# cellscript abi: stack collection reverse swap element_size={}", element_width));
        for byte_index in 0..element_width {
            if byte_index <= 2047 {
                self.emit(format!("lbu t0, {}(t5)", byte_index));
                self.emit(format!("lbu t1, {}(t6)", byte_index));
                self.emit(format!("sb t1, {}(t5)", byte_index));
                self.emit(format!("sb t0, {}(t6)", byte_index));
            } else {
                self.emit_large_addi("t2", "t5", byte_index as i64);
                self.emit_large_addi("t3", "t6", byte_index as i64);
                self.emit("lbu t0, 0(t2)");
                self.emit("lbu t1, 0(t3)");
                self.emit("sb t1, 0(t2)");
                self.emit("sb t0, 0(t3)");
            }
        }
        self.emit_stack_load("t0", left_offset);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", left_offset);
        self.emit_stack_load("t1", right_offset);
        self.emit("addi t1, t1, -1");
        self.emit_stack_store("t1", right_offset);
        self.emit(format!("j {}", loop_label));
        self.emit_label(&done_label);
        true
    }

    fn emit_collection_truncate(&mut self, collection: &IrOperand, len: &IrOperand) -> Result<()> {
        self.emit("# collection truncate");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("len", len);
        if self.emit_stack_collection_truncate(collection, len) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection truncate is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_truncate(&mut self, collection: &IrOperand, len: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }

        self.emit("# cellscript abi: stack collection truncate");
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", len);
        let done_label = self.fresh_label("stack_collection_truncate_done");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", done_label));
        self.emit("sd t1, -8(t4)");
        self.emit_label(&done_label);
        true
    }

    fn emit_collection_swap(&mut self, collection: &IrOperand, left: &IrOperand, right: &IrOperand) -> Result<()> {
        self.emit("# collection swap");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("left", left);
        self.emit_operand_comment("right", right);
        if self.emit_stack_collection_swap(collection, left, right) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection swap is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_swap(&mut self, collection: &IrOperand, left: &IrOperand, right: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width == 0 || element_width > RUNTIME_COLLECTION_BUFFER_SIZE {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection swap element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", left);
        self.emit_operand_to_register("t2", right);

        let left_ok = self.fresh_label("stack_collection_swap_left_ok");
        self.emit("sltu t3, t1, t0");
        self.emit(format!("bnez t3, {}", left_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&left_ok);

        let right_ok = self.fresh_label("stack_collection_swap_right_ok");
        self.emit("sltu t3, t2, t0");
        self.emit(format!("bnez t3, {}", right_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&right_ok);

        self.emit(format!("li t3, {}", element_width));
        self.emit("mul t5, t1, t3");
        self.emit("add t5, t4, t5");
        self.emit("mul t6, t2, t3");
        self.emit("add t6, t4, t6");
        self.emit(format!("# cellscript abi: stack collection swap bytes element_size={}", element_width));
        for byte_index in 0..element_width {
            if byte_index <= 2047 {
                self.emit(format!("lbu t0, {}(t5)", byte_index));
                self.emit(format!("lbu t1, {}(t6)", byte_index));
                self.emit(format!("sb t1, {}(t5)", byte_index));
                self.emit(format!("sb t0, {}(t6)", byte_index));
            } else {
                self.emit_large_addi("t2", "t5", byte_index as i64);
                self.emit_large_addi("t3", "t6", byte_index as i64);
                self.emit("lbu t0, 0(t2)");
                self.emit("lbu t1, 0(t3)");
                self.emit("sb t1, 0(t2)");
                self.emit("sb t0, 0(t3)");
            }
        }
        true
    }

    fn emit_collection_contains(&mut self, dest: &IrVar, collection: &IrOperand, value: &IrOperand) -> Result<()> {
        self.emit("# collection contains");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("value", value);
        if self.emit_stack_collection_contains(dest, collection, value) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection contains is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_contains(&mut self, dest: &IrVar, collection: &IrOperand, value: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(value_width) = self.constructed_byte_vector_part_width(value) else {
            return false;
        };
        let element_width =
            molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes).unwrap_or(value_width);
        if element_width == 0 || element_width != value_width {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection contains element_size={}", element_width));
        let index_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        self.emit_stack_store("zero", index_offset);
        self.emit_stack_store("zero", dest.id * 8);
        let loop_label = self.fresh_label("stack_collection_contains_loop");
        let next_label = self.fresh_label("stack_collection_contains_next");
        let found_label = self.fresh_label("stack_collection_contains_found");
        let done_label = self.fresh_label("stack_collection_contains_done");
        self.emit_label(&loop_label);
        self.emit_stack_load("t1", index_offset);
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t2, -8(t4)");
        self.emit(format!("beq t1, t2, {}", done_label));

        if element_width <= 8 && fixed_scalar_operand_width(value).is_some() {
            self.emit(format!("li t2, {}", element_width));
            self.emit("mul t3, t1, t2");
            self.emit("add t4, t4, t3");
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, element_width);
            self.emit_operand_to_register("t5", value);
            self.emit("sub t6, t0, t5");
            self.emit(format!("beqz t6, {}", found_label));
        } else {
            let Some(source) = self.expected_fixed_byte_source(value, element_width) else {
                return false;
            };
            self.emit_prepare_fixed_byte_source(&source, element_width, "stack collection contains");
            for byte_index in 0..element_width {
                self.emit_stack_load("t1", index_offset);
                self.emit_stack_load("t4", collection.id * 8);
                self.emit(format!("li t2, {}", element_width));
                self.emit("mul t3, t1, t2");
                self.emit("add t4, t4, t3");
                if byte_index <= 2047 {
                    self.emit(format!("lbu t0, {}(t4)", byte_index));
                } else {
                    self.emit_large_addi("t2", "t4", byte_index as i64);
                    self.emit("lbu t0, 0(t2)");
                }
                self.emit_fixed_byte_source_byte_to("t5", "t6", &source, byte_index);
                self.emit("sub t0, t0, t5");
                self.emit(format!("bnez t0, {}", next_label));
            }
            self.emit(format!("j {}", found_label));
        }

        self.emit_label(&next_label);
        self.emit_stack_load("t1", index_offset);
        self.emit("addi t1, t1, 1");
        self.emit_stack_store("t1", index_offset);
        self.emit(format!("j {}", loop_label));
        self.emit_label(&found_label);
        self.emit("li t0, 1");
        self.emit_stack_store("t0", dest.id * 8);
        self.emit_label(&done_label);
        true
    }

    fn emit_collection_remove(&mut self, dest: &IrVar, collection: &IrOperand, index: &IrOperand) -> Result<()> {
        self.emit("# collection remove");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("index", index);
        if self.emit_stack_collection_remove(dest, collection, index) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection remove is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_remove(&mut self, dest: &IrVar, collection: &IrOperand, index: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        let dest_scalar = fixed_scalar_width(&dest.ty, Some(element_width)).is_some();
        let dest_fixed_bytes = self.fixed_byte_like_width(&dest.ty).is_some_and(|width| width == element_width);
        if !dest_scalar && !dest_fixed_bytes {
            return false;
        }
        let removed_value_slots = if dest_fixed_bytes { element_width.div_ceil(8) } else { 0 };
        if dest_fixed_bytes && removed_value_slots + 1 > RUNTIME_EXPR_TEMP_SLOTS {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection remove element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", index);

        let bounds_ok = self.fresh_label("stack_collection_remove_bounds_ok");
        self.emit("sltu t2, t1, t0");
        self.emit(format!("bnez t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t1, t2");
        self.emit("add t5, t4, t3");
        if dest_scalar {
            self.emit_unaligned_scalar_load("t5", "t6", "t2", 0, element_width);
            self.emit_stack_store("t6", dest.id * 8);
        } else {
            let removed_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
            self.emit(format!("# cellscript abi: stack collection remove snapshot fixed bytes size={}", element_width));
            for byte_index in 0..element_width {
                if byte_index <= 2047 {
                    self.emit(format!("lbu t6, {}(t5)", byte_index));
                } else {
                    self.emit_large_addi("t2", "t5", byte_index as i64);
                    self.emit("lbu t6, 0(t2)");
                }
                self.emit_sp_addi("t2", removed_offset + byte_index);
                self.emit("sb t6, 0(t2)");
            }
            self.emit_sp_addi("t6", removed_offset);
            self.emit_stack_store("t6", dest.id * 8);
        }

        let index_offset = self.runtime_expr_temp_offset(removed_value_slots).expect("runtime temp slot");
        self.emit_stack_store("t1", index_offset);
        let shift_loop = self.fresh_label("stack_collection_remove_shift_loop");
        let shift_done = self.fresh_label("stack_collection_remove_shift_done");
        self.emit(format!("# cellscript abi: stack collection remove shift element_size={}", element_width));
        self.emit_label(&shift_loop);
        self.emit_stack_load("t1", index_offset);
        self.emit("addi t2, t1, 1");
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit("sltu t3, t2, t0");
        self.emit(format!("beqz t3, {}", shift_done));
        self.emit(format!("li t3, {}", element_width));
        self.emit("mul t5, t1, t3");
        self.emit("add t5, t4, t5");
        self.emit("mul t6, t2, t3");
        self.emit("add t6, t4, t6");
        for byte_index in 0..element_width {
            if byte_index <= 2047 {
                self.emit(format!("lbu t0, {}(t6)", byte_index));
                self.emit(format!("sb t0, {}(t5)", byte_index));
            } else {
                self.emit_large_addi("t0", "t6", byte_index as i64);
                self.emit("lbu t0, 0(t0)");
                self.emit_large_addi("t2", "t5", byte_index as i64);
                self.emit("sb t0, 0(t2)");
            }
        }
        self.emit_stack_load("t1", index_offset);
        self.emit("addi t1, t1, 1");
        self.emit_stack_store("t1", index_offset);
        self.emit(format!("j {}", shift_loop));
        self.emit_label(&shift_done);
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit("addi t0, t0, -1");
        self.emit("sd t0, -8(t4)");
        true
    }

    fn emit_collection_pop(&mut self, dest: &IrVar, collection: &IrOperand) -> Result<()> {
        self.emit("# collection pop");
        self.emit_operand_comment("collection", collection);
        if self.emit_stack_collection_pop(dest, collection) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection pop is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_pop(&mut self, dest: &IrVar, collection: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        let dest_scalar = fixed_scalar_width(&dest.ty, Some(element_width)).is_some();
        let dest_fixed_bytes = self.fixed_byte_like_width(&dest.ty).is_some_and(|width| width == element_width);
        if !dest_scalar && !dest_fixed_bytes {
            return false;
        }

        self.emit(format!("# cellscript abi: stack collection pop element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        let bounds_ok = self.fresh_label("stack_collection_pop_bounds_ok");
        self.emit(format!("bnez t0, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit("addi t1, t0, -1");
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t1, t2");
        self.emit("add t5, t4, t3");
        if dest_scalar {
            self.emit_unaligned_scalar_load("t5", "t6", "t2", 0, element_width);
            self.emit_stack_store("t6", dest.id * 8);
        } else {
            self.emit("# cellscript abi: stack collection pop fixed bytes");
            self.emit_stack_store("t5", dest.id * 8);
        }
        self.emit("sd t1, -8(t4)");
        true
    }

    fn emit_collection_insert(&mut self, collection: &IrOperand, index: &IrOperand, value: &IrOperand) -> Result<()> {
        self.emit("# collection insert");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("index", index);
        self.emit_operand_comment("value", value);
        if self.emit_stack_collection_insert(collection, index, value) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection insert is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_insert(&mut self, collection: &IrOperand, index: &IrOperand, value: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(value_width) = self.constructed_byte_vector_part_width(value) else {
            return false;
        };
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width != value_width {
            return false;
        }
        let value_scalar = element_width <= 8 && fixed_scalar_operand_width(value).is_some();
        let fixed_byte_source = if value_scalar {
            None
        } else {
            if element_width > (RUNTIME_EXPR_TEMP_SLOTS - 2) * 8 {
                return false;
            }
            let Some(source) = self.expected_fixed_byte_source(value, element_width) else {
                return false;
            };
            Some(source)
        };

        self.emit(format!("# cellscript abi: stack collection insert element_size={}", element_width));
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", index);

        let bounds_ok = self.fresh_label("stack_collection_insert_bounds_ok");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t0, t2");
        self.emit(format!("li t5, {}", RUNTIME_COLLECTION_BUFFER_SIZE));
        self.emit("sub t6, t5, t3");
        self.emit("sltu t6, t6, t2");
        let capacity_ok = self.fresh_label("stack_collection_insert_capacity_ok");
        self.emit(format!("beqz t6, {}", capacity_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&capacity_ok);

        let index_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let current_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        self.emit_stack_store("t1", index_offset);
        self.emit_stack_store("t0", current_offset);
        if let Some(source) = fixed_byte_source.as_ref() {
            self.emit_prepare_fixed_byte_source(source, element_width, "stack collection insert");
            let value_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
            self.emit(format!("# cellscript abi: stack collection insert snapshot fixed bytes size={}", element_width));
            for byte_index in 0..element_width {
                self.emit_fixed_byte_source_byte_to("t1", "t6", source, byte_index);
                self.emit_sp_addi("t6", value_offset + byte_index);
                self.emit("sb t1, 0(t6)");
            }
        }
        let shift_loop = self.fresh_label("stack_collection_insert_shift_loop");
        let shift_done = self.fresh_label("stack_collection_insert_shift_done");
        self.emit(format!("# cellscript abi: stack collection insert shift element_size={}", element_width));
        self.emit_label(&shift_loop);
        self.emit_stack_load("t0", current_offset);
        self.emit_stack_load("t1", index_offset);
        self.emit(format!("beq t0, t1, {}", shift_done));
        self.emit("addi t2, t0, -1");
        self.emit_stack_load("t4", collection.id * 8);
        self.emit(format!("li t3, {}", element_width));
        self.emit("mul t5, t0, t3");
        self.emit("add t5, t4, t5");
        self.emit("mul t6, t2, t3");
        self.emit("add t6, t4, t6");
        if element_width <= 8 {
            self.emit_unaligned_scalar_load("t6", "t0", "t2", 0, element_width);
            match element_width {
                1 => self.emit("sb t0, 0(t5)"),
                2 => self.emit("sh t0, 0(t5)"),
                4 => self.emit("sw t0, 0(t5)"),
                8 => self.emit("sd t0, 0(t5)"),
                _ => return false,
            }
        } else {
            for byte_index in 0..element_width {
                if byte_index <= 2047 {
                    self.emit(format!("lbu t0, {}(t6)", byte_index));
                    self.emit(format!("sb t0, {}(t5)", byte_index));
                } else {
                    self.emit_large_addi("t0", "t6", byte_index as i64);
                    self.emit("lbu t0, 0(t0)");
                    self.emit_large_addi("t2", "t5", byte_index as i64);
                    self.emit("sb t0, 0(t2)");
                }
            }
        }
        self.emit_stack_load("t0", current_offset);
        self.emit("addi t0, t0, -1");
        self.emit_stack_store("t0", current_offset);
        self.emit(format!("j {}", shift_loop));
        self.emit_label(&shift_done);

        self.emit_stack_load("t4", collection.id * 8);
        self.emit_stack_load("t0", index_offset);
        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t0, t2");
        self.emit("add t5, t4, t3");
        if value_scalar {
            self.emit_operand_to_register("t1", value);
            match element_width {
                1 => self.emit("sb t1, 0(t5)"),
                2 => self.emit("sh t1, 0(t5)"),
                4 => self.emit("sw t1, 0(t5)"),
                8 => self.emit("sd t1, 0(t5)"),
                _ => return false,
            }
        } else {
            let value_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
            self.emit(format!("# cellscript abi: stack collection insert copy fixed bytes size={}", element_width));
            for byte_index in 0..element_width {
                self.emit_sp_addi("t6", value_offset + byte_index);
                self.emit("lbu t1, 0(t6)");
                if byte_index <= 2047 {
                    self.emit(format!("sb t1, {}(t5)", byte_index));
                } else {
                    self.emit_large_addi("t0", "t5", byte_index as i64);
                    self.emit("sb t1, 0(t0)");
                }
            }
        }
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit("addi t0, t0, 1");
        self.emit("sd t0, -8(t4)");
        true
    }

    fn emit_collection_set(&mut self, collection: &IrOperand, index: &IrOperand, value: &IrOperand) -> Result<()> {
        self.emit("# collection set");
        self.emit_operand_comment("collection", collection);
        self.emit_operand_comment("index", index);
        self.emit_operand_comment("value", value);
        if self.emit_stack_collection_set(collection, index, value) {
            return Ok(());
        }
        self.emit("# cellscript abi: collection set is not available for this collection");
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        Ok(())
    }

    fn emit_stack_collection_set(&mut self, collection: &IrOperand, index: &IrOperand, value: &IrOperand) -> bool {
        let IrOperand::Var(collection) = collection else {
            return false;
        };
        if !self.stack_collection_vars.contains(&collection.id) {
            return false;
        }
        let Some(value_width) = self.constructed_byte_vector_part_width(value) else {
            return false;
        };
        let Some(element_width) = molecule_vector_element_fixed_width(&collection.ty, &self.type_fixed_sizes, &self.enum_fixed_sizes)
        else {
            return false;
        };
        if element_width == 0 || element_width > RUNTIME_COLLECTION_BUFFER_SIZE || element_width != value_width {
            return false;
        }
        let value_scalar = element_width <= 8 && fixed_scalar_operand_width(value).is_some();
        let fixed_byte_source = if value_scalar {
            None
        } else {
            let Some(source) = self.expected_fixed_byte_source(value, element_width) else {
                return false;
            };
            Some(source)
        };

        self.emit(format!("# cellscript abi: stack collection set element_size={}", element_width));
        if let Some(source) = fixed_byte_source.as_ref() {
            self.emit_prepare_fixed_byte_source(source, element_width, "stack collection set");
        }
        self.emit_stack_load("t4", collection.id * 8);
        self.emit("ld t0, -8(t4)");
        self.emit_operand_to_register("t1", index);

        let bounds_ok = self.fresh_label("stack_collection_set_bounds_ok");
        self.emit("sltu t2, t1, t0");
        self.emit(format!("bnez t2, {}", bounds_ok));
        self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&bounds_ok);

        self.emit(format!("li t2, {}", element_width));
        self.emit("mul t3, t1, t2");
        self.emit("add t5, t4, t3");
        if value_scalar {
            self.emit_operand_to_register("t1", value);
            match element_width {
                1 => self.emit("sb t1, 0(t5)"),
                2 => self.emit("sh t1, 0(t5)"),
                4 => self.emit("sw t1, 0(t5)"),
                8 => self.emit("sd t1, 0(t5)"),
                _ => return false,
            }
        } else {
            let source = fixed_byte_source.as_ref().expect("fixed byte source");
            self.emit(format!("# cellscript abi: stack collection set copy fixed bytes size={}", element_width));
            for byte_index in 0..element_width {
                self.emit_fixed_byte_source_byte_to("t1", "t6", source, byte_index);
                if byte_index <= 2047 {
                    self.emit(format!("sb t1, {}(t5)", byte_index));
                } else {
                    self.emit_large_addi("t0", "t5", byte_index as i64);
                    self.emit("sb t1, 0(t0)");
                }
            }
        }
        true
    }

    fn emit_ckb_fixed_hash_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !is_ckb_fixed_hash_helper(func) {
            return Ok(false);
        }
        self.emit(format!("# call {}", func));
        let Some(dest) = dest else {
            self.emit("# cellscript abi: fail closed because hash helper result has no destination");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(arg) = args.first() else {
            self.emit("# cellscript abi: fail closed because hash helper is missing input");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: fail closed because hash helper output buffer was not allocated");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(source) = self.expected_fixed_byte_source(arg, 32) else {
            self.emit("# cellscript abi: fail closed because hash helper input is not a 32-byte value");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        self.emit_prepare_fixed_byte_source(&source, 32, "hash_blake2b input");
        if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &source) {
            self.emit("# cellscript abi: fail closed because hash helper input pointer is not materializable");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        }
        self.emit_sp_addi("a1", dest_offset);
        self.emit("call __ckb_hash_blake2b");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    fn emit_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<()> {
        if self.emit_ckb_fixed_hash_call(dest, func, args)? {
            return Ok(());
        }
        if func.contains("::") {
            return Err(CompileError::new(
                format!(
                    "external function call '{}' is not linkable yet; importable function summaries are only used for type/effect checking",
                    func
                ),
                crate::error::Span::default(),
            ));
        }
        self.emit(format!("# call {}", func));

        let abi = self.callable_abis.get(func).cloned();
        let outgoing_stack_arg_bytes = call_abi_arg_count(abi.as_ref(), args).saturating_sub(8) * 8;
        let mut abi_index = 0usize;
        for (arg_index, arg) in args.iter().enumerate() {
            if let Some(abi) = &abi {
                if let Some(param) = abi.params.get(arg_index) {
                    let needs_type_hash = abi.type_hash_param_indices.contains(&arg_index);
                    if !self.emit_call_param_arg(func, param, needs_type_hash, &mut abi_index, arg, outgoing_stack_arg_bytes) {
                        return Ok(());
                    }
                    continue;
                }
            }
            if !self.emit_call_scalar_arg(func, &format!("arg{}", arg_index), &mut abi_index, arg, outgoing_stack_arg_bytes) {
                return Ok(());
            }
        }

        if outgoing_stack_arg_bytes > 0 {
            self.emit(format!("# cellscript abi: reserve {} bytes for outgoing stack call arguments", outgoing_stack_arg_bytes));
            self.emit_large_addi("sp", "sp", -(outgoing_stack_arg_bytes as i64));
        }
        self.emit(format!("call {}", func));
        if outgoing_stack_arg_bytes > 0 {
            self.emit_large_addi("sp", "sp", outgoing_stack_arg_bytes as i64);
        }

        if let Some(d) = dest {
            if let IrType::Tuple(items) = &d.ty {
                self.emit_stack_store("a0", d.id * 8);
                for index in 0..items.len().min(8) {
                    let field = index.to_string();
                    if let Some(field_var_id) = self.tuple_call_return_field_slots.get(&(d.id, field)).copied() {
                        self.emit_stack_store(&format!("a{}", index), field_var_id * 8);
                    }
                }
            } else {
                self.emit_stack_store("a0", d.id * 8);
            }
        }

        Ok(())
    }

    fn emit_call_param_arg(
        &mut self,
        func: &str,
        param: &IrParam,
        needs_type_hash: bool,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        if named_type_name(&param.ty).is_some() {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} pointer={} length={}",
                func,
                param.name,
                abi_arg_label(*abi_index),
                abi_arg_label(*abi_index + 1)
            ));
            if !self.emit_call_pointer_arg(func, &param.name, abi_index, arg, None, outgoing_stack_arg_bytes) {
                return false;
            }
            if !self.emit_call_length_arg(func, &param.name, abi_index, arg, CallLengthKind::Schema, outgoing_stack_arg_bytes) {
                return false;
            }
            if needs_type_hash {
                self.emit(format!(
                    "# cellscript abi: call {} schema param {} type_hash pointer={} length={} size=32",
                    func,
                    param.name,
                    abi_arg_label(*abi_index),
                    abi_arg_label(*abi_index + 1)
                ));
                if !self.emit_call_type_hash_pointer_arg(func, &param.name, abi_index, arg, outgoing_stack_arg_bytes) {
                    return false;
                }
                if !self.emit_call_type_hash_length_arg(func, &param.name, abi_index, arg, outgoing_stack_arg_bytes) {
                    return false;
                }
            }
            return true;
        }

        let fixed_pointer_width = fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty));
        if let Some(width) = fixed_pointer_width {
            self.emit(format!(
                "# cellscript abi: call {} fixed-byte param {} pointer={} length={} size={}",
                func,
                param.name,
                abi_arg_label(*abi_index),
                abi_arg_label(*abi_index + 1),
                width
            ));
            if !self.emit_call_pointer_arg(func, &param.name, abi_index, arg, Some(width), outgoing_stack_arg_bytes) {
                return false;
            }
            if !self.emit_call_length_arg(func, &param.name, abi_index, arg, CallLengthKind::FixedBytes, outgoing_stack_arg_bytes) {
                return false;
            }
            return true;
        }

        self.emit_call_scalar_arg(func, &param.name, abi_index, arg, outgoing_stack_arg_bytes)
    }

    fn emit_call_scalar_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        self.emit(format!("# cellscript abi: call {} scalar {} -> {}", func, label, register));
        self.emit_operand_to_register(&register, arg);
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_pointer_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        const_width: Option<usize>,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        if const_width.is_some() && matches!(arg, IrOperand::Const(_)) {
            self.emit(format!(
                "# cellscript abi: call {} pointer param {} uses a constant unsupported by the call ABI; pass null pointer",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        } else {
            self.emit_operand_to_register(&register, arg);
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_length_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        kind: CallLengthKind,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        let size_offset = match (arg, kind) {
            (IrOperand::Var(var), CallLengthKind::Schema) => self.schema_pointer_size_offsets.get(&var.id).copied(),
            (IrOperand::Var(var), CallLengthKind::FixedBytes) => self.fixed_byte_param_size_offsets.get(&var.id).copied(),
            _ => None,
        };
        if let Some(size_offset) = size_offset {
            self.emit_stack_load(&register, size_offset);
        } else if let CallLengthKind::FixedBytes = kind {
            if matches!(arg, IrOperand::Const(_)) {
                self.emit(format!(
                    "# cellscript abi: call {} fixed-byte const param {} has no materialized pointer; pass zero length to fail closed",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            } else {
                self.emit(format!(
                    "# cellscript abi: call {} fixed-byte param {} has no tracked ABI length; pass zero length to fail closed",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            }
        } else {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} has no tracked ABI length; pass zero length to fail closed",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_type_hash_pointer_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        if let IrOperand::Var(var) = arg {
            if let Some(pointer_offset) = self.param_type_hash_pointer_offsets.get(&var.id).copied() {
                self.emit_stack_load(&register, pointer_offset);
            } else {
                self.emit(format!(
                    "# cellscript abi: call {} schema param {} has no tracked TypeHash pointer; pass null pointer",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            }
        } else {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} TypeHash source is not a variable; pass null pointer",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_type_hash_length_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        if let IrOperand::Var(var) = arg {
            if let Some(size_offset) = self.param_type_hash_size_offsets.get(&var.id).copied() {
                self.emit_stack_load(&register, size_offset);
            } else {
                self.emit(format!(
                    "# cellscript abi: call {} schema param {} has no tracked TypeHash length; pass zero length to fail closed",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            }
        } else {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} TypeHash length source is not a variable; pass zero length",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_outgoing_call_stack_arg_store(&mut self, register: &str, abi_index: usize, outgoing_stack_arg_bytes: usize) {
        if abi_index < 8 {
            return;
        }
        let stack_slot_offset = (abi_index - 8) * 8;
        let offset = i64::try_from(stack_slot_offset).expect("call stack slot should fit in i64")
            - i64::try_from(outgoing_stack_arg_bytes).expect("call stack argument area should fit in i64");
        self.emit(format!(
            "# cellscript abi: stage outgoing stack arg{} at pre-call sp{}{}",
            abi_index,
            if offset < 0 { "" } else { "+" },
            offset
        ));
        self.emit_sp_store_signed(register, offset);
    }

    fn emit_sp_store_signed(&mut self, register: &str, offset: i64) {
        if small_signed_immediate(offset) {
            self.emit(format!("sd {}, {}(sp)", register, offset));
        } else {
            let scratch = scratch_register_avoiding(&[register]);
            self.emit(format!("li {}, {}", scratch, offset));
            self.emit(format!("add {}, sp, {}", scratch, scratch));
            self.emit(format!("sd {}, 0({})", register, scratch));
        }
    }

    fn call_abi_register(&self, abi_index: usize) -> String {
        if abi_index < 8 {
            format!("a{}", abi_index)
        } else {
            "t0".to_string()
        }
    }

    fn emit_read_ref(&mut self, dest: &IrVar, ty: &str) -> Result<()> {
        if self.cell_buffer_offsets.contains_key(&dest.id) {
            self.emit(format!("# read_ref {} (preloaded from CellDep)", ty));
            return Ok(());
        }

        // Runtime fallback: emit LOAD_CELL_DATA syscall to load the cell dep data
        // into the scratch buffer and store the pointer.
        let Some(dep_index) = self.read_ref_indices.get(&dest.id).copied() else {
            self.emit("# cellscript abi: fail closed because read_ref CellDep index was not allocated");
            self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
            return Ok(());
        };
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();

        self.emit(format!("# read_ref {}", ty));
        self.emit(format!("# cellscript abi: runtime read_ref CellDep index={}", dep_index));
        self.emit_load_cell_data_syscall_to_offsets(
            "read_ref",
            CKB_SOURCE_CELL_DEP,
            dep_index,
            size_offset,
            buffer_offset,
            RUNTIME_SCRATCH_BUFFER_SIZE,
        );
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_sp_addi("t0", buffer_offset);
        self.emit_stack_store("t0", dest.id * 8);

        // Also store the size so that subsequent schema operations can use it
        self.schema_pointer_size_offsets.insert(dest.id, size_offset);
        self.cell_buffer_size_offsets.insert(dest.id, size_offset);
        self.cell_buffer_offsets.insert(dest.id, buffer_offset);

        Ok(())
    }

    fn emit_move(&mut self, dest: &IrVar, src: &IrOperand) -> Result<()> {
        if dest.ty == IrType::U128 {
            let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
                self.emit("# cellscript abi: fail closed because u128 move destination has no fixed-byte storage");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return Ok(());
            };
            let Some(source) = self.expected_fixed_byte_source(src, 16) else {
                self.emit("# cellscript abi: fail closed because u128 move source is not addressable");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return Ok(());
            };
            self.emit_fixed_byte_source_scalar_to("t0", "t2", "t4", &source, 0, 8);
            self.emit_fixed_byte_source_scalar_to("t1", "t2", "t4", &source, 8, 8);
            self.emit_stack_store("t0", dest_offset);
            self.emit_stack_store("t1", dest_offset + 8);
            self.emit_sp_addi("t0", dest_offset);
            self.emit_stack_store("t0", dest.id * 8);
            return Ok(());
        }
        if let IrOperand::Var(src_var) = src {
            if src_var.ty != dest.ty {
                self.emit("# cellscript abi: fail closed because Move cannot change value type");
                self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
                return Ok(());
            }
        }
        self.emit_operand_to_register("t0", src);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_cast(&mut self, dest: &IrVar, src: &IrOperand) -> Result<()> {
        if dest.ty == IrType::U128 {
            self.emit("# cellscript abi: fail closed because runtime casts to u128 are not supported");
            self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
            return Ok(());
        }

        self.emit_operand_to_register("t0", src);
        let src_ty = match src {
            IrOperand::Var(var) => var.ty.clone(),
            IrOperand::Const(value) => const_ir_type(value),
        };
        if src_ty == IrType::Bool {
            self.emit_bool_canonical_check("t0");
        }

        match dest.ty {
            IrType::Bool => self.emit_checked_scalar_fits("t0", 1),
            IrType::U8 => {
                self.emit_checked_scalar_fits("t0", 8);
                self.emit("andi t0, t0, 255");
            }
            IrType::U16 => {
                self.emit_checked_scalar_fits("t0", 16);
                self.emit_truncate_register_to_width("t0", 16);
            }
            IrType::U32 => {
                self.emit_checked_scalar_fits("t0", 32);
                self.emit_truncate_register_to_width("t0", 32);
            }
            IrType::U64 => {}
            _ => {
                self.emit("# cellscript abi: fail closed because runtime cast target is unsupported");
                self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
                return Ok(());
            }
        }

        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_tuple(&mut self, dest: &IrVar, fields: &[IrOperand]) -> Result<()> {
        self.emit(format!("# cellscript abi: construct tuple aggregate var{} fields={}", dest.id, fields.len()));
        self.emit_stack_store("zero", dest.id * 8);
        Ok(())
    }

    fn emit_operand_to_register(&mut self, register: &str, operand: &IrOperand) {
        match operand {
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("li {}, {}", register, if *b { 1 } else { 0 })),
            IrOperand::Const(value) => {
                if let Some(bytes) = fixed_byte_const_bytes(value) {
                    let label = self.const_data_label_for_bytes(bytes);
                    self.emit(format!("la {}, {}", register, label));
                } else {
                    self.emit(format!("li {}, 0", register));
                }
            }
            IrOperand::Var(v) => self.emit_stack_load(register, v.id * 8),
        }
    }

    /// consume
    fn emit_consume(&mut self, operand: &IrOperand) -> Result<()> {
        self.emit("# consume");
        if let IrOperand::Var(var) = operand {
            if self.consume_indices.contains_key(&var.id) {
                self.emit("# cellscript abi: consumed input pointer retained for verifier field checks");
                return Ok(());
            }
            // Consume a local variable: the actual LOAD_CELL input data loading
            // already happened in the action prelude (generate_consume).
            // Here we only zero out the local binding to enforce linear ownership.
            self.emit_stack_store("zero", var.id * 8);
            return Ok(());
        }
        // Non-Var consume: this should not happen in valid IR, but fail with
        // a specific error code instead of blocking ELF emission.
        self.emit("# cellscript abi: fail closed because consume operand is not a variable");
        self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
        Ok(())
    }

    /// create
    fn emit_create(&mut self, dest: &IrVar, pattern: &CreatePattern) -> Result<()> {
        let output_index = self.operation_output_indices.get(&dest.id).copied().unwrap_or(self.next_virtual_output);
        if pattern.operation == "output" {
            self.emit(format!("# constrain named output {}", pattern.ty));
            for (field, value) in &pattern.fields {
                match value {
                    IrOperand::Const(IrConst::U64(n)) => self.emit(format!("#   field {} = {}", field, n)),
                    IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("#   field {} = {}", field, b)),
                    IrOperand::Var(var) => self.emit(format!("#   field {} <- {}", field, var.name)),
                    _ => self.emit(format!("#   field {} <- <value>", field)),
                }
            }
            if pattern.lock.is_some() {
                self.emit("#   with_lock <expr>");
            }
            if let Some(var_id) = self.output_param_ids.get(&pattern.binding).copied() {
                let Some(size_offset) = self.cell_buffer_size_offsets.get(&var_id).copied() else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                };
                let Some(buffer_offset) = self.cell_buffer_offsets.get(&var_id).copied() else {
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                };
                if pattern.fields.is_empty() {
                    self.emit_state_transition_check(pattern, size_offset, buffer_offset);
                } else if self.can_verify_create_output_fields(pattern) {
                    self.emit_create_output_checks_at(pattern, size_offset, buffer_offset);
                } else {
                    self.emit("# cellscript abi: ordered named output field verification incomplete");
                    self.emit("# cellscript abi: fail closed because the output state is not fully verified");
                    self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                    return Ok(());
                }
                if let Some(lock) = &pattern.lock {
                    if !(self.can_verify_output_lock(pattern) && self.emit_output_lock_hash_check(output_index, lock)) {
                        self.emit("# cellscript abi: output lock verification incomplete for this named output");
                        self.emit("# cellscript abi: fail closed because the output lock is not fully verified");
                        self.emit_fail(CellScriptRuntimeError::EntryWitnessMagicMismatch);
                        return Ok(());
                    }
                }
            } else {
                self.emit_fail(CellScriptRuntimeError::AssertionFailed);
                return Ok(());
            }
            self.emit(format!("li t0, {}", output_index));
            self.emit_stack_store("t0", dest.id * 8);
            self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
            return Ok(());
        }

        self.generate_create(pattern, output_index, false, false)?;
        self.emit(format!("# create {}", pattern.ty));
        for (field, value) in &pattern.fields {
            match value {
                IrOperand::Const(IrConst::U64(n)) => self.emit(format!("#   field {} = {}", field, n)),
                IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("#   field {} = {}", field, b)),
                IrOperand::Var(var) => self.emit(format!("#   field {} <- {}", field, var.name)),
                _ => self.emit(format!("#   field {} <- <value>", field)),
            }
        }
        if pattern.lock.is_some() {
            self.emit("#   with_lock <expr>");
        }
        self.emit(format!("li t0, {}", output_index));
        self.emit_stack_store("t0", dest.id * 8);
        self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
        Ok(())
    }

    fn emit_create_unique_identity_check(&mut self, output_index: usize, pattern: &CreatePattern, identity: &IrIdentityPolicy) {
        self.emit(format!(
            "# cellscript abi: create_unique identity policy {} for Output#{}",
            identity_policy_label(identity),
            output_index
        ));
        match identity {
            IrIdentityPolicy::None => {}
            IrIdentityPolicy::CkbTypeId => {
                self.emit_output_type_hash_present_check(output_index, "create_unique_ckb_type_id_output_type_hash");
            }
            IrIdentityPolicy::Field(field) => {
                self.emit_create_unique_field_identity_anchor(output_index, pattern, field);
            }
            IrIdentityPolicy::ScriptArgs => {
                self.emit_cell_field_hash_equality(
                    "create_unique_group_input_lock_hash",
                    CKB_SOURCE_GROUP_INPUT,
                    0,
                    "create_unique_output_lock_hash",
                    CKB_SOURCE_OUTPUT,
                    output_index,
                    CKB_CELL_FIELD_LOCK_HASH,
                    "LockHash",
                    "create_unique script_args identity anchor",
                    CellScriptRuntimeError::LockHashPreservationMismatch,
                );
            }
            IrIdentityPolicy::SingletonType => {
                self.emit_cell_field_hash_equality(
                    "create_unique_group_input_type_hash",
                    CKB_SOURCE_GROUP_INPUT,
                    0,
                    "create_unique_output_type_hash",
                    CKB_SOURCE_OUTPUT,
                    output_index,
                    CKB_CELL_FIELD_TYPE_HASH,
                    "TypeHash",
                    "create_unique singleton_type identity anchor",
                    CellScriptRuntimeError::TypeHashMismatch,
                );
            }
        }
    }

    fn emit_create_unique_field_identity_anchor(&mut self, output_index: usize, pattern: &CreatePattern, field: &str) {
        let Some(layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(field)).cloned() else {
            self.emit(format!(
                "# cellscript abi: fail closed because create_unique identity field {}.{} has no layout",
                pattern.ty, field
            ));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            self.emit(format!(
                "# cellscript abi: fail closed because create_unique identity field {}.{} is not fixed-width",
                pattern.ty, field
            ));
            self.emit_fail(CellScriptRuntimeError::DynamicFieldValueMismatch);
            return;
        };
        let output_size_offset = self.runtime_scratch_size_offset();
        let output_buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_load_cell_data_syscall("create_unique_identity_field", CKB_SOURCE_OUTPUT, output_index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::CellLoadFailed);
        let output_pointer_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let output_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        let context = format!("create_unique identity field {}.{}", pattern.ty, field);
        if self.type_fixed_sizes.contains_key(&pattern.ty) {
            self.emit_loaded_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                &layout,
                width,
                &context,
                output_pointer_offset,
            );
        } else if let Some(field_count) = self.type_layouts.get(&pattern.ty).map(|fields| fields.len()) {
            self.emit_dynamic_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                &layout,
                field_count,
                width,
                &context,
                output_pointer_offset,
                output_len_offset,
            );
        } else {
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        }
        self.emit(format!(
            "# cellscript abi: create_unique field identity anchored by verified Output#{} {}.{} size={}",
            output_index, pattern.ty, field, width
        ));
    }

    fn emit_replace_unique_identity_check(
        &mut self,
        output_index: usize,
        operand: &IrOperand,
        pattern: &CreatePattern,
        identity: &IrIdentityPolicy,
    ) {
        self.emit(format!(
            "# cellscript abi: replace_unique identity policy {} for Output#{}",
            identity_policy_label(identity),
            output_index
        ));
        let input_index = match operand {
            IrOperand::Var(var) => self.consume_indices.get(&var.id).copied().unwrap_or(0),
            _ => {
                self.emit("# cellscript abi: fail closed because replace_unique identity input is not a consumed cell variable");
                self.emit_fail(CellScriptRuntimeError::DestroyInvalidOperand);
                return;
            }
        };
        match identity {
            IrIdentityPolicy::None => {}
            IrIdentityPolicy::CkbTypeId | IrIdentityPolicy::SingletonType => {
                self.emit_cell_field_hash_equality(
                    "replace_unique_input_type_hash",
                    CKB_SOURCE_INPUT,
                    input_index,
                    "replace_unique_output_type_hash",
                    CKB_SOURCE_OUTPUT,
                    output_index,
                    CKB_CELL_FIELD_TYPE_HASH,
                    "TypeHash",
                    "replace_unique type identity preservation",
                    CellScriptRuntimeError::TypeHashMismatch,
                );
            }
            IrIdentityPolicy::ScriptArgs => {
                self.emit_cell_field_hash_equality(
                    "replace_unique_input_lock_hash",
                    CKB_SOURCE_INPUT,
                    input_index,
                    "replace_unique_output_lock_hash",
                    CKB_SOURCE_OUTPUT,
                    output_index,
                    CKB_CELL_FIELD_LOCK_HASH,
                    "LockHash",
                    "replace_unique script_args identity preservation",
                    CellScriptRuntimeError::LockHashPreservationMismatch,
                );
            }
            IrIdentityPolicy::Field(field) => {
                self.emit_replace_unique_field_identity_check(output_index, operand, pattern, field);
            }
        }
    }

    fn emit_replace_unique_field_identity_check(
        &mut self,
        output_index: usize,
        operand: &IrOperand,
        pattern: &CreatePattern,
        field: &str,
    ) {
        let input_var = match operand {
            IrOperand::Var(var) => var,
            _ => {
                self.emit("# cellscript abi: fail closed because replace_unique identity input is not a cell variable");
                self.emit_fail(CellScriptRuntimeError::DestroyInvalidOperand);
                return;
            }
        };
        let (Some(input_size_offset), Some(input_buffer_offset)) =
            (self.cell_buffer_size_offsets.get(&input_var.id).copied(), self.cell_buffer_offsets.get(&input_var.id).copied())
        else {
            self.emit("# cellscript abi: fail closed because replace_unique identity input cell data is unavailable");
            self.emit_fail(CellScriptRuntimeError::CellLoadFailed);
            return;
        };
        let Some(layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(field)).cloned() else {
            self.emit(format!(
                "# cellscript abi: fail closed because replace_unique identity field {}.{} has no layout",
                pattern.ty, field
            ));
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        };
        let Some(width) = layout_fixed_byte_width(&layout) else {
            self.emit(format!(
                "# cellscript abi: fail closed because replace_unique identity field {}.{} is not fixed-width",
                pattern.ty, field
            ));
            self.emit_fail(CellScriptRuntimeError::DynamicFieldValueMismatch);
            return;
        };

        let output_size_offset = self.runtime_scratch_size_offset();
        let output_buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_load_cell_data_syscall("replace_unique_identity_field_output", CKB_SOURCE_OUTPUT, output_index);
        self.emit_return_on_syscall_error(CellScriptRuntimeError::CellLoadFailed);
        let input_pointer_offset = self.runtime_expr_temp_offset(0).expect("runtime temp slot 0");
        let input_len_offset = self.runtime_expr_temp_offset(1).expect("runtime temp slot 1");
        let output_pointer_offset = self.runtime_expr_temp_offset(2).expect("runtime temp slot 2");
        let output_len_offset = self.runtime_expr_temp_offset(3).expect("runtime temp slot 3");
        let input_context = format!("replace_unique input identity field {}.{}", pattern.ty, field);
        let output_context = format!("replace_unique output identity field {}.{}", pattern.ty, field);
        if self.type_fixed_sizes.contains_key(&pattern.ty) {
            self.emit_loaded_fixed_field_pointer_to_stack(
                input_size_offset,
                input_buffer_offset,
                &layout,
                width,
                &input_context,
                input_pointer_offset,
            );
            self.emit_loaded_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                &layout,
                width,
                &output_context,
                output_pointer_offset,
            );
        } else if let Some(field_count) = self.type_layouts.get(&pattern.ty).map(|fields| fields.len()) {
            self.emit_dynamic_fixed_field_pointer_to_stack(
                input_size_offset,
                input_buffer_offset,
                &layout,
                field_count,
                width,
                &input_context,
                input_pointer_offset,
                input_len_offset,
            );
            self.emit_dynamic_fixed_field_pointer_to_stack(
                output_size_offset,
                output_buffer_offset,
                &layout,
                field_count,
                width,
                &output_context,
                output_pointer_offset,
                output_len_offset,
            );
        } else {
            self.emit_fail(CellScriptRuntimeError::AssertionFailed);
            return;
        }
        self.emit_fixed_pointer_equality(
            input_pointer_offset,
            output_pointer_offset,
            width,
            &format!("replace_unique identity field {}.{} Input == Output#{}", pattern.ty, field, output_index),
            CellScriptRuntimeError::DynamicFieldValueMismatch,
        );
    }

    /// create_unique
    fn emit_create_unique(&mut self, dest: &IrVar, pattern: &CreatePattern, identity: &IrIdentityPolicy) -> Result<()> {
        let output_index = self.operation_output_indices.get(&dest.id).copied().unwrap_or(self.next_virtual_output);
        self.generate_create(pattern, output_index, false, false)?;
        self.emit_create_unique_identity_check(output_index, pattern, identity);
        self.emit(format!("# create_unique {} identity={}", pattern.ty, identity_policy_label(identity)));
        for (field, value) in &pattern.fields {
            match value {
                IrOperand::Const(IrConst::U64(n)) => self.emit(format!("#   field {} = {}", field, n)),
                IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("#   field {} = {}", field, b)),
                IrOperand::Var(var) => self.emit(format!("#   field {} <- {}", field, var.name)),
                _ => self.emit(format!("#   field {} <- <value>", field)),
            }
        }
        if pattern.lock.is_some() {
            self.emit("#   with_lock <expr>");
        }
        self.emit(format!("li t0, {}", output_index));
        self.emit_stack_store("t0", dest.id * 8);
        self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
        Ok(())
    }

    /// replace_unique
    fn emit_replace_unique(
        &mut self,
        dest: &IrVar,
        operand: &IrOperand,
        pattern: &CreatePattern,
        identity: &IrIdentityPolicy,
    ) -> Result<()> {
        let output_index = self.operation_output_indices.get(&dest.id).copied().unwrap_or(self.next_virtual_output);
        self.emit(format!("# replace_unique {} identity={}", pattern.ty, identity_policy_label(identity)));
        self.emit_operand_comment("input", operand);
        for (field, value) in &pattern.fields {
            match value {
                IrOperand::Const(IrConst::U64(n)) => self.emit(format!("#   field {} = {}", field, n)),
                IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("#   field {} = {}", field, b)),
                IrOperand::Var(var) => self.emit(format!("#   field {} <- {}", field, var.name)),
                _ => self.emit(format!("#   field {} <- <value>", field)),
            }
        }
        // replace_unique is a consume + create with identity preservation.
        // The output occupies a virtual output slot, similar to transfer.
        self.generate_create(pattern, output_index, false, false)?;
        self.emit_replace_unique_identity_check(output_index, operand, pattern, identity);
        if self.emit_verified_operation_output_handle(dest, "replace_unique") {
            return Ok(());
        }
        self.emit(format!("# cellscript abi: replace_unique output handle Output#{}", output_index));
        self.emit(format!("li t0, {}", output_index));
        self.emit_stack_store("t0", dest.id * 8);
        self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
        Ok(())
    }

    /// transfer
    fn emit_transfer(&mut self, dest: &IrVar, operand: &IrOperand, to: &IrOperand) -> Result<()> {
        self.emit("# transfer");
        self.emit_operand_comment("asset", operand);
        self.emit_operand_comment("to", to);
        if self.emit_verified_operation_output_handle(dest, "transfer") {
            return Ok(());
        }
        if let Some(output_index) = self.operation_output_indices.get(&dest.id).copied() {
            self.emit(format!("# cellscript abi: transfer output handle Output#{} (unverified)", output_index));
            self.emit(format!("li t0, {}", output_index));
            self.emit_stack_store("t0", dest.id * 8);
            self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
            return Ok(());
        }
        self.emit("# cellscript abi: fail closed because transfer output relation is unknown");
        self.emit_fail(CellScriptRuntimeError::DestroyInvalidOperand);
        Ok(())
    }

    /// claim
    fn emit_claim(&mut self, dest: &IrVar, receipt: &IrOperand) -> Result<()> {
        self.emit("# claim");
        self.emit_operand_comment("receipt", receipt);
        if self.emit_verified_operation_output_handle(dest, "claim") {
            return Ok(());
        }
        if let Some(output_index) = self.operation_output_indices.get(&dest.id).copied() {
            self.emit(format!("# cellscript abi: claim output handle Output#{} (unverified)", output_index));
            self.emit(format!("li t0, {}", output_index));
            self.emit_stack_store("t0", dest.id * 8);
            self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
            return Ok(());
        }
        self.emit("# cellscript abi: fail closed because claim output relation is unknown");
        self.emit_fail(CellScriptRuntimeError::DestroyInvalidOperand);
        Ok(())
    }

    /// settle
    fn emit_settle(&mut self, dest: &IrVar, operand: &IrOperand) -> Result<()> {
        self.emit("# settle");
        self.emit_operand_comment("value", operand);
        if self.emit_verified_operation_output_handle(dest, "settle") {
            return Ok(());
        }
        if let Some(output_index) = self.operation_output_indices.get(&dest.id).copied() {
            self.emit(format!("# cellscript abi: settle output handle Output#{} (unverified)", output_index));
            self.emit(format!("li t0, {}", output_index));
            self.emit_stack_store("t0", dest.id * 8);
            self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
            return Ok(());
        }
        self.emit("# cellscript abi: fail closed because settle output relation is unknown");
        self.emit_fail(CellScriptRuntimeError::DestroyInvalidOperand);
        Ok(())
    }

    fn emit_verified_operation_output_handle(&mut self, dest: &IrVar, operation: &str) -> bool {
        if !self.verified_operation_outputs.contains(&dest.id) {
            return false;
        }
        let output_index = self.operation_output_indices.get(&dest.id).copied().unwrap_or(self.next_virtual_output);
        self.emit(format!("# cellscript abi: {} output relation verified by prelude Output#{}", operation, output_index));
        self.emit(format!("li t0, {}", output_index));
        self.emit_stack_store("t0", dest.id * 8);
        self.next_virtual_output = self.next_virtual_output.max(output_index + 1);
        true
    }

    /// destroy
    fn emit_destroy(&mut self, operand: &IrOperand, policy: &IrDestructionPolicy) -> Result<()> {
        self.emit(format!("# destroy policy={}", destruction_policy_label(policy)));
        if let IrOperand::Var(_) = operand {
            self.emit_operand_comment("destroyed input retained for verifier field checks", operand);
            if destroy_policy_uses_output_absence_scan(policy) {
                self.emit("# cellscript abi: destroy consumed input is checked by policy-specific Output absence scan");
            } else {
                self.emit("# cellscript abi: destroy policy is recorded as runtime-required verifier metadata");
            }
            self.emit("# cellscript abi: retain consumed input pointer for post-destroy output verification");
            return Ok(());
        }
        // Non-Var destroy: this should not happen in valid IR, fail with specific error.
        self.emit("# cellscript abi: fail closed because destroy operand is not a variable");
        self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
        Ok(())
    }

    fn emit_operand_comment(&mut self, label: &str, operand: &IrOperand) {
        let rendered = match operand {
            IrOperand::Var(var) => format!("{}: {}", label, var.name),
            IrOperand::Const(IrConst::U64(n)) => format!("{}: {}", label, n),
            IrOperand::Const(IrConst::Bool(b)) => format!("{}: {}", label, b),
            IrOperand::Const(IrConst::Address(_)) => format!("{}: <address>", label),
            IrOperand::Const(IrConst::Hash(_)) => format!("{}: <hash>", label),
            IrOperand::Const(IrConst::Array(items)) => format!("{}: <array:{}>", label, items.len()),
            IrOperand::Const(_) => format!("{}: <const>", label),
        };
        self.emit(format!("#   {}", rendered));
    }

    fn static_length(&self, operand: &IrOperand) -> Option<usize> {
        match operand {
            IrOperand::Var(var) => Self::static_length_from_type(&var.ty),
            IrOperand::Const(IrConst::Array(items)) => Some(items.len()),
            _ => None,
        }
    }

    fn static_length_from_type(ty: &IrType) -> Option<usize> {
        match ty {
            IrType::Array(_, size) => Some(*size),
            IrType::Ref(inner) | IrType::MutRef(inner) => Self::static_length_from_type(inner),
            _ => None,
        }
    }


    fn assemble(&self, format: ArtifactFormat) -> Result<Vec<u8>> {
        let assembly_text = self.assembly.join("\n");
        match format {
            ArtifactFormat::RiscvAssembly => Ok(assembly_text.into_bytes()),
            ArtifactFormat::RiscvElf => {
                // All former non-executable runtime paths now have real RISC-V
                // lowerings or fail-closed traps with specific error codes.
                // ELF emission is always permitted.
                assemble_elf(&self.assembly)
            }
        }
    }
}

pub fn generate(ir: &IrModule, options: &CodegenOptions, format: ArtifactFormat) -> Result<Vec<u8>> {
    let generator = CodeGenerator::new(options.clone());
    generator.generate(ir, format)
}

pub fn analyze_backend_shape(assembly: &str) -> Result<BackendShapeMetrics> {
    let lines = assembly.lines().map(str::to_string).collect::<Vec<_>>();
    MachineLayoutPlan::build(&lines).map(|plan| plan.metrics.into())
}

fn first_entrypoint(ir: &IrModule) -> Option<(&str, &[IrParam])> {
    for item in &ir.items {
        if let IrItem::Action(action) = item {
            if action.name == "main" {
                return Some((&action.name, &action.params));
            }
        }
    }
    for item in &ir.items {
        if let IrItem::Action(action) = item {
            if action.params.is_empty() {
                return Some((&action.name, &action.params));
            }
        }
    }
    for item in &ir.items {
        if let IrItem::Action(action) = item {
            return Some((&action.name, &action.params));
        }
    }
    for item in &ir.items {
        if let IrItem::Lock(lock) = item {
            return Some((&lock.name, &lock.params));
        }
    }
    None
}


pub(crate) fn named_type_name(ty: &IrType) -> Option<&str> {
    match ty {
        IrType::Named(name) => Some(name.as_str()),
        IrType::Ref(inner) | IrType::MutRef(inner) => named_type_name(inner),
        _ => None,
    }
}

fn consumed_operand_var(instruction: &IrInstruction) -> Option<&IrVar> {
    let operand = match instruction {
        IrInstruction::Consume { operand }
        | IrInstruction::Transfer { operand, .. }
        | IrInstruction::Destroy { operand, .. }
        | IrInstruction::Settle { operand, .. }
        | IrInstruction::ReplaceUnique { operand, .. } => operand,
        IrInstruction::Claim { receipt, .. } => receipt,
        _ => return None,
    };
    match operand {
        IrOperand::Var(var) if named_type_name(&var.ty).is_some() => Some(var),
        _ => None,
    }
}

mod assembler;
mod runtime;
mod abi;

pub use assembler::BackendShapeMetrics;
#[allow(unused_imports)]
pub(crate) use abi::{
    abi_arg_label, call_abi_arg_count, call_param_abi_arg_count, entry_abi_arg_count,
    entry_witness_payload_layout, CallLengthKind, CallableAbi, ENTRY_WITNESS_BUFFER_OFFSET,
    ENTRY_WITNESS_BUFFER_SIZE, ENTRY_WITNESS_FRAME_SIZE, ENTRY_WITNESS_HEADER_SIZE,
    ENTRY_WITNESS_LABEL, ENTRY_WITNESS_MAGIC, ENTRY_WITNESS_RA_OFFSET, ENTRY_WITNESS_SIZE_OFFSET,
    ENTRY_SCRIPT_ARGS_CURSOR_OFFSET, ENTRY_SCRIPT_ARGS_LEN_OFFSET, ENTRY_SCRIPT_ARGS_START_OFFSET,
    ENTRY_SCRIPT_BUFFER_OFFSET, ENTRY_SCRIPT_BUFFER_SIZE, ENTRY_SCRIPT_SIZE_OFFSET,
};
pub(crate) use runtime::{is_ckb_fixed_hash_helper, runtime_syscall_abi, RuntimeSyscallAbi};
#[allow(unused_imports)] // Many items are used only by the #[cfg(test)] module below
pub(crate) use assembler::{
    assemble_elf, assemble_elf_internal, BackendLayoutMetrics,
    strip_comment, parse_register, parse_immediate, small_signed_immediate, scratch_register_avoiding,
    align_up, align_frame, signed_bits_fit, encode_i_type, encode_s_type, encode_ecall,
    CKB_SCRIPT_STACK_TOP, ELF_HEADER_SIZE, ELF_PROGRAM_HEADER_SIZE,
    ParsedAssembly, MachineLayoutPlan, SectionKind, SectionLayout, AsmOp, Instruction,
    MachineCfg, MachineBlock, MachineTerminator, MachineLayoutOrder, MachineCfgEdge,
    MachineCfgEdgeKind, MachinePlacedBlock, TextOpLayout, BranchSizeMode, SymbolDef,
    MachineBlockCoverage,
    memory_operand_offset_and_base, is_min_call, is_runtime_header_u64_call,
    ckb_source_name, encode_li_sequence, unreachable_machine_block_count,
    build_machine_layout_order, validate_machine_layout_order, validate_explicit_toolchain_path,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS: &[(&str, &str)] = &[
        ("add", "add t0, a0, a1"),
        ("addi", "addi t0, t0, -1"),
        ("and", "and t2, a0, a1"),
        ("andi", "andi a0, a0, 255"),
        ("beq", "beq a0, a1, branch_target"),
        ("bge", "bge a0, a1, branch_target"),
        ("bgeu", "bgeu a0, a1, branch_target"),
        ("bgez", "bgez a0, branch_target"),
        ("bgt", "bgt a0, a1, branch_target"),
        ("blt", "blt a1, a0, branch_target"),
        ("bltu", "bltu a1, a0, branch_target"),
        ("bne", "bne a0, a1, branch_target"),
        ("bnez", "bnez a0, branch_target"),
        ("beqz", "beqz a0, branch_target"),
        ("call", "call helper"),
        ("div", "div t5, a0, a1"),
        ("divu", "divu t5, a0, a1"),
        ("ecall", "ecall"),
        ("j", "j done"),
        ("la", "la t3, data_label"),
        ("lbu", "lbu t2, 8(sp)"),
        ("ld", "ld t1, 0(sp)"),
        ("li", "li a0, 8"),
        ("mul", "mul t4, a0, a1"),
        ("mv", "mv s9, a0"),
        ("neg", "neg s6, a0"),
        ("or", "or t3, a0, a1"),
        ("rem", "rem t6, a0, a1"),
        ("remu", "remu t6, a0, a1"),
        ("ret", "ret"),
        ("sb", "sb t1, 8(sp)"),
        ("sd", "sd t0, 0(sp)"),
        ("seqz", "seqz s4, a0"),
        ("sgt", "sgt s2, a0, a1"),
        ("sgtu", "sgtu s2, a0, a1"),
        ("sh", "sh t1, 10(sp)"),
        ("slli", "slli s7, a0, 3"),
        ("slt", "slt s0, a1, a0"),
        ("sltu", "sltu s1, a1, a0"),
        ("snez", "snez s5, a0"),
        ("srli", "srli s8, a0, 1"),
        ("sub", "sub t1, a0, a1"),
        ("sw", "sw t1, 12(sp)"),
        ("xor", "xor a0, a0, a1"),
        ("xori", "xori s3, a0, 1"),
    ];

    const INTENTIONALLY_UNSUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS: &[(&str, &str)] = &[
        ("addiw", "addiw a0, a0, 1"),
        ("addw", "addw a0, a0, a1"),
        ("amoadd.w", "amoadd.w a0, a1, (a2)"),
        ("auipc", "auipc a0, 0"),
        ("ble", "ble a0, a1, target"),
        ("bleu", "bleu a0, a1, target"),
        ("blez", "blez a0, target"),
        ("bgtu", "bgtu a0, a1, target"),
        ("bgtz", "bgtz a0, target"),
        ("bltz", "bltz a0, target"),
        ("c.nop", "c.nop"),
        ("csrr", "csrr a0, cycle"),
        ("fence", "fence"),
        ("flw", "flw fa0, 0(sp)"),
        ("jal", "jal ra, target"),
        ("jalr", "jalr zero, 0(ra)"),
        ("jr", "jr ra"),
        ("lb", "lb a0, 0(sp)"),
        ("lh", "lh a0, 0(sp)"),
        ("lhu", "lhu a0, 0(sp)"),
        ("lui", "lui a0, 1"),
        ("lw", "lw a0, 0(sp)"),
        ("lwu", "lwu a0, 0(sp)"),
        ("nop", "nop"),
        ("not", "not a0, a1"),
        ("ori", "ori a0, a0, 1"),
        ("sll", "sll a0, a0, a1"),
        ("slti", "slti a0, a0, 1"),
        ("sltiu", "sltiu a0, a0, 1"),
        ("sra", "sra a0, a0, a1"),
        ("srai", "srai a0, a0, 1"),
        ("srl", "srl a0, a0, a1"),
        ("subw", "subw a0, a0, a1"),
        ("tail", "tail target"),
    ];

    #[test]
    fn internal_assembler_relaxes_out_of_range_conditional_branch() {
        let mut lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, far_target".to_string(),
        ];
        for _ in 0..1500 {
            lines.push("addi t0, t0, 0".to_string());
        }
        lines.push("far_target:".to_string());
        lines.push("ret".to_string());

        let elf = assemble_elf_internal(&lines).expect("internal assembler should relax long conditional branches");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn internal_assembler_encodes_register_conditional_branches() {
        for mnemonic in ["beq", "bne", "blt", "bge", "bltu", "bgeu"] {
            let lines = vec![
                ".section .text".to_string(),
                ".global entry".to_string(),
                "entry:".to_string(),
                "li a0, 1".to_string(),
                "li a1, 1".to_string(),
                format!("{} a0, a1, target", mnemonic),
                "li a0, 2".to_string(),
                "target:".to_string(),
                "ret".to_string(),
            ];

            let elf = assemble_elf_internal(&lines).unwrap_or_else(|err| panic!("internal assembler should encode {mnemonic}: {err}"));
            assert!(elf.starts_with(b"\x7fELF"), "expected ELF output for {mnemonic}");
        }
    }

    #[test]
    fn internal_assembler_encodes_emitted_instruction_surface() {
        let lines = supported_instruction_surface_lines();

        let elf = assemble_elf_internal(&lines).expect("internal assembler should encode the emitted instruction surface");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn internal_assembler_rejects_intentionally_unsupported_mnemonics() {
        for (mnemonic, instruction) in INTENTIONALLY_UNSUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS {
            let lines = vec![
                ".section .text".to_string(),
                ".global entry".to_string(),
                "entry:".to_string(),
                (*instruction).to_string(),
                "target:".to_string(),
                "ret".to_string(),
            ];
            let err = match assemble_elf_internal(&lines) {
                Ok(_) => panic!("internal assembler unexpectedly accepted unsupported mnemonic {mnemonic}"),
                Err(err) => err,
            };
            assert!(
                err.message.contains("unsupported assembly instruction"),
                "unexpected error for unsupported mnemonic {mnemonic}: {err}"
            );
        }
    }

    #[test]
    fn generated_public_assembly_mnemonics_are_declared() {
        let surfaces = [
            ("stdlib", crate::stdlib::StdLib::generate_assembly()),
            ("collections", crate::stdlib::collections::Collections::generate_assembly()),
        ];
        let supported = SUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS.iter().map(|(mnemonic, _)| *mnemonic).collect::<BTreeSet<_>>();
        let mut undeclared = Vec::new();

        for (surface, assembly) in surfaces {
            for (line_number, mnemonic) in emitted_mnemonics(&assembly).into_iter() {
                if !supported.contains(mnemonic.as_str()) {
                    undeclared.push(format!("{surface}:{line_number}: {mnemonic}"));
                }
            }
        }

        assert!(
            undeclared.is_empty(),
            "generated public assembly used mnemonics outside the declared internal assembler surface:\n{}",
            undeclared.join("\n")
        );
    }

    #[test]
    fn bundled_example_codegen_mnemonics_are_declared() {
        let examples = ["amm_pool.cell", "launch.cell", "multisig.cell", "nft.cell", "timelock.cell", "token.cell", "vesting.cell"];
        let supported = SUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS.iter().map(|(mnemonic, _)| *mnemonic).collect::<BTreeSet<_>>();
        let mut undeclared = Vec::new();

        for example in examples {
            let path = camino::Utf8PathBuf::from(format!("{}/examples/{}", env!("CARGO_MANIFEST_DIR"), example));
            let result = crate::compile_file(
                path,
                crate::CompileOptions { target: Some("riscv64-asm".to_string()), ..crate::CompileOptions::default() },
            )
            .unwrap_or_else(|err| panic!("{example} should compile to assembly: {}", err.message));
            let assembly = std::str::from_utf8(&result.artifact_bytes)
                .unwrap_or_else(|err| panic!("{example} emitted invalid utf-8 assembly: {err}"));

            for (line_number, mnemonic) in emitted_mnemonics(assembly).into_iter() {
                if !supported.contains(mnemonic.as_str()) {
                    undeclared.push(format!("{example}:{line_number}: {mnemonic}"));
                }
            }
        }

        assert!(
            undeclared.is_empty(),
            "bundled examples used mnemonics outside the declared internal assembler surface:\n{}",
            undeclared.join("\n")
        );
    }

    #[test]
    fn binary_codegen_materializes_narrow_integer_constants() {
        let program = r#"
module codegen::narrow_constants

const MAX_COUNT: u8 = 10

action check_count(count: u64) -> u64
where
    assert(count <= MAX_COUNT as usize, "too many")
    return count
"#;
        let result = crate::compile(
            program,
            crate::CompileOptions { target: Some("riscv64-asm".to_string()), ..crate::CompileOptions::default() },
        )
        .expect("narrow integer constant comparison should compile");
        let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly should be utf-8");

        assert!(
            assembly.contains("li t1, 10\n    sgtu t0, t0, t1"),
            "binary comparison should materialize the u8 constant value instead of falling back to zero:\n{}",
            assembly
        );
    }

    #[test]
    fn narrow_arithmetic_codegen_truncates_to_declared_width() {
        let program = r#"
module codegen::narrow_wrap

action wrap8(x: u8) -> u8
where
    return x + 1

action wrap16(x: u16) -> u16
where
    return x + 1

action wrap32(x: u32) -> u32
where
    return x + 1
"#;
        let result = crate::compile(
            program,
            crate::CompileOptions { target: Some("riscv64-asm".to_string()), ..crate::CompileOptions::default() },
        )
        .expect("narrow arithmetic should compile");
        let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly should be utf-8");

        assert!(assembly.contains("andi t0, t0, 255"), "u8 arithmetic should mask to 8 bits:\n{}", assembly);
        assert!(assembly.contains("slli t0, t0, 48\n    srli t0, t0, 48"), "u16 arithmetic should truncate to 16 bits:\n{}", assembly);
        assert!(assembly.contains("slli t0, t0, 32\n    srli t0, t0, 32"), "u32 arithmetic should truncate to 32 bits:\n{}", assembly);
    }

    #[test]
    fn runtime_cast_codegen_checks_narrowing_and_bool_canonicality() {
        let program = r#"
module codegen::runtime_casts

action cast_u8(x: u64) -> u8
where
    return x as u8

action cast_bool(x: u64) -> bool
where
    return x as bool
"#;
        let result = crate::compile(
            program,
            crate::CompileOptions { target: Some("riscv64-asm".to_string()), ..crate::CompileOptions::default() },
        )
        .expect("runtime casts should compile");
        let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly should be utf-8");

        assert!(assembly.contains("srli t2, t0, 8"), "u64->u8 cast should check high bits:\n{}", assembly);
        assert!(assembly.contains("andi t0, t0, 255"), "u64->u8 cast should materialize a canonical u8:\n{}", assembly);
        assert!(assembly.contains("srli t2, t0, 1"), "numeric->bool cast should check canonical 0/1:\n{}", assembly);
    }

    #[test]
    fn division_codegen_guards_zero_divisors() {
        let program = r#"
module codegen::div_guard

action div(x: u64, y: u64) -> u64
where
    return x / y

action rem(x: u64, y: u64) -> u64
where
    return x % y
"#;
        let result = crate::compile(
            program,
            crate::CompileOptions { target: Some("riscv64-asm".to_string()), ..crate::CompileOptions::default() },
        )
        .expect("division should compile");
        let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly should be utf-8");

        assert!(assembly.contains("bnez t1, .L"), "division/rem should guard zero divisor:\n{}", assembly);
        assert!(assembly.contains("divu t0, t0, t1"), "division should use unsigned div:\n{}", assembly);
        assert!(assembly.contains("remu t0, t0, t1"), "modulo should use unsigned rem:\n{}", assembly);
    }

    #[test]
    fn u128_delta_arithmetic_codegen_uses_fixed_byte_storage() {
        let program = r#"
module codegen::u128_delta

struct Wide {
    value: u128
}

action add_delta(input: Wide, delta: u64) -> bool
where
    let next: u128 = input.value + delta
    return next == input.value

action sub_delta(input: Wide, delta: u64) -> bool
where
    let next: u128 = input.value - delta
    return next == input.value
"#;
        let result = crate::compile(
            program,
            crate::CompileOptions { target: Some("riscv64-asm".to_string()), ..crate::CompileOptions::default() },
        )
        .expect("u128 delta arithmetic should compile");
        let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly should be utf-8");

        assert!(assembly.contains("u128 Add with u64 delta"), "u128 + u64 lowering missing:\n{}", assembly);
        assert!(assembly.contains("u128 Sub with u64 delta"), "u128 - u64 lowering missing:\n{}", assembly);
        assert!(assembly.contains("sltu t2, t5, t0"), "u128 add carry check missing:\n{}", assembly);
        assert!(assembly.contains("sltu t2, t0, t1"), "u128 subtract borrow check missing:\n{}", assembly);
    }

    fn supported_instruction_surface_lines() -> Vec<String> {
        let mut lines = vec![".section .text".to_string(), ".global entry".to_string(), "entry:".to_string(), "li a1, 4".to_string()];
        for (mnemonic, instruction) in SUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS {
            if !matches!(*mnemonic, "ecall" | "ret") {
                lines.push((*instruction).to_string());
            }
        }
        lines.extend([
            "branch_target:".to_string(),
            "ecall".to_string(),
            "helper:".to_string(),
            "ret".to_string(),
            "done:".to_string(),
            "ret".to_string(),
            ".section .rodata".to_string(),
            "data_label:".to_string(),
            ".word 7".to_string(),
            ".byte 1".to_string(),
            ".ascii \"x\"".to_string(),
            ".align 3".to_string(),
        ]);
        lines
    }

    fn emitted_mnemonics(assembly: &str) -> Vec<(usize, String)> {
        assembly
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let clean = strip_comment(line)?;
                if clean.is_empty() || clean.starts_with('.') || clean.ends_with(':') {
                    return None;
                }
                let mnemonic = clean.split_whitespace().next()?.trim_end_matches(',');
                Some((index + 1, mnemonic.to_string()))
            })
            .collect()
    }

    #[test]
    fn internal_assembler_encodes_full_width_li_literals() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 9223372036854775808".to_string(),
            "li a1, 18446744073709551615".to_string(),
            "ret".to_string(),
        ];

        let elf = assemble_elf_internal(&lines).expect("internal assembler should encode u64-width li literals");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn rv64_li_boundary_values_materialize_correct_bits() {
        let cases = [(0x7fff_f7ffi128, 8usize), (0x7fff_f800i128, 60usize), (0x7fff_ffffi128, 60usize), (0x8000_0000i128, 60usize)];

        for (value, expected_size) in cases {
            let mut bytes = Vec::new();
            encode_li_sequence(&mut bytes, 10, value).expect("li should encode");
            assert_eq!(bytes.len(), expected_size, "unexpected li size for {value:#x}");
            assert_eq!(simulate_li_sequence(&bytes, 10), value as u64, "li materialized wrong bits for {value:#x}");
        }
    }

    fn simulate_li_sequence(bytes: &[u8], register: usize) -> u64 {
        let mut regs = [0u64; 32];
        for chunk in bytes.chunks_exact(4) {
            let inst = u32::from_le_bytes(chunk.try_into().expect("instruction chunk should be four bytes"));
            let opcode = inst & 0x7f;
            let rd = ((inst >> 7) & 0x1f) as usize;
            let funct3 = (inst >> 12) & 0x7;
            let rs1 = ((inst >> 15) & 0x1f) as usize;
            match (opcode, funct3) {
                (0x37, _) => {
                    regs[rd] = ((inst & 0xffff_f000) as i32 as i64) as u64;
                }
                (0x13, 0b000) => {
                    let imm = sign_extend(inst >> 20, 12);
                    regs[rd] = regs[rs1].wrapping_add(imm as u64);
                }
                (0x13, 0b001) => {
                    let shamt = (inst >> 20) & 0x3f;
                    regs[rd] = regs[rs1] << shamt;
                }
                _ => panic!("unexpected instruction in li sequence: 0x{inst:08x}"),
            }
            regs[0] = 0;
        }
        regs[register]
    }

    fn sign_extend(value: u32, bits: u32) -> i64 {
        let shift = 64 - bits;
        ((u64::from(value) << shift) as i64) >> shift
    }

    #[test]
    fn stack_pointer_offsets_are_emitted_through_helpers() {
        let implementation = include_str!("mod.rs").split("\n#[cfg(test)]").next().expect("source should contain implementation");
        let offenders = implementation
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let emits_stack_memory =
                    (line.contains("self.emit(format!(") || line.contains("self.emit(\"")) && line.contains("(sp)");
                let emits_stack_addi =
                    (line.contains("self.emit(\"addi ") || line.contains("self.emit(format!(\"addi ")) && line.contains(", sp,");
                let allowed_stack_memory = line.contains("self.emit(format!(\"{} {}, {}(sp)\", opcode, register, offset))");
                let allowed_outgoing_stack_memory = line.contains("self.emit(format!(\"sd {}, {}(sp)\", register, offset))");
                let allowed_stack_addi = line.contains("self.emit(format!(\"addi {}, sp, {}\", rd, offset))");
                ((emits_stack_memory && !allowed_stack_memory && !allowed_outgoing_stack_memory)
                    || (emits_stack_addi && !allowed_stack_addi))
                    .then(|| format!("{}: {}", index + 1, line.trim()))
            })
            .collect::<Vec<_>>();

        assert!(offenders.is_empty(), "stack pointer accesses must go through stack helpers:\n{}", offenders.join("\n"));
    }

    #[test]
    fn large_addi_avoids_clobbering_source_register() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.emit_large_addi("t0", "t6", 2048);
        generator.emit_large_addi("t6", "t6", 4096);

        assert_eq!(generator.assembly, vec!["    li t5, 2048", "    add t0, t6, t5", "    li t5, 4096", "    add t6, t6, t5",]);
    }

    #[test]
    fn sp_addi_large_offsets_clobber_only_destination_register() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.emit_sp_addi("t4", 4096);
        generator.emit_sp_addi("t6", 8192);

        assert_eq!(generator.assembly, vec!["    li t4, 4096", "    add t4, sp, t4", "    li t6, 8192", "    add t6, sp, t6",]);
    }

    #[test]
    fn state_transition_edges_use_explicit_consumed_binding() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.consume_order = vec![1, 2];
        generator.consume_type_names.insert(1, "Offer".to_string());
        generator.consume_type_names.insert(2, "Offer".to_string());
        generator.consume_binding_ids.insert("left".to_string(), 1);
        generator.consume_binding_ids.insert("right".to_string(), 2);

        let state_edge = IrStateTransitionEdge {
            input_binding: Some("right".to_string()),
            output_binding: None,
            type_name: "Offer".to_string(),
            field_name: "state".to_string(),
            from: "Live".to_string(),
            to: "Filled".to_string(),
            from_index: 1,
            to_index: 2,
        };

        assert_eq!(generator.consumed_var_for_state_transition("Offer", &[state_edge]), Some(2));
    }

    #[test]
    fn consumed_schema_params_use_loaded_cell_size_for_field_checks() -> crate::error::Result<()> {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        let binding = IrVar { id: 0, name: "auth".to_string(), ty: IrType::Named("MintAuthority".to_string()) };
        let params = vec![IrParam {
            name: "auth".to_string(),
            ty: binding.ty.clone(),
            is_mut: false,
            is_ref: false,
            is_read_ref: false,
            source: ParamSource::Default,
            binding: binding.clone(),
        }];
        let body = IrBody {
            consume_set: vec![CellPattern {
                operation: "input".to_string(),
                type_hash: None,
                binding: "auth".to_string(),
                fields: Vec::new(),
                destruction_policy: None,
            }],
            read_refs: Vec::new(),
            create_set: Vec::new(),
            mutate_set: Vec::new(),
            write_intents: Vec::new(),
            blocks: Vec::new(),
        };

        generator.prepare_function_layout(&body, &params)?;

        let loaded_size_offset =
            generator.cell_buffer_size_offsets.get(&binding.id).copied().expect("consumed input should have size slot");
        assert_eq!(generator.schema_pointer_size_offsets.get(&binding.id), Some(&loaded_size_offset));
        Ok(())
    }

    #[test]
    fn unaligned_scalar_load_large_offsets_preserve_live_accumulator() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.emit_unaligned_scalar_load("t4", "t6", "t2", 2048, 2);

        assert_eq!(
            generator.assembly,
            vec![
                "    li t6, 0",
                "    li t5, 2048",
                "    add t5, t4, t5",
                "    lbu t2, 0(t5)",
                "    or t6, t6, t2",
                "    li t5, 2049",
                "    add t5, t4, t5",
                "    lbu t2, 0(t5)",
                "    slli t2, t2, 8",
                "    or t6, t6, t2",
            ]
        );
    }

    #[test]
    fn generated_large_offsets_are_normalized_before_assembly() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.emit("sd t0, 2048(sp)");
        generator.emit("ld t6, 2056(sp)");
        generator.emit("lbu t2, 2048(t4)");
        generator.emit("addi t0, t4, 2048");
        generator.emit("sb t0, 4096(t6)");

        assert_eq!(
            generator.assembly,
            vec![
                "    li t6, 2048",
                "    add t6, sp, t6",
                "    sd t0, 0(t6)",
                "    li t5, 2056",
                "    add t5, sp, t5",
                "    ld t6, 0(t5)",
                "    li t6, 2048",
                "    add t6, t4, t6",
                "    lbu t2, 0(t6)",
                "    li t6, 2048",
                "    add t0, t4, t6",
                "    li t5, 4096",
                "    add t5, t6, t5",
                "    sb t0, 0(t5)",
            ]
        );
    }

    #[test]
    fn read_ref_runtime_fallback_records_cell_buffer_state() {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.frame_size = align_frame(RUNTIME_EXPR_TEMP_SIZE + RUNTIME_SCRATCH_SIZE + 16);
        let dest = IrVar { id: 42, name: "cfg".to_string(), ty: IrType::Named("Config".to_string()) };
        generator.read_ref_indices.insert(dest.id, 0);

        generator.emit_read_ref(&dest, "Config").expect("read_ref fallback should lower");

        let size_offset = generator.runtime_scratch_size_offset();
        let buffer_offset = generator.runtime_scratch_buffer_offset();
        assert_eq!(generator.schema_pointer_size_offsets.get(&dest.id), Some(&size_offset));
        assert_eq!(generator.cell_buffer_size_offsets.get(&dest.id), Some(&size_offset));
        assert_eq!(generator.cell_buffer_offsets.get(&dest.id), Some(&buffer_offset));
    }

    #[test]
    fn explicit_external_toolchain_paths_are_strict() {
        let err = validate_explicit_toolchain_path("CELLSCRIPT_RISCV_CC", PathBuf::from("riscv64-unknown-elf-gcc")).unwrap_err();
        assert!(err.message.contains("must be an absolute path"), "unexpected error: {}", err.message);

        let err = validate_explicit_toolchain_path("CELLSCRIPT_RISCV_CC", std::env::temp_dir()).unwrap_err();
        assert!(err.message.contains("must point to an executable file"), "unexpected error: {}", err.message);

        let current_exe = std::env::current_exe().expect("test executable path should be available");
        let validated =
            validate_explicit_toolchain_path("CELLSCRIPT_RISCV_CC", current_exe.clone()).expect("current test binary is executable");
        assert_eq!(validated, current_exe);
    }

    #[test]
    fn generated_stdlib_assembly_is_internal_assembler_clean() {
        let lines = crate::stdlib::StdLib::generate_assembly().lines().map(|line| line.to_string()).collect::<Vec<_>>();

        let elf = assemble_elf_internal(&lines).expect("generated stdlib assembly should assemble internally");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn generated_collection_assembly_is_internal_assembler_clean() {
        let lines =
            crate::stdlib::collections::Collections::generate_assembly().lines().map(|line| line.to_string()).collect::<Vec<_>>();

        let elf = assemble_elf_internal(&lines).expect("generated collection assembly should assemble internally");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn internal_assembler_rejects_unresolved_call_targets() {
        let lines = vec![".section .text".to_string(), ".global main".to_string(), "main:".to_string(), "call missing".to_string()];
        let err = assemble_elf_internal(&lines).unwrap_err();

        assert!(err.message.contains("unknown assembly label 'missing'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn internal_assembler_relaxes_out_of_range_register_conditional_branch() {
        for mnemonic in ["beq", "bne", "blt", "bge", "bltu", "bgeu"] {
            let mut lines = vec![
                ".section .text".to_string(),
                ".global entry".to_string(),
                "entry:".to_string(),
                "li a0, 0".to_string(),
                "li a1, 0".to_string(),
                format!("{} a0, a1, far_target", mnemonic),
            ];
            for _ in 0..1500 {
                lines.push("addi t0, t0, 0".to_string());
            }
            lines.push("far_target:".to_string());
            lines.push("ret".to_string());

            let plan = MachineLayoutPlan::build(&lines).unwrap_or_else(|err| panic!("machine layout should relax {mnemonic}: {err}"));
            assert_eq!(plan.metrics.relaxed_branch_count, 1, "expected one relaxed branch for {mnemonic}");
            let elf = assemble_elf_internal(&lines).unwrap_or_else(|err| panic!("internal assembler should relax {mnemonic}: {err}"));
            assert!(elf.starts_with(b"\x7fELF"), "expected ELF output for relaxed {mnemonic}");
        }
    }

    #[test]
    fn machine_layout_plan_reports_branch_relaxation_metrics() {
        let mut lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, far_target".to_string(),
        ];
        for _ in 0..1500 {
            lines.push("addi t0, t0, 0".to_string());
        }
        lines.push("far_target:".to_string());
        lines.push("ret".to_string());

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        assert_eq!(plan.metrics.relaxed_branch_count, 1);
        assert!(
            plan.metrics.max_cond_branch_abs_distance > 4096,
            "synthetic branch should exceed RV64 B-type range: {:?}",
            plan.metrics
        );
        assert_eq!(plan.metrics.text_size, plan.parsed.section_size(SectionKind::Text));
        assert_eq!(plan.metrics.covered_text_op_count, plan.metrics.executable_text_op_count);
        assert!(plan.metrics.executable_text_op_count > 1500, "synthetic text ops should be visible: {:?}", plan.metrics);
        assert_eq!(plan.metrics.layout_order_block_count, plan.metrics.machine_block_count);
        assert_eq!(plan.metrics.layout_order_text_size, plan.metrics.text_size);
        assert_eq!(plan.metrics.conditional_branch_block_count, 1);
        assert!(plan.metrics.machine_cfg_edge_count >= 2, "far branch CFG edges should be visible: {:?}", plan.metrics);
        assert_eq!(plan.metrics.machine_call_edge_count, 0);
        assert_eq!(plan.metrics.unreachable_machine_block_count, 0);
        assert!(plan.metrics.machine_block_count >= 2, "far branch should produce multiple machine blocks: {:?}", plan.metrics);
        assert!(
            plan.metrics.max_machine_block_size > 4096,
            "large fallthrough block should be visible in layout metrics: {:?}",
            plan.metrics
        );
    }

    #[test]
    fn machine_layout_plan_builds_explicit_machine_blocks() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, done".to_string(),
            "li a0, 1".to_string(),
            "j done".to_string(),
            "done:".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        let cfg = &plan.cfg;
        let blocks = &cfg.blocks;
        assert_eq!(blocks.len(), 3, "expected entry, fallthrough, and done blocks: {:?}", blocks);
        assert_eq!(blocks[0].label.as_deref(), Some("entry"));
        assert_eq!(blocks[0].terminator, MachineTerminator::ConditionalBranch { target: "done".to_string() });
        assert_eq!(blocks[1].terminator, MachineTerminator::Jump { target: "done".to_string() });
        assert_eq!(blocks[2].label.as_deref(), Some("done"));
        assert_eq!(blocks[2].terminator, MachineTerminator::Return);

        assert_eq!(cfg.blocks.len(), 3);
        assert_eq!(plan.order.block_order, vec![0, 1, 2]);
        assert_eq!(plan.order.placed_blocks.len(), 3);
        assert_eq!(
            plan.order.placed_blocks,
            vec![
                MachinePlacedBlock { block_index: 0, byte_start: 0, byte_size: cfg.blocks[0].byte_size },
                MachinePlacedBlock { block_index: 1, byte_start: cfg.blocks[0].byte_size, byte_size: cfg.blocks[1].byte_size },
                MachinePlacedBlock {
                    block_index: 2,
                    byte_start: cfg.blocks[0].byte_size + cfg.blocks[1].byte_size,
                    byte_size: cfg.blocks[2].byte_size
                },
            ]
        );
        assert_eq!(plan.order.text_size, plan.metrics.text_size);
        assert_eq!(plan.metrics.executable_text_op_count, 5);
        assert_eq!(plan.metrics.covered_text_op_count, 5);
        assert_eq!(plan.metrics.layout_order_block_count, 3);
        assert_eq!(
            cfg.edges,
            vec![
                MachineCfgEdge { from: 0, to: 2, kind: MachineCfgEdgeKind::ConditionalTaken },
                MachineCfgEdge { from: 0, to: 1, kind: MachineCfgEdgeKind::ConditionalFallthrough },
                MachineCfgEdge { from: 1, to: 2, kind: MachineCfgEdgeKind::Jump },
            ]
        );
        assert_eq!(unreachable_machine_block_count(&plan.parsed, cfg), 0);
    }

    #[test]
    fn machine_layout_plan_builds_register_conditional_branch_blocks() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "li a1, 0".to_string(),
            "bgeu a0, a1, done".to_string(),
            "li a0, 1".to_string(),
            "j done".to_string(),
            "done:".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        let cfg = &plan.cfg;
        assert_eq!(cfg.blocks.len(), 3, "expected entry, fallthrough, and done blocks: {:?}", cfg.blocks);
        assert_eq!(cfg.blocks[0].label.as_deref(), Some("entry"));
        assert_eq!(cfg.blocks[0].terminator, MachineTerminator::ConditionalBranch { target: "done".to_string() });
        assert_eq!(
            cfg.edges,
            vec![
                MachineCfgEdge { from: 0, to: 2, kind: MachineCfgEdgeKind::ConditionalTaken },
                MachineCfgEdge { from: 0, to: 1, kind: MachineCfgEdgeKind::ConditionalFallthrough },
                MachineCfgEdge { from: 1, to: 2, kind: MachineCfgEdgeKind::Jump },
            ]
        );
    }

    #[test]
    fn machine_cfg_tracks_call_edges_to_local_helpers() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "call local_helper".to_string(),
            "ret".to_string(),
            "local_helper:".to_string(),
            "li a0, 0".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        let cfg = &plan.cfg;
        assert_eq!(cfg.blocks.len(), 2, "expected entry and local helper blocks: {:?}", cfg.blocks);
        assert_eq!(cfg.blocks[0].label.as_deref(), Some("entry"));
        assert_eq!(cfg.blocks[1].label.as_deref(), Some("local_helper"));
        assert!(
            cfg.edges.contains(&MachineCfgEdge { from: 0, to: 1, kind: MachineCfgEdgeKind::Call }),
            "call edge to local helper should be explicit: {:?}",
            cfg.edges
        );
        assert_eq!(plan.metrics.machine_call_edge_count, 1);
        assert_eq!(unreachable_machine_block_count(&plan.parsed, cfg), 0);
    }

    #[test]
    fn machine_reachability_uses_entry_label_not_every_global() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "ret".to_string(),
            ".global unused_export".to_string(),
            "unused_export:".to_string(),
            "li a0, 1".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        assert_eq!(plan.parsed.entry_label.as_deref(), Some("entry"));
        assert_eq!(plan.cfg.blocks.len(), 2, "expected entry and unused export blocks: {:?}", plan.cfg.blocks);
        assert_eq!(plan.metrics.unreachable_machine_block_count, 1);
        assert_eq!(unreachable_machine_block_count(&plan.parsed, &plan.cfg), 1);
    }

    #[test]
    fn machine_layout_order_rejects_missing_duplicate_or_unknown_blocks() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, done".to_string(),
            "li a0, 1".to_string(),
            "j done".to_string(),
            "done:".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("machine layout plan");
        assert!(validate_machine_layout_order(&plan.cfg, &[0, 1]).is_err());
        assert!(validate_machine_layout_order(&plan.cfg, &[0, 1, 1]).is_err());
        assert!(validate_machine_layout_order(&plan.cfg, &[0, 1, 3]).is_err());
        let permuted = build_machine_layout_order(&plan.cfg, vec![2, 0, 1]).expect("permuted layout order should be valid");
        assert_eq!(permuted.block_order, vec![2, 0, 1]);
        assert_eq!(permuted.placed_blocks[0].block_index, 2);
        assert_eq!(permuted.placed_blocks[0].byte_start, 0);
        assert_eq!(permuted.placed_blocks[1].byte_start, plan.cfg.blocks[2].byte_size);
        assert_eq!(permuted.text_size, plan.order.text_size);
    }

    #[test]
    fn machine_layout_plan_rejects_branch_target_outside_text() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, data_label".to_string(),
            "ret".to_string(),
            ".section .rodata".to_string(),
            "data_label:".to_string(),
            ".word 1".to_string(),
        ];

        let err = MachineLayoutPlan::build(&lines).expect_err("branch targets outside text blocks should be rejected");
        assert!(err.message.contains("does not start a machine block"), "unexpected error for invalid CFG target: {}", err.message);
    }

    #[test]
    fn generated_functions_use_shared_epilogue_tail() {
        let ir = IrModule {
            name: "shape_test".to_string(),
            items: vec![IrItem::Action(IrAction {
                name: "shape".to_string(),
                params: vec![],
                return_type: Some(IrType::U64),
                state_transition_edges: vec![],
                effect_class: EffectClass::Pure,
                scheduler_hints: SchedulerHints::default(),
                body: IrBody {
                    consume_set: vec![],
                    read_refs: vec![],
                    create_set: vec![],
                    mutate_set: vec![],
                    write_intents: vec![],
                    blocks: vec![IrBlock {
                        id: BlockId(0),
                        instructions: vec![],
                        terminator: IrTerminator::Return(Some(IrOperand::Const(IrConst::U64(7)))),
                    }],
                },
            })],
            external_type_defs: vec![],
            external_callable_abis: vec![],
            enum_fixed_sizes: HashMap::new(),
        };
        let assembly = CodeGenerator::new(CodegenOptions::default()).generate(&ir, ArtifactFormat::RiscvAssembly).unwrap();
        let assembly = String::from_utf8(assembly).unwrap();
        let shape_start = assembly.find("shape:\n").expect("shape function label");
        let runtime_start =
            assembly[shape_start..].find(".section .text").map(|offset| shape_start + offset).unwrap_or(assembly.len());
        let shape_assembly = &assembly[shape_start..runtime_start];

        assert!(shape_assembly.contains("j .Lshape_epilogue"), "return sites should jump to the shared epilogue:\n{}", shape_assembly);
        assert_eq!(
            shape_assembly.matches(".Lshape_epilogue:").count(),
            1,
            "a function should emit one shared epilogue label:\n{}",
            shape_assembly
        );
        assert_eq!(
            shape_assembly.matches("ret").count(),
            1,
            "a function should emit one physical return in its shared epilogue:\n{}",
            shape_assembly
        );
    }
}
