# CellScript Package Provenance and Deployment Identity

**Status**: implementation contract for the current CellScript CKB profile.
Phase 1 landed in the 0.19 line; Phase 2 source-package, generated-builder,
deployment identity, and trust-metadata checks extend through 0.20 and the
0.21 RC.

**Scope**: Source package registry, deployment registry, lockfile binding, and
builder verification for CellScript on CKB

**Historical scope anchors**: v0.12 stable developer surface, v0.17 CKB
protocol semantics, and v0.18 first-class ScriptRef / ScriptArgs work.

**Forum thread**: <https://talk.nervos.org/t/cellscript-package-and-deployment-registry-early-design-discussion/10210>

**Production boundary ADR**:
[`CELLSCRIPT_REGISTRY_PRODUCTION_BOUNDARY_ADR.md`](CELLSCRIPT_REGISTRY_PRODUCTION_BOUNDARY_ADR.md)

## Motivation

For ordinary development, a package registry can look like crates.io or npm:
resolve a package name and version, download source, build it, and use it.

For smart contracts, that is not enough.

A production CellScript dependency eventually needs to answer questions such as:

1. Which source package was used?
2. Which compiler version produced the artifact?
3. What schema and ABI commitments were used?
4. What constraints report was generated?
5. What exact RISC-V artifact was deployed?
6. Which CKB CellDep, OutPoint, data_hash, dep_type, lock/type identity, or
   type-id lineage corresponds to that artifact?
7. Can a wallet or builder verify that the package used in a transaction is the
   same one the developer intended?

A source package version is useful for development, but production use also needs
deployment truth.

## Core Principle

> CellScript packages should be distributed like development packages, but
> verified like smart-contract deployments.

The off-chain registry optimizes for source distribution and developer
experience. CKB records only compact, verifiable deployment truth where it is
actually useful. The lockfile binds the two.

## Profile Compatibility Boundary

`namespace/name/version` is a stable naming layer, not proof that every named
object is a CellScript source package. The current Phase 1 implementation uses
one concrete profile:

```text
cellscript_source_package_v1
  carrier: Cell.toml + registry.json + Cell.lock + Deployed.toml
  resolver: cellc package/dependency resolver
  source hash: Cell.toml plus .cell source roots and explicit entry parent
  deployment identity: CKB script cell facts when the package is deployed
```

Future registry services may discover other CKB ecosystem artifacts under the
same naming convention, such as verifier binaries, deployed script records,
profile libraries, or reproducible build outputs like `ckb-bootstrapper`.
Those objects must use explicit artifact profiles. They must not be silently
accepted by the CellScript package resolver merely because they have a
`namespace/name` and a Git URL.

The compatibility rule is:

```text
Discovery can be broad.
Resolution is profile-specific.
No resolver may coerce one profile into another.
```

For backward compatibility, a registry record without an explicit future
profile is interpreted by current `cellc` commands as a CellScript source
package candidate and must still satisfy the existing `Cell.toml`,
`registry.json`, source-hash, build-identity, and deployment-identity checks.
A future registry proxy or discovery index may expose multiple profiles for the
same `namespace/name`, but the lockfile must record which profile was selected.

## Publisher Identity Model

CellScript Registry uses a **JoyID-rooted publisher identity** without a
separate registry account system. The current accepted publisher principal type
is `joyid_ckb`; ordinary publish operations use a delegated local credential:

```text
principal_type = joyid_ckb
principal_id = <normalized JoyID-CKB identity binding>

JoyID identity
  -> root publisher principal
  -> authorises local publisher credential
  -> credential signs scoped registry requests
```

The data model stays principal-typed instead of hard-coding product policy into
every record. The current registry policy accepts only `joyid_ckb`, while the
record shape still names the principal type and concrete principal id.

The preferred `principal_id` is derived from the JoyID signer key as a
normalized JoyID-CKB identity binding. The registry verifies that the JoyID
signature's key type and public key match the `principal_id` in capability and
revocation payloads; display addresses are presentation data only.

The intended interactive flow is:

```text
cellc auth capability create --principal-id <principal_id> --scope publish:namespace/package --expires 90d --json > capability-payload.json
  -> CLI creates a registry signing key and stores it in the OS keychain
  -> CLI prints an authorize_capability payload with capability_pubkey and requested scopes
  -> browser/CCC/JoyID signs that exact payload
cellc auth capability submit --payload capability-payload.json --joyid-signature joyid-signature.json
  -> signed payload is submitted to the registry write API
  -> registry records principal_type, principal_id, scopes, expiry, and revocation status
```

Daily publishing then avoids JoyID signing prompts and never exposes root
publisher authority to CI:

```text
cellc publish
  -> signs the concrete publish payload with the local publisher credential
  -> registry verifies signature, nonce, expiry, origin, and ACL scope
  -> registry accepts the entry and returns its canonical URL
```

The JoyID authorisation payload must bind the local capability key:

```text
protocol: cellscript-registry-auth-v1
action: authorize_capability
registry_origin: https://api.registry.cellscript.dev
principal_type: joyid_ckb
principal_id: <normalized JoyID-CKB identity binding>
capability_pubkey: ...
requested_scopes: [publish:cellscript/amm_pool]
capability_expires_at: ...
nonce: ...
issued_at: ...
expires_at: ...
cli_version: ...
```

The daily publish payload must bind the action and package identity:

```text
action: publish
namespace: cellscript
package: amm_pool
version: 1.2.0
source_hash: ...
manifest_hash: ...
registry_origin: https://api.registry.cellscript.dev
nonce: ...
expires_at: ...
```

The central ACL model is namespace/package ownership:

```text
namespace -> owner principals
package   -> maintainer principals
credential -> scoped permissions
```

Example scopes:

```text
publish:cellscript/amm_pool
yank:cellscript/amm_pool
attest:cellscript/amm_pool
manage-maintainers:cellscript/*
```

This keeps the user-facing identity simple — "my JoyID is my CellScript
publisher identity" — while the engineering surface remains revocable, scoped,
CI-safe, and auditable.

## Three-Layer Identity Model

CellScript packages exist in three identity layers, each with a distinct
verification scope:

```
┌─────────────────────────────────────────────────────────────┐
│  Package Identity                                           │
│  namespace / name / version / source_hash                  │
│  Carrier: Cell.toml + source registry index                │
│  Verified: compile time                                     │
├─────────────────────────────────────────────────────────────┤
│  Build Identity                                             │
│  compiler_version / metadata_schema / schema_hash /        │
│  abi_hash / artifact_hash / constraints_hash                │
│  Carrier: Cell.lock [package.build]                        │
│  Verified: build time                                       │
├─────────────────────────────────────────────────────────────┤
│  Deployment Identity                                        │
│  chain / network / code_cell / out_point / data_hash /      │
│  dep_type / type_id_lineage / script_role                   │
│  Carrier: Deployed.toml                                     │
│  Verified: runtime / production                             │
└─────────────────────────────────────────────────────────────┘
```

Each layer is independently meaningful but cryptographically bound to the
layers above and below through the lockfile.

### Package States and Visibility

A CellScript package can exist in several operational states:

**Source-published / indexed-pending package.** `cellc publish` has admitted a
new version into the registry. The entry is addressable by direct URL and
visible to the author, but it may be excluded from default search until basic
schema, hash, quota, and abuse checks pass.

**Source-only / undeployed package.** A normal development package containing
`.cell` source files, interfaces, schemas, docs, tests, examples, and
reproducible build metadata. It can be imported, compiled, tested, audited, and
used as a library dependency. However, it does not by itself claim any
production deployment identity on CKB.

**Deployment-bound package.** A package version whose built artifact has been
deployed, and whose deployment identity can be verified. For CKB, this means
binding the package version to facts such as CellDep, OutPoint, data_hash,
dep_type, script/code hash, schema/ABI commitments, constraints report,
compiler version, and possibly type-id lineage.

A deployment-bound package is what wallets and production builders should rely
on when constructing real transactions.

**On-chain-attested package.** A deployment claim has an explicit JoyID-rooted
attestation or chain-indexed record. This is a stronger statement about who made
the deployment claim, but it still does not replace source, build, deployment,
and live-chain verification.

**Deprecated, yanked, or quarantined package.** Historical entries remain
addressable for reproducibility, but default search and recommendation surfaces
may suppress them. Quarantine is for abuse or high-risk packages; yanking is a
maintainer action that preserves exact-pin warning metadata.

The same source package version may have zero, one, or many deployment
bindings. For example, `amm@1.2.0` may start as a source-only package, later
gain a CKB testnet deployment, then eventually a CKB mainnet deployment. These
are separate deployment records attached to the same source/package identity,
not separate source packages.

```
amm@1.2.0
  ├─ source:  blake2b:0xabcd...
  ├─ build:   artifact=0x1234... abi=0xdef0...
  ├─ deployed:
  │   ├─ aggron4:  out_point=0xaaaa...:0  status=active
  │   └─ mainnet:  status=candidate
  └─ (same source version, multiple deployment bindings)
```

### Mixed Profile Cases

The following cases are intentionally different, even when they share a
namespace:

| Case | Correct modelling | Must not be modelled as |
|---|---|---|
| A CellScript package imports a CellScript library | `[dependencies]` entry resolved by the CellScript source-package profile | A generic artifact record |
| A CellScript package needs a deployed verifier CellDep | Deployment or verifier artifact profile with code/data hash, OutPoint, status, and ABI/IPC identity | A source library dependency merely because it lives in Git |
| A CellScript build uses an external reproducible tool output | Build-input or reproducible-artifact profile that records recipe hash, toolchain/input lock, and output hashes | A `.cell` source dependency |
| `ckb-bootstrapper` proves a CKB binary | Reproducible-binary profile with source, build recipe, pinned inputs, output binary hashes, and optional CKB commitment/index facts | `cellscript_source_package_v1` |
| A cookbook/template is copied and edited | Copy/scaffold flow; after copying, the files become local project source | A registry dependency that can affect the verified dependency graph |
| A generic artifact references a CellScript deployment | Artifact profile may cite CellScript package/deployment identity as evidence | Automatic cross-profile dependency resolution |

This keeps Janx's `ckb-bootstrapper` use case compatible with the architecture:
it can reuse the registry service, naming convention, proxy/cache layer, and
hash-bound identity chain, but it needs its own profile contract. That profile
would answer "which reproducible binary did this recipe produce?" rather than
"which `.cell` source package did `cellc` import?".

## Why Not Pure On-Chain Packages?

It is unlikely that publishing every CellScript source package directly to CKB
is the right default.

Source archives, docs, examples, tests, schema manifests, and editor metadata
are development artifacts, not consensus-critical state. Frequent package
releases would create unnecessary permanent state churn, and CKB capacity costs
make source-package storage especially unattractive.

The chain should probably record compact deployment facts and commitments, not
replace the whole source distribution system.

## Why Not Pure Off-Chain Packages?

A pure off-chain registry also seems insufficient.

For production CKB contracts, builders and wallets need concrete deployment
identity: CellDep, OutPoint, data_hash, dep_type, script/code hash checks,
schema/ABI commitments, and ideally provenance back to the source package,
compiler version, and constraints report.

A compromised or stale source registry should not be enough to trick a
production builder into using the wrong deployed artifact.

## File Responsibility Split

Inspired by Move/Sui's `Move.toml` / `Move.lock` / `Published.toml` separation,
but adapted to CKB's CellDep/OutPoint-based deployment model rather than Sui's
native package-object model.

### Cell.toml — Source Package Declaration (Extended)

`Cell.toml` gains a `namespace` field in `[package]` and a `namespace` field
in detailed dependencies. No other structural changes are required.

```toml
[package]
name = "amm_pool"
version = "1.2.0"
namespace = "cellscript"          # NEW: must match the module declaration
entry = "src/main.cell"

[dependencies]
# Simple: version-only, auto-resolve namespace from discovery index
token = "0.3.0"

# Detailed with explicit namespace (recommended for production)
token = { version = "0.3.0", namespace = "cellscript" }

# Path dependency (unchanged, bypasses registry)
token = { version = "0.3.0", path = "../token" }

# Git dependency (unchanged, bypasses registry)
token = { version = "0.3.0", git = "https://github.com/cellscript/token", tag = "v0.3.0" }

[build]
target_profile = "ckb"

[deploy.ckb]
hash_type = "data1"
dep_type = "code"

[[deploy.ckb.cell_deps]]
name = "secp256k1"
out_point = "0x...:0"
dep_type = "dep_group"
hash_type = "type"
```

#### `[package]` namespace Field

The `namespace` field in `[package]` must match the namespace used in the
module declaration. For a source file that begins with `module cellscript::amm_pool`,
the `[package]` section must have `namespace = "cellscript"`.

This field serves three purposes:

1. **Publishing**: `cellc publish` uses `namespace` to select the registry ACL
   scope and canonical `namespace/name` coordinate.
2. **Verification**: The resolver checks that the declared namespace matches
   the `module` declaration in source files.
3. **Ambiguity resolution**: When a `Simple` dependency (version-only string)
   matches packages in multiple namespaces, the resolver uses the consuming
   package's own namespace as the default.

If `namespace` is absent, the package is treated as a local-only package
that cannot be published to the registry.

#### Dependency Syntax and Registry Resolution

The dependency key (e.g., `token`) is a **local alias** used to identify the
dependency in the project. The resolver maps this alias to a registry package
through the `namespace` field:

| Syntax | Resolution |
|---|---|
| `token = "0.3.0"` | Auto-resolve: search discovery index for `token`; if ambiguous, default to the consuming package's namespace |
| `token = { version = "0.3.0", namespace = "cellscript" }` | Explicit: look up `cellscript/token` in discovery index |
| `token = { version = "0.3.0", path = "../token" }` | Local path, bypasses registry |
| `token = { version = "0.3.0", git = "...", tag = "v0.3.0" }` | Git clone, bypasses registry |

The resolution priority is: `path` > `git` > `registry`. If `path` or `git`
is specified, the dependency is resolved locally and the `namespace` field
is ignored for resolution (but may still be used for display purposes).

**Relationship to source `use` statements**: The dependency key is a local alias.
Source code references types via their full module path (e.g.,
`use cellscript::fungible_token::Token`). The resolver maps the dependency alias
`token` to the package whose `[package] name = "fungible_token"` and
`namespace = "cellscript"`, so that the `use` statement resolves correctly.

**Key invariant**: `Cell.toml` describes deployment *intents* (what hash_type
should be), not deployment *facts* (which specific out_point was deployed to).
Intents are determined at compile time; facts are determined after deployment.

### Cell.lock — Build Identity Lock (Extended)

The existing `Cell.lock` records dependency versions and sources. The registry
extension adds build identity hashes, deployment references, and enriches the
registry source type with git provenance.

**Lockfile schema**:

```toml
version = 1

[package]
name = "amm_pool"
version = "1.2.0"
namespace = "cellscript"
source_hash = "blake2b:0xabcd..."

[package.build]
compiler_version = "0.21.0"
target_profile = "ckb"
artifact_hash = "blake2b:0x1234..."
metadata_hash = "blake2b:0x5678..."
schema_hash = "blake2b:0x9abc..."
abi_hash = "blake2b:0xdef0..."
constraints_hash = "blake2b:0x1111..."

# Registry dependency — resolved from discovery index
[dependencies.token]
version = "0.3.0"
namespace = "cellscript"
source = { registry = "cellscript/token", url = "https://github.com/cellscript/token", revision = "a1b2c3d4..." }
source_hash = "blake2b:0x2222..."
build = { artifact_hash = "blake2b:0x3333...", abi_hash = "blake2b:0x4444..." }

# Path dependency (unchanged)
[dependencies.helper]
version = "0.1.0"
source = { path = "../helper" }
source_hash = "blake2b:0x5555..."

# Git dependency (unchanged)
[dependencies.legacy]
version = "1.0.0"
source = { git = "https://github.com/other/legacy", revision = "e5f6g7h8..." }
source_hash = "blake2b:0x6666..."

[deployment.ckb.aggron4]
status = "deployed"
record = "ckb-testnet:0x5678..."
record_hash = "blake2b:0x9a9a..."

[deployment.ckb.mainnet]
status = "undeployed"
```

#### LockedSource::Registry Extension

The existing `LockedSource::Registry { name, version }` is extended to carry
full git provenance, enabling re-verification without re-querying the
discovery index:

| Field | Purpose | Phase |
|---|---|---|
| `namespace` | Which namespace the package belongs to | Phase 1 |
| `registry` | Full registry path `namespace/name` | Phase 1 |
| `url` | Git repository URL (from discovery index) | Phase 1 |
| `revision` | Exact git commit hash | Phase 1 |
| `version` | Package version string | Phase 1 (existing) |

The `url` and `revision` fields make the lockfile self-sufficient for
re-verification: `cellc package verify` can clone the exact source commit
without re-querying the discovery index. This is analogous to how `go.sum`
records the exact module version and hash, making the `go.mod` file
independently verifiable.

The existing `LockedSource::Path { path }` and `LockedSource::Git { url, revision }`
are unchanged.

**Cross-file binding**: The `record` field references the deployment by network
and identifier. The `record_hash` field is the Blake2b-256 hash of the
corresponding `[[deployments]]` entry in `Deployed.toml`, serialized as
**canonical JSON** (not canonical TOML). TOML has no standardized canonical
serialization; JSON does. This is consistent with the existing `metadata_hash`
computation in `src/cli/commands.rs`, which uses `ckb_blake2b256(serde_json::to_vec(&metadata))`.

The `record_hash` computation:
1. Deserialize the `[[deployments]]` TOML entry into a Rust struct.
2. Serialize the struct to canonical JSON (`serde_json::to_string` with sorted
   keys, compact, no whitespace).
3. `record_hash = ckb_blake2b256(canonical_json_bytes)`.

Phase 1 makes `record_hash` optional: if present, `cellc registry verify`
checks that it matches the actual `Deployed.toml` entry; if absent, the
verification step is skipped with a warning. Future phases may require
`record_hash` for production packages.

**Backward compatibility**: The lockfile uses a single version 1 schema.
The `[package.build]` and `[deployment.*]` sections are optional; their absence
simply means the package has not been built or deployed yet.

**Key invariants**:

- `Cell.lock` is the cryptographic bind point between source and deployment.
- Any hash mismatch between `Cell.lock`, compiled artifacts, and `Deployed.toml`
  records causes fail-closed rejection.
- The `[deployment.*]` section references deployment records in `Deployed.toml`
  by network. It does not duplicate the full deployment facts; those live in
  `Deployed.toml`.
- Stale or mismatched artifact/metadata/deployment hashes fail closed.

### Deployed.toml — Deployment Fact Record (New)

`Deployed.toml` is the CKB analogue of Move/Sui's `Published.toml`. It is
automatically generated by the deployment tool after the on-chain transaction is
confirmed, and records immutable deployment facts derived from the chain.

#### Who Generates and Manages Deployed.toml

`Deployed.toml` is generated by the CellScript deployment tool (`cellscript-deploy`
or the adapter crate's `CellScriptAdapter::deploy_artifact()` API). It is not
hand-authored.

The generation path is trust-free by construction: the existing adapter crate
architecture is headless-first, meaning all deployment facts are computed
locally before the transaction is submitted, and the only chain-derived value
needed after submission is the `tx_hash`.

**Generation flow** (matches existing `deploy_artifact` → `build_deploy_transaction`
→ `build_deployment_manifest_from_evidence` pipeline):

```
1. cellc build
   → produces artifact, metadata, constraints, schema, ABI
   → all build hashes computed locally (artifact_hash, metadata_hash,
     schema_hash, abi_hash, constraints_hash)

2. build_deploy_transaction(spec)
   → headless: computes TYPE_ID args, data_hash, code_hash,
     occupied capacity, change output locally
   → returns (TransactionView, ResolvedDeployEvidence)
   → evidence already contains: code_hash, hash_type, type_id_args,
     artifact_hash, occupied_capacity, tx_size

3. submit + wait_for_commitment
   → sends transaction through full node RPC
   → waits for committed status
   → receives tx_hash from the node response

4. build_deployment_manifest_from_evidence(evidence, tx_hash, output_index)
   → constructs DeploymentManifest from locally-computed evidence + tx_hash
   → no get_transaction call needed: all hash fields already known
   → extends to Deployed.toml by adding network, chain_id, build section,
     and Cell.lock record_hash
```

**Why no `get_transaction` / on-chain re-derivation is needed**: The existing
adapter crate's `build_deploy_transaction` already computes `data_hash =
blake2b(artifact_binary)` locally (line 447 of `lib.rs`). The
`ResolvedDeployEvidence` already carries `code_hash`, `hash_type`, and
`type_id_args`. The only chain-derived value is `tx_hash`, which is returned
by `send_transaction`. The full node RPC is used for submission and commitment
waiting, not for re-deriving fields that the tool already knows.

**Verification path**: 0.19 Phase 1 verification is off-chain and checks that
`Deployed.toml` matches the package/build identity recorded in `Cell.lock`.
0.20 adds live-chain verification where `cellc registry verify --live` (or an
equivalent mode) calls `get_live_cell` to confirm that the on-chain code cell's
data matches `data_hash` in `Deployed.toml`. This separation keeps the trust
model clean: Phase 1 generation/verification is self-contained, while live
chain proof is independently reproducible when RPC is available.

**Data source requirement**: 0.19 Phase 1 registry acceptance does not require
a CKB full node RPC endpoint. Transaction submission, commitment waiting, and
`get_live_cell` verification are 0.20 live-chain concerns. Light client support
is a possible later enhancement.

**Immutability**: Once generated, `Deployed.toml` must not be modified. Any
re-deployment or upgrade produces a new `[[deployments]]` entry with a distinct
set of chain facts, not an edit to an existing entry.

```toml
version = 1

[package]
name = "amm_pool"
version = "1.2.0"
source_hash = "blake2b:0xabcd..."

[build]
compiler_version = "0.21.0"
artifact_hash = "blake2b:0x1234..."
metadata_hash = "blake2b:0x5678..."
schema_hash = "blake2b:0x9abc..."
abi_hash = "blake2b:0xdef0..."
constraints_hash = "blake2b:0x1111..."

[[deployments]]
network = "aggron4"
chain_id = "ckb-testnet"
script_role = "type"
tx_hash = "0xaaaa..."
output_index = 0
code_hash = "0xbbbb..."
hash_type = "data1"
dep_type = "code"
out_point = "0xaaaa...:0"
data_hash = "0xcccc..."
type_id = "0xdddd..."

[[deployments.cell_deps]]
name = "secp256k1"
tx_hash = "0xeeee..."
output_index = 1
dep_type = "dep_group"
hash_type = "type"

[[deployments]]
network = "ckb-mainnet"
chain_id = "ckb-mainnet"
script_role = "type"
status = "candidate"
```

**Relationship to existing `DeploymentManifest`**: The current
`DeploymentManifest` type in `crates/cellscript-ckb-adapter/src/lib.rs` has
`DeploymentRef` with `name/code_hash/hash_type/args/dep_type/out_point`.
`Deployed.toml` is an enhanced deployment manifest that adds:

- `network` and `chain_id` — which chain this deployment targets
- `script_role` — lock, type, dual-role, or helper dependency
- `data_hash` — the data hash of the deployed code cell
- `type_id` — TYPE_ID upgrade lineage where applicable
- `status` — deployment lifecycle state
- The full `[build]` section — binding the deployment to build identity

The adapter crate's `load_deployment_manifest` /
`parse_deployment_manifest` functions should be extended to support the new
schema while maintaining backward compatibility with the existing
`cellscript-ckb-deployment-manifest-v0.19` schema.

## End-to-End Package Lifecycle

This section traces a package through its complete lifecycle, showing how
`Cell.toml`, `registry.json`, `Cell.lock`, and `Deployed.toml` interact at
each stage.

### Stage 1: Authoring

A developer creates a new package:

```bash
cellc init amm_pool --namespace cellscript
```

This generates:

```toml
# Cell.toml
[package]
name = "amm_pool"
version = "0.1.0"
namespace = "cellscript"
entry = "src/main.cell"
```

Source code uses the module declaration consistent with the namespace:

```
// src/main.cell
module cellscript::amm_pool

use cellscript::fungible_token::Token
```

At this stage, there is no `Cell.lock`, no `registry.json`, and no
`Deployed.toml`. The package is purely local.

### Stage 2: Adding Dependencies

The developer adds a registry dependency:

```toml
# Cell.toml
[dependencies]
token = { version = "0.3.0", namespace = "cellscript" }
```

Running `cellc build` triggers dependency resolution:

1. Read `Cell.toml` `[dependencies]` → find `token` with `namespace = "cellscript"`.
2. Query the discovery index (`cellscript-registry` Git repo) →
   `cellscript/token.json` →
   `source = "https://github.com/cellscript/token"`.
3. Clone the source repo, find the latest `0.3.x` tag (e.g., `v0.3.2`).
4. Read `registry.json` from the cloned repo → verify `source_hash` matches.
5. Parse the dependency's `Cell.toml` → resolve transitive dependencies.
6. Write `Cell.lock` with resolved versions and git provenance.

Generated `Cell.lock`:

```toml
version = 1

[package]
name = "amm_pool"
version = "0.1.0"
namespace = "cellscript"
source_hash = "blake2b:0xabcd..."

[package.build]
compiler_version = "0.21.0"
target_profile = "ckb"
artifact_hash = "blake2b:0x1234..."
metadata_hash = "blake2b:0x5678..."
schema_hash = "blake2b:0x9abc..."
abi_hash = "blake2b:0xdef0..."
constraints_hash = "blake2b:0x1111..."

[dependencies.token]
version = "0.3.2"
namespace = "cellscript"
source = { registry = "cellscript/token", url = "https://github.com/cellscript/token", revision = "f7e8d9c0..." }
source_hash = "blake2b:0x2222..."
build = { artifact_hash = "blake2b:0x3333...", abi_hash = "blake2b:0x4444..." }
```

Key property: `Cell.lock` is **self-sufficient** for re-verification. The `url`
and `revision` fields allow `cellc package verify` to re-clone the exact
source commit without re-querying the discovery index.

### Stage 3: Publishing

The developer publishes a new version:

```bash
cellc auth capability create --principal-id <principal_id> --scope publish:cellscript/amm_pool --expires 90d --json > capability-payload.json
cellc auth capability submit --payload capability-payload.json --joyid-signature joyid-signature.json
cellc publish
```

This automatically:

1. Registers a JoyID-authorised delegated capability key with the write API.
2. Reads `Cell.toml` -> gets `name`, `namespace`, `version`.
3. Computes `source_hash` from the current source tree.
4. Reads build artifacts for `artifact_hash`, `abi_hash`, `schema_hash`, etc.
5. Signs a concrete publish payload with the local capability key from the OS
   keychain, or with an externally supplied CI signature.
6. Uploads an immutable source snapshot.
7. Submits the entry to the registry write API for ACL, schema, hash, size,
   idempotency, quota, and duplicate checks.
8. Creates a canonical registry entry in `source_published` or
   `indexed_pending` state.

Capability revocation is also JoyID-bound:

```bash
cellc auth capability revoke --principal-id <principal_id> --capability-key-id <capability_key_id> --json > revoke-payload.json
cellc auth capability revoke --payload revoke-payload.json --joyid-signature joyid-signature.json --reason "rotate delegated key"
```

The explicit signing flow is:

```bash
cellc publish --print-payload --json > publish-payload.json
# sign the canonical_payload field with the authorised capability key
cellc publish --payload publish-payload.json --capability-signature <signature>
```

The same version entry can be mirrored into `registry.json` in the source repo
for audit, offline fixtures, and direct-Git fallback:

```json
{
  "name": "amm_pool",
  "namespace": "cellscript",
  "versions": [
    {
      "version": "1.2.0",
      "tag": "v1.2.0",
      "source_hash": "blake2b:0xabcd...",
      "cellscript_version": "0.19.0",
      "dependencies": {
        "token": { "namespace": "cellscript", "version": "0.3.0" }
      },
      "abi_index": "blake2b:0xdef0...",
      "schema_hash": "blake2b:0x9abc...",
      "license": "MIT",
      "released_at": "2026-05-06T00:00:00Z",
      "yanked": false
    }
  ]
}
```

Then the developer may commit and tag the mirrored metadata:

```bash
cellc publish --offline
git add registry.json
git commit -m "publish v1.2.0"
git tag v1.2.0
git push --tags
```

No separate registry account is needed. The JoyID-rooted publisher identity
authorises the local credential, and the registry ACL decides whether that
credential may publish to the namespace/package. No PR to the
`cellscript-registry` discovery index is needed for ordinary version updates;
discovery changes are for package claims, source-location changes, and
ownership metadata.

### Stage 4: Deploying

The developer deploys to CKB testnet:

```bash
cellc deploy --network aggron4
```

This triggers the existing headless deployment pipeline:

1. `cellc build` → produces artifact, metadata, constraints, schema, ABI.
2. `build_deploy_transaction(spec)` → computes all deployment facts locally
   (data_hash, code_hash, TYPE_ID args, capacity).
3. Submit + wait for commitment → receives `tx_hash`.
4. `build_deployment_manifest_from_evidence(evidence, tx_hash, output_index)` →
   generates `Deployed.toml`.
5. Update `Cell.lock` `[deployment.ckb.aggron4]` section.

Generated `Deployed.toml`:

```toml
version = 1

[package]
name = "amm_pool"
version = "1.2.0"
source_hash = "blake2b:0xabcd..."

[build]
compiler_version = "0.21.0"
artifact_hash = "blake2b:0x1234..."
metadata_hash = "blake2b:0x5678..."
schema_hash = "blake2b:0x9abc..."
abi_hash = "blake2b:0xdef0..."
constraints_hash = "blake2b:0x1111..."

[[deployments]]
network = "aggron4"
chain_id = "ckb-testnet"
script_role = "type"
tx_hash = "0xaaaa..."
output_index = 0
code_hash = "0xbbbb..."
hash_type = "data1"
dep_type = "code"
out_point = "0xaaaa...:0"
data_hash = "0xcccc..."
type_id = "0xdddd..."
```

Updated `Cell.lock` deployment section:

```toml
[deployment.ckb.aggron4]
status = "deployed"
record = "ckb-testnet:0xaaaa..."
record_hash = "blake2b:0x9a9a..."
```

### Stage 5: Consuming as a Dependency

Another developer uses `amm_pool` as a dependency:

```toml
# their project's Cell.toml
[dependencies]
amm = { version = "1.2.0", namespace = "cellscript" }
```

Resolution flow:

1. Query discovery index → `cellscript/amm_pool.json` →
   `source = "https://github.com/cellscript/amm_pool"`.
2. Clone at tag `v1.2.0` → read `registry.json` → verify `source_hash`.
3. Read the dependency's `Cell.lock` (if present) →
   find deployment record for `aggron4` →
   `code_hash`, `out_point`, `data_hash` available for builder verification.
4. Write the consumer's `Cell.lock` with resolved versions and git provenance.

The consumer's builder can now verify the full identity chain:
source → build → deployment, all bound by cryptographic hashes in
`Cell.lock`.

### File Interaction Summary

```
                         ┌─────────────┐
                         │  Cell.toml   │
                         │  (source)    │
                         └──────┬───────┘
                                │
                    cellc build │ + cellc install
                                │
                    ┌───────────▼───────────┐
                    │      Cell.lock         │
                    │  (build identity)      │
                    │  - source_hash         │
                    │  - artifact_hash       │
                    │  - registry url+rev    │
                    └───────────┬───────────┘
                                │
                    cellc deploy│ + confirm
                                │
                    ┌───────────▼───────────┐
                    │    Deployed.toml       │
                    │  (deployment facts)    │
                    │  - code_hash           │
                    │  - out_point           │
                    │  - data_hash           │
                    └────────────────────────┘


     Discovery Index            Source Repository
     (cellscript-registry)      (github.com/cellscript/amm_pool)
     ┌─────────────────┐       ┌──────────────────────────────────┐
     │ cellscript/     │       │ Cell.toml                        │
     │   amm_pool.json │──────►│ registry.json   ← cellc publish --offline mirror │
     │   token.json    │       │ src/                             │
     └─────────────────┘       │ Cell.lock       ← cellc build    │
                               │ Deployed.toml   ← cellc deploy   │
                               └──────────────────────────────────┘
```

The discovery index maps `namespace/name` → source repository URL.
The source repository contains everything else: source code, version index
(`registry.json`), build identity (`Cell.lock`), and deployment facts
(`Deployed.toml`). The public registry service is the write authority for
`cellc publish`; the source repository and `registry.json` mirror are the
audit/offline path. This preserves the Go-style source layout without making
Git push permissions the registry's public write authority.

## Deployment Record Field Classification

Fields are classified by necessity:

### Required Fields (Phase 1 — minimum for deploy verifiable)

| Field | Purpose |
|---|---|
| `network` | Which network this deployment targets |
| `chain_id` | Chain identifier |
| `tx_hash` | Deployment transaction hash |
| `output_index` | Output index in deployment transaction |
| `code_hash` | Script identity |
| `hash_type` | data / type / data1 / data2 |
| `dep_type` | code / dep_group |
| `data_hash` | Artifact data hash |
| `out_point` | CellDep reference |

### Recommended Fields (Phase 1 — build provenance binding)

| Field | Purpose |
|---|---|
| `artifact_hash` | RISC-V binary hash |
| `metadata_hash` | Compiler metadata hash |
| `schema_hash` | Schema manifest hash |
| `abi_hash` | ABI hash |
| `constraints_hash` | Constraints report hash |
| `compiler_version` | Compiler version that produced the artifact |

### Optional Fields (Phase 2 — governance and upgrade)

| Field | Purpose |
|---|---|
| `type_id` | TYPE_ID upgrade lineage |
| `script_role` | lock / type / dual-role / helper |
| `status` | active / candidate / deprecated / revoked |
| `upgrade_lineage` | TYPE_ID upgrade chain |
| `audit_report_hash` | Audit report hash |
| `publisher_signature` | Publisher identity signature |

### Deployment Status Lifecycle

```
                 deploy to network
  (undeployed) ─────────────────────► candidate
                                      │
                          confirm +   │  revoke or
                          audit pass  │  supersede
                                      ▼               ▼
                                    active          deprecated
                                      │
                          supersede   │
                                      ▼
                                    deprecated
                                      │
                          revoke     │
                                      ▼
                                    revoked
```

A deployment record must not be treated as production-ready until its status
reaches `active`. The `candidate` state allows builders to preview and dry-run
against a deployment, but production transaction construction should require
`active` status unless explicitly overridden.

## Source Package Registry (Off-Chain)

### Design Choice: Registry Write Service, Static Read Surface

The public registry uses an authenticated write service and a static,
cache-friendly read surface. The data model remains inspired by Go's approach
(source lives in its own repo, metadata can travel with the source), but the
public write authority is the registry service, not Git push access.

1. **Discovery index** — a lightweight map from `namespace/name` to the source
   repository URL and ownership metadata. Updated when a package is claimed,
   transferred, or its source location changes.
2. **Per-package version index** — a canonical registry entry mirrored as
   `registry.json` for audit, offline fixtures, and direct-Git fallback. The
   public entry is updated by authenticated `cellc publish`; the local mirror is
   written explicitly with `cellc publish --offline`.

Rationale:

- Does not block the v0.12 stable release.
- `cellc publish` has the expected package-registry semantics: after successful
  authentication and queue admission, the registry has a new addressable entry.
- The read path can remain CDN/static and independently verifiable.
- Git/source mirrors remain valuable for audit, local fixtures, and fallback.
- Namespace ownership, maintainer ACLs, yanking, quarantine, quotas, and abuse
  controls need one authoritative write boundary.
- The CKB ecosystem can start with a small write service because expensive
  verification work is asynchronous and bounded.

### Discovery Index Repository

A single Git repository (e.g., `github.com/cellscript/cellscript-registry`)
serves as the discovery index. It is organized by namespace:

```
cellscript-registry/
├── _schema.json               # { "schema_version": 1 }
├── cellscript/
│   ├── amm.json
│   └── token.json
└── other-protocol/
    └── swap.json
```

Each entry contains only the package name, namespace, and source repository
URL — no version details:

```json
{
  "name": "amm",
  "namespace": "cellscript",
  "source": "https://github.com/cellscript/amm"
}
```

This file is created or updated when a package is claimed, transferred, or
moved. Subsequent version releases do not require a discovery update unless
the source location or ownership metadata changes.

### Per-Package Version Index (registry.json)

The registry service stores the canonical per-package version entry. The same
shape can be mirrored to a `registry.json` file at the source repository root,
alongside `Cell.toml`, for audit and offline use:

```json
{
  "schema_version": 1,
  "name": "amm",
  "namespace": "cellscript",
  "versions": [
    {
      "version": "1.2.0",
      "tag": "v1.2.0",
      "source_hash": "blake2b:0xabcd...",
      "cellscript_version": "0.19.0",
      "dependencies": {
        "token": { "namespace": "cellscript", "version": "0.3.0" }
      },
      "abi_index": "blake2b:0xdef0...",
      "schema_hash": "blake2b:0x9abc...",
      "license": "MIT",
      "released_at": "2026-04-24T00:00:00Z",
      "yanked": false,
      "audit": {
        "report_hash": "blake2b:0x5555...",
        "acceptance_gate": "passed"
      }
    }
  ]
}
```

The `tag` field maps each version to a git tag in the source repository.
This allows `cellc install` to clone the exact commit without needing
a separate archive storage layer.

### Publishing Flow

```bash
# First use, or after credential expiry/revocation
cellc auth capability create --principal-id <principal_id> --scope publish:cellscript/amm_pool --expires 90d --json > capability-payload.json
cellc auth capability submit --payload capability-payload.json --joyid-signature joyid-signature.json

# Publish a new version to the registry
cellc publish
# → reads Cell.toml
# → computes source_hash from current source tree
# → reads build artifacts for abi_hash, schema_hash, etc.
# → signs publish payload with the local publisher credential
# → submits to the registry write API
# → returns canonical registry URL and an initial entry state

# Optional audit/offline mirror
cellc publish --offline
git add registry.json
git commit -m "publish v1.2.0"
git tag v1.2.0
git push --tags
```

No PR to an external registry repository is required for ordinary version
updates. The registry entry is authoritative for public discovery, while the
source repository mirror helps consumers audit and reproduce the same metadata
when `cellc install` clones a tagged version.

The discovery index only changes when claiming a brand-new package, changing
source location, or changing ownership metadata.

Initial entry visibility is staged:

```text
source_published  -> direct URL and author dashboard visible
indexed_pending   -> waiting for asynchronous verifier/indexer workers
verified_build    -> build evidence accepted
deployed          -> deployment facts attached and verified locally
on_chain_attested -> feature-gated JoyID/chain-backed deployment attestation
deprecated/yanked -> historical entry retained, default resolution suppressed
quarantined       -> direct URL retained, default search suppressed
```

The default resolver must not automatically select `source_published`,
`indexed_pending`, or `quarantined` entries. Direct installs may target those
entries only with an explicit risk flag such as `--allow-unverified`; quarantine
requires a stronger explicit flag such as `--allow-quarantined`. Default search,
recommendations, and production-visible package lists only include entries that
passed the required baseline checks.

A mirrored `registry.json` version entry with no `status` is treated as
`source_published`, not as verified. Public registry writes must emit an
explicit status; legacy mirrors need explicit risk flags before direct install.

### Installation Flow

```bash
# Install a package from the registry
cellc install cellscript/amm@1.2.0
```

Internally:

1. Clone or update the `cellscript-registry` discovery index (cached locally).
2. Look up `cellscript/amm.json` → get source repository URL.
3. Clone the source repository at tag `v1.2.0`.
4. Read `registry.json` from the cloned repository.
5. Verify `source_hash` matches the current source tree.
6. Parse `Cell.toml` and resolve transitive dependencies.

### Write Path DDoS and Spam Boundary

Once `cellc publish` writes to the public registry, the write API is part of
the security boundary. The read and write paths must stay separate:

```text
registry.cellscript.dev
  -> static website
  -> cached JSON indexes
  -> immutable mirrored metadata / artifact URLs

api.registry.cellscript.dev
  -> WAF / edge limits
  -> schema fail-fast
  -> auth and ACL checks
  -> quota and deduplication
  -> object storage
  -> bounded verification queues
```

Synchronous publish checks must remain cheap:

- signature, nonce, expiry, origin, and credential revocation;
- namespace/package ownership and scoped permission checks;
- request body size, metadata field length, tarball/artifact size caps;
- manifest/schema validation;
- `source_hash` / `manifest_hash` sanity and duplicate-hash rejection;
- idempotency keys for retry-safe publishes;
- per IP, ASN, JoyID principal, credential, namespace, and package quotas.

Expensive work is asynchronous:

- source mirror fetches;
- full build reproduction;
- artifact, ABI, schema, and constraint verification;
- deployment-fact verification;
- chain RPC reads;
- search indexing and ranking.

JoyID signatures are identity evidence, not an anti-spam mechanism by
themselves. New namespace claims, high-volume publishing, typosquatting-risk
names, and on-chain deployment attestations may require cooldown, review, or
community challenge. The first production source-package write path does not
require an on-chain fee or bond, but the schema and policy hooks must allow
later fee, refundable-deposit, or challengeable-record rules for higher-risk
actions. Suspicious packages move to quarantine rather than being silently
deleted, so exact pins and incident reviews remain reproducible.

### CLI Integration

```bash
# Authorise a local publisher credential with JoyID-rooted identity
cellc auth capability create --principal-id <principal_id> --scope publish:cellscript/amm --expires 90d --json > capability-payload.json
cellc auth capability submit --payload capability-payload.json --joyid-signature joyid-signature.json

# Publish a new version to the registry
cellc publish

# Optional local/offline discovery mirror
cellc registry add --namespace cellscript --name amm --source https://github.com/cellscript/amm

# Yank an existing version while preserving exact-pin warning metadata
cellc registry edit --yank 1.2.0 --reason "security advisory" --replaced-by 1.2.1

# Install from the source registry
cellc install cellscript/amm@1.2.0

# Verify package integrity against source and build artifacts
cellc package verify

# Verify deployment identity against chain facts
cellc registry verify
```

The `resolve_from_registry` path in `src/package/mod.rs` now implements the
two-tier source-package resolver: discovery index lookup, source repo clone,
tag checkout, `registry.json` identity and schema checks, `source_hash`
verification, `Cell.toml` parsing, and transitive dependency resolution. A
discovery failure reports the namespace, package, requested version, and
registry URL instead of falling through to a local-path placeholder.

## Deployment Registry (Chain-Indexed)

### Design Choice: Off-Chain First, Chain-Indexed When Needed

**Phase 1**: Pure off-chain `Deployed.toml` records, verified through
`Cell.lock` hash binding.

**Phase 2**: Optional on-chain type script index, driven by ecosystem demand.

Rationale:

- CKB capacity costs make on-chain source-package storage unattractive.
- Deployment facts through `Deployed.toml` + `Cell.lock` hash binding are
  sufficient for builder-level verification.
- An on-chain index script adds complexity and should be driven by actual
  ecosystem demand, not speculative design.

### Builder Verification Flow

The builder must verify the full identity chain before constructing a
production transaction:

```
cellc build
  → generates artifact, metadata, schema, abi, constraints
  → writes Cell.lock [package.build]

cellc deploy plan
  → reads Cell.lock [package.build]
  → reads Cell.toml [deploy.ckb] intent
  → produces deployment plan JSON

After deployment transaction is confirmed on-chain
  → generates Deployed.toml (chain facts)
  → updates Cell.lock [deployment.ckb.<network>]

cellc registry verify
  → reads Cell.lock build hashes
  → reads Deployed.toml deployment facts
  → verifies:
    1. source_hash matches between Cell.lock and Deployed.toml
    2. artifact_hash matches between Cell.lock and Deployed.toml
    3. data_hash = blake2b(artifact) against on-chain code cell
    4. code_hash in Deployed.toml matches on-chain script
    5. out_point is reachable as CellDep
    6. schema_hash / abi_hash consistent with metadata
    7. constraints_hash consistent with constraints report
  → any mismatch → FAIL CLOSED
```

### Action Builder Integration

The CellScript Action Builder is now the v0.20 target. It consumes the 0.19
package/build/deployment identity through the `registry-client` module:

```
┌──────────────┐     ┌──────────────────┐     ┌───────────────┐
│ metadata-    │     │ registry-client  │     │ cell-resolver │
│ loader       │────►│                  │────►│               │
│              │     │ resolve package  │     │ select live   │
│ load/validate│     │ resolve deploy   │     │ cells via     │
│ metadata,    │     │ verify hashes    │     │ CCC/indexer   │
│ ABI, recipe  │     │ against lockfile │     │               │
└──────────────┘     └──────────────────┘     └───────────────┘
```

For 0.20 builder work, the `registry-client` module is responsible for:

1. Resolving package records from the source registry index.
2. Resolving deployment records from `Deployed.toml`.
3. Verifying that resolved hashes match `Cell.lock`.
4. Rejecting hash mismatches, missing ABI records, and incompatible metadata
   schema versions.

The Action Builder must not accept a package by name alone. It must verify that
the resolved source package, build artifact, constraints report, and deployment
identity all match the 0.19 lockfile/provenance records before it constructs a
transaction.

## Integration With Existing Code

### Files That Change

| Component | Current | Change |
|---|---|---|
| `PackageInfo` | In `src/package/mod.rs`, no `namespace` field | Add `namespace: String` with `#[serde(default)]`. Required for `cellc publish`; absent means local-only package. |
| `DetailedDependency` | In `src/package/mod.rs`, no `namespace` field | Add `namespace: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Used for explicit registry resolution. |
| `PackageManifest` | `Cell.toml` schema | Unchanged structure. `[deploy.ckb]` already supported. `namespace` flows through `PackageInfo`. |
| `Lockfile` | `version/dependencies` only | Extend with `[package.build]`, `[deployment.*]`, `namespace`, `source_hash` on dependencies. |
| `LockedDependency` | `version` + `source` only | Add `namespace: Option<String>`, `source_hash: Option<String>`, `build: Option<LockedBuildInfo>`. All with `#[serde(default)]`. |
| `LockedSource::Registry` | `{ name, version }` only | Extend to `{ namespace, name, version, url, revision }`. The `url` and `revision` fields carry git provenance from the discovery index. |
| `DeploymentManifest` | In `crates/cellscript-ckb-adapter/src/lib.rs` | Extend to `Deployed.toml` schema: add `network`, `chain_id`, `script_role`, `data_hash`, `status`, `[build]` section. |
| `DeploymentRef` | In adapter crate | Add `network`, `chain_id`, `script_role`, `data_hash`, `status` fields as `Option<String>`. |
| `PackageManager::resolve_from_registry` | Implemented two-tier source-package resolver: discovery lookup → source repo clone → tag checkout → `registry.json` verification → source hash check → `Cell.toml` parsing. | Keep non-CellScript artifact profiles fail-closed until profile-specific resolver contracts exist. |
| `build_deployment_manifest_from_evidence` | In adapter crate | Extend to populate new fields. |
| `ManifestCellDepResolver` | In adapter crate | Unchanged. Still resolves CellDeps from manifest. |

### constraints_hash Generation

The `constraints_hash` field is critical for deployment safety: it binds the
deployment to the exact set of constraints the compiler generated, preventing
a compromised constraints report from being substituted after deployment.

**Phase 1 approach — same-version stability**: `cellc build` generates
`constraints_hash` using the same method as the existing `metadata_hash`
computation:

```
constraints_hash = ckb_blake2b256(serde_json::to_vec(&constraints))
```

This matches the existing pattern in `src/cli/commands.rs` where
`metadata_hash` is computed as `ckb_blake2b256(serde_json::to_vec(&result.metadata))`.

**Determinism guarantees in Phase 1**:
- Same compiler version + same source + same compile options → same
  `ConstraintsMetadata` struct → same `serde_json::to_vec` output → same
  `constraints_hash`. This is sufficient for Phase 1 because `constraints_hash`
  is only compared within the same compiler version.
- The `ConstraintsMetadata` struct fields are ordered by Rust struct field
  definition order, which is stable within a compiler version.
- Vec fields (`entry_abi`, `runtime_errors`, `warnings`, `failures`) are
  emitted in the compiler's internal iteration order, which is deterministic
  for the same input within the same compiler version.

**Known limitation**: Cross-compiler-version `constraints_hash` comparison is
not supported and should not be attempted. The `metadata_schema_version` field
in `CompileMetadata` serves as the envelope version gate, and
`constraints_metadata_schema_version` gates the constraints surface specifically
-- if schema versions differ, verification must reject the comparison, not
attempt hash matching.

**Phase 2 enhancement**: For stronger cross-build determinism (e.g.,
verifying that two independent builds of the same source produce the same
`constraints_hash`), the `ConstraintsMetadata` struct should:
- Sort all `Vec` fields by a stable key (`entry_name`, `code`, etc.)
- Replace any `HashMap` with `BTreeMap` for key ordering
- Pin the `serde_json` serialization to compact output with sorted keys

These changes are backward-compatible: they only affect the hash computation,
not the schema. A Phase 2 migration can compute both the old and new hashes
to bridge the transition.

### Backward Compatibility

- `Cell.lock` uses a single version 1 schema from the start. The `[package.build]`
  and `[deployment.*]` sections are optional; their absence simply means the
  package has not been built or deployed yet.
- The `Deployed.toml` format uses a distinct schema identifier
  (`cellscript-deployed-v0.19`) to avoid confusion with the existing deployment
  manifest schema.
- The `LockedDependency` type gains `source_hash` and `build` fields with
  `#[serde(default)]` to maintain deserialization compatibility.
- All new fields on `DeploymentRef` use `Option<String>` type (not typed
  structs like `H256` or enums), consistent with the existing `DeploymentRef`
  which stores `code_hash`, `hash_type`, `args`, `dep_type`, and `out_point` as
  plain `String` values. Each new field uses `#[serde(default,
  skip_serializing_if = "Option::is_none")]` so that existing
  `DeploymentManifest` JSON files with the
  `cellscript-ckb-deployment-manifest-v0.19` schema continue to parse without
  error. Typed field wrappers (e.g., `H256`, `ScriptRole`, `DeploymentStatus`)
  are a Phase 2 concern; Phase 1 keeps everything as `Option<String>` for
  maximum serialization compatibility.
- The validation logic in `parse_deployment_manifest` is extended to check
  for the new schema identifier. Old-format manifests (without the new fields)
  parse successfully with `None` for all new fields. New-format manifests must
  have the required fields populated; missing required fields in the new format
  are rejected, but missing optional fields are accepted.

### Non-Breaking Approach

The implementation should follow this ordering:

1. Add `Deployed.toml` parsing as a new capability alongside existing
   `DeploymentManifest` parsing. New fields on `DeploymentRef` use
   `Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`
   so existing manifests continue to parse.
2. Extend `Lockfile` with optional `[package.build]` and `[deployment.*]` fields.
   New `record_hash` field on `[deployment.*]` entries is optional in Phase 1;
   computed via canonical JSON serialization (not canonical TOML) to match
   the existing `metadata_hash` convention.
3. Add `constraints_hash` to `cellc build` output using the same method as
   `metadata_hash`: `ckb_blake2b256(serde_json::to_vec(&constraints))`. Same-version
   determinism is sufficient for Phase 1; Phase 2 adds Vec sorting for
   cross-build determinism.
4. Extend `build_deployment_manifest_from_evidence` to populate the new
   `DeploymentRef` fields (`network`, `chain_id`, `data_hash`, `type_id`,
   `status`, and the `[build]` section) from the existing `ResolvedDeployEvidence`
   and adapter configuration.
5. Implement `resolve_from_registry` without changing existing path/git
   resolution.
6. Add `cellc package verify` and `cellc registry verify` as new subcommands.
7. Defer wiring the `registry-client` module into the generated Action Builder
   pipeline to 0.20; 0.19 consumes it from package/build verification.

## Version Control Audit

### Audit Findings

The document covers three layers of identity (Package, Build, Deployment) but
has gaps in version control across multiple dimensions. This section documents
the gaps and the resolutions adopted.

#### 1. Package Version Semver Rules

**Gap**: The document shows `version = "0.3.0"` in dependencies but does not
define what this means. Is it `^0.3.0` (compatible) or `=0.3.0` (exact)?
What constitutes a breaking change for a CellScript package?

**Resolution**: Adopt Cargo's semver convention:

- `"0.3.0"` means `^0.3.0` (any `0.3.x`, not `0.4.0`)
- `"=0.3.0"` means exact version
- `"*"` means any version
- `">=0.3.0, <0.4.0"` means range

The existing `VersionReq` enum in `src/package/mod.rs` already implements
this. No code change needed; the document should reference this convention.

**Breaking change definition for CellScript**:

| Change | Breaking? |
|---|---|
| New action | No |
| New shared type field | No (additive) |
| Removed action | Yes |
| Removed shared type field | Yes |
| Changed action signature | Yes |
| Changed ABI layout | Yes |
| New dependency | No |
| Changed dependency version (major) | Yes |

#### 2. Cell.lock Version — Dual Version Identifier

**Gap**: `version = 1` and `lock_schema = "cellscript-lock-v1"` are redundant.
No migration path is defined between lockfile schema generations.

**Resolution**: Remove `lock_schema`. The `version` field is sufficient —
it is an integer that increments on breaking schema changes. Migration
strategy: when `cellc` reads a lockfile with an older version, it writes
a new lockfile preserving all compatible fields. The `version` field alone
is the schema identifier.

#### 3. Deployed.toml Schema — Dual Version Identifier

**Gap**: `version = 1` and `schema = "cellscript-deployed-v0.19"` serve
overlapping purposes. The `schema` string ties the format to a specific
cellscript version, but format evolution is independent of compiler
version.

**Resolution**: Keep `version = 1` as the schema identifier (integer,
stable). Remove `schema = "cellscript-deployed-v0.19"`. The relationship
to the existing `cellscript-ckb-deployment-manifest-v0.19` schema is:
`Deployed.toml` version 1 is a superset of the existing manifest schema.
The parser accepts both; the `version` field distinguishes them.

#### 4. registry.json Dependencies Missing Namespace

**Gap**: The `dependencies` field in `registry.json` uses
`{ "token": "0.3.0" }` — no namespace information. A consumer cannot
determine which namespace `token` belongs to.

**Resolution**: Change the dependencies format to include namespace:

```json
"dependencies": {
  "token": { "namespace": "cellscript", "version": "0.3.0" }
}
```

This matches the Cell.toml dependency syntax and enables unambiguous
resolution without consulting the discovery index.

#### 5. registry.json Format Version

**Gap**: No schema version identifier in `registry.json`. If the format
needs to change (e.g., add a `replaced_by` field for yanking), the
parser cannot distinguish old vs new format.

**Resolution**: Add a `schema_version` field:

```json
{
  "schema_version": 1,
  "name": "amm_pool",
  "namespace": "cellscript",
  "versions": [...]
}
```

#### 6. Compiler Version Compatibility Window

**Gap**: No defined compatibility window. Different `cellc` versions may
produce different `constraints_hash` for the same source.

**Resolution**: Define a compatibility rule:

- Same major.minor version (e.g., `0.19.x`) → `constraints_hash` is
  expected to be identical for the same source + same compile options.
- Different major.minor → `constraints_hash` may differ; verification
  must not attempt cross-version hash comparison.
- The `metadata_schema_version` field in `CompileMetadata` serves as the
  envelope version gate, and `constraints_metadata_schema_version` gates the
  constraints surface specifically.

This is already partially documented in the `constraints_hash Generation`
section, but the rule should be stated more explicitly as a version
compatibility policy, not just a known limitation.

#### 7. ABI Compatibility Model

**Gap**: `abi_hash` and `schema_hash` are content hashes. They can tell
you two ABIs are identical, but not whether they are compatible.

**Resolution**: Phase 1 treats `abi_hash` as an exact match gate: if the
hash differs, the ABIs are considered incompatible. Phase 2 may introduce
ABI compatibility checking (e.g., structural subtyping for additive
changes). This is deferred because:

- For deployed contracts, ABI changes are always breaking — existing
  on-chain cells were created with the old ABI.
- Source-level compatibility is the semver contract, not the hash.

#### 8. Git Tag Convention

**Gap**: No defined tag naming convention. No validation that the tag
matches the `version` field.

**Resolution**:

- Tag format: `v{version}` (e.g., `v1.2.0`).
- Pre-release: `v1.2.0-rc.1`.
- `cellc publish` validates that the `version` field in `Cell.toml`
  matches the `version` in `registry.json`.
- `cellc install` validates that the git tag `v{version}` exists and
  points to the same commit as `revision` in `Cell.lock`.

#### 9. Yanking Semantics

**Gap**: `yanked` is a boolean with no replacement pointer or timestamp.

**Resolution**: Extend the yanking model for Phase 2:

```json
{
  "version": "1.2.0",
  "yanked": true,
  "yanked_at": "2026-06-01T00:00:00Z",
  "yanked_reason": "security: reentrancy in swap()",
  "replaced_by": "1.2.1"
}
```

The resolver keeps `yanked` as a boolean for normal selection (yanked versions
are filtered out by `find_matching_version`), and additionally carries the
Phase 2 metadata fields `yanked_at`, `yanked_reason`, and `replaced_by`. When a
yanked version is reached through an exact `=x.y.z` pin, the resolver emits a
warning to stderr that names the reason and suggests the `replaced_by` version
(or the latest non-yanked version when no `replaced_by` is declared). Existing
`Cell.lock` entries referencing yanked versions are not automatically broken —
the lockfile is the source of truth.

#### 10. Dependency Version Conflict Resolution

**Gap**: No defined strategy when two dependencies require different
versions of the same package.

**Resolution**: The resolver uses a conservative single-version strategy:

- A single version of each package exists in the dependency graph.
- If `amm` requires `token ^0.3.0` and `vesting` requires `token ^0.3.1`,
  the resolver picks `token 0.3.2` when that version is the latest version
  satisfying the first resolved constraint and also satisfies the later
  constraint.
- If a transitive request names a version requirement that the already-selected
  version does not satisfy, resolution **fails closed** with a
  `version conflict for '<pkg>': already resolved to '<v>', which does not
  satisfy requirement '<req>'` error, instead of silently keeping whichever
  version was resolved first.
- The current implementation does not backtrack or re-solve the whole graph
  when a later constraint would require a different still-compatible version;
  that remains future resolver work.
- Future work may support multiple versions (like Go's `MVS` + `replace`),
  but the current resolver keeps it simple: one version per package per graph,
  with fail-closed conflict detection.

#### 11. Discovery Index Format Version

**Gap**: The discovery index JSON files have no version identifier.

**Resolution**: Add a top-level `schema_version` field to each namespace
directory:

```
cellscript-registry/
├── _schema.json           # { "schema_version": 1 }
├── cellscript/
│   ├── amm.json
│   └── token.json
└── other-protocol/
    └── swap.json
```

The `_schema.json` file at the repository root defines the format version.
This is a single file for the entire repository, not per-package.

#### 12. Network Identifier Mapping

**Gap**: `network` and `chain_id` are free-form strings with no canonical
mapping. The `deployment.ckb.aggron4` section key mixes platform and
network.

**Resolution**: Define a canonical network registry:

| Network | `chain_id` | `network` value |
|---|---|---|
| CKB Mainnet | `ckb-mainnet` | `mainnet` |
| CKB Testnet (Aggron4) | `ckb-testnet` | `aggron4` |
| CKB Devnet | `ckb-devnet` | `devnet` |

The `deployment` section key format is `[deployment.{platform}.{network}]`.
For Phase 1, only `ckb` platform is supported.

### Audit Summary

| # | Gap | Severity | Phase 1 Action |
|---|---|---|---|
| 1 | Semver rules | **High** | Reference existing `VersionReq` in document |
| 2 | Dual lockfile version | Medium | Remove `lock_schema`, keep `version` |
| 3 | Dual Deployed.toml version | Medium | Remove `schema` string, keep `version` |
| 4 | registry.json deps missing namespace | **High** | Add namespace to dependencies |
| 5 | registry.json format version | Medium | Add `schema_version` |
| 6 | Compiler version compatibility | **High** | Define major.minor compatibility window |
| 7 | ABI compatibility model | Low | Phase 1: exact hash match; Phase 2: structural |
| 8 | Git tag convention | Medium | Define `v{version}` convention with validation |
| 9 | Yanking semantics | Low | Phase 1: simple boolean; Phase 2: reason + replacement |
| 10 | Version conflict resolution | **High** | Define unified resolution strategy |
| 11 | Discovery index version | Low | Add `_schema.json` to repo root |
| 12 | Network identifier mapping | Medium | Define canonical network table |

## Phased Implementation

### Public Registry Publication Policy

`cellc publish` is the public registry write path: it authenticates the
publisher, checks ACL/scope/quota, admits a package entry, and returns a
canonical registry URL. Git commits, Git tags, and `registry.json` remain audit,
mirror, local-fixture, and offline-fallback surfaces; they are not the public
registry admission authority.

| Policy | Evidence |
|---|---|
| JoyID-rooted publisher identity | `cellc auth capability create --principal-id <principal_id> --scope publish:ns/pkg --expires 90d --json > capability-payload.json` plus `cellc auth capability submit --payload capability-payload.json --joyid-signature joyid-signature.json` uses the CCC-backed JoyID flow, records `principal_type = joyid_ckb`, binds `principal_id` to a local publisher credential, and stores that credential in the OS keychain |
| Scoped publisher credentials | Capability-style signing key with namespace/package/action scopes, expiry, revocation, nonce/origin checks, and CI-safe delegation |
| Namespace/package ACL | Namespace owners, package maintainers, yanking authority, attestation authority, maintainer rotation, and source-location update permissions |
| Abuse controls | Separate static read path from write API; WAF/rate limits/body caps/hash dedup/bounded queues/quarantine/cooldown; fee/bond rules remain later policy hooks |
| Entry visibility state machine | `source_published` -> `indexed_pending` -> `verified_build` -> `deployed` -> `on_chain_attested`; `deprecated`/`yanked`/`quarantined` suppress default search without deleting history |

### Phase 0 — No Block on v0.12

The v0.12 release ships without registry support. The existing
`resolve_from_registry` stub remains. `Cell.lock` version 1 continues to work.
No deployment registry records are generated.

### Phase 1 — v0.19 Scope

This phase makes the registry usable for local development and verification.
Items are ordered by dependency; each item includes its version-control
implications from the audit above.

| # | Work | Evidence | Audit Ref |
|---|---|---|---|
| 1 | Add `namespace` to `PackageInfo` and `DetailedDependency` | `Cell.toml` with `namespace` parses correctly; `cellc init --namespace` sets it | — |
| 2 | Extend `LockedSource::Registry` with `namespace`, `url`, `revision` | `Cell.lock` writes registry deps with git provenance; re-verification works without discovery index | #2 |
| 3 | Remove `lock_schema` from Cell.lock; keep `version = 1` | Single version identifier; no dual version confusion | #2 |
| 4 | Add `schema_version: 1` to `registry.json` format | `cellc publish --offline` writes `schema_version`; `cellc install` rejects unknown versions | #5 |
| 5 | Fix `registry.json` dependencies to include namespace | `dependencies: { "token": { "namespace": "cellscript", "version": "0.3.0" } }` | #4 |
| 6 | Remove `schema` string from Deployed.toml; keep `version = 1` | Single version identifier; parser accepts both old manifest and new Deployed.toml | #3 |
| 7 | Define canonical network table (mainnet/aggron4/devnet) | `cellc deploy --network aggron4` writes correct `network` + `chain_id` | #12 |
| 8 | Add `_schema.json` to discovery index repository | `{ "schema_version": 1 }` at repo root | #11 |
| 9 | `Cell.lock` with `[package.build]` hash section | `cellc build` writes artifact/metadata/schema/abi/constraints hashes to lockfile | — |
| 10 | `Deployed.toml` format definition and parsing | Adapter crate can load and validate `Deployed.toml` records | — |
| 11 | Implement `resolve_from_registry` with two-tier resolution | Discovery index lookup → source repo clone → `registry.json` verification → `Cell.toml` parsing | — |
| 12 | Define semver compatibility rules and unified version resolution | `cellc build` fails on unsatisfiable version constraints; `"0.3.0"` means `^0.3.0` | #1, #10 |
| 13 | Define compiler major.minor compatibility window for `constraints_hash` | `cellc registry verify` rejects cross-version hash comparison; same `0.19.x` → same hash | #6 |
| 14 | Define git tag convention `v{version}` with validation | `cellc publish` validates tag matches version; `cellc install` validates tag exists | #8 |
| 15 | `cellc package verify` | Validates package metadata against source and build artifacts | — |
| 16 | `cellc registry verify` | Validates build artifacts against deployment facts; checks `record_hash` if present | — |
| 17 | Registry fixture acceptance | Local registry fixture can publish, resolve, and verify a package | — |
| 18 | Hash mismatch rejection | Resolver rejects registry schema/name/namespace/version/tag/source-hash mismatches and package/build/deployment identity mismatches | — |

### Phase 2 — v0.20 Or Later

| Work | Evidence |
|---|---|
| Deployment status lifecycle | `DeploymentStatus` enum (candidate/active/deprecated/revoked); `cellc registry verify` and generated builders fail closed unless status is `active` |
| TYPE_ID upgrade lineage tracking | `Deployed.toml` carries `upgrade_lineage`; `cellc registry verify` rejects self-referential and empty lineage (off-chain consistency; on-chain TYPE_ID upgrade-chain proof remains a live-RPC concern) |
| Publisher signature binding | `Deployed.toml` optionally carries `publisher_signature`; `cellc registry verify --require-publisher-signature` enforces presence (metadata-presence only; cryptographic verification is a later security milestone) |
| Yanking metadata | `registry.json` version entries carry `yanked_at`, `yanked_reason`, `replaced_by`; resolver warns and suggests the replacement when a yanked version is reached |
| `cellc deploy plan` / `cellc deploy verify` / `cellc deploy lock-deps` | CLI commands emit or verify deployment registry records |
| Stale-deployment rejection | Builder refuses to build when deployment record does not match package metadata |
| Registry mismatch fixtures | Wrong network, wrong code hash, stale metadata hash, missing CellDep, deprecated deployment rejection paths |
| On-chain type script index (if needed) | Deferred — optional chain-indexed deployment lookup driven by ecosystem demand |

### Phase 3 — Read-Path Scaling and Cross-Profile Registry

The registry write service is the public admission authority. The read path is
static, cacheable, and mirrorable; Git/source mirrors remain an audit and
fallback mechanism, not the public write authority.

| Work | Evidence |
|---|---|
| Static registry proxy | `registry.cellscript.dev` / `proxy.cellscript.dev` serves cached indexes, source mirrors, and immutable metadata; `cellc install` falls back to direct Git if cache entries are unavailable |
| Yanking and supersession | Index supports `yanked` flag and supersession metadata |
| Maintainer rotation | Namespace owner principal and publisher credential rotation |
| Cross-protocol CellFabric registry discovery | Registry-backed protocol discovery for multi-protocol intent composition |
| Reproducible build proofs | Optional build attestation and verification beyond hash matching |
| Audit signature requirement | Packages require audit signatures before being marked production-ready |

## Responses to Open Questions

### Should CellScript eventually have its own source registry, or reuse/adapt an existing registry protocol?

CellScript should have its own small registry write service because
`cellc publish` must create a public registry entry, enforce namespace/package
ownership, and handle yanking, quarantine, and abuse controls. The read path
should remain Go-like and cacheable: discovery maps `namespace/name` to source
URLs, per-package metadata can be mirrored as `registry.json`, and clients
verify hashes rather than trusting transport. Git/source mirrors are permanent
audit and fallback surfaces, but public write authority belongs to the registry
service.

### What is the minimal useful CKB deployment record without wasting capacity?

Seven required fields: `tx_hash`, `output_index`, `code_hash`, `hash_type`,
`dep_type`, `data_hash`, and `network`. This is approximately 200 bytes for a
single deployment record. Additional fields are recommended but not required
for Phase 1.

### Should deployment records live under one global registry type script, namespace-specific type scripts, or mostly off-chain with chain-indexed commitments?

Phase 1: purely off-chain `Deployed.toml` with `Cell.lock` hash binding.
Phase 2: optional chain-indexed commitments if ecosystem demand justifies the
capacity cost. A global registry type script is possible but should not be the
default; namespace-specific type scripts may be more appropriate for protocol
teams that want on-chain deployment discovery.

### Which fields should be considered essential for CKB deployment identity?

See the Field Classification table above. The essential set is:
`tx_hash` + `output_index` + `code_hash` + `hash_type` + `dep_type` +
`data_hash` + `network`. Build provenance fields (`artifact_hash`,
`metadata_hash`, `schema_hash`, `abi_hash`, `constraints_hash`,
`compiler_version`) are recommended but not required for Phase 1.

### How should wallets and transaction builders verify CellScript dependencies before constructing production transactions?

Through `cellc registry verify`, which performs a seven-step verification chain:
1. source_hash matches between Cell.lock and Deployed.toml
2. artifact_hash matches between Cell.lock and Deployed.toml
3. data_hash = blake2b(artifact) against on-chain code cell
4. code_hash in Deployed.toml matches on-chain script
5. out_point is reachable as CellDep
6. schema_hash / abi_hash consistent with metadata
7. constraints_hash consistent with constraints report

Any failure in this chain causes fail-closed rejection.

### Who should own namespaces and maintainer keys?

Namespace ownership is the core registry ACL. A namespace has owner principals;
packages have maintainer principals; publisher credentials are scoped to
actions such as `publish`, `yank`, `attest`, and `manage-maintainers`. The root
publisher principal is `joyid_ckb`, while daily operations use delegated
publisher credentials that can expire and be revoked. The exact bootstrap
policy for first namespace claim (review, cooldown, reserved namespaces, or
later fee/bond hooks) is an ecosystem decision.

### Should reproducible build proofs or audit signatures be required before a package is considered production-ready?

Phase 1 requires hash matching but not build attestations or audit signatures.
Phase 2 adds optional publisher signatures and audit report hashes. Whether
audit signatures become mandatory for production readiness is an ecosystem
policy decision, not a toolchain enforcement decision. The toolchain should
support the mechanism; the policy should be set by the community.

### How should yanking, supersession, and maintainer rotation work?

Yanking is a scoped maintainer action over a package version. The index keeps
`yanked`, `yanked_at`, `yanked_reason`, and `replaced_by` metadata so exact pins
can warn without destroying history. Supersession metadata links a deprecated
or yanked record to its replacement. Maintainer rotation is performed by
namespace owners by adding/revoking scoped publisher credentials and package
maintainer principals. Quarantine is separate from yanking: it is an abuse or
risk-control state that suppresses default search while preserving direct URL
and audit history.

Package versions are not hard-deleted from registry history. If legal, security,
or clearly malicious content must be hidden, public access to the artifact or
source snapshot may be disabled, but the registry must retain a tombstone,
package history, audit record, actor identity, reason, and timestamps.

## Non-Goals

- Do not replace CCC. The Action Builder consumes deployment records; it does
  not become a wallet, indexer, or chain submission layer.
- Do not introduce a separate registry account system alongside JoyID-rooted
  publisher identity.
- Do not require an interactive JoyID signature for every `cellc publish`;
  JoyID authorises scoped publisher credentials, and credentials sign daily
  publish payloads.
- Do not introduce hidden signer authority or hidden sighash defaults.
- Do not infer transaction semantics from protocol/action names.
- Do not treat package registry resolution as deployment verification. These
  are separate layers with separate verification obligations.
- Do not make the public website or registry read path call CKB RPC for ordinary
  browsing/search. Chain checks belong in asynchronous workers or explicit
  verifier commands.
- Do not mark a deployment mainnet-certified without external audit and chain
  evidence.
- Do not make builder success a substitute for CKB VM acceptance.
- Do not claim full CellFabric intent composition in the registry release.
- Do not force on-chain deployment records when off-chain verification is
  sufficient.
- Do not claim generated Action Builder or live-chain registry certification as
  part of the 0.19 Phase 1 registry closure.

## Acceptance Gate

Phase 1 acceptance requires:

```
cellc package verify                        # source ↔ build hash binding
cellc registry verify                       # build ↔ deployment hash binding
local registry fixture: publish / resolve / verify
hash mismatch rejection fixtures
README and docs distinguish package discovery from deployment discovery
```

0.20 acceptance adds:

```
cellc registry verify --live
cellc gen-builder --target typescript
npm test for generated builders
local CKB dry-run for generated action transactions
local CKB submitted stateful flows for canonical examples
negative builder-shape rejection fixtures
deployment registry mismatch rejection fixtures
```
