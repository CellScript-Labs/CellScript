# CellScript Codegen Backend Refactor Audit

## Motivation

`src/codegen/mod.rs` was a monolithic backend file containing the RISC-V64
code generator, internal assembler, runtime helper emission, calling
convention logic, collection lowering, schema layout helpers, and cell
operation verification — all in a single file that exceeded 13,000 lines.

The goal of this refactor was **behaviour-preserving decomposition** into
reviewable, auditable subsystems. No semantic changes were intended. Every
extraction was verified against the same test suite and assembly snapshots
that existed before the refactor began.

## Extracted Modules

### assembler.rs (1,903 lines)

**Responsibility:** RISC-V machine code assembler and ELF binary emitter.

Parses textual assembly produced by the code generator, builds machine-level
control-flow graphs, performs branch relaxation, and emits position-independent
ELF binaries suitable for CKB-VM execution. Also contains external toolchain
support (GCC/LD) when available.

**What was moved:** Instruction parsing, register encoding, immediate handling,
machine block construction, CFG reachability analysis, branch relaxation,
ELF header/program header emission, and external toolchain invocation paths.

**What must not be moved here:** Code generation logic, type-layout
computation, cell operation verification, runtime helper emission. The
assembler is a pure consumer of textual assembly; it does not decide what
assembly to emit.

**Why the boundary matters:** The assembler is the last line of defence
before bytes hit the CKB-VM. Incorrect branch relaxation or encoding
produces silently wrong on-chain behaviour. Keeping the assembler isolated
means its encoding surface can be audited independently of lowering logic.

### runtime.rs (540 lines)

**Responsibility:** Runtime support function emission.

Generates built-in helper functions (`memcmp`, `memzero`, size guards),
Blake2b-256 hash integration, CKB syscall wrappers (header field, input
field, cell data loading, witness loading, script loading), and v0.14
surface helpers.

**What was moved:** `generate_runtime_support` and all helper functions it
emits: `__cellscript_require_min_size`, `__cellscript_memcmp_fixed`,
`__cellscript_memzero`, `__ckb_source_*` wrappers, `__ckb_hash_blake2b`,
`__ckb_sighash_all`, `__ckb_witness_lock`, `__ckb_witness_type`,
`__ckb_load_cell_by_field`, and v0.14 spawn/predicate helpers.

**What must not be moved here:** Code generation orchestration, cell
operation verification, schema layout computation, collection lowering.
Runtime helpers are callable subroutines; the decision to call them belongs
to the lowering layer.

**Why the boundary matters:** Runtime helpers have a stable calling
convention that generated code depends on. Changing a helper's register
usage or stack layout without updating all call sites will break on-chain
contracts. Isolating the helpers makes the calling convention contract
visible.

### abi.rs (755 lines)

**Responsibility:** Calling convention and entry witness envelope.

Contains the `CallableAbi` registry, entry witness frame layout constants,
witness wrapper emission, ABI parameter marshalling helpers, and parameter-
counting functions that bridge IR parameters to RISC-V64 calling conventions.

**What was moved:** `CallableAbi` struct and all methods, witness wrapper
emission (`emit_entry_witness_wrapper`), ABI argument labelling, parameter
ABI classification, entry witness frame layout constants, and outgoing
stack-argument staging.

**What must not be moved here:** Instruction-level code generation,
runtime helper bodies, cell operation verification, schema layout.
The ABI layer translates between IR parameter semantics and RISC-V64
registers/stack; it does not generate computation.

**Why the boundary matters:** The entry witness wrapper is the first code
that runs when a CKB script is invoked. Its frame layout, magic validation,
and argument deserialization are a security boundary. Incorrect changes here
can make all scripts uncallable or, worse, accept malformed witness data.

### collections.rs (1,195 lines)

**Responsibility:** Collection lowering.

Emits stack-allocated and dynamic Molecule vector operations: index, length,
capacity, new, push, pop, insert, remove, set, swap, reverse, truncate,
extend, clear, contains, and fixed-aggregate/dynamic index access.

**What was moved:** All `emit_*` methods whose IR instruction name starts
with `Collection*` or `Index`/`Length` and operates on collection types,
plus the fixed-aggregate index and dynamic Molecule vector index accessors.

**What must not be moved here:** Schema layout computation, cell operation
verification, runtime helper bodies, ABI marshalling. Collection lowering
uses schema layout data but does not define it.

**Why the boundary matters:** Collection operations affect cell data layout
and Molecule vector encoding. Incorrect collection lowering corrupts
on-chain data. The boundary keeps the Molecule encoding logic auditable.

### schema.rs (1,845 lines)

**Responsibility:** Schema layout data model and type-width helpers.

Owns the core data types (`SchemaFieldLayout`, `SchemaFieldValueSource`,
`ExpectedFixedByteSource`, `SourcePointer`, `AggregatePointerSource`) and
all fixed-width / static-length / aggregate-layout computation. Also
contains Molecule table field bounds/span helpers, fixed-byte comparison
and loading, prelude u64 value resolution, and field access dispatch.

**What was moved:**
- 5 data types and their field definitions
- 21 free functions for width/length/layout computation
- 5 impl methods for type-width queries and static length
- 44 existing impl methods for schema field access, Molecule table helpers,
  fixed-byte comparison/loading, prelude value resolution, and field
  access dispatch

**What must not be moved here:** Destruction policy, identity/field
uniqueness checks, create-output field verification, state-transition edge
matching, consume/destroy/replace/transfer/settle lowering, mutate
replacement transition checks, or any code that decides *whether* a cell
operation is valid. Schema interprets data shape; cell_ops judges legality.

**Why the boundary matters:** Schema helpers are called from nearly every
other module. If schema absorbs verification logic, the distinction between
"what does the data look like" and "is this state transition legal" becomes
invisible, making future audits and ProofPlan integration significantly
harder.

### cell_ops.rs (2,260 lines)

**Responsibility:** Cell operation lowering and verification.

Contains consume, create, create_unique, replace_unique, transfer, claim,
settle, and destroy lowering; identity and destruction policy helpers;
mutate replacement verification (preserved fields, transition checks,
dynamic table checks, append checks, u128 transition checks); create-output
field verification; state-transition checks; uniqueness verification; and
layout queries specific to mutation or output verification.

**What was moved:**
- 4 free functions (`identity_policy_label`, `destruction_policy_label`,
  `destroy_policy_uses_output_absence_scan`, `consumed_operand_var`)
- 39 impl methods for destroy/verify/mutate, output verification, and
  emit cell operations (consume, create, create_unique, replace_unique,
  transfer, claim, settle, destroy)

**What must not be moved here:** General type-width computation that is not
specific to cell operation verification, collection lowering, ABI
marshalling, runtime helper emission, instruction dispatch, expression
lowering.

**Why the boundary matters:** Cell operation verification is the core
security property of CellScript. Create-output checks, state-transition
checks, and identity preservation are what prevent invalid state changes
on-chain. Keeping this code isolated means every verifier change is
reviewable against the DSL-level semantics without wading through
unrelated lowering code.

### frame.rs (724 lines)

**Responsibility:** Frame layout, stack access primitives, and parameter
spilling.

Owns prologue/epilogue emission, stack load/store helpers (with
large-offset fallback), function layout preparation (slot allocation for
locals, cell buffers, collection regions, scratch areas), variable
recording, runtime scratch/expr-temp offset computation, ABI parameter
spilling, and data-arg staging helpers.

**What was moved:**
- 19 `pub(crate)` methods: prologue/epilogue (`emit_prologue`,
  `emit_epilogue`, `emit_shared_epilogue`, `emit_epilogue_body`),
  stack access (`emit_stack_load`, `emit_stack_load_byte`,
  `emit_stack_store`, `emit_stack_store_byte`, `emit_stack_access`,
  `emit_sp_addi`, `emit_large_addi`), layout (`prepare_function_layout`,
  `runtime_scratch_size_offset`, `runtime_scratch_buffer_offset`,
  `runtime_scratch2_size_offset`, `runtime_scratch2_buffer_offset`,
  `runtime_expr_temp_offset`), and parameter spilling (`emit_param_spills`,
  `emit_store_data_args_at`)
- 7 private methods: `emit_spill_abi_arg`, `emit_stack_access`,
  `record_instruction_var`, `record_instruction_fixed_byte_local`,
  `record_terminator_var`, `record_operand`, `record_var`,
  `emit_store_const_bytes_to_stack`

**What must not be moved here:** Instruction lowering, type-width
computation, cell operation policy, call emission, collection lowering,
or any code that decides what to emit beyond frame management.

**Why the boundary matters:** Stack layout errors corrupt register saves,
cell buffer pointers, and collection regions. A single off-by-eight in
`prepare_function_layout` can make every runtime scratch access read wrong
data. Keeping frame layout isolated makes the slot-allocation contract
auditable independently of what consumes the slots.

### calls.rs (359 lines)

**Responsibility:** Call emission and outgoing argument handling.

Contains direct/internal call emission, CKB fixed-hash helper dispatch,
ABI argument placement for calls (scalar, pointer, length, type_hash),
outgoing stack argument area management, signed SP-relative store, and
ABI register name resolution.

**What was moved:**
- 2 `pub(crate)` methods: `emit_call` (main call emission, called from
  mod.rs instruction dispatch), `emit_sp_store_signed` (signed
  SP-relative store, called from abi.rs witness wrapper)
- 9 private methods: `emit_ckb_fixed_hash_call`, `emit_call_param_arg`,
  `emit_call_scalar_arg`, `emit_call_pointer_arg`,
  `emit_call_length_arg`, `emit_call_type_hash_pointer_arg`,
  `emit_call_type_hash_length_arg`, `emit_outgoing_call_stack_arg_store`,
  `call_abi_register`

**What must not be moved here:** ABI entry wrapper logic (owned by
`abi.rs`), frame layout or stack access primitives (owned by
`frame.rs`), expression lowering as a whole, cell operations, schema/layout
computation, collection lowering, or runtime helper emission.

**Why the boundary matters:** Call emission bridges between IR operand
semantics and the RISC-V64 calling convention. Incorrect argument
marshalling — swapping pointer/length pairs, misplacing stack arguments,
or miscomputing the outgoing stack area size — produces silently wrong
behaviour that only manifests at runtime. Isolating call emission makes
the ABI contract between caller and callee auditable.

### expr.rs (373 lines)

**Responsibility:** Scalar expression helper emission.

Contains constant and variable loading, truncation, bounds checking,
boolean canonicalisation, division guards, binary arithmetic/comparison
emission, dynamic byte comparison, unary emission, move/cast/tuple
emission, and operand-to-register/comment utilities.

**What was moved:**
- 12 `pub(crate)` methods: constant/variable loading (`emit_load_const`,
  `emit_load_var`, `emit_store_var`), expression lowering (`emit_binary`,
  `emit_unary`, `emit_move`, `emit_cast`, `emit_tuple`), validation
  helpers (`emit_bool_canonical_check`, `emit_divisor_nonzero_guard`),
  operand utilities (`emit_operand_to_register`, `emit_operand_comment`)
- 4 private methods: `emit_truncate_register_to_type`,
  `emit_truncate_register_to_width`, `emit_checked_scalar_fits`,
  `emit_dynamic_byte_comparison`

**What must not be moved here:** Instruction dispatch, field access,
type hash emission, prelude analysis, syscall loaders, cell operations,
schema layout computation, collection lowering, call emission, frame
management, or runtime helper emission.

**Why the boundary matters:** Expression helpers are shared across
multiple lowering paths (instruction dispatch, schema field access, cell
ops, collections, calls). Keeping them in one place makes the scalar
value model auditable and prevents divergent truncation or
canonicalisation logic from creeping into individual lowering modules.

## Remaining mod.rs Responsibilities (3,268 lines)

`mod.rs` retains:
- `CodeGenerator` struct definition and `generate()` entry point
- `generate_action`, `generate_lock`, `generate_pure_fn` — action/lock/pure
  function orchestration
- `generate_body`, `generate_instruction`, `generate_terminator` — instruction
  dispatch
- `generate_consume`, `generate_create`, `generate_mutate_replacement`,
  `generate_read_ref`, `emit_mutate_parameter_binding` — cell operation
  dispatch (delegates to cell_ops)
- Field access and type hash: `emit_field_access`,
  `dynamic_length_from_size_offset`, `emit_type_hash`,
  `emit_runtime_type_hash`
- Parameter binding and prelude analysis: `set_schema_pointer_params`,
  `set_consumed_schema_pointers`, `set_pointer_aliases`,
  `set_schema_field_value_sources`, `set_verified_operation_outputs`,
  `set_constructed_byte_vectors`, prelude query methods
- Syscall loading helpers: `emit_load_cell_data_syscall`,
  `emit_load_cell_by_field_syscall`, `emit_load_witness_syscall`, etc.
- General utilities: `emit_fail`, `emit_runtime_error_comment`,
  `operand_cell_location`, `fresh_label`, `const_data_label_for_bytes`,
  `emit_memory_load_with_avoid`, `emit_unaligned_scalar_load`,
  `emit_large_immediate_access_if_needed`

## Refactor Discipline

Every extraction followed the same process:

1. **Exact source movement.** Code was copied verbatim from the current
   clean baseline. No manual rewriting of emitter bodies. A single wrong
   register, label, or branch in a reconstructed method can silently change
   generated assembly and break on-chain contracts.

2. **Compile/test after each step.** `cargo check`, `cargo test`, and
   assembly snapshot tests were run after every extraction. The codegen
   tests include end-to-end assembly assertions that catch transcription
   errors.

3. **Preserve generated assembly.** If assembly snapshot tests failed,
   the diff was investigated. Snapshots were never updated to paper over
   an extraction error.

4. **Prefer smaller extraction over larger.** Each extraction moved one
   coherent subsystem at a time, with a consolidation pass between
   extractions for documentation and visibility tightening.

5. **Use `pub(crate)` temporarily.** Cross-module `impl` blocks on the
   same struct need method visibility to match call sites. After extraction,
   visibility was tightened back to private for methods only used within
   the defining module.

6. **Delete from back to front.** When removing code by line number,
   later ranges were deleted first to keep earlier line numbers stable.

7. **Brace-count after every deletion.** `python3 -c` was used to verify
   brace balance before attempting compilation.

### The ABI Extraction Lesson

During the `abi.rs` extraction, the entry witness wrapper was initially
reconstructed from prose description rather than copied verbatim. This
produced failing tests because the reconstructed code had subtle differences
in register allocation and error-path structure.

Replacing the reconstruction with exact code from `git show` restored
correctness. The conclusion: **emitter extraction must be treated as code
movement, not prose translation.** Even when the prose is accurate, the
gap between "describes the same logic" and "produces the same bytes" can
be one branch direction or one register swap.

This lesson is codified in `CODING_STYLE.md` under "Backend Refactor:
Behaviour-Preserving Emitter Extraction."

## Assembly Snapshot Guard

`tests/assembly_snapshots.rs` contains five snapshot-style tests that assert
specific structural properties of generated RISC-V assembly. These are
behaviour-preserving stability guards, not exhaustive correctness proofs.

### Covered cases

| Test | Source | What it guards |
|---|---|---|
| `snapshot_simple_action_assembly` | Minimal action returning a constant | Entry point shape, frame size, shared epilogue, tail-call dispatch |
| `snapshot_lock_args_assembly` | Lock with `lock_args` Hash and witness comparison | Witness wrapper, `lock_args` parameter decoding, `LOAD_SCRIPT` syscall, fixed-byte memcmp, shared epilogue with single `ret` |
| `snapshot_blake2b_helper_assembly` | v0.14 Blake2b hash comparison | Blake2b runtime helper invocation, fixed-byte result comparison |
| `snapshot_witness_schema_syscall_assembly` | v0.14 witness source with schema loading | `LOAD_CELL_DATA` syscall for schema params, runtime helper calls (`__ckb_source_group_input`, `__ckb_witness_lock`, `__ckb_sighash_all`), error path structure |
| `snapshot_assemblies_contain_no_leaked_overflow_diagnostics` | All four sources above | No assembler overflow diagnostics leaked into generated output (guards against regressions in large-immediate handling) |

### Why this matters

During backend refactoring, it is easy to accidentally change label numbering,
frame layout, or instruction selection in ways that pass type checks and
unit tests but produce different machine code. The snapshot tests catch
these drifts by asserting exact mnemonic sequences, register choices, and
ABI comment strings that are known to be correct.

If a snapshot legitimately changes during a future refactor:
1. Verify the new assembly is semantically equivalent.
2. Update the expected string and explain the change in the commit message.
3. Do not disable the test or widen assertions beyond recognition.

## Boundary Model

```
mod.rs         Orchestration: action/lock/pure generation, instruction
               dispatch, field access, type hash, parameter binding,
               prelude analysis, syscall loaders, general utilities.
               Owns the CodeGenerator struct.

cell_ops.rs    Cell operation verification: consume, create, create_unique,
               replace_unique, transfer, claim, settle, destroy lowering;
               identity/destruction policy; mutate replacement verification;
               create-output field verification; state-transition checks;
               uniqueness verification.

schema.rs      Data shape interpretation: SchemaFieldLayout, type-width
               computation, Molecule table field bounds/span, fixed-byte
               comparison/loading, prelude u64 value resolution, field
               access dispatch.

frame.rs       Frame layout: prologue/epilogue, stack load/store primitives,
               function layout preparation (slot allocation), variable
               recording, runtime scratch/expr-temp offsets, parameter
               spilling.

calls.rs       Call emission: direct/internal call emission, CKB fixed-hash
               dispatch, ABI argument placement (scalar, pointer, length,
               type_hash), outgoing stack argument area management.

expr.rs        Expression helpers: constant/variable loading, truncation,
               bounds checking, boolean canonicalisation, division guards,
               binary/unary/move/cast/tuple emission, operand utilities.

assembler.rs   Machine assembly: instruction parsing, register encoding,
               branch relaxation, CFG construction, ELF emission.

runtime.rs     Emitted helper routines: memcmp, memzero, size guards,
               Blake2b-256, CKB syscall wrappers, v0.14 surface helpers.

abi.rs         Entry/calling convention: CallableAbi registry, entry
               witness envelope, argument marshalling, lock/script args.

collections.rs Collection lowering: Molecule vector index/length/capacity,
               push/pop/insert/remove/set/swap/reverse/truncate/extend/clear,
               fixed-aggregate and dynamic index access.
```

Cross-module call dependencies are acceptable; semantic ownership boundaries
are not. If a helper is shared across ownership layers, it stays in `mod.rs`
or the most general sub-module that needs it.

## Commit History

| Commit | Description |
|---|---|
| `6be119e` | Split assembler backend into separate module |
| `1a8fc73` | Extract runtime support emission into runtime.rs |
| `a67f44f` | Extract ABI/calling-convention code into abi.rs |
| `4c54b5e` | Tighten runtime visibility, add backend refactor rules |
| `b612653` | Add assembly snapshot stability guard |
| `da59d95` | Extract collection lowering into collections.rs |
| `2861ef7` | Extract schema/field-access helpers into schema.rs |
| `3cefa86` | Update docs and boundary rules for schema extraction |

## Future Work

- **Cell ops stabilisation before further splitting.** `cell_ops.rs`
  should not be split further until ProofPlan and strict-mode semantics
  stabilise. Premature splitting would create cross-module churn as the
  verification model evolves.

- **ProofPlan / strict-mode alignment.** New verification features should
  respect the schema vs cell_ops boundary: schema tells you what the data
  looks like; cell_ops tells you whether the state transition is legal.
  New ProofPlan rules belong in cell_ops.rs or a dedicated verifier module,
  not in schema.rs.

- **Prelude analysis extraction.** The prelude analysis methods
  (`set_schema_pointer_params`, `set_schema_field_value_sources`,
  `set_pointer_aliases`, etc.) form a coherent analysis phase that could
  be extracted into a separate module once the calling conventions between
  analysis and emission stabilise.

## Closure

The backend decomposition phase is closed. The monolithic 13,000+ line
`mod.rs` has been decomposed into eleven files across ten sub-modules,
with mod.rs reduced to 3,268 lines of orchestration, dispatch, and
shared glue. Every extraction was verified against 686 passing tests and
five assembly snapshot guards with zero semantic changes.

Future refactors should be **feature-driven or bug-driven, not
speculative splitting.** New modules should only appear when a new
concern (e.g. ProofPlan, strict-mode verification, or a new target
architecture) demands a home that does not fit the existing boundaries.
