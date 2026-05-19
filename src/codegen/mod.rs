//! CellScript RISC-V64 code generator.
//!
//! This module lowers CellScript IR to RISC-V assembly and ELF binaries for
//! the CKB-VM. The code generator is organised into eleven files:
//!
//! - **`mod.rs`** (this file): orchestration layer. Contains the
//!   `CodeGenerator` struct, `generate()` entry point, and all IR-to-assembly
//!   lowering for type definitions, actions, locks, pure functions, and
//!   control flow.
//!
//! - **`cell_ops.rs`**: cell operation lowering and verification. Contains
//!   consume, create, create_unique, replace_unique, transfer, claim, settle,
//!   and destroy lowering, identity/destruction policy helpers, mutate
//!   replacement verification (preserved fields, transition checks, dynamic
//!   table checks), create-output field verification, state-transition checks,
//!   and uniqueness verification.
//!
//! - **`schema.rs`**: schema layout data model and type-width helpers. Owns
//!   `SchemaFieldLayout`, `ExpectedFixedByteSource`, and all fixed-width /
//!   static-length / aggregate-layout computation. Also contains Molecule
//!   table helpers, fixed-byte comparison/loading, prelude u64 value
//!   resolution, and field access dispatch.
//!
//! - **`frame.rs`**: frame layout, stack access primitives, and parameter
//!   spilling. Contains prologue/epilogue emission, stack load/store helpers,
//!   function layout preparation (slot allocation), variable recording, runtime
//!   scratch/expr-temp offset computation, and ABI parameter spilling.
//!
//! - **`calls.rs`**: call emission and outgoing argument handling. Contains
//!   direct/internal call emission, CKB fixed-hash helper dispatch, ABI
//!   argument placement (scalar, pointer, length, type_hash), outgoing stack
//!   argument area management, and signed SP-relative store.
//!
//! - **`expr.rs`**: scalar expression helper emission. Contains constant and
//!   variable loading, truncation, bounds checking, boolean canonicalisation,
//!   division guards, binary arithmetic/comparison emission, dynamic byte
//!   comparison, unary emission, move/cast/tuple emission, and
//!   operand-to-register/comment utilities.
//!
//! - **`assembler.rs`**: RISC-V machine code assembler and ELF emitter.
//!   Parses textual assembly, builds machine CFGs, performs branch relaxation,
//!   and emits position-independent ELF binaries. Also contains external
//!   toolchain support (GCC/LD) when available.
//!
//! - **`runtime.rs`**: runtime support emission. Generates built-in helper
//!   functions (`memcmp`, `memzero`, size guards), Blake2b-256 hash, CKB
//!   syscall wrappers (header field, input field), and v0.14 surface helpers.
//!
//! - **`abi.rs`**: calling convention and entry witness envelope. Contains the
//!   `CallableAbi` registry, entry witness frame layout, parameter marshalling,
//!   and the witness wrapper that validates and deserialises ABI arguments
//!   before tail-calling the user action/lock function.
//!
//! - **`collections.rs`**: collection lowering. Emits stack-allocated and
//!   dynamic Molecule vector operations (index, length, capacity, new, push,
//!   pop, insert, remove, set, swap, reverse, truncate, extend, clear,
//!   contains) and fixed aggregate / dynamic index access.

use crate::ast::{BinaryOp, ParamSource, UnaryOp};
use crate::codegen::cell_ops::consumed_operand_var;
use crate::error::{CompileError, Result};

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
pub(crate) const CKB_SOURCE_INPUT: u64 = 0x01;
pub(crate) const CKB_SOURCE_OUTPUT: u64 = 0x02;
const CKB_SOURCE_CELL_DEP: u64 = 0x03;
const CKB_SOURCE_HEADER_DEP: u64 = 0x04;
const CKB_SOURCE_GROUP_FLAG: u64 = 0x0100_0000_0000_0000;
const CKB_SOURCE_GROUP_INPUT: u64 = CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT;
pub(crate) const CKB_CELL_FIELD_CAPACITY: u64 = 0;
pub(crate) const CKB_CELL_FIELD_LOCK_HASH: u64 = 3;
pub(crate) const CKB_CELL_FIELD_TYPE_HASH: u64 = 5;
const CKB_INDEX_OUT_OF_BOUND: u64 = 1;
const CKB_ITEM_MISSING: u64 = 2;
pub(crate) const RUNTIME_SCRATCH_BUFFER_SIZE: usize = 512;
const RUNTIME_SCRATCH_SLOT_SIZE: usize = 8 + RUNTIME_SCRATCH_BUFFER_SIZE;
const RUNTIME_SCRATCH_SIZE: usize = RUNTIME_SCRATCH_SLOT_SIZE * 2;
pub(crate) const RUNTIME_EXPR_TEMP_SLOTS: usize = 16;
const RUNTIME_EXPR_TEMP_SIZE: usize = RUNTIME_EXPR_TEMP_SLOTS * 8;
const RUNTIME_CELL_BUFFER_SIZE: usize = 512;
const RUNTIME_CELL_SLOT_SIZE: usize = 8 + RUNTIME_CELL_BUFFER_SIZE;
const RUNTIME_COLLECTION_BUFFER_SIZE: usize = 256;

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

#[derive(Debug, Clone)]
pub(crate) enum PreludeU64OperandSource {
    Const(u64),
    ParamVar(usize),
    StackVar(usize),
    Field(SchemaFieldValueSource),
    Expr(Box<PreludeU64ValueSource>),
}

#[derive(Debug, Clone)]
pub(crate) enum PreludeU64ValueSource {
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
                    if let IrType::Tuple(items) = &v.ty {
                        if items.len() > 8 {
                            return Err(CompileError::new(
                                format!("tuple return ABI supports at most 8 fields, but return value has {} fields", items.len()),
                                crate::error::Span::default(),
                            ));
                        }
                        if !self.tuple_aggregate_fields.contains_key(&v.id) {
                            return Err(CompileError::new(
                                "tuple return ABI requires a directly materialized tuple aggregate",
                                crate::error::Span::default(),
                            ));
                        }
                    }
                    if let Some(fields) = self.tuple_aggregate_fields.get(&v.id).cloned() {
                        if fields.len() > 8 {
                            return Err(CompileError::new(
                                format!("tuple return ABI supports at most 8 fields, but return value has {} fields", fields.len()),
                                crate::error::Span::default(),
                            ));
                        }
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

    fn emit_runtime_error_comment(&mut self, error: CellScriptRuntimeError) {
        self.emit(format!("# cellscript runtime error {} {}", error.code(), error.name()));
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

    /// Generic field access: when specialized paths don't match, try to compute the
    /// field offset from type_layouts and emit an unaligned load from the pointer
    /// stored in the object's stack slot. This works for any named-type variable
    /// whose type has a registered layout, even if it wasn't classified as a
    /// schema_pointer_var or aggregate_pointer_source.
    pub(crate) fn dynamic_length_from_size_offset(&self, operand: &IrOperand) -> Option<usize> {
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
    pub(crate) fn emit_type_hash(&mut self, dest: &IrVar, operand: &IrOperand) -> Result<()> {
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
    pub(crate) fn emit_runtime_type_hash(&mut self, dest: &IrVar, operand: &IrOperand) -> bool {
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

    fn emit_read_ref(&mut self, dest: &IrVar, ty: &str) -> Result<()> {
        if self.cell_buffer_offsets.contains_key(&dest.id) {
            self.emit(format!("# read_ref {} (preloaded from CellDep)", ty));
            return Ok(());
        }

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

        self.schema_pointer_size_offsets.insert(dest.id, size_offset);
        self.cell_buffer_size_offsets.insert(dest.id, size_offset);
        self.cell_buffer_offsets.insert(dest.id, buffer_offset);

        Ok(())
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

mod abi;
mod assembler;
mod calls;
mod cell_ops;
mod collections;
mod expr;
mod frame;
mod runtime;
mod schema;

#[allow(unused_imports)]
pub(crate) use abi::{
    abi_arg_label, call_abi_arg_count, call_param_abi_arg_count, entry_abi_arg_count, entry_witness_payload_layout, CallLengthKind,
    CallableAbi, ENTRY_SCRIPT_ARGS_CURSOR_OFFSET, ENTRY_SCRIPT_ARGS_LEN_OFFSET, ENTRY_SCRIPT_ARGS_START_OFFSET,
    ENTRY_SCRIPT_BUFFER_OFFSET, ENTRY_SCRIPT_BUFFER_SIZE, ENTRY_SCRIPT_SIZE_OFFSET, ENTRY_WITNESS_BUFFER_OFFSET,
    ENTRY_WITNESS_BUFFER_SIZE, ENTRY_WITNESS_FRAME_SIZE, ENTRY_WITNESS_HEADER_SIZE, ENTRY_WITNESS_LABEL, ENTRY_WITNESS_MAGIC,
    ENTRY_WITNESS_RA_OFFSET, ENTRY_WITNESS_SIZE_OFFSET,
};
pub use assembler::BackendShapeMetrics;
pub(crate) use runtime::{runtime_syscall_abi, RuntimeSyscallAbi};
// Note: referenced_v014_runtime_helpers and is_v014_runtime_helper are private to runtime.rs
#[allow(unused_imports)] // Many items are used only by the #[cfg(test)] module below
pub(crate) use assembler::{
    align_frame, align_up, assemble_elf, assemble_elf_internal, build_machine_layout_order, ckb_source_name, encode_ecall,
    encode_i_type, encode_li_sequence, encode_s_type, is_min_call, is_runtime_header_u64_call, memory_operand_offset_and_base,
    parse_immediate, parse_register, scratch_register_avoiding, signed_bits_fit, small_signed_immediate, strip_comment,
    unreachable_machine_block_count, validate_explicit_toolchain_path, validate_machine_layout_order, AsmOp, BackendLayoutMetrics,
    BranchSizeMode, Instruction, MachineBlock, MachineBlockCoverage, MachineCfg, MachineCfgEdge, MachineCfgEdgeKind,
    MachineLayoutOrder, MachineLayoutPlan, MachinePlacedBlock, MachineTerminator, ParsedAssembly, SectionKind, SectionLayout,
    SymbolDef, TextOpLayout, CKB_SCRIPT_STACK_TOP, ELF_HEADER_SIZE, ELF_PROGRAM_HEADER_SIZE,
};
#[allow(unused_imports)]
pub(crate) use schema::{
    aggregate_field_layout, aggregate_type_label, const_ir_type, const_usize_operand, constructed_byte_vector_part_width,
    fixed_aggregate_pointer_param_width, fixed_byte_const_bytes, fixed_byte_pointer_param_width, fixed_byte_width,
    fixed_register_width, fixed_scalar_const_value, fixed_scalar_operand_width, fixed_scalar_width, layout_fixed_byte_width,
    layout_fixed_scalar_width, molecule_vector_element_fixed_width, named_type_name, operand_fixed_byte_width,
    tuple_return_field_type, type_static_length, AggregatePointerSource, ExpectedFixedByteSource, SchemaFieldLayout,
    SchemaFieldValueSource,
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
    fn outgoing_stack_arg_area_is_16_byte_aligned_at_call_boundaries() {
        let cases = [
            (0, 0),
            (1, 0),
            (8, 0),
            (9, 16),
            (10, 16),
            (11, 32),
            (12, 32),
            (13, 48),
            (16, 64),
            (17, 80),
            (18, 80),
            (19, 96),
            (20, 96),
        ];

        for (abi_arg_count, expected_bytes) in cases {
            let bytes = super::abi::outgoing_stack_arg_bytes(abi_arg_count);
            assert_eq!(bytes, expected_bytes, "unexpected outgoing stack size for {} ABI args", abi_arg_count);
            if bytes > 0 {
                assert_eq!(bytes % 16, 0, "outgoing stack area must preserve RISC-V psABI sp alignment");
            }
        }
    }

    #[test]
    fn internal_assembler_keeps_near_unconditional_jump_compact() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "j done".to_string(),
            "done:".to_string(),
            "ret".to_string(),
        ];

        let plan = MachineLayoutPlan::build(&lines).expect("near unconditional jump should assemble");
        assert_eq!(plan.metrics.text_size, 8, "near j should remain a compact 4-byte jal x0 sequence");
        assert_eq!(plan.metrics.relaxed_branch_count, 0, "near j should not be marked as a relaxed long jump");
    }

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
    fn internal_assembler_encodes_far_unconditional_jump() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "j far_target".to_string(),
            format!(".ascii \"{}\"", "x".repeat(1_100_000)),
            "far_target:".to_string(),
            "ret".to_string(),
        ];

        let elf = assemble_elf_internal(&lines).expect("internal assembler should encode far unconditional jumps");
        assert!(elf.starts_with(b"\x7fELF"));
    }

    #[test]
    fn internal_assembler_relaxes_far_conditional_branch_with_long_jump() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "beqz a0, far_target".to_string(),
            format!(".ascii \"{}\"", "x".repeat(1_100_000)),
            "far_target:".to_string(),
            "ret".to_string(),
        ];

        let elf = assemble_elf_internal(&lines).expect("internal assembler should relax far conditional branches through long jumps");
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
        let modules: &[(&str, &str)] = &[
            ("mod.rs", include_str!("mod.rs")),
            ("frame.rs", include_str!("frame.rs")),
            ("abi.rs", include_str!("abi.rs")),
            ("calls.rs", include_str!("calls.rs")),
            ("cell_ops.rs", include_str!("cell_ops.rs")),
            ("collections.rs", include_str!("collections.rs")),
            ("schema.rs", include_str!("schema.rs")),
            ("expr.rs", include_str!("expr.rs")),
            ("runtime.rs", include_str!("runtime.rs")),
        ];
        let mut offenders = Vec::new();
        for (file_name, source) in modules {
            let implementation = source.split("\n#[cfg(test)]").next().expect("source should contain implementation");
            for (index, line) in implementation.lines().enumerate() {
                let emits_stack_memory =
                    (line.contains("self.emit(format!(") || line.contains("self.emit(\"")) && line.contains("(sp)");
                let emits_stack_addi =
                    (line.contains("self.emit(\"addi ") || line.contains("self.emit(format!(\"addi ")) && line.contains(", sp,");
                let allowed_stack_memory = line.contains("self.emit(format!(\"{} {}, {}(sp)\", opcode, register, offset))");
                let allowed_outgoing_stack_memory = line.contains("self.emit(format!(\"sd {}, {}(sp)\", register, offset))");
                let allowed_stack_addi = line.contains("self.emit(format!(\"addi {}, sp, {}\", rd, offset))");
                if (emits_stack_memory && !allowed_stack_memory && !allowed_outgoing_stack_memory)
                    || (emits_stack_addi && !allowed_stack_addi)
                {
                    offenders.push(format!("{}:{}: {}", file_name, index + 1, line.trim()));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "stack pointer accesses must go through centralized helpers in frame.rs or calls.rs:\n{}",
            offenders.join("\n")
        );
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
