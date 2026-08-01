# CellScript Registry: Artifact and Deployment Contract

**Status**: implemented public contract for the CellScript Registry. The
admission, verification, discovery, deployment-evidence, CLI, and website
surfaces described here are checked in on the current release line.

The Registry indexes CKB ecosystem artifacts. A coordinate is
`namespace/name`; a release adds an immutable version. The coordinate does not
imply that the object is a CellScript dependency, executable, deployed Script,
or reusable source library. Those meanings are explicit in the artifact
descriptor and in three independent state axes.

The production boundary and operator controls remain in
[`CELLSCRIPT_REGISTRY_PRODUCTION_BOUNDARY_ADR.md`](CELLSCRIPT_REGISTRY_PRODUCTION_BOUNDARY_ADR.md).

## One Public Model

Every artifact has this descriptor:

```json
{
  "kind": "deployable_contract",
  "profile": "ckb_executable",
  "consumption_mode": "deployment",
  "language": "rust"
}
```

The Registry accepts these kinds:

| Kind | Profile | Consumption | Required immutable objects |
|---|---|---|---|
| `source_library` | `cellscript_source` | `dependency` | CellScript source snapshot |
| `profile_library` | `cellscript_source` | `dependency` | CellScript source snapshot |
| `runtime_verifier` | `ckb_executable` | `tcb` | source, executable, ABI |
| `deployable_contract` | `ckb_executable` | `deployment` | source, executable, ABI |
| `reproducible_binary` | `reproducible_build` | `tcb` | source, executable, build recipe |
| `template` | `copy_material` | `copy` | source material |

`cellc install` deliberately accepts only the `cellscript_source` +
`dependency` contract. An executable, verifier, reproducible tool, or template
can be discovered and audited through the same Registry, but it cannot be
silently interpreted as a CellScript dependency.

There is one public route family: `/v1/artifacts`. The Registry does not expose
a second package route with a competing data shape.

## Independent States

Each release exposes three orthogonal states:

- `verification_status`: `pending`, `hash_bound`, `verified`, `evidence_required`, or
  `rejected`;
- `deployment_status`: `not_applicable`, `undeployed`, `deployed`, or
  `chain_verified`;
- `availability_status`: `active`, `deprecated`, `yanked`, or `quarantined`.

These states must not be collapsed into one lifecycle label. A reproducible
binary may be verified but have no deployment concept. A CKB executable may be
verified and still undeployed. A previously chain-verified release may later be
deprecated without rewriting its evidence.

## Artifact Identity

The Registry separates four questions:

1. **Coordinate identity**: which publisher-controlled name and release?
2. **Source identity**: which immutable source or input bytes?
3. **Build identity**: which executable, ABI, recipe, compiler, and metadata?
4. **Deployment identity**: which live mainnet Cell contains the executable?

Source and build identity come from immutable, hash-bound bundle objects.
Deployment identity is an additional signed evidence record; publishing an
executable never claims that it is already deployed.

For CKB executables, `artifact_hash` is the CKB Blake2b-256 hash of the
executable bytes. A deployment record must bind the same value as `data_hash`.
The Registry then calls mainnet `get_live_cell` and verifies:

- the OutPoint is live;
- the returned Cell data hash equals the published executable hash;
- for `hash_type = type`, the returned Type Script hash equals `code_hash`;
- for data-hash variants, `code_hash` equals the executable data hash.

For `dep_type = dep_group`, the Registry decodes the live DepGroup Cell as the
canonical Molecule `OutPointVec`, loads its members, and requires a live member
whose code/data identity matches the published executable. The DepGroup
container bytes are never treated as executable code.

Only CKB mainnet deployment records are accepted. Testnet is neither a Registry
deployment state nor a selectable website network.

## Publishing CellScript Dependencies

A normal CellScript package uses `Cell.toml` and the native publish path:

```bash
cellc package verify --json
cellc publish --dry-run
cellc publish
```

Profile libraries use the same compiler-backed snapshot contract and declare
their distinct kind explicitly:

```bash
cellc publish --artifact-kind profile_library --dry-run
cellc publish --artifact-kind profile_library
```

The verifier compiles the snapshot with the real CellScript compiler and
checks its canonical manifest, source hash, build identity, metadata, and
compatibility-profile identity. Publisher-supplied state is never treated as
verification evidence.

## Publishing Other Artifacts

Non-CellScript artifacts use `Artifact.toml` plus a bounded JSON bundle:

```toml
schema = "cellscript-registry-artifact"
namespace = "acme"
name = "vault-lock"
release = "1.0.0"
kind = "deployable_contract"
language = "rust"
bundle = "vault-lock.bundle.json"
description = "Mainnet vault lock Script"
repository = "https://github.com/acme/vault-lock"
keywords = ["lock", "vault"]
```

The referenced bundle carries a closed, typed profile contract. For a
deployable contract, canonicalize this object recursively by key and encode the
resulting JSON as the bundle's `manifest_json` string:

```json
{
  "schema": "cellscript-registry-profile-contract-v1",
  "artifact_kind": "deployable_contract",
  "profile": "ckb_executable",
  "build": {
    "target": "riscv64imac-unknown-none-elf",
    "toolchain": "rustc 1.97.1",
    "profile": "release",
    "source_revision": "<immutable revision>",
    "reproducible": false
  },
  "security": { "status": "review_required" },
  "ckb": {
    "vm_version": "2",
    "script_role": "lock",
    "hash_type": "data1",
    "dep_type": "code",
    "abi_hash": "<CKB Blake2b-256 of the ABI object>"
  }
}
```

The bundle has this shape:

```json
{
  "schema": "cellscript-registry-bundle",
  "namespace": "acme",
  "name": "vault-lock",
  "release": "1.0.0",
  "profile": "ckb_executable",
  "manifest_json": "<canonical cellscript-registry-profile-contract-v1 JSON>",
  "objects": [
    { "role": "source", "content_base64": "..." },
    { "role": "executable", "content_base64": "..." },
    { "role": "abi", "content_base64": "..." }
  ]
}
```

For `reproducible_binary`, use profile `reproducible_build` and replace `abi`
with `build_recipe`; its contract binds the environment, deterministic command,
recipe hash, and expected artifact hash. `runtime_verifier` additionally
requires `verifier_id`, `ipc_abi`, and the IPC ABI hash. For `template`, use
profile `copy_material`, include only `source`, and encode it as a
`cellscript-template-file-map-v1` whose relative paths, contents, and hashes are
authenticated. The CLI rejects unknown contract fields, missing or duplicate
roles, malformed values, unsafe copy paths, and hashes that do not bind the
immutable objects.

A `ckb_executable` may also set `build.reproducible = true`, include a
`build_recipe` object, and use the same `reproduction` contract. Deployment and
reproducibility are independent axes: the former is proven by a live mainnet
Cell, while the latter still needs reproducible-build evidence beyond a recipe
declaration.

When `security.status = "audited"`, the contract must include
`security.audit_report_hash` and the bundle must contain exactly one non-empty
`audit_report` object with that CKB Blake2b-256 hash. The status is still a
publisher declaration; the binding prevents the referenced report from being
swapped or omitted.

```bash
cellc publish --artifact-manifest Artifact.toml --dry-run
cellc publish --artifact-manifest Artifact.toml
```

The independent verifier checks the profile-specific object set and recomputes
the published hashes. Generic executable and copy bundles are `hash_bound`; this
does not claim executable semantics, reproducibility, or a security review. A
reproducible build is marked `evidence_required` until
appropriate build evidence exists; merely uploading output bytes does not prove
reproducibility.

## Consuming Other Artifacts

Generic artifacts never pass through `cellc install`. Use the explicit
consumer commands:

```bash
cellc artifact fetch acme/vault-lock@1.0.0 --output vault-lock.bundle.json
cellc artifact verify --bundle vault-lock.bundle.json --receipt vault-lock.bundle.json.receipt.json
cellc artifact pin acme/vault-lock@1.0.0 --output Artifacts.lock --accept-hash-bound
cellc artifact copy acme/starter@1.0.0 --destination ./new-project --accept-hash-bound
cellc artifact record-deployment acme/vault-lock@1.0.0 --code-hash <hash> --hash-type data1 --dep-type code --tx-hash <tx_hash> --index 0 --capability-key-id <key_id>
cellc artifact cell-dep acme/vault-lock@1.0.0 --output CellDep.json --accept-hash-bound
cellc artifact commitment acme/vault-lock@1.0.0 --output RegistryCommitment.json
```

`fetch` checks the immutable object's SHA-256 identity and every CKB object
hash. `verify` repeats those checks offline from the receipt. `pin` records the
exact Registry identity and requires an explicit trust decision for
integrity-only evidence. `copy` is no-overwrite and rejects traversal,
platform-specific, duplicate, or unauthenticated paths. `cell-dep` requires an
attached RPC-verified mainnet deployment and preserves the DepGroup container
and resolved code-member identities. It never turns an `undeployed` release
into a CellDep.

`record-deployment` derives the artifact/data identity from the signed Registry
release, signs a mainnet-only payload with the scoped capability key, and sends
it to the API for live-Cell verification.

`commitment` produces the canonical `cellscript-registry-commitment-v1`
payload, CKB Blake2b commitment, and compact `CSREGv1 || hash` Cell data. The
Registry accepts an on-chain attestation only after reading that live mainnet
Cell and matching its exact data, attestor Lock hash, and Registry Type Script
hash used for chain indexing.

## Publisher Authorisation

The website presents a single “Connect CKB wallet” entry. Its modal lists all
supported CKB wallet connectors, but only connectors actually detected in the
current browser can sign immediately; install links are shown for the rest.
Network selection is not exposed because authorisation and deployment are
mainnet-only.

The wallet signs a narrowly scoped capability authorisation. Daily publishes
use a P-256 capability key stored by `cellc`, so the wallet seed and mnemonic
never leave the wallet. Namespace ownership, capability scope, expiry,
revocation, nonce consumption, idempotency, quotas, and audit events are
enforced by the API.

The submit form remains hidden until a wallet principal is connected or the
publisher explicitly confirms that an active capability already exists.

## Public Reads

```text
GET  /health
GET  /ready
GET  /v1/artifacts
GET  /v1/artifacts/:namespace/:name
GET  /v1/artifacts/:namespace/:name/releases/:release/evidence
GET  /v1/artifacts/:namespace/:name/releases/:release/commitment
GET  /artifacts/:namespace/:name/releases/:release.json
POST /v1/artifacts/:namespace/:name/releases
POST /v1/artifacts/:namespace/:name/releases/:release/deployments
```

The list endpoint accepts `q`, `namespace`, `kind`, `verification`,
`deployment`, `availability`, `limit`, and `offset`. Static release objects and
immutable bundles are served separately from the write database so consumers
can hash-verify and cache them independently.

Example discovery request:

```bash
curl --fail 'https://api.registry.cellscript.dev/v1/artifacts?kind=deployable_contract&deployment=chain_verified'
```

The website exposes Registry, Submit, and API as peer tabs. Detail pages show
artifact kind, consumption mode, all three state axes, release hashes,
verification evidence, and mainnet deployment evidence without pretending
that every artifact is installable.

## Fail-Closed Rules

- Unknown kinds, profiles, languages, object roles, and state values fail.
- Identifiers are 1–64 lowercase letters or digits; `_` and `-` are allowed
  only between characters.
- A source dependency resolver rejects every non-CellScript profile.
- A CKB deployment requires prior verified-build evidence.
- Deployment evidence must match the published executable hash and a live
  mainnet Cell.
- Quarantined releases are not returned by public detail or evidence routes.
- Immutable bundle writes complete before release admission.
- State transitions append evidence; they do not mutate hash identity.

## Validation

Registry changes are covered by the repository gates:

```bash
./scripts/cellscript_gate.sh dev
./scripts/cellscript_gate.sh ci
```

The `ci` gate typechecks and tests the API, builds Node API/verifier bundles,
runs the independent Rust verifier, checks the website build, and validates the
compiler and CLI surfaces that create and consume Registry records.
