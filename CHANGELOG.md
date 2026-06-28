# Changelog

## 0.20.0 - 2026-06-28

- Bring `cellc` CLI discovery and direct-source diagnostics closer to Rust's
  developer experience: top-level help now shows package commands plus direct
  compile mode, `cellc --list` enumerates commands, unknown bare commands get
  nearest-command suggestions, and direct parse/lex/compile errors print
  `file:line:column` source snippets. The top-level `cellc --explain <CODE>`
  alias now mirrors the existing `cellc explain` command, and multi-diagnostic
  package checks render each frontend error with its own source context instead
  of collapsing them into one summary string.
- Fix numeric-width soundness by requiring exact non-literal numeric type
  equality while preserving declared integer literal widths through type
  checking and IR lowering.
- Reduce generated branch cost by skipping unconditional jumps to the physical
  fall-through block and by selecting `beqz`/`bnez` branch forms that keep the
  fall-through path implicit.
- Extend `cellc opt-report` and constraint artifact metadata with backend
  shape counters and estimated-cycle deltas across optimisation levels.
- Replace the incremental parallel compiler's identity ordering with a real
  dependency topological sort and cycle-safe fallback ordering.
- Close the registry source-package plan in docs and tooling: registry install,
  build, and update use the two-tier Git resolver with yanked-version skipping
  and `source_hash` verification, while `registry add` now prints next steps
  for the actual cloned discovery worktree.
- Harden the CKB/devnet acceptance path with an ELF entry ABI gate that checks
  RX-only executable segments, `filesz == memsz`, and entry trampoline
  stack-pointer preservation before local-node evidence is accepted.
- Require launch, token, and AMM bootstrap examples to carry passing ELF entry
  ABI evidence alongside existing builder-backed action, lock-spend, cycle,
  transaction-size, occupied-capacity, and stateful lifecycle checks.
- Add 0.20 release notes documenting the strengthened devnet acceptance
  boundary and the remaining difference between compile-only and live local
  devnet evidence.
- Promote multi-file package support into the 0.20 compiler/tooling boundary:
  exact-path imports, source-graph diagnostics, dependency-aware cache keys,
  package-aware LSP diagnostics, and an additive WASM multi-source metadata API.
- Mature cross-file helper reuse by inlining aliased imports, fully-qualified
  calls, same-basename dependency helpers, and transitive helper calls into the
  entry artifact with stable internal labels, while keeping ELF-linker and
  cross-script runtime-linking claims out of scope.
- Record the 0.20 evidence gate for protocol-source multi-file showcases:
  NovaSeal, iCKB, and DobEvo / DOB-EVO source refactors may demonstrate shared
  schema/type imports only when the matching devnet or CKB VM evidence is
  regenerated, and playground multi-file import/export remains browser-local.
- Add the first protocol-source multi-file candidate in NovaSeal
  fungible-xUDT by moving shared schema structs into
  `nova_fungible_xudt_schema.cell` and importing them from both profile and
  lifecycle entries. Metadata/artifact preparation records the shared schema
  source unit, and live local devnet stateful evidence passes issue, transfer,
  settle, and required negative cases for lifecycle data hash
  `0x394da78133cb2f5a5d6cd911feceeab9e97e6ad5d36c0e50f18be56653af85e5`.

## 0.17.0 - 2026-05-04

- Add the research iCKB protocol-equivalence surface with partial CKB VM
  differential evidence, including 75 original-vs-CellScript executed rows,
  14 CellScript-only VM rows, 8 original-side VM rows, and an explicit
  `NOT_PROVEN` production-equivalence gate.
- Add 0.17 strict CKB protocol helpers for SourceView, DAO accumulated-rate
  and maturity checks, xUDT group amount helpers, script args/hash guards,
  MetaPoint/OutPoint relation scans, and C256 product requirements.
- Add executable iCKB benchmark specs and matrix evidence under
  `tests/benchmarks`, while keeping iCKB-specific receipt layout and fixture
  logic out of the generic compiler/runtime surface.
- Keep production equivalence deliberately unclaimed until owner-auth witness
  fixtures, byte-accurate receipt decoding, full DAO redeem accounting,
  generic aggregate lowering, and production manifest closure are complete.

## 0.16.1 - 2026-06-15

- Close the bundled token/AMM/launch bootstrap lifecycle gaps with explicit
  first-cell actions and strict original scoped CKB coverage.
- Rename the token authority mint action to `mint_with_authority` and the
  launch bootstrap action to `bootstrap_token` so builder-facing action names
  match the required input topology.
- Add `nft.cell::create_collection` and stateful coverage for the
  `create_collection -> mint -> create_listing -> buy_from_listing` path.
- Document and validate the CLI-first builder handoff through
  `--entry-action`, `cellc abi`, `cellc entry-witness`,
  `cellc explain-assumptions`, and `cellc validate-tx`.
- Re-run production local CKB acceptance with strict original scoped artifacts,
  complete bundled action coverage, and stateful lifecycle scenarios.

## 0.16.0 - 2026-06-14

- Add the scoped metadata-assurance release surface: operational semantics,
  ProofPlan soundness checks, builder-assumption metadata, transaction-shape
  validation, solver templates, deployment reports, proof diffs, profiling,
  transaction traces, and audit bundles.
- Ship NovaSeal as bundled proposal packages with local devnet/profile
  acceptance tooling, while keeping production claims blocked on external
  BIP340 TCB, public BTC SPV, public/shared CellDep, and profile-specific
  attestations.
- Tighten the NovaSeal public BTC SPV evidence contract so BTC-facing profile
  cases must bind current live CKB report hashes, service-builder hashes,
  CKB-side BTC commitment hashes, raw Bitcoin transaction material, block
  header and Merkle proof data, confirmation heights, and canonical SPV
  material hashes.
- Harden the 0.16 compiler-freeze gate with explicit IR poison rejection,
  instruction-level IR provenance, reserved-register contract checks, syscall
  ABI baseline coverage, and line-exact diagnostic regression directives.
- Align `cellc --help`, README command tables, and the VS Code active-file
  command surface with the 0.16 builder, transaction-template, deployment,
  profile, and audit-bundle tooling.
- Add `--primitive-strict=0.16`, which includes the 0.15 primitive vocabulary
  rules and rejects metadata-only/runtime-required ProofPlan gaps in strict
  assurance mode.
- Add descriptive standard CKB compatibility fixture manifests for sUDT, xUDT,
  ACP, Cheque, Omnilock, NervosDAO since/epoch behavior, Type ID,
  ScriptGroup, and `outputs_data` shapes.
- Add CKB stdlib protocol module schema stubs for sUDT, xUDT, TYPE_ID, HTLC,
  Cheque, ACP, and DAO-facing descriptors while keeping executable protocol
  lowering deferred.
- Carry the 0.15 proof/invariant scope forward without overstating it:
  aggregate invariant lowering, full ProofPlan soundness proofs, macro-only
  lowering, covenant stdlib helpers, strict address/script type separation,
  entry role syntax, versioned layout migration, and executable fixture
  matrices remain tracked for later releases.
- Merge the 0.15 strict syntax and example cleanup into the 0.16 assurance
  branch, including canonical `transition`/`where` action syntax, kernel-effect
  capabilities, stdlib lifecycle metadata, and VS Code packaging dry-runs.
- Keep the 0.16 documentation honest about scope: ProofPlan soundness and
  builder evidence are strict metadata-assurance gates, NovaSeal devnet
  certification is proposal-local evidence, and full production claims still
  require CKB dry-run/commit evidence plus required external attestations.

## 0.15.0 - 2026-05-26

- Add scoped invariant declarations with explicit trigger, scope, reads,
  coverage, and runtime-obligation metadata for CKB covenant auditing.
- Add Covenant ProofPlan records and `cellc explain-proof` so action, lock,
  invariant, aggregate, identity, and lifecycle obligations are inspectable in
  human-readable and JSON form.
- Add aggregate invariant primitives such as `assert_sum`,
  `assert_conserved`, `assert_delta`, `assert_distinct`, and
  `assert_singleton`; these currently emit metadata-only runtime obligations
  until executable aggregate verifier lowering is promoted.
- Promote cell identity policies and identity-aware lifecycle forms through
  `identity(...)`, `create_unique`, and `replace_unique`, including TYPE_ID,
  field, script-args, and singleton-type metadata.
- Add explicit destruction-policy forms and carry destruction policy through
  IR/codegen while keeping bare `destroy` available as the default policy.
- Reset resource capabilities from protocol verbs to 0.15 kernel effects such
  as `create`, `consume`, `replace`, `burn`, `relock`, `retarget_type`, and
  `read_ref`.
- Add `--primitive-compat 0.14` and `--primitive-strict 0.15` migration modes
  across direct `cellc` compilation and package commands, with CS0151-CS0160
  diagnostics for legacy `destroy` capability.
- Allow direct lifecycle operations to be authorized by kernel-effect
  equivalents: `destroy` accepts `consume + burn`.
- Convert canonical bundled examples, language examples, README examples, wiki
  tutorials, and release gates to strict 0.15 kernel-effect capabilities.
- Extend strict acceptance and syntax-combination gates so bundled examples
  compile directly under `--primitive-strict 0.15`, and update release
  documentation to keep 0.15 P0 scope separate from deferred 0.16 proof
  soundness and compatibility-suite work.

## 0.14.0 - 2026-05-09

- Add the CKB semantic-completeness surface for typed Source and WitnessArgs
  views, fixed-width `lock_args`, explicit `env::sighash_all(...)`, and
  profile-visible since, time, and epoch policy helpers.
- Add bounded Spawn/IPC verifier composition through `spawn`, `wait`, `pipe`,
  inherited file descriptors, and close/read/write helpers, with
  metadata-visible script references and type-checker rejection of static
  descriptor leaks, double closes, and use-after-close paths.
- Report a structured CKB target-profile ABI contract for witness data, lock
  args, Source encoding, Spawn/IPC, since/time, CellDep and script references,
  `outputs` / `outputs_data`, capacity floors, TYPE_ID, and CKB transaction
  version.
- Validate profile ABI metadata, runtime-access metadata, ScriptGroup evidence,
  TYPE_ID output plans, script references, and `outputs_data` bindings so
  release evidence fails closed when compiler policy and metadata drift apart.
- Expose declarative output capacity floors through
  `with_capacity_floor(...)` and `occupied_capacity(...)` while keeping builder
  funding, transaction-size, occupied-capacity, and acceptance evidence as
  explicit production responsibilities.
- Add executable fixed-Hash Blake2b support through CKB's
  `ckb-default-hash` personalization and metadata-visible `CKB_BLAKE2B`
  runtime access.
- Complete the state-edge spelling cleanup from legacy `move` to
  `transition`, and refresh examples, docs, formatter behavior, LSP
  completions, VS Code snippets, and syntax highlighting for the 0.14 surface.
- Add language examples for delegate verification, Spawn/IPC pipelines,
  witness/source views, TYPE_ID creation, capacity/time policy, and canonical
  style.
- Harden malformed input handling across metadata tampering, scheduler and CLI
  decoding, LSP incremental edits, static width calculations, entry-witness
  widths, and package-version parsing.
- Add the reusable 0.14 scope audit gate and document the release boundary:
  metadata/tamper validation and strict compilation now, with full
  accepted/rejected CKB transaction fixture matrices left to the later
  compatibility-suite track.

## 0.13.2 - 2026-05-03

- Complete syntax-governance layering for lifecycle semantics by keeping
  `claim`, `settle`, and `transfer` out of the executable core expression
  surface and implementing the corresponding stdlib patterns explicitly.
- Implement `std::cell::same_lock`, `std::cell::preserve_lock`, and
  `std::cell::preserve_capacity` through canonical cell metadata verifier
  checks.
- Make `std::lifecycle::transfer`, `std::receipt::claim`, and
  `std::lifecycle::settle` expand to consumed inputs, locked named outputs,
  and complete output field preservation.
- Harden preserve and require sugar so preserved fields are type-equivalent
  to their canonical require expansion and anonymous require blocks remain
  pure boolean verifier constraints.
- Remove the remaining compiler-level claim witness/signature special cases
  and reserve the old claim-signature runtime error code.
- Add example and editor-tooling coverage for the stdlib lifecycle and cell
  metadata helper surface.
- Add an executable syntax-combination audit runner for parser/formatter/type
  checking/lowering metadata/codegen oracles, wire the quick audit into local
  gates, and run the broader CI matrix in GitHub Actions and the full release
  gate.
- Make CI run on nightly branches and version tags, and add syntax-audit mode
  contracts so accidental coverage shrinkage fails closed.
- Sync the 0.13 roadmap/release scope with the 0.13.2 governance boundary and
  add a release-gate check that keeps those docs aligned.
- Pin VS Code extension packaging to `@vscode/vsce` and make local VSIX
  packaging dry-runs part of the release gate.
- Document the syntax-combination audit as a reusable release acceptance
  preflight that runs before builder-backed CKB acceptance.
- Finalize the 0.13.2 release notes under `docs/releases/`, add a docs map, and
  move historical 0.13 planning documents into `docs/archive/0.13/`.

## 0.13.0 - 2026-04-30

- Complete the internal RISC-V ELF assembler branch surface used by current
  codegen, including `beq`, `bne`, `blt`, `bge`, `bltu`, `bgeu`, `beqz`,
  `bnez`, and branch relaxation coverage.
- Harden the stack-backed `Vec<T>` helper boundary so unsupported receivers,
  invalid `extend_from_slice` element types, and unrefined `Vec::new()` slice
  extension cases fail at compile time instead of drifting into hidden runtime
  paths.
- Add `examples/language/order_book.cell` as a non-production language example
  for local stack-backed order vectors.
- Add the CKB release-gate wrapper script and document the difference between
  quick compile-only evidence and full production acceptance.
- Add builder-backed local CKB valid-spend and invalid-spend acceptance coverage
  for all 16 bundled lock entries, in the same production gate as the 43 action
  flows.
- Fix lock predicate lowering so tail-expression lock results are preserved and
  `false` exits with a stable non-zero CKB script error.
- Complete the low-risk CellScript surface pass: canonicalize bundled example
  module names, capability declarations, field shorthand, typed `Vec<T>`
  literals, and the staged syntax RFC boundaries.
- Add create/struct field shorthand (`field` as `field: field`) and format
  redundant field initializers into shorthand form.
- Add contextual bounded `Vec<T>` literals for typed local bindings and
  create/struct field initializers, lowering to the existing stack collection
  constructor and push path without changing untyped array literal semantics.
- Add lock-boundary surface syntax for `protected` Cell parameters, `witness`
  data parameters, and `require` fail-closed predicates; reserve `lock_args`
  until explicit CKB script-args binding is implemented.
- Keep signer authority out of the 0.13 syntax surface: no implicit `Address`
  signer semantics, no hidden sighash defaults, and no first-class signer values
  before explicit CKB signature verification primitives.
- Split bundled examples into clean business examples and profiled acceptance
  examples, so scheduler/effect hints stay in release evidence without
  crowding the canonical teaching surface.
- Refresh LSP completions and the VS Code grammar/snippets for the new
  lock-boundary syntax.

## 0.12.0 - 2026-04-24

- Add a stable CellScript runtime error registry and expose code/name/hint
  entries through metadata and `cellc constraints`.
- Add CKB Blake2b builder/release helpers with pinned `ckb-default-hash`
  vectors through `cellc ckb-hash`.
- Add manifest-level CKB `hash_type` and `cell_deps`/DepGroup reporting, plus
  structured timelock and capacity evidence contracts.
- Add the standalone `tools/ckb-tx-measure` helper for CKB packed transaction
  size and occupied-capacity evidence, with CKB acceptance building the same
  source through a generated manifest for nested checkouts.
- Add `cellc abi`, `cellc scheduler-plan`, and `cellc opt-report` for entry
  witness inspection, scheduler-hint consumption, and optimization measurement.
- Use CKB Blake2b hashes for compiler metadata and release evidence.
- Expand entry witness tests to cover scalar, fixed-byte, `Vec<Address>`,
  `Vec<Hash>`, opaque nested `Vec<Vec<u8>>`, `Vec<u8>`, missing payload, and
  wrong-width payload cases.
- Add 0.12 production documentation for runtime errors, CKB authoring,
  deployment manifests, capacity, entry witnesses, collections, mutate,
  linear ownership, scheduler hints, migration, examples, and release evidence.
- Keep crates.io package contents narrow by excluding workflow, docs, editor,
  auxiliary tool directories, and unpublished helper binaries from the
  published crate.

## 0.11.0 - 2026-04-23

- Release CellScript 0.11.0 as the standalone CKB compiler package.
- Close the current CKB bundled-example production acceptance suite: all seven
  production examples strict-admit, all 43 actions and 16 locks strict-compile,
  and every bundled business action has an original-scoped on-chain production
  harness. Lock coverage is scoped compile coverage; `registry.cell` remains a
  compiler/tooling language example outside this production action matrix.
- Keep compatibility intact while documenting the remaining
  production hardening track around action builders, malformed matrices, and
  measured mass/cycle constraints.
- Preserve the production safety gates added in the 2026-04-23 development
  log: no CKB policy bypass, no unresolved-call ELF stubs, audit-only
  Wasm, tightened backend shape reporting, narrowed crates.io packaging, and
  explicit profile-aware constraints metadata.
- Promote the VS Code extension to production-grade local tooling with
  compiler-backed validation, formatting, scratch compilation, metadata and
  constraints reports, CKB target-profile arguments, status feedback, and stricter
  extension validation.

## 2026-04-23

- Marked Wasm output as audit-only instead of metadata-only production output.
- Removed the old ELF feature surface from runtime metadata.
- Reduced crates.io package contents by excluding GitHub workflow, wiki, and
  VS Code extension packaging files.
- Cleaned remaining clippy mechanical warnings and documented the intentional
  broad compiler-helper signature allowances so `cargo clippy --locked
  --all-targets -- -D warnings` is a release gate.
- Removed the remaining artifact-validation surface by returning a
  source-free `ValidatedArtifact` for metadata verification instead of building
  a synthetic AST.
- Kept scheduler witness metadata Molecule-only.
- Marked Wasm report output as audit-only and excluded standalone docs from the
  crates.io package contents.
- Stripped externally-linked RISC-V ELF artifacts when an external toolchain is
  available, matching the internal production artifact surface more closely.
- Made external RISC-V toolchains explicit opt-in via `CELLSCRIPT_RISCV_CC` or
  `CELLSCRIPT_RISCV_AS`/`CELLSCRIPT_RISCV_LD`, so production ELF output and
  backend shape budgets no longer depend on tools accidentally present in PATH.
- Hardened those external toolchain overrides to require absolute paths to
  existing executable files instead of accepting relative command names.
- Rebased the multisig bundled-example ELF budget on the deterministic internal
  ELF artifact size while keeping the assembly text/CFG budgets unchanged.
- Removed the executable Wasm pseudo-lowering path; the Wasm module now remains
  audit-only and rejects action/function modules instead of emitting approximate
  code.
- Removed empty module doc comments and simplified duplicated verifier branches
  reported by clippy.
- Kept lifecycle state storage explicit in cell data while allowing lifecycle
  state names in `create` initializers and qualified expressions such as
  `Ticket::Active`, avoiding hidden layout changes and numeric state
  boilerplate.
- Added LSP completions for qualified lifecycle states such as `Ticket::Active`.
- Clarified README CLI docs that `cellc test` is a compiler/policy harness, not
  trusted runtime execution.
- Removed the old CKB acceptance policy exception path so the CKB target
  profile now rejects unsupported CKB artifacts through the normal production policy
  gate.
- Removed unresolved-call ELF stub generation; production ELF emission now
  fails when a generated call target has not been lowered.
- Added executable cross-module callable linking for resolver-backed imports,
  so `launch.cell` links the real `seed_pool` callee and its transitive `isqrt`
  helper instead of relying on a synthetic fail-closed stub.
- Tightened launch example regression coverage to ensure imported callees are
  linked without pulling unrelated AMM actions into the artifact.
- Added `env::current_timepoint()` as a chain-neutral runtime time source:
  CKB lowers it to header epoch number.
- Switched bundled `vesting.cell` to the chain-neutral timepoint API, allowing
  original scoped `grant_vesting` artifacts under the CKB target profile.
- Added original scoped CKB on-chain acceptance for
  `vesting.cell::grant_vesting` with real Token/VestingConfig inputs,
  VestingGrant output verification, header dependency timepoint input, and
  malformed output rejection.
- Marked dynamic Molecule vector `len()` results as verifier-covered u64
  transition sources, so `collection.total_supply += recipients.len()` style
  CKB mutations are checked at runtime instead of reported as mutable-cell
  transition blockers.
- Fixed fixed-aggregate field byte-source lowering so original CKB verifier
  output lock checks can compare tuple-array address fields without fail-closed
  traps.
- Increased verifier expression temp slots and added regression coverage for
  the original launch bootstrap eight-recipient remaining-output sum.
- Switched CKB acceptance launch coverage from a standalone synthetic harness to
  the original scoped launch bootstrap artifact.
- Fixed dynamic Molecule table create-output checks for fixed/scalar fields so
  original `multisig.cell::create_wallet` verifies table fields through
  Molecule offsets instead of fixed-struct offsets.
- Switched the CKB multisig `create_wallet` acceptance harness to the original
  scoped artifact with dynamic `Vec<Address>` signer data.
- Preserved scalar verifier values across expected-expression evaluation and
  dynamic output decoding, fixing original `multisig.cell::propose_transfer`
  CKB checks for `Proposal.proposal_id` and `MultisigWallet.nonce`.
- Switched the CKB multisig `propose_transfer` acceptance harness to the
  original scoped artifact with dynamic `MultisigWallet` and `Proposal`
  Molecule table data.
- Switched CKB multisig `add_signature`, `propose_add_signer`,
  `propose_remove_signer`, and `propose_change_threshold` acceptance to
  original scoped artifacts with dynamic `Proposal` table/vector data.
- Switched CKB multisig `execute_proposal` and `cancel_proposal` acceptance to
  original scoped artifacts, removing the last standalone on-chain action
  harnesses from the bounded CKB matrix.
- Fixed destroy lowering to retain consumed input pointers for post-destroy
  output verification while relying on the checked Output absence scan for the
  actual destroy rule.
- Fixed scalar output verification to prefer schema/prelude expression sources
  but use runtime stack values for ordinary scalar variables, covering
  branch/match-derived bool outputs such as `ExecutionRecord.success`.
- Switched CKB token `mint`, `transfer_token`, `burn`, and `merge` acceptance
  from standalone harness sources to original scoped `token.cell` artifacts.
- Switched CKB NFT non-batch action acceptance from standalone harness sources
  to original scoped `nft.cell` artifacts, including dynamic `Collection`
  Molecule table data for `mint`.
- Switched CKB timelock `create_absolute_lock`, `create_relative_lock`,
  `lock_asset`, `request_release`, `request_emergency_release`, and
  `approve_emergency_release`, `execute_release`, `execute_emergency_release`,
  and `extend_lock` acceptance from standalone harness sources to original
  scoped `timelock.cell` artifacts.
- Fixed the CKB Molecule vector append verifier to compare fixvec payload
  bytes after the 4-byte count header, enabling original dynamic approval-list
  append checks.
- Switched CKB AMM pure-entry `isqrt` and `min` acceptance from standalone
  harness sources to original scoped `amm_pool.cell` artifacts.

## 2026-04-22

- Tightened backend CFG reachability analysis so unreachable-block metrics are rooted at the selected ELF entry label instead of treating every `.global` text symbol as reachable.
- Added a regression test proving unused global exports are still counted as unreachable from the entry root.
- Removed old `global_text_labels` parser storage after entry-root reachability replaced global-root reachability.
- Rebased bundled-example unreachable-block budgets on the stricter entry-root metric while keeping call-edge and CFG shape budgets enforced.
- Declared Rust 1.85.0 as the standalone crate MSRV so CI and users run with Cargo support for Edition 2024 dependencies.
- Updated standalone CI to archive backend-shape reports as release evidence.
- Added a committed standalone `Cargo.lock` and changed standalone CI to run with `--locked`.
