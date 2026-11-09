# CellScript Coding Style

This document is the tracked project standard for compiler, backend, docs, and
release work. Local notes may exist in `*.local.md`, but they are not part of the
project contract.

## General Rust Rules

- Specialization labels are encoder output and lookup keys, not semantic
  evidence. Retain owner/template/type arguments in the AST provenance index
  and IR contract. Use the declaration's explicit origin to distinguish source
  and generated items; missing provenance must reject. Backend storage choices
  come from retained IR contracts, never `__mono__` spelling. Metadata readers
  validate labels by encoding separately supplied arguments, not decoding them.
  Source rendering must retain the declaration's own parameter scope; a bound
  parameter shadows entries in the concrete specialization index.

- Keep changes scoped to the compiler phase being modified.
- Prefer existing AST, IR, metadata, and codegen structures over parallel
  stringly typed paths.
- Parser support alone is not a feature boundary. New syntax must agree across
  parsing, formatting, type checking, lowering, metadata, LSP/editor behavior,
  examples, docs, and tests.
- Use enums and typed fields when the concept already has a structured
  representation.
- The authoring grammar in `docs/CELLSCRIPT_AUTHORING_GRAMMAR.md` owns the
  current source spelling. Keep explicit discard statements distinct from
  tail values through AST transforms, checking, IR lowering and simulation;
  newlines are not statement boundaries. Do not keep a legacy grammar mode.
- Source rewrites consume the shared parser's lossless document, preserve
  comments/token grouping and validate exhaustive semantic AST structure and
  idempotence before writing. Debug strings, effects summaries and artifact
  hashes do not establish source equivalence. AST display output is a separate
  API and must not be used to reconstruct an existing source file.
- Current type syntax uses `Type::source_syntax`. Established packed/hash V0
  domains retain their existing type-spelling renderer. A syntax cleanup does
  not authorize silently changing a cryptographic domain or wire schema.
- Transaction view schema arguments remain AST type nodes and admitted IR
  `ContextView` roles. Type checking, lowering, effects and codegen must not
  recover schemas or source authority from `InputView<...>`, `OutputView<...>`
  or `CellDataView<...>` text. Metadata/editor spelling readers remain explicit
  text boundaries; the independent checker retains its wire validation.
  Internal context handles preserve their existing two-word helper ABI, without
  granting Cell lifecycle ownership or an escaping view ABI.
- Nested source views retain type/unsigned-constant arguments and distinguish
  byte elements from nominal record symbols before resolving their declarations.
  Shape aliases expand to type nodes. Parsing checks numeric limits; semantic
  element admission follows generic expansion. Checking and IR lowering must
  not render a nested application into a nominal name for later parsing.
  External metadata/editor readers keep their bounded wire grammar; checker
  alias compatibility requires independently validated declaration bindings.
- Bounded Cell sets and output lists retain their element type and cardinality
  as AST arguments and an admitted IR family. Consumers must not reconstruct a
  nominal application or infer group authority from its rendered spelling.
  Source classification, linear ownership and the finite executable runtime
  profile remain separate checks. Preserve metadata/helper ABI contracts at
  output boundaries; the independent checker keeps its own wire reader.
- Committed values retain unary AST arguments and distinct Commitment/Opening
  IR nodes. Digest extents are fixed at 32 bytes; explicit opening witnesses
  remain one-shot spans. Derive widths and result types from those nodes.
  Rendering the existing packed-hash type spelling is a wire-domain operation,
  never permission to reparse it into semantic evidence. Independent machine
  admission must bind authentication to the actual materialized source/extent,
  including incoming branches, copy destinations and scalar byte assembly.
- Error messages should name the rejected boundary and the next valid action.
- Run `./scripts/cellscript_gate.sh commit` before ordinary commits and pushes,
  plus focused checks for changed behavior. Documentation-only changes need no
  VM execution, reproducible ELF build or full compiler test suite.
- Use `./scripts/cellscript_gate.sh dev` at integration checkpoints, not before
  every commit. A checkpoint failure does not prevent an unrelated documentation
  commit whose own checks pass; report the failure without claiming integration
  readiness.
- Run `./scripts/cellscript_gate.sh ci` before merge-readiness claims.
- Run `./scripts/cellscript_gate.sh backend` before claiming integration readiness
  for IR, codegen, assembler, ABI, ELF, or RISC-V changes. Intermediate commits
  use `commit` and focused regressions for the changed contract.
- Run `./scripts/cellscript_gate.sh release` for publication and production
  acceptance. A passing commit/dev gate never substitutes for release evidence.
- Focused commands such as `cargo check --locked -p cellscript --all-targets`,
  `cargo test --locked -p cellscript`, clippy with `-D warnings`, and
  `git diff --check` remain useful while debugging, but passing one component
  does not replace the matching gate.
- Run each Rust suite once within a gate. CI/backend must not repeat focused
  subsets after the complete compiler suite. Keep additional syntax audits,
  independent checker mutations, feature profiles and real-node acceptance
  separate. Dev's focused backend runner must name the owning test binaries
  and reject zero executed tests; a successful Cargo filter is not feature
  coverage. Invalidate generated evidence before its producer suite and validate
  the resulting report afterward.
- Keep new lint allowances narrow. Prefer item-level `#[allow(...)]` with a
  short reason; crate-wide or module-wide clippy allowances are only for
  documented legacy or transition boundaries.

## CellScript Example Organization

- Organize `examples/language/` by semantic purpose: `core`, `ckb`,
  `ownership`, `verification`, `collections`, or `batches`.
- A `.cell` filename must describe behavior and must not contain a release or
  version marker. The native source-policy gate checks tracked and untracked
  CellScript sources and rejects forms such as `v0_26_batch.cell`,
  `contract-v1.cell`, and `token_0.26.0.cell`.
- Keep release history in `CHANGELOG.md` and release notes. Do not encode it in
  source paths, module names, fixture identities, or user-facing example names.
- When moving an example, update its module declaration, documentation links,
  embedded fixtures, website references, and production-evidence inventory in
  the same change.

## On-Chain Registry Script Rules

`contracts/registry-type-script` is an independent `no_std` CKB Script crate.
Its release binary is part of the Registry trust boundary, not a host utility.

- Build only with the pinned repository toolchain and
  `build_reproducible_release.sh`; the script path-remaps sources, strips the
  RISC-V ELF, and verifies both SHA-256 and CKB data hash against the tracked
  release manifest.
- Keep host checksum tooling portable: reproducible scripts may use GNU
  `sha256sum` or Perl `shasum`, must select one explicitly, and must fail closed
  when neither exists.
- Keep Script args equal to the 32-byte custody Lock Script hash and the
  accepted Cell data exactly `CSREGv1 || 32-byte commitment hash`. Every group
  Cell must use that Lock and every transition must consume a Cell using it;
  otherwise an unauthorised creator could impersonate an official commitment.
  Format changes require a new protocol prefix and migration plan, not a
  permissive parser.
- Run the `ckb-testtool` suite for every Script change. Positive creation,
  replacement, and destruction plus unauthorised creation, incorrect custody
  Locks, malformed input/output, and non-canonical args are mandatory evidence.
- Production deployment requires a live mainnet code Cell, the standard
  custody Lock CellDep, sufficient confirmations, and a committed deployment
  manifest. Local CKB-VM tests are not mainnet deployment evidence.

## IR Ownership And Consistency

The [IR contract](docs/CELLSCRIPT_IR_CONTRACT.md) defines phase ownership and
the limits of compiler structural validation. The executable representation is
a typed CFG with mutable locals and retained declaration evidence. Do not assume
universal SSA, single assignment or dominance validation from a local ID.

- Source resolution and checking own declaration identity, source types,
  ownership and contextual return rules. AST optimization must preserve lexical
  scope, contextual types, effects and observable failure before rechecking.
- Shared AST-to-IR conversion owns structured type conversion. Field, external
  declaration and specialization paths must not introduce partial parallel
  conversions that silently omit admitted families.
- Retained AST validity predicates are declaration templates and audit evidence.
  Executable uses must bind and lower their checks; rendered text is not an
  execution or proof authority.
- Keep encoded widths, machine argument transport, dynamic schema codecs and
  physical storage plans distinct. They answer different questions; matching
  widths or word counts do not establish type identity or storage safety.
- Type declarations and imported type aliases must respect the exact builtin
  representation names selected by `is_reserved_builtin_type_name`. Do not
  expand this into a substring rule or reserve ordinary function labels.
- Executable type admission includes the common CFG/local structural verifier,
  including directly supplied IR entering codegen. Mutable backing aliases and
  empty-vector element refinement retain their explicit contracts. This does
  not replace instruction typing, borrow checking, dominance or independent
  machine verification.
- Compiler consumers select runtime semantics through `IrCallTarget` and
  `RuntimeHelper`. Wire readers first distinguish local and runtime contracts
  before applying specialized helper rules. A source label cannot grant a
  runtime ProofPlan or ABI.
- Simulator expression statements discard values except at an explicit
  supported tail position. Loops consume the configured step budget and must
  report exhaustion; never return apparent success after silent truncation or
  swallow runtime/budget errors while evaluating fields.
- Persistent caches contain versioned build products, not executable internal
  IR snapshots. Include wire schema versions in cache identity and intentionally
  update the cache generation for incompatible same-schema compiler behavior.
  Successful validation of an old consistent artifact cannot substitute for
  invalidation of a changed compilation policy.

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
  `andi`, `or`, `xor`, `mul`, `mulhu`, `div`, `divu`, `rem`, `remu`, `slt`, `sltu`,
  `xori`, `ld`, `lbu`, `sb`, `sh`, `sw`, `sd`, `sll`, `srl`, `sra`, `slli`,
  `srli`, `srai`, `rori`, `roriw`, `beq`, `bne`, `blt`, `bge`, `bltu`, `bgeu`, `ret`, and
  `ecall`.
- Treat pseudo-instructions and aliases as explicit API. `li`, `la`, `call`,
  `j`, `mv`, `seqz`, `snez`, `neg`, `sgt`, `bgt`, `bgez`, `beqz`, and
  `bnez` are supported. Retain `sgt` as an admitted alias even when current main
  lowering chooses a reversed `slt`; `sgtu` is not currently admitted.
- Keep the producer's operand-count and smoke-fixture inventory together in
  `assembler_instruction_forms!`. Generated main/runtime/stdlib/collection
  surfaces must match that inventory. The independent checker retains its own
  instruction decoding and effects; the producer table is not proof authority.
- Parse generated assembly once and freeze final instruction expansion,
  padding, labels and offsets before emitting bytes and machine evidence.
  Text alignment includes the fixed entry prefix and uses whole four-byte NOP
  fill; a label before alignment denotes that padding, while a label after it
  denotes the aligned operation. Read-only data padding is zero-filled.
  Revalidate short branches after alignment and promote them until stable;
  never assume every branch distance shrinks when other code shrinks.
- Tier 2 candidates may be added when an optimizer, typed emission path, or
  constant materializer needs them: `nop`, `lui`, `auipc`, raw `jal`/`jalr`,
  `ori`, `addw`, `addiw`, and `subw`.
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
- Memory operations and add-immediates must enter `emission.rs` with typed
  operation, register and integer operands. Do not format an instruction and
  recover these operands from its text. Loads and address operations may reuse
  an overwritten destination; stores and in-place address updates need the
  structured emitter's live-register contract.
  Scratch exhaustion must fail compilation, including register aliases; it must
  never fall back to a register in the avoid set.
- Constants that need an address must use concrete `.rodata` labels. Do not emit
  references to placeholder labels that are not materialized.
- Fixed-byte values wider than 8 bytes must use fixed-byte storage and byte
  comparison/copy helpers. Do not silently pass them through the 64-bit scalar
  stack slot model.
- Unsupported runtime semantics must fail closed with a specific
  `CellScriptRuntimeError`; do not emit a clean success path for unsupported DSL.
- Retain `IrCallTarget` through lowering, rewriting, inference and code generation.
  A source function label never acquires a runtime ABI/effect/failure contract
  from its spelling. Only selected intrinsics construct `Runtime` targets;
  unknown internal contracts remain explicit compilation failures. Normalize
  import/function labels only on `Function` targets.
- Dispatch runtime callers exhaustively over `RuntimeHelper`. One selected
  emitter owns its operand/result ABI; malformed operands must not fall through
  to generic argument emission or another emitter. Machine-body verification
  remains independent of the description and caller dispatcher.
- Diagnostic/metadata/assembly symbol rendering is an explicit text boundary.
  Call targets must not implement implicit string dereference or label equality
  that erases function/runtime provenance.
- Do not add domain-specific verifier rules by matching action/function names in
  codegen. Business rules must be explicit in DSL source, structured IR, or
  metadata before the backend lowers them.

## Verified Artifact Boundary Rules

- Treat the ELF, compile metadata, canonical lowering record, and canonical
  source map as one build bundle. A change to any identity, schema, mapping, or
  structural claim must update all producers, consumers, tests, docs, and gate
  checks in the same change.
- Keep `cellscript-artifact-checker` independent of the parser, resolver, type
  checker, IR, optimizer, assembler, and code generator. Production
  dependencies may provide only bounded parsing, versioned schema, canonical
  hashing, stable diagnostics, and minimal ELF utilities.
- Shared compiler/checker code may define closed wire vocabulary, canonical
  syntax and immutable constants. Keep source/resolver interpretation, fact
  reconstruction traversals, machine evidence and admission decisions independent.
  A shared descriptor is a trusted description, not proof that a helper body
  satisfies it. Document this trust boundary and test hostile machine mutations.
- WitnessBytesView syntax belongs to the checker-owned bounded grammar and
  shared conformance vectors. Callers select explicit whitespace syntax and
  independently enforce source/extent/ownership rules. Do not introduce private
  copies of the parser in compiler or checker consumers; source policy enforces
  this boundary. Source rendering and wire/identity rendering have different
  contracts and must not be merged by spelling alone.
- Extract validation modules by preserved responsibility and dependencies.
  Pure moves preserve function bodies and validation order; keep semantic changes
  in a separate reviewed step. File length is not a module ownership rule.
- Checker traversal must be preceded by byte/count budgets. Unknown schemas or
  fields, malformed ranges, path escape, mismatched identities, and budget
  exhaustion fail closed with one stable `V24xx` rejection code and bounded
  diagnostics.
- Do not label structural validation semantic equivalence. Keep binding,
  structural, lowering-record, CKB-VM, and chain evidence as separate fields.
- Any new checker invariant requires a deterministic negative mutation and a
  valid compiler-produced fixture. ELF/codegen changes also require the
  `backend` gate because mapped ranges, block digests, control flow, stack
  discipline, or instruction policy may change.

## Executable Package Scenario Rules

- `cellc test` success must name and run `simulator`, `ckb-vm`, or `all` unless
  `--no-run` is explicitly selected. Compile-only discovery is never described
  as executed test evidence.
- Scenario and report schemas reject unknown fields. Source/oracle paths are
  relative and confined; Cell names, replacement edges, scripts, witnesses,
  runtime errors, and declared limits are validated before execution.
- Simulator results remain `development-non-consensus`. CKB-VM results remain
  runtime evidence. Neither may be promoted to RPC admission, deployment,
  commitment, confirmation, or complete source equivalence.
- The v1 local live-Cell model proves bookkeeping only; it does not inject
  scenario Cells into CKB syscalls. Transaction-shaped cases continue to cite
  the stateful CKB oracle until a syscall harness is explicitly promoted.

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

- Write routine test, benchmark and intermediate replay reports under `target/`.
  Commit report payloads only for intentional final acceptance evidence or a
  necessary reproduction baseline; keep concise decisions and comparisons in docs.
- When removing historical report payloads, retain exact Git snapshot links and
  stored-byte hashes, update local links and checksum inventories, and document
  which historical manifests require restoring archived files before validation.
- Do not describe a feature as implemented unless parser, type checking,
  lowering, metadata, LSP/editor behavior, tests, examples, and docs agree on
  the same boundary.
- Use "reserved", "deferred", or "fail-closed" when syntax exists but executable
  semantics are intentionally unavailable.
- Release notes should separate highlights, scope boundaries, validation
  commands, and links to detailed docs.
- Release notes describe what shipped. Future work belongs in a concrete RFC
  or proposal with explicit ownership and acceptance criteria, not in release
  documentation.

## Tests

- For syntax changes, add parser, formatter, type-checker, lowering, metadata,
  and LSP/editor tests where applicable.
- For CKB-facing changes, add negative tests for unsafe or ambiguous forms.
- For assembler/codegen changes, add targeted tests for the exact generated
  instruction surface and at least one compile-through `riscv64-elf` path.
- Prefer focused tests during development, then broaden validation before
  completion.
- Register shared helper tests through an explicit owning test target, rather
  than every `#[path]` consumer. Keep multiple owners when dependency closures
  or independent implementations provide distinct coverage.

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

5. **Check delimiters after every deletion.** Run `cargo fmt --check`, then the
   focused `cargo check --locked -p cellscript --all-targets` before the next
   extraction. Off-by-one deletion ranges can leave orphaned lines or consume
   closing braces.

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

**`emission.rs`** — structured memory/address operands, destination-owned large
immediate expansion and final instruction rendering. The generic text writer
must not inspect instructions or select address lowering. Dynamic allocated
stores pass their typed operation, original base and signed offset to
`value_arena/write_guard.rs` before emission; guard spills do not recursively
guard themselves. Final assembly decoding for ELF, layout evidence and audits
remains an artifact boundary, not an emitter input representation.

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
