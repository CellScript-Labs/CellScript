# CellScript 0.13 Release Notes Draft

**Status**: Release notes for the 0.13.2 release-candidate line on
`nightly-0.13`.

**Updated**: 2026-05-03.

## Collections Scope

CellScript 0.13 adds executable stack-backed `Vec<T>` helper support for
bounded value vectors where element width is known. This is separate from the
0.12 schema/ABI work.

Already present before 0.13:

- `Vec<u8>`, `Vec<Address>`, `Vec<Hash>`, and supported nested witness payload
  vectors in Molecule schema/ABI and entry-witness paths.
- `Vec<Address>` declarations in examples such as multisig/timelock.
- Read-oriented dynamic Molecule vector support where the runtime has schema
  metadata and witness/cell bytes.

New in 0.13:

- Stack-backed local `Vec<u64>` helpers.
- Stack-backed local fixed-byte helpers for `Vec<Address>` and `Vec<Hash>`
  width-compatible values.
- Stack-backed fixed-width named schema values, covered by the `Vec<Snapshot>`
  helper matrix plus field reads from popped/indexed elements.
- Runtime lowering for `new`, `with_capacity`, `capacity`, `push`,
  `extend_from_slice`, `len`, `is_empty`, indexing, `first`, `last`,
  `contains`, `set`, `remove`, `pop`, `insert`, `reverse`, `truncate`,
  `swap`, and `clear`.
- Negative type-check coverage for unsupported helper/type combinations.
- Stable fail-closed metadata names for unsupported collection paths.
- `examples/language/registry.cell` documents supported local `Vec<Address>` /
  `Vec<Hash>` helper usage without implying full `HashMap<K, V>` support. The
  top-level `examples/registry.cell` remains a compatibility mirror. It is a
  compiler/tooling language example, not part of the seven-example CKB
  production action acceptance matrix.
- `examples/language/order_book.cell` is a non-production language example for
  local stack-backed order vectors. It compiles through the bounded `Vec<T>`
  helper surface, but it does not persist orders as Cells, prove map membership,
  settle assets, or enforce exchange-level authorization.
- The canonical business examples are now mirrored under `examples/business/`,
  while production/profile metadata lives under `examples/acceptance/`. The CKB
  acceptance script compiles the profiled copies when present, keeping
  `#[effect(...)]` and `#[scheduler_hint(...)]` out of reader-facing business
  files without dropping release evidence. Subdirectory copies use scoped module
  namespaces so they can coexist with the canonical top-level examples during
  module loading.
- Runtime and constraints metadata expose each checked stack-backed
  fixed-width `Vec<T>` instantiation, including scope, element type/width,
  backing capacity, status, and helper set. Constructor helpers now preserve
  `Vec::new` versus `Vec::with_capacity` instead of collapsing both to `new`.
- `cellc explain-generics` exposes the checked bounded `Vec<T>` instantiation
  set in text or JSON form for local audit.
- Metadata schema version is now 30.

Important boundaries:

- `Vec::capacity()` reports the fixed stack backing capacity
  (`256 / element_width`), not the requested `Vec::with_capacity(n)` value.
- Full generic `HashMap<K, V>` / `HashSet<T>` runtime support is not part of
  0.13.
- `Vec<Cell<T>>`, `Vec<Resource<T>>`, and other cell-backed / linear ownership
  collections remain fail-closed until an executable ownership model exists.
- `Option<T>` is still reserved for a future explicit error/optional-value
  model and is not implemented in 0.13.
- 0.13 must not re-count 0.12 `Vec<Address>` / `Vec<Hash>` schema and ABI
  support as new work.

## Surface Syntax And Example Canonicalization

The 2026-04-26 surface pass is a syntax and example-organization pass, not an
authorization redesign. It makes the canonical examples shorter and makes CKB
lock data sources more visible while keeping authority-sensitive features
explicit or fail-closed.

Completed syntax delta from the `0.12` tag:

- Namespace-style module and import paths are the documented style:
  `module cellscript::token` plus grouped imports such as
  `use cellscript::asset::{Token, MintAuthority}`.
- Persistent declarations use DSL-native capability lists:
  `resource T has store, transfer, destroy`, `shared T has store`, and
  `receipt T has store, claim`.
- Persistent declarations can declare their default CKB hash type with
  `with_default_hash_type(Data | Data1 | Data2 | Type)` after the capability
  list and before the field block.
- Canonical action output bindings are named in the signature:
  `action f(input: T) -> output: T` and
  `action f(input: T) -> (left: T, right: Receipt)`. These named outputs are
  deterministic proposed transaction outputs.
- Action and lock parameters support prefix source qualifiers:
  `input name: T`, `read name: T`, `protected name: T`, `witness name: T`,
  and `lock_args name: T`. Proposed output Cells are named on the action return
  side, not in the parameter list. Expression `read_ref<T>()` remains the
  explicit CellDep read effect.
- One-to-one input/output transitions are expressed by action signatures and
  explicit constraints, not a separate lineage keyword: `action(before: T) ->
  after: T` names the consumed input and proposed output, while `require` proves
  continuity.
- Explicit state transitions use the singular action clause
  `move before.state: Live -> after.state: Filled`.
- Action bodies use a structured `where` proof block. `move` clauses sit between
  the signature and `where`; proof logic (`require`, `let`, `if`, `match`,
  helper calls) lives under `where`.
- Signature direction defines input/output topology, `move` defines state edges,
  and `require` defines continuity or accounting constraints.
- The type checker now rejects asymmetric proof branches when an output field is
  constrained by `require` in one `if`/`match` branch but not all sibling
  branches, unless that field was already constrained in the dominating proof
  scope.
- State graphs use `flow` declarations:
  `flow Name for Type.field { A -> B by action; }` or compact
  `flow Type.field { A -> B; }`. A state field may have exactly one flow.
- State is ordinary schema data, usually an explicit enum field, and the
  compiler does not inject hidden Molecule fields.
- `create output = T { ... }` constrains a named proposed output binding and is
  the canonical verifier surface used by docs and examples.
- `create` and ordinary struct literals support field shorthand; examples use
  it where the field name and source binding are identical.
- Declared state names can be used in `create` initializers, and qualified state
  names such as `Ticket::Active` can be used in guards and expressions instead
  of raw numeric indexes.
- `require condition` and `require condition, "message"` are verifier-boundary
  guards for actions and locks. `assert(condition, "message")` is the canonical
  internal assertion spelling; the formatter emits the non-macro form.
- Typed empty `Vec<T>` literals such as `let mut keys: Vec<Hash> = []` and
  contextual field literals such as `data: []` lower through the existing
  `Vec::new()` path when the expected `Vec<T>` type is known.
- Cell updates are expressed with signature-direction outputs, `move`, and
  explicit `require` constraints.
- Bundled locks use `protected`, `witness`, `lock_args`, and `require` to
  distinguish guarded input Cell views, typed script args, transaction witness
  data, and script failure predicates.
- Clean business examples are separated from profiled acceptance examples, while
  the flat `examples/*.cell` files remain compatibility mirrors.
- LSP completions plus VS Code grammar and snippets are refreshed for the new
  source qualifiers, named outputs, `flow`, `move`, and named
  `create` syntax.

Important boundaries:

- `lock_args` is implemented for fixed-width lock parameters by decoding the
  executing lock Script.args bytes. Sighash/signature verification remains
  explicit future work.
- 0.13 does not introduce first-class signer values, implicit `Address` signer
  semantics, or hidden sighash defaults.
- `witness Address` means decoded witness data only; it is not a cryptographic
  authorization proof.
- `protects T { self ... }` remains deferred until protected-input selection and
  lock-group aggregation semantics are exact.
- Acceptance/profiled copies still carry scheduler and effect metadata because
  they are part of release evidence.

## Verification

Current release-gate commands:

```bash
./scripts/cellscript_ckb_release_gate.sh quick
./scripts/cellscript_ckb_release_gate.sh full
```

The full gate is the release-facing command. It expands to the compiler,
tooling, syntax-combination, VS Code, documentation-boundary, and
builder-backed local CKB acceptance checks. Useful component commands are:

```bash
./scripts/cellscript_syntax_combo_audit.sh ci
cargo fmt --all --check
cargo clippy --locked -p cellscript --all-targets -- -D warnings
cargo test --locked -p cellscript -- --test-threads=1
git diff --check
```

CKB-facing repository gates:

```bash
./scripts/cellscript_ckb_release_gate.sh
./scripts/cellscript_ckb_release_gate.sh production
./scripts/ckb_cellscript_acceptance.sh --production
```

The default `cellscript_ckb_release_gate.sh` mode is the quick gate. It includes
the quick syntax-combination audit plus compile-only production acceptance and
is useful before push. The `production`/`full` mode also runs the broader
syntax-combination CI matrix before the full local CKB acceptance script, and is
the release-facing gate.

## Syntax Governance And Standard Library

New in 0.13.2:

- Core `transfer`, `claim`, and `settle` lifecycle expression semantics are not
  part of the executable core surface. The stable spelling is through explicit
  compiler-recognized stdlib patterns.
- `std::lifecycle::transfer(input, output, to) { fields }` consumes `input`,
  creates the named output with `with_lock(to)`, preserves only the listed data
  fields, and checks type continuity.
- `std::receipt::claim(receipt, output, lock) { fields }` consumes the receipt
  and creates the receipt-declared output type with the supplied lock.
- `std::lifecycle::settle(input, output, lock) { fields }` follows the same
  consume-plus-named-output lowering as transfer/claim.
- `std::cell::same_lock`, `std::cell::preserve_lock`, and
  `std::cell::preserve_capacity` lower to canonical Cell metadata verifier
  checks.
- `preserve` sugar is type-equivalent to its canonical `require` expansion:
  field names and field types must match on both sides.
- Anonymous `require` blocks remain pure boolean proof syntax; lifecycle stdlib
  calls and other Cell effects are rejected inside those blocks.
- Codegen no longer derives claim authorization checks from action names. If a
  protocol needs signature authorization, it must use an explicit future
  verification primitive instead of compiler naming magic.
- The syntax-combination audit runs parser, formatter, type, lowering,
  metadata, codegen, and negative obsolete-syntax oracles with fail-closed mode
  contracts.

Important boundaries:

- Stdlib helpers are audit-visible shorthand, not a place for hidden protocol
  policy.
- The current stdlib does not infer signer authority, generate change outputs,
  implement generic maps, or model Cell-backed collection ownership.

## Backend And ELF Emission

New in 0.13:

- The internal ELF assembler covers the emitted instruction surface used by the
  current compiler and stdlib tests.
- The assembler support surface is now guarded by an explicit supported
  mnemonic allowlist plus an intentionally unsupported mnemonic list. Bundled
  example codegen output, generated stdlib assembly, and generated collection
  assembly must stay inside the declared supported surface, so public generated
  assembly cannot quietly drift into GNU assembler mnemonics that the internal
  assembler does not encode.
- Register conditional branches `beq`, `bne`, `blt`, `bge`, `bltu`, and `bgeu`
  are accepted and encoded.
- Zero-compare branches `beqz` and `bnez` remain supported.
- Conditional branch relaxation is covered for both zero-compare and register
  branch forms, so generated local `Vec<T>` helpers such as `insert` and
  `contains` can compile to ELF without relying on an external assembler.
- Large immediates emitted by CellScript lowering are normalized before internal
  ELF assembly. This covers full-width `u64` `li` literals, large stack-frame
  offsets, and fixed schema field offsets beyond the RISC-V 12-bit load/store
  or `addi` immediate range, including non-`sp` base registers used for
  schema/data pointers.
- Stack-frame load/store emission is centralized behind stack helpers instead
  of scattered handwritten `offset(sp)` formatting. This makes large stack
  offset handling a codegen invariant, with a regression test guarding against
  direct stack pointer memory/access emission outside the helpers.
- Large `addi` lowering now chooses a scratch register that does not overwrite
  the source/base register, preventing large fixed-byte collection copy paths
  from losing a live pointer when it is held in `t6`.
- Large `sp + offset` address materialization now clobbers only the requested
  destination register instead of using `t6` as a hidden scratch register.
- RV64 `li` materialization avoids the `lui` sign-extension cliff near the
  positive 32-bit boundary. Values such as `0x7ffff800` and `0x7fffffff` now
  use the long materialization path instead of silently producing sign-extended
  wrong-code.
- Pool token-pair TypeHash admission is no longer emitted from a `seed_pool`
  function-name hook in codegen. AMM examples express the rule as a normal DSL
  `token_a.type_hash() != token_b.type_hash()` invariant, which lowers through
  the generic runtime `type_hash()` and fixed-byte comparison paths.
- Internal function calls and parameterized entry wrappers now stage ABI
  arguments beyond `a7` on the outgoing call stack, so callees that require
  schema pointer/length plus TypeHash ABI pairs do not silently turn into
  fail-closed "arg beyond register" paths.
- Entry-witness wrappers stage those outgoing stack arguments below the local
  witness frame before adjusting `sp` for the call, preventing stack-spill ABI
  slots from overwriting decoded witness payload bytes such as fixed-byte
  `Address` parameters.
- `env::current_timepoint()` is documented as the CKB HeaderDep#0 epoch number
  under the CKB profile, not as a Unix timestamp.
- Large-offset unaligned scalar loads now materialize the load address with an
  explicit live-register avoid set, so accumulator registers such as `t6` are
  not clobbered by the fallback address scratch.
- Large fixed schema field regression coverage now includes both scalar loads
  and fixed-byte field pointer paths, so valid DSL such as a schema with a
  2048-byte prefix field compiles through `riscv64-elf`.
- Fixed-byte constants now materialize through concrete `.rodata` labels, so
  local `Address::zero()`, `Hash::zero()`, array, and `u128` constants can
  round-trip through internal ELF emission.
- IR join value transfers now use the same operand materialization path as normal loads,
  so fixed-byte constants selected by `if`/join control flow keep their rodata
  pointers instead of degrading to a null pointer.
- Generic `u128` comparison and supported `u128 +/- u64` lowering now use
  explicit 16-byte storage/comparison and carry/borrow arithmetic instead of
  falling through a single-slot 8-byte register model.
- Parameterized entry wrappers now reject witness payloads larger than their
  local witness buffer before decoding dynamic payload lengths, and reject
  trailing payload bytes after all static or dynamic witness arguments are
  consumed.
- State storage remains explicit cell data: the compiler does not
  inject hidden state fields or mutate Molecule layout. `create` initializers
  may now use declared state names such as `state: Created`, while
  guards and computed expressions can use qualified names such as
  `Ticket::Active` instead of numeric state indexes. The LSP now completes
  those qualified flow states after `Type::`.
- Declarative flows can now be expressed without hidden layout changes:
  `flow Name for Type.field { A -> B by action; }` and compact
  `flow Type.field { A -> B; }` declare the graph, while action signatures can
  bind the edge they prove with explicit field-to-field `move` clauses such as
  `move old.state: Live -> new.state: Filled`. Cross-cell state edges require an
  explicit field-to-field `move`; `flow ... by action` validates the action's
  exact `from -> to` edge instead of accepting any move on the same field. The
  type checker, state static checks, IR metadata, runtime verifier, formatter,
  docs generator, and LSP all carry the explicit state field name. A state field
  may have only one flow declaration; CellScript does not merge partial flow
  declarations.
- The semantic core for state transitions is now proposed-cell verification:
  `action(before: T) -> after: T` treats `before` as a transaction input and
  `after` as a proposed transaction output. `output after: T` parameter syntax
  was removed before release; named action outputs are the only output-binding
  surface. `create after = T { ... }` constrains a declared output binding
  rather than allocating runtime storage.
- Cell updates are expressed with signature-direction outputs, `move`, and
  explicit `require` constraints, keeping the CKB transaction shape visible in
  source.
- Action proof scopes now use `where`: state edges remain action-level `move`
  clauses before `where`, and verifier obligations stay as explicit proof
  statements inside the block.
- Flow checking no longer treats enum or declaration order as a hidden
  linear state sequence: initial creates may use any declared state, and declared
  edges may return to the first state. `by action` edges require the action to consume the corresponding
  owned input and create exactly one proposed output when no explicit
  field-to-field `move` names the output. Explicit `move` clauses validate the
  named input and output parameter bindings directly.
- Output preserved-field verification now fails closed when not every preserved
  field is verifier-addressable; metadata no longer classifies oversized
  data-except fallback paths as checked-runtime.
- `read_ref` runtime fallback no longer reuses the output counter as a CellDep
  index. If a CellDep index was not allocated, the generated verifier fails
  closed.
- `read_ref` runtime fallback also records the loaded CellDep buffer and size
  offsets consistently, so later schema and type-hash operations see the same
  cell-backed state as preplanned read refs.
- External RISC-V toolchain fallback now cleans its temporary directory on both
  success and error paths.
- External RISC-V toolchain overrides must now be absolute paths to existing
  executable files. Relative command names and directories are rejected before
  the backend launches a process.

Important boundary:

- This is not a claim of full arbitrary RISC-V assembly support. The internal
  assembler is kept aligned to the CellScript-emitted surface and guarded by an
  emitted-instruction-surface regression test.
- Common GNU/RISC-V conveniences such as `lui`, `addiw`, `nop`, `andi`, `ori`,
  register-register `xor`, raw `jal`/`jalr`, signed sub-word loads, CSR
  operations, atomics, floating-point, compressed instructions, `fence`, and
  broad pseudo-instruction support remain outside the 0.13 backend contract
  unless future codegen starts emitting them.

## CLI Ergonomics

New in 0.13:

- `cellc build` uses O1 for non-release builds and still uses O3 for
  `--release`.
- `cellc new` provides a Cargo-style package creation workflow with `--path`,
  `--lib`, `--vcs git`, `--vcs none`, and JSON summaries.
- `cellc new --lib` and `cellc init --lib` now keep generated package layout and
  `Cell.toml` aligned: the entry is `src/lib.cell`, and no stale
  `src/main.cell` entry file is left behind.
- `cellc explain <error-code>` reports runtime error registry entries.
- `cellc explain-generics [--json]` reports checked stack-backed
  `Vec<T: FixedWidth>` instantiations, including element width, fixed backing
  capacity, backing model, status, and exact helper set.
- CLI stderr uses `error[E####]` plus a `cellc explain E####` hint when a
  policy or compile error maps to the runtime error registry.

## Lock Boundary Surface

New in 0.13:

- Lock parameters can classify CKB data sources with `protected` and `witness`.
  `protected T` is a typed view of one selected input Cell in the current script
  group whose spend is guarded by the lock invocation. `witness T` is decoded
  transaction witness data.
- `require` is available as the canonical lock predicate form. A false
  condition fails the current script validation; it does not create
  authorization by itself.
- `lock_args T` binds fixed-width lock parameters to typed bytes decoded from
  the executing lock Script.args. The entry wrapper rejects trailing args bytes
  after the declared typed parameters.
- The bundled production locks now have builder-backed local CKB valid-spend and
  invalid-spend matrix coverage in the production acceptance report.

Important boundaries:

- `Address` is not a signer proof by name.
- `witness Address` is not witness-sighash authorization.
- Hidden sighash defaults are not part of 0.13. Future signature verification
  syntax must expose digest mode, script group scope, witness layout, and replay
  assumptions.

## Backend Shape Baseline

The current 0.13 implementation still passes the bundled example backend-shape
budget test.
Snapshot from `bundled_examples_backend_shape_report_serializes`:

| Example | Assembly lines | Text bytes | Machine blocks | CFG edges | Call edges |
|---|---:|---:|---:|---:|---:|
| `amm_pool.cell` | 8836 | 34496 | 1370 | 2354 | 329 |
| `launch.cell` | 5742 | 21912 | 740 | 1263 | 219 |
| `multisig.cell` | 20502 | 78672 | 3531 | 5602 | 273 |
| `nft.cell` | 12849 | 48288 | 2421 | 4003 | 307 |
| `timelock.cell` | 10585 | 40176 | 1876 | 3098 | 248 |
| `token.cell` | 2673 | 10112 | 481 | 793 | 85 |
| `vesting.cell` | 4007 | 15088 | 587 | 1017 | 191 |
