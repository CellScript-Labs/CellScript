# CellScript Coding Style

This document is the tracked project standard for compiler, backend, docs, and
release work. Local notes may exist in `*.local.md`, but they are not part of the
project contract.

## General Rust Rules

- Keep changes scoped to the compiler phase being modified.
- Prefer existing AST, IR, metadata, and codegen structures over parallel
  stringly typed paths.
- Parser support alone is not a feature boundary. New syntax must agree across
  parsing, formatting, type checking, lowering, metadata, examples, docs, and
  tests.
- Use enums and typed fields when the concept already has a structured
  representation.
- Error messages should name the rejected boundary and the next valid action.
- Run `cargo fmt --all` before committing Rust changes.
- Run `cargo check --locked -p cellscript --all-targets`,
  `cargo test --locked -p cellscript`, and `git diff --check` before completing
  compiler work. For broad Rust changes, also run
  `cargo clippy --locked -p cellscript --all-targets -- -D warnings`.

## Backend And Codegen Rules

`src/codegen/mod.rs` is currently a legacy monolith covering layout planning,
CKB syscall lowering, verifier pattern emission, RISC-V assembly generation, the
internal ELF assembler, and backend-shape tests. New code should reduce coupling
where practical and must not make the implicit backend contracts more implicit.

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
- The current Tier 1 canonical forms are `add`, `addi`, `sub`, `and`, `or`,
  `mul`, `div`, `rem`, `slt`, `sltu`, `xori`, `ld`, `lbu`, `sb`, `sh`, `sw`,
  `sd`, `slli`, `srli`, `beq`, `bne`, `blt`, `bge`, `bltu`, `bgeu`, `ret`, and
  `ecall`.
- Treat pseudo-instructions as explicit API. `li`, `la`, `call`, `j`, `mv`,
  `seqz`, `snez`, `neg`, `sgt`, `bgt`, `bgez`, `beqz`, and `bnez` are supported
  because current generated surfaces use them.
- Tier 2 candidates may be added when an optimizer, typed emission path, or
  constant materializer needs them: `nop`, `lui`, `auipc`, raw `jal`/`jalr`,
  `andi`, `ori`, register-register `xor`, `sll`, `srl`, `sra`, `srai`, `addw`,
  `addiw`, and `subw`.
- Tier 3 instructions remain demand-driven: signed byte/half/word loads,
  unsigned half/word loads, `slti`, `sltiu`, branch aliases such as `ble`,
  `bleu`, `bgtu`, `bltz`, `bgtz`, `blez`, plus `not` and `jr`.
- Do not add CSR operations, atomics, floating-point instructions, compressed
  instructions, `fence`, `tail`, or the full GNU pseudo-instruction surface
  unless a concrete CellScript backend contract requires them.
- Do not hand-write stack offsets. All stack access must go through
  `emit_stack_load`, `emit_stack_load_byte`, `emit_stack_store`, or
  `emit_stack_store_byte`.
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
  lowering, metadata, tests, examples, and docs agree on the same boundary.
- Use "reserved", "deferred", or "fail-closed" when syntax exists but executable
  semantics are intentionally unavailable.
- Release notes should separate highlights, scope boundaries, validation
  commands, and links to detailed docs.
- Do not keep roadmap promises in `docs/`. Release notes may describe what
  shipped; future plans belong in dedicated roadmap/proposal files.

## Tests

- For syntax changes, add parser, formatter, type-checker, lowering, and
  metadata tests where applicable.
- For CKB-facing changes, add negative tests for unsafe or ambiguous forms.
- For assembler/codegen changes, add targeted tests for the exact generated
  instruction surface and at least one compile-through `riscv64-elf` path.
- Prefer focused tests during development, then broaden validation before
  completion.
