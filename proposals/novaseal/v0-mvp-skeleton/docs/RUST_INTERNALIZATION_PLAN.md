# NovaSeal Rust Removal And Internalization Plan

**Status**: first internalization slice implemented in compiler surface; Rust
verifier crates intentionally retained until replacement evidence is equal or
better.
**Scope**: remove NovaSeal-owned Rust verifier crates from the proposal tree by
internalizing the generic BTC verifier capability into CellScript, while keeping
NovaSeal itself outside the stable standard library surface.

## Goal

NovaSeal currently carries Rust verifier code under:

```text
verifier/novaseal_btc_verifier
verifier/novaseal_btc_verifier_core
verifier/novaseal_btc_verifier_riscv
```

That code is valuable, but most of it is not truly NovaSeal-specific. The core
capability is a BTC/BIP340 verifier with a fixed IPC envelope and CKB-VM spawn
execution model.

The separate Rust crate under `harness/ckb_vm` is not deployable verifier code.
It is local evidence-generation tooling that constructs transactions, executes
CKB VM/script verification, and writes reports. Its presence does not mean the
NovaSeal runtime path is still Rust-owned.

The target shape is:

```text
CellScript-owned generic BTC verifier capability
  -> used by NovaSeal
  -> later extractable as an independent package/crate
```

NovaSeal should become a protocol capsule that depends on this generic verifier
surface instead of owning the verifier implementation.

## Non-Goals

- Do not add `std::novaseal`.
- Do not make NovaSeal part of the stable CellScript stdlib.
- Do not hide the verifier as compiler magic.
- Do not claim production readiness just because the verifier moved.
- Do not delete existing NovaSeal evidence until the new internalized path has
  equal or better evidence.
- Do not treat `harness/ckb_vm` as runtime surface; it may stay as evidence
  tooling while the `.cell` runtime remains free of NovaSeal-specific Rust
  verifier plumbing.

## Namespace And Registry Policy

Use the 0.20 split as precedent:

- `xudt` is a CKB standard script wrapper, so it can live under the CKB stdlib
  protocol surface.
- iCKB is a concrete protocol equivalence track, so it stays under benchmarks,
  evidence, fixtures, and claim manifests instead of becoming `std::ickb`.

Apply the same rule, but keep one extra distinction for verifier artifacts:

```text
btc::...                    optional source-level BTC verifier facade
verifier::btc::bip340       verifier capability namespace
cellscript-labs/btc-bip340
                            registry package identity for the verifier artifact
protocol::novaseal::v0      NovaSeal protocol capsule
```

Do not use `std::btc::bip340` as the canonical registry or deployment identity.
`std` is too easy to read as "compiler-bundled and stable". A source-level
facade may eventually re-export the capability, but the deployable artifact
should be identified as a verifier/runtime dependency.

The registry model should distinguish package intent, build output, and
deployment facts. For this work, distinguish at least these artifact roles:

```text
contract            primary lock/type script package
library             source-only or compile-time helper package
spawn-verifier      executable runtime verifier called through spawn/IPC
protocol-capsule    protocol package that owns schemas, fixtures, and policy
tooling             off-chain generator, harness, or evidence tool
```

NovaSeal should depend on a `spawn-verifier` package, not on a `std` package:

```toml
[[runtime_dependencies]]
id = "btc.bip340.v0"
kind = "spawn-verifier"
namespace = "cellscript-labs"
name = "btc-bip340"
version = "0.1.0"
binding = "CellDep#0"
ipc_abi = "cellscript-btc-bip340-ipc-v0"
```

The corresponding deployment record should then bind this runtime dependency to
real chain facts:

```toml
[[deployments]]
artifact_role = "spawn-verifier"
verifier_id = "btc.bip340.v0"
network = "ckb-testnet"
script_role = "helper"
out_point = "0x...:0"
code_hash = "0x..."
hash_type = "data1"
data_hash = "0x..."
dep_type = "code"
```

Rust-side organization should separate the source facade from the deployable
verifier artifact:

```text
src/btc/                         source-level BTC facade metadata
src/verifier_registry/btc.rs     CKB spawn target, artifact, and ABI registry
crates/cellscript-btc-core/     no_std BIP340/IPC core, internal workspace crate
crates/cellscript-btc-riscv/    RISC-V verifier shell, internal workspace crate
tests/support/btc_verifier/     reusable CKB-VM verifier harness utilities
protocol_capsules/novaseal/     NovaSeal-specific manifest/evidence adapter
```

The public source-level calls should be explicit about verifier semantics:

```cellscript
verifier::btc::bip340::require_signature(message, pubkey, signature)
```

If the syntax later wants a shorter alias, `btc::bip340::require_signature(...)`
can be a facade over the same verifier dependency. The registry/deployment
identity should remain `cellscript-labs/btc-bip340` until the verifier is
promoted to the curated production-capable verifier namespace.

The capability descriptor should report:

```text
name = "verifier::btc::bip340"
registry_package = "cellscript-labs/btc-bip340"
stability = "runtime-backed-experimental"
tcb = "spawned-riscv-verifier"
artifact_role = "spawn-verifier"
production = false
```

This keeps the capability reusable without pretending it is already stable.

### UX Rule: Manifest-Scoped Alias First

The safest user-facing surface is not a globally available module. A verifier
should become callable only after the package declares a local alias for it.
That avoids polluting `std`, `btc`, or any global namespace and keeps the
runtime dependency visible at the call site.

Preferred eventual package shape:

```toml
[verifiers.btc_bip340]
package = "cellscript-labs/btc-bip340"
version = "0.1.0"
kind = "spawn-verifier"
stability = "experimental"
binding = "CellDep#0"
ipc_abi = "cellscript-btc-bip340-ipc-v0"
```

Preferred eventual source shape:

```cellscript
use verifier btc_bip340

lock btc_authority(...) -> bool {
    btc_bip340::require_signature(digest, sig.pubkey, sig.signature)
    true
}
```

The alias `btc_bip340` is local to the package. Another package could choose a
different alias, and the registry/deployment identity would still be the
package hash + build hash + deployed verifier Cell facts.

Until that syntax exists, `verifier::btc::bip340::require_signature(...)` may be
used as an internal transitional spelling. It should not be documented as the
long-term ergonomic source API.

### UX Rule: Do Not Hide External Effects

A verifier call is not a pure stdlib helper. The compiler and tooling should
make these facts visible:

- it spawns or otherwise invokes an executable runtime verifier;
- it requires a declared runtime dependency;
- it requires a builder to place the correct CellDep;
- it depends on a deployed artifact identity, not just source code;
- it consumes cycles from an external verifier artifact;
- it is part of the TCB for the calling lock/type script.

Recommended diagnostics:

```text
external verifier 'btc_bip340' is not declared; add [verifiers.btc_bip340]
verifier 'btc_bip340' has no deployment binding for target profile ckb
verifier call requires CellDep#0 but builder evidence binds a different artifact
```

Recommended LSP hover text:

```text
btc_bip340::require_signature
runtime verifier call: spawns btc-bip340 verifier artifact through CellDep#0.
Not a pure stdlib function. Requires deployment identity and builder evidence.
```

Recommended ProofPlan features:

```text
runtime-verifier:btc_bip340:declared
runtime-verifier:btc_bip340:cell-dep-binding
runtime-verifier:btc_bip340:ipc-envelope
runtime-verifier:btc_bip340:exit-status
```

### Registry Naming Rule

Use conservative registry names until the verifier is production-audited.

Experimental/internal stage:

```text
cellscript-labs/btc-bip340
```

Curated production-capable stage:

```text
cellscript-verifiers/btc-bip340
```

Avoid:

```text
std/btc-bip340
cellscript/std-btc
std::btc::bip340 as deployment identity
```

Those names imply a stable language standard library or compiler-bundled trust
boundary. The verifier may become widely reused without becoming part of the
language standard library.

### API Naming Rule

Prefer fail-closed verbs for spawned verifier APIs:

```cellscript
btc_bip340::require_signature(...)
```

Avoid first exposing:

```cellscript
btc_bip340::verify(...) -> bool
```

Returning a boolean makes the call look like a cheap pure predicate and creates
room for callers to accidentally ignore or branch around the result. A future
boolean API can exist only after the effect system and diagnostics clearly mark
it as a runtime verifier call.

## Target Architecture

### 1. Generic BTC Verifier Core

Move the reusable no-std code from:

```text
proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_core
```

to an internal CellScript workspace crate:

```text
crates/cellscript-btc-core
```

The crate must not mention NovaSeal. It should expose only generic types and
functions:

```text
Bip340PublicKey
Bip340Signature
BtcVerifierRequestV0
BtcVerifierResult
parse_bip340_ipc_request(...)
verify_bip340(...)
```

The current `NSBV0IPC` envelope should be renamed before promotion. Suggested
generic name:

```text
CSBTCV0
```

The old NovaSeal envelope can remain accepted behind an explicit compatibility
flag while migration is underway, but new evidence should be generated against
the generic envelope.

### 2. Generic RISC-V Verifier Shell

Move the reusable shell from:

```text
proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_riscv
```

to:

```text
crates/cellscript-btc-riscv
```

The produced binary should be named generically:

```text
cellscript_btc_bip340_verifier_riscv
```

CellScript metadata should treat it as a runtime verifier artifact with a
declared ABI:

```text
verifier_id = "cellscript-btc-bip340-v0"
artifact_role = "spawn-verifier"
spawn_target = "CellDep#0"
ipc_abi = "cellscript-btc-bip340-ipc-v0"
scheme = "bip340_schnorr_secp256k1"
```

NovaSeal may pin that verifier ID in its capsule manifest, but the verifier
must not depend on NovaSeal schemas, receipts, policies, or fixture names.

### 3. Host Vector Tooling

Move the host verifier from:

```text
verifier/novaseal_btc_verifier
```

to either:

```text
tools/btc-verifier-vectors
```

or an internal test binary under:

```text
tests/support/btc_verifier
```

This tool should verify generic BIP340 vectors and generic IPC envelope vectors.
NovaSeal-specific vector generation should stay in the NovaSeal capsule as an
adapter that produces generic BTC verifier input vectors.

### 4. CKB-VM Harness

Split the current NovaSeal harness into two layers:

```text
tests/support/btc_verifier_harness/
  child verifier CKB-VM runner
  parent spawn/pipe/wait runner
  resolved transaction verifier utilities

protocol_capsules/novaseal/harness/
  fixture adapter
  NovaSeal witness adapter
  NovaSeal lock/type expected-outcome matrix
```

The generic harness should know about:

- spawned verifier ELF,
- inherited fd input,
- pipe/write/wait/close behavior,
- `cell_deps[0]` spawn target binding,
- CKB-VM cycles,
- resolved transaction construction.

It should not know about:

- `NovaSealCellV0`,
- `ProofReceiptV0`,
- `policy_hash`,
- `receipt_hash`,
- NovaSeal fixture names.

### 5. CellScript DSL Surface

Add a generic verifier descriptor similar to 0.20 `ckb_protocols`, but keep it
outside the stable stdlib namespace and mark it experimental:

```rust
VerifierCapability {
    name: "verifier::btc::bip340",
    registry_package: "cellscript-labs/btc-bip340",
    stability: "runtime-backed-experimental",
    artifact_role: "spawn-verifier",
    proof_plan_trigger: "lock_or_type_group",
    proof_plan_scope: "selected_group",
    builder_assumptions: [
        "btc-bip340-verifier-cell-dep-available",
        "spawn-target-cell-dep0-bound",
        "ipc-envelope-version-bound"
    ],
}
```

The DSL call:

```cellscript
verifier::btc::bip340::require_signature(message, pubkey, signature)
```

should lower to the same generic spawn/IPC primitives already proven by the
NovaSeal branch:

```text
pipe
spawn_with_fd("cellscript_btc_bip340_verifier_riscv", read_fd)
pipe_write fixed envelope words
wait
require status == 0
```

Generated ProofPlan records should be generic:

```text
verifier:btc-bip340:signature
verifier:btc-bip340:spawn-target
verifier:btc-bip340:ipc-envelope
verifier:btc-bip340:exit-status
```

They must not say `NovaSeal`.

### 6. NovaSeal Capsule After Internalization

After the BTC verifier moves, NovaSeal should keep only:

```text
src/nova_state_type.cell
src/nova_btc_authority_lock.cell
src/nova_receipt_type.cell
schemas/
fixtures/
proofs/
docs/
scripts/novaseal_* adapters
```

Its lock should become smaller and protocol-specific:

```cellscript
lock btc_authority(
    protected cell: NovaSealCellV0,
    lock_args expected_btc_authority_hash: Hash,
    witness intent: NovaSealSignedIntentV0,
    witness state_hash_commitment: Hash,
    witness sig: SignaturePayload
) -> bool {
    let digest = hash_blake2b_packed(intent)
    require expected_btc_authority_hash == cell.btc_authority_hash
    require intent.core.policy_hash == cell.policy_hash
    verifier::btc::bip340::require_signature(digest, sig.pubkey, sig.signature)
    true
}
```

NovaSeal still owns:

- intent preimage rules,
- receipt semantics,
- state transition policy,
- fixture matrix,
- production acceptance criteria.

CellScript owns:

- BIP340 verification,
- verifier IPC envelope,
- RISC-V verifier artifact,
- spawn target metadata,
- generic CKB-VM verifier evidence.

## Migration Phases

### Phase 0: Freeze Current Evidence

Before moving code, record the current NovaSeal evidence snapshot:

- audit bundle summary,
- BTC verifier vector report,
- IPC vector report,
- child verifier CKB-VM report,
- parent lock report,
- combined transaction report,
- staged verifier ELF hash.

No code should be deleted in this phase.

### Phase 1: Introduce Generic BTC Verifier Names Beside Existing NovaSeal Names

Add generic aliases while keeping the old NovaSeal verifier crates intact:

```text
cellscript_btc_bip340_verifier_riscv
cellscript-btc-bip340-ipc-v0
verifier::btc::bip340::require_signature
cellscript-labs/btc-bip340
```

The old `novaseal_btc_verifier_riscv` name remains as a compatibility target.

Acceptance:

- old NovaSeal harness still passes;
- new generic BTC vector tests pass;
- generated ProofPlan contains generic BTC records;
- no `NovaSeal` string appears in generic BTC verifier modules.

Current implementation slice:

- `verifier::btc::bip340::require_signature(...)` is accepted by the type
  checker as a runtime verifier call;
- the IR lowers it to the existing generic spawn/IPC primitives;
- metadata and ProofPlan records use generic `btc-bip340` verifier labels;
- NovaSeal source calls the generic helper instead of hand-writing the 18-word
  envelope.

### Phase 2: Move Core And Shell Into CellScript-Owned Crates

Move no-std core and RISC-V shell into internal workspace crates.

Acceptance:

- RISC-V target check passes;
- RISC-V shell builds in debug and release;
- staged release ELF hash is recorded;
- BTC vector and IPC vector counts remain equal or larger than the NovaSeal
  baseline.

### Phase 3: Make NovaSeal Use The Generic BTC Verifier Capability

Replace hand-written pipe/spawn/write/wait code in NovaSeal locks with the
generic BTC helper.

Acceptance:

- `cellc check --target-profile ckb` passes;
- `cellc check --target-profile ckb --primitive-strict 0.16` passes for the current NovaSeal core package;
- NovaSeal audit bundle shows `verifier:btc-bip340:*` ProofPlan records;
- combined eleven-fixture lock+type harness still accepts 1 and rejects 10;
- max cycle regression is recorded and explained.

### Phase 4: Retire Proposal-Owned Rust Verifier Crates

Delete or archive:

```text
proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier
proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_core
proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_riscv
```

Keep NovaSeal-specific harness adapters only if they are fixture adapters, not
generic verifier implementations.

Acceptance:

- no NovaSeal Rust verifier crate remains under the proposal tree;
- NovaSeal evidence is regenerated through the CellScript-owned BTC verifier;
- docs no longer identify NovaSeal as the owner of the BIP340 TCB.

### Phase 5: Prepare Future Extraction

Once the generic BTC verifier stabilizes, it can be split out without changing
NovaSeal source:

```text
cellscript-btc-core
cellscript-btc-riscv
cellscript-btc-harness
```

The contract between CellScript and the extracted package should already be the
same manifest-driven verifier contract used internally:

```text
verifier_id
ipc_abi
artifact_hash
spawn_target_binding
proof_plan_features
fixture_matrix_hash
```

## Production Gates After Internalization

Internalization is not production readiness. Production claims still require:

1. live/full-node or accepted dry-run evidence;
2. deployment identity for the BTC verifier cell dep;
3. stable fixed-width wallet signing vectors;
4. public/shared deployment attestation for the materialised receipt output
   shape;
5. artifact hash and source hash provenance;
6. external review of the BTC verifier TCB;
7. claim manifest that distinguishes generated audit coverage from external
   harness evidence.

## Implemented First Patch

The first implementation patch is intentionally small:

1. Add the generic `verifier::btc::bip340` descriptor with
   `runtime-backed-experimental` stability and `artifact_role =
   "spawn-verifier"`.
2. Add a verifier registry entry for `cellscript-labs/btc-bip340`.
3. Add `verifier::btc::bip340::require_signature(...)` lowering as a wrapper
   over the existing spawn/IPC primitive shape.
4. Keep the existing NovaSeal verifier crates untouched.
5. Add a test that compiles a tiny lock using
   `verifier::btc::bip340::require_signature` and asserts that ProofPlan records
   are generic verifier records, not NovaSeal records.

That wrapper is now proven by compiler tests, strict package checks, the
NovaSeal audit surface, and the spawn backend probe. The current NovaSeal slice
also materialises `ProofReceiptV0` as `Output#1` and attaches combined harness
cycle/tx-size/capacity measurements. The next step is Phase 2: move the no-std
core and RISC-V shell into CellScript-owned crates without dropping any existing
evidence.
