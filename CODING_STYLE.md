# CellScript Coding Style

This document is the tracked project standard for compiler, backend, docs, and
release work. Local notes may exist in `*.local.md`, but they are not part of the
project contract.

## General Rust Rules

- Keep changes scoped to the compiler phase being modified.
- Prefer existing AST, IR, metadata, and codegen structures over parallel
  stringly typed paths.
- Parser support alone is not a feature boundary. New syntax must agree across
  parsing, formatting, type checking, lowering, metadata, LSP/editor behavior,
  examples, docs, and tests.
- Use enums and typed fields when the concept already has a structured
  representation.
- Error messages should name the rejected boundary and the next valid action.
- Run `./scripts/cellscript_gate.sh dev` before committing routine compiler or
  documentation changes.
- Run `./scripts/cellscript_gate.sh ci` before merge-readiness claims.
- Run `./scripts/cellscript_gate.sh backend` for IR, codegen, assembler, ABI,
  ELF, or RISC-V changes.
- Focused commands such as `cargo check --locked -p cellscript --all-targets`,
  `cargo test --locked -p cellscript`, clippy with `-D warnings`, and
  `git diff --check` remain useful while debugging, but passing one component
  does not replace the matching gate.
- Keep new lint allowances narrow. Prefer item-level `#[allow(...)]` with a
  short reason; crate-wide or module-wide clippy allowances are only for
  documented legacy or transition boundaries.

## Backend And Codegen Rules

`src/codegen/mod.rs` is the orchestration layer of a multi-file backend.
Sub-modules handle separate concerns: `cell_ops.rs` (cell operation lowering
and verification), `schema.rs` (layout data model and type-width helpers),
`frame.rs` (frame layout, stack access primitives, and parameter spilling),
`calls.rs` (call emission and outgoing argument handling), `expr.rs` (scalar
expression helper emission), `assembler.rs` (RISC-V machine code and ELF),
`runtime.rs` (helper functions and CKB syscall wrappers), `abi.rs` (calling
convention and entry witness envelope), and `collections.rs` (collection
lowering). New code should respect these boundaries and must not make the
implicit backend contracts more implicit.

- Treat emitted assembly as a compiler contract. Any new mnemonic or pseudo-op
  emitted by codegen, stdlib, or collection helpers must be supported by the
  internal assembler in the same change.
- Updating the assembler surface means updating `Instruction`,
  `parse_instruction`, `encode_instruction`, instruction sizing, CFG/terminator
  handling when relevant, and regression tests for generated assembly.
- Keep the internal assembler aligned to the CellScript-emitted surface, not to
  the full GNU assembler surface. Do not add broad RISC-V support unless codegen
  emits it or a public generated-assembly path needs it.
- Tier 1 is a release-blocking closure requirement: every mnemonic emitted by
  main codegen, generated stdlib assembly, generated collection assembly, or
  internal lowering helpers must be accepted and correctly encoded by the
  internal assembler.
- The current Tier 1 real instruction forms are `add`, `addi`, `sub`, `and`,
  `andi`, `or`, `xor`, `mul`, `div`, `divu`, `rem`, `remu`, `slt`, `sltu`,
  `xori`, `ld`, `lbu`, `sb`, `sh`, `sw`, `sd`, `slli`, `srli`, `beq`, `bne`,
  `blt`, `bge`, `bltu`, `bgeu`, `ret`, and `ecall`.
- Treat pseudo-instructions and aliases as explicit API. `li`, `la`, `call`,
  `j`, `mv`, `seqz`, `snez`, `neg`, `sgt`, `sgtu`, `bgt`, `bgez`, `beqz`, and
  `bnez` are supported because current generated surfaces use them.
- Tier 2 candidates may be added when an optimizer, typed emission path, or
  constant materializer needs them: `nop`, `lui`, `auipc`, raw `jal`/`jalr`,
  `ori`, `sll`, `srl`, `sra`, `srai`, `addw`, `addiw`, and `subw`.
- Tier 3 instructions remain demand-driven: signed byte/half/word loads,
  unsigned half/word loads, `slti`, `sltiu`, branch aliases such as `ble`,
  `bleu`, `bgtu`, `bltz`, `bgtz`, `blez`, plus `not` and `jr`.
- Do not add CSR operations, atomics, floating-point instructions, compressed
  instructions, `fence`, `tail`, or the full GNU pseudo-instruction surface
  unless a concrete CellScript backend contract requires them.
- Do not hand-write stack offsets. All stack access must go through
  `emit_stack_load`, `emit_stack_load_byte`, `emit_stack_store`, or
  `emit_stack_store_byte`.
- Outgoing call-stack ABI arguments are the exception to the local-frame helper
  rule: stage them through the dedicated outgoing stack-argument helpers before
  adjusting `sp`, so caller-local buffers such as entry witness payloads are not
  overwritten.
- Do not hand-write large pointer arithmetic. Use `emit_large_addi` or a helper
  that takes an explicit live-register avoid set.
- Do not rely on blind textual normalization when structured codegen knows
  register liveness. Large memory accesses inside helpers should use a typed
  helper that avoids destination, source, base, and live accumulator registers.
- Keep register liveness local and visible. If a helper needs scratch registers,
  document the live registers through arguments or an avoid set rather than
  assuming `t6` is free.
- Constants that need an address must use concrete `.rodata` labels. Do not emit
  references to placeholder labels that are not materialized.
- Fixed-byte values wider than 8 bytes must use fixed-byte storage and byte
  comparison/copy helpers. Do not silently pass them through the 64-bit scalar
  stack slot model.
- Unsupported runtime semantics must fail closed with a specific
  `CellScriptRuntimeError`; do not emit a clean success path for unsupported DSL.
- Do not add domain-specific verifier rules by matching action/function names in
  codegen. Business rules must be explicit in DSL source, structured IR, or
  metadata before the backend lowers them.

## CKB Semantics

- Use CKB terms precisely: input Cell, output Cell, lock script, type script,
  script args, WitnessArgs, lock group, CellDep, `since`, capacity, and
  transaction validation.
- `protected T` is a typed view of one selected input Cell guarded by the current
  lock invocation. It is not a global scan or an output Cell.
- Witness data is not authority unless cryptographically verified.
- Compile-only evidence is weaker than builder-backed acceptance evidence. Keep
  production claims tied to valid and invalid lock-spend evidence, cycle
  measurement, transaction size, occupied capacity, and under-capacity checks.

## Documentation And Release Notes

- Do not describe a feature as implemented unless parser, type checking,
  lowering, metadata, LSP/editor behavior, tests, examples, and docs agree on
  the same boundary.
- Use "reserved", "deferred", or "fail-closed" when syntax exists but executable
  semantics are intentionally unavailable.
- Release notes should separate highlights, scope boundaries, validation
  commands, and links to detailed docs.
- Do not keep roadmap promises in `docs/`. Release notes may describe what
  shipped; future plans belong in dedicated roadmap/proposal files.

## Tests

- For syntax changes, add parser, formatter, type-checker, lowering, metadata,
  and LSP/editor tests where applicable.
- For CKB-facing changes, add negative tests for unsafe or ambiguous forms.
- For assembler/codegen changes, add targeted tests for the exact generated
  instruction surface and at least one compile-through `riscv64-elf` path.
- Prefer focused tests during development, then broaden validation before
  completion.

### Backend Refactor: Behaviour-Preserving Emitter Extraction

When extracting `&mut self` emitter methods from `codegen/mod.rs` into a
sub-module (e.g. `assembler.rs`, `runtime.rs`, `abi.rs`):

1. **Use exact source movement.** Extract the original code verbatim with
   `git show` or equivalent. Never manually reconstruct emitter logic from
   memory. A single wrong register, label, or branch in a reconstructed
   method will silently change generated assembly and break on-chain contracts.

2. **Verify generated assembly is unchanged.** Run the full test suite after
   each extraction. The codegen tests include end-to-end assembly assertions
   that catch transcription errors.

3. **Prefer `pub(crate)` temporarily.** Cross-module `impl` blocks on the same
   struct need method visibility to match call sites. Use `pub(crate)` for
   methods called from other modules within the crate. Fields of types shared
   across module boundaries also need `pub(crate)`.

4. **Delete from back to front.** When removing code by line number with `sed`,
   delete later ranges first to keep earlier line numbers stable.

5. **Brace-count after every deletion.** Use `python3 -c` to verify brace
   balance before attempting compilation. Off-by-one `sed` ranges can leave
   orphaned lines or eat closing braces.

### Module Boundary: Schema vs Cell Operations vs Orchestration

The codegen backend is split across three ownership layers. Code must land in
the layer that matches its semantic responsibility, not merely the layer that
happens to call it.

**`schema.rs`** — layout computation and field access helpers. It must **not**
absorb cell operation policy or state-transition verification. Specifically:

- **Schema module may contain**: type-width helpers (`fixed_scalar_width`,
  `fixed_byte_width`, `type_static_length`, etc.), aggregate/tuple layout
  computation, Molecule table field bounds/span helpers, fixed-byte comparison
  and loading, prelude u64 value resolution, and field access dispatch.
- **Schema module must not contain**: destruction policy, identity/field
  uniqueness checks, create-output field verification, state-transition edge
  matching, consume/destroy/replace/transfer/settle lowering, mutate
  replacement transition checks, or any code that decides *whether* a cell
  operation is valid.

**`cell_ops.rs`** — cell operation lowering and verification. Owns all code
that decides whether a cell operation is valid or emits verification assembly:

- **Cell ops module may contain**: consume, create, create_unique,
  replace_unique, transfer, claim, settle, destroy lowering; identity and
  destruction policy helpers; mutate replacement verification (preserved
  fields, transition checks, dynamic table checks); create-output field
  verification; state-transition checks; uniqueness verification; and layout
  queries that are specific to mutation or output verification.
- **Cell ops module must not contain**: general type-width computation that is
  not specific to cell operation verification, collection lowering, ABI
  marshalling, runtime helper emission, or instruction dispatch.

**`mod.rs`** — orchestration and dispatch. Owns the `CodeGenerator` struct,
`generate()` entry point, action/lock/pure-function generation, instruction
dispatch (`generate_instruction`, `generate_body`), field access, type hash
emission, parameter analysis, syscall loaders, and shared helpers used by
multiple sub-modules.

**`frame.rs`** — frame layout, stack access primitives, and parameter spilling.
Owns all code related to stack frame construction and access:

- **Frame module may contain**: prologue/epilogue emission, stack load/store
  helpers (`emit_stack_load`, `emit_stack_store`, etc.), `emit_sp_addi`,
  `emit_large_addi`, function layout preparation (`prepare_function_layout`),
  variable recording (`record_instruction_var`, `record_operand`, etc.),
  runtime scratch/expr-temp offset computation, ABI parameter spilling
  (`emit_param_spills`, `emit_spill_abi_arg`), and data-arg staging helpers.
- **Frame module must not contain**: instruction lowering, type-width
  computation, cell operation policy, collection lowering, or any code that
  decides what to emit beyond frame management.

**`calls.rs`** — call emission and outgoing argument handling. Owns all code
related to emitting function calls and marshalling call arguments:

- **Calls module may contain**: direct/internal call emission (`emit_call`),
  CKB fixed-hash helper dispatch (`emit_ckb_fixed_hash_call`), ABI argument
  placement helpers (`emit_call_param_arg`, `emit_call_scalar_arg`,
  `emit_call_pointer_arg`, `emit_call_length_arg`,
  `emit_call_type_hash_pointer_arg`, `emit_call_type_hash_length_arg`),
  outgoing stack argument area management (`emit_outgoing_call_stack_arg_store`),
  signed SP-relative store (`emit_sp_store_signed`), and ABI register
  resolution (`call_abi_register`).
- **Calls module must not contain**: ABI entry wrapper logic (owned by
  `abi.rs`), frame layout or stack access primitives (owned by `frame.rs`),
  expression lowering as a whole, cell operations, schema/layout computation,
  collection lowering, or runtime helper emission.

**`expr.rs`** — scalar expression helper emission. Owns constant/variable
loading, truncation, bounds checking, boolean canonicalisation, division
guards, binary/unary/move/cast/tuple emission, and operand-to-register and
operand-comment utilities.

- **Expr module may contain**: `emit_load_const`, `emit_load_var`,
  `emit_store_var`, `emit_truncate_register_to_type`,
  `emit_truncate_register_to_width`, `emit_checked_scalar_fits`,
  `emit_bool_canonical_check`, `emit_divisor_nonzero_guard`,
  `emit_binary`, `emit_dynamic_byte_comparison`, `emit_unary`,
  `emit_move`, `emit_cast`, `emit_tuple`, `emit_operand_to_register`,
  `emit_operand_comment`.
- **Expr module must not contain**: instruction dispatch, field access,
  type hash emission, prelude analysis, syscall loaders, cell operations,
  call emission, frame management, or runtime helper emission.

Cross-module call dependencies are acceptable; semantic ownership boundaries
are not. If a helper is shared across ownership layers, it stays in `mod.rs`
or the most general sub-module that needs it.
