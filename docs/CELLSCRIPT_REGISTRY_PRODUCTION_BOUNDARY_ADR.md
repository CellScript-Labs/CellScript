# ADR: CellScript Registry Production Boundary

**Status**: accepted and implemented; amended 2026-08-01 for the unified
artifact model and mainnet deployment-evidence path.

**Decision date**: 2026-06-23
**Current amendment**: 2026-08-01

## Context

CKB ecosystem discovery spans objects with materially different trust and use
contracts: CellScript dependency source, profile libraries, CKB-VM executables,
deployed Script Cells, reproducible tooling, and copy-only starters. Treating
all of them as “packages” hides whether an object can be installed, executed,
deployed, or only copied. Treating publication, build verification, deployment,
and availability as one status creates false assurance.

The service also needs a wallet-rooted publisher identity without taking
custody of seed material, a static hash-verifiable read path, and an auditable
operator boundary.

## Decision

The production Registry uses:

1. one public `/v1/artifacts` resource family;
2. a closed artifact descriptor for kind, verification profile, language, and
   consumption mode;
3. independent verification, deployment, and availability states;
4. typed CKB wallet principals that authorise scoped delegated capabilities;
5. Postgres as the write authority and immutable static objects as the normal
   content transport;
6. an isolated, profile-aware verification worker;
7. signed, live-RPC-verified CKB mainnet deployment evidence;
8. fail-closed CellScript dependency resolution that accepts only the
   `cellscript_source` + `dependency` contract.

There is no account-style Registry identity, no Git convention as public
resolver authority, no testnet deployment option, and no second public package
route.

## Artifact Profiles

The accepted contracts are:

| Kinds | Profile | Consumption | Verification boundary |
|---|---|---|---|
| source/profile library | `cellscript_source` | dependency | compile authenticated snapshot |
| runtime verifier | `ckb_executable` | TCB | source + executable + ABI + optional recipe hashes |
| deployable contract | `ckb_executable` | deployment | source + executable + ABI + optional recipe hashes |
| reproducible binary | `reproducible_build` | TCB | source + output + recipe hashes and evidence |
| template | `copy_material` | copy | authenticated file-map hash |

The coordinate is shared discovery vocabulary, not shared consumption
semantics. A caller must select on profile and consumption mode, not infer them
from a name or file extension.

## State Model

Every release records:

```text
verification_status = pending | hash_bound | verified | evidence_required | rejected
deployment_status   = not_applicable | undeployed | deployed | chain_verified
availability_status = active | deprecated | yanked | quarantined
```

The axes are independent. Publication sets initial values only. Verification
and deployment claims require accepted evidence. Operator actions modify only
availability. Evidence and immutable identities are append-only.

The implementation may retain a derived internal status column while migrating
the deployed schema, but public responses and frontend decisions use the three
orthogonal fields.

## Publisher Principal and Capability

Accepted principals are:

```text
joyid_ckb       = normalized JoyID CKB public-key binding
ckb_secp256k1   = normalized compressed secp256k1 public-key binding
```

Display addresses may be retained for support but are not authority keys. The
API verifies scheme, canonical challenge, public-key recovery/binding, and
principal identity before storing a capability.

The wallet root authorises a P-256 capability scoped to a namespace/artifact,
with expiry and revocation. Daily publish and deployment requests use the
delegated key. Seed phrases and private wallet keys never cross the wallet
boundary.

Capabilities do not claim namespaces implicitly. The namespace must be active
and owned by the capability principal. Reserved names may require attributed
operator review.

## Wallet Product Boundary

The website exposes one CKB wallet entry that opens a compact chooser. The
chooser contains the supported CKB wallet directory; CCC-discovered signers can
connect directly, and unavailable connectors link to the official wallet or
use the external signature handoff.

The Registry does not pretend that catalog presence means runtime support.
Backend signature verification is identical for browser and external handoff
flows. Recovery phrases are never accepted by the frontend or API.

The network is fixed to CKB mainnet and is not shown as a selectable control.

## Write Path

Release admission verifies the active capability, scope, namespace ownership,
route/payload equality, closed artifact descriptor, immutable coordinate,
manifest/source hashes, signature, nonce, idempotency record, snapshot/bundle,
and initial state claims.

Immutable bundle and static release writes happen before database admission.
The release, verifier job, capability use, audit event, nonce, and completed
idempotency response commit transactionally. Admission reports verification as
queued; it is not verification evidence.

Non-CellScript artifacts use an explicit `Artifact.toml` and JSON bundle. The
closed `cellscript-registry-profile-contract-v1` binds build, declared security,
CKB/ABI, verifier IPC, reproducibility, or copy semantics to the immutable
objects. The bundle is bounded to 5 MiB. Unknown fields and unknown or duplicate
roles fail closed in admission, publisher CLI, and isolated verifier.

## Verification Boundary

The worker leases jobs with `FOR UPDATE SKIP LOCKED`, bounded retry, dead-letter
handling, and crash recovery. The verifier runs under resource and filesystem
bounds.

For CellScript source it authenticates and compiles the real snapshot. For
other profiles it validates bundle identity, required roles, and published
hashes. Reproducible output remains `evidence_required` until appropriate
evidence exists. A copied template is never promoted into dependency or TCB
semantics.

Evidence insertion and the worker publishing checkpoint commit together.
Static-object refresh happens afterward; reclaiming a crashed publishing job
repeats only the static write.

## Mainnet Deployment Boundary

A CKB executable begins as `undeployed`. Deployment evidence uses a separate
signed protocol and requires prior verified-build evidence.

The API accepts only `network = mainnet`, calls `get_live_cell` for the declared
OutPoint, and requires a live Cell whose data hash equals the published
executable hash. For Type-hash references it computes the returned Type Script
hash from canonical Molecule serialization; for data-hash references it
requires code hash and data hash equality.

For DepGroups, the API decodes the live container data as canonical Molecule
`OutPointVec`, loads the members, and verifies the matching live code Cell. The
container hash is not substituted for the member executable identity.

Success appends hash-addressed evidence and sets `deployment_status` to
`chain_verified`. It does not alter verification or availability.

The Registry may additionally attest the release/deployment tuple in a live
mainnet Cell. Canonical `cellscript-registry-commitment-v1` JSON is CKB
Blake2b-hashed into `CSREGv1 || hash` Cell data. Acceptance checks that exact
data, the attestor Lock hash, and a Registry Type Script hash used for chain
indexing. A public commitment-proof route returns the preimage, expected Cell
data, and accepted attestation evidence. The full source, ABI, build recipe,
compiler metadata, audit corpus, and publisher history remain off-chain and
content-addressed.

## Read Path

Production domains:

```text
api.registry.cellscript.dev  -> authenticated writes and dynamic artifact reads
registry.cellscript.dev      -> immutable bundles and static release JSON
cellscript.dev/registry      -> static Astro discovery and publishing UI
```

Static release objects use:

```text
https://registry.cellscript.dev/artifacts/:namespace/:name/releases/:release.json
```

The static origin does not require Postgres. Objects include immutable bundle
identity, artifact descriptor, all state axes, and accepted evidence. Consumers
verify object, file, source, build, and deployment hashes independently.

Public list/detail/evidence routes suppress quarantined releases. The API list
supports explicit kind, verification, deployment, availability, namespace,
query, and pagination filters.

Generic consumers use explicit `cellc artifact` operations. Fetch/verify check
the receipt and all immutable identities; pin records TCB/deployment inputs;
copy safely materializes only an authenticated file map; record-deployment
submits mainnet evidence; CellDep generation requires attached RPC evidence;
commitment generation produces the canonical chain payload. Generic artifacts
never flow through dependency installation.

## Resolver Boundary

`cellc install` resolves through the public artifact API and rejects profiles
other than `cellscript_source` or consumption modes other than `dependency`.
The resolver downloads the immutable source snapshot, verifies its object and
file identities, and only then materializes it.

Unverified releases require an explicit `--allow-unverified`; quarantined
releases are absent from public reads and require operator remediation rather
than accidental fallback. Resolver failure never falls back to a conventional
Git URL. Path and explicit Git dependencies remain independent user-selected
dependency sources.

## Abuse and Operations

The service enforces bounded bodies, per-IP/ASN/principal/capability/artifact
quota hooks, namespace claim cooldown, reserved-name policy, signed one-use
nonces, and idempotency conflict detection. Successful and rejected sensitive
actions are attributable through the audit log.

Admin availability changes accept only `active`, `deprecated`, `yanked`, and
`quarantined`. Generic admin mutation cannot manufacture build or deployment
assurance. Evidence-specific recovery paths validate identity and predecessor
evidence.

API and static responses use HSTS, no-sniff, anti-framing, no-referrer,
restrictive browser permissions, and deny-all JSON CSP. Postgres is internal;
static serving is read-only.

## Deployment Choice

The live self-hosted slice uses Postgres 17, Node 22, an isolated verifier,
persistent object storage, and read-only nginx behind the production TLS proxy.
Cloudflare Worker, Hyperdrive, Neon, and R2 remain a supported equivalent
deployment shape.

Migrations are additive after the frozen `0001` baseline. The artifact-model
migration intentionally refuses to transform non-empty legacy release data
because no released public contract exists that would justify a lossy mapping.

Readiness covers database/object access, admin configuration, and the verifier
heartbeat. Backups contain a Postgres custom dump, object archive, image
identity, and checksum manifest; restores are rehearsed into empty volumes
before traffic cut-over.

## Consequences

Benefits:

- broad CKB discovery without weakening CellScript dependency safety;
- deployed and undeployed executables are distinguishable;
- publication cannot masquerade as verification;
- mainnet deployment claims are independently checked against live Cells;
- static content survives write-database incidents;
- wallet authority stays outside the Registry.

Costs:

- publishers of generic artifacts must construct an explicit bundle;
- reproducibility needs evidence beyond uploaded bytes;
- deployment recording requires live mainnet RPC availability;
- internal storage still carries derived compatibility fields during the
  additive migration.

## Rejected Alternatives

- **One `status` field**: conflates assurance, deployment, and availability.
- **Infer artifact type from content**: ambiguous and unsafe for resolution.
- **Treat every entry as installable**: permits executable/template confusion.
- **Git convention as resolver authority**: conflates naming with ownership and
  availability.
- **Store the full evidence corpus on chain**: expensive and unnecessary; the
  chain should carry runtime commitments while full evidence remains
  content-addressed off chain.
- **Accept testnet deployment records**: creates a misleading production state
  in a public Registry whose deployed status is used for mainnet discovery.
